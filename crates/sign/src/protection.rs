//! PdfCraft page protection recorded inside the signed revision. Other readers need not enforce it.
use crate::SignError;
use pdfcraft_cos::{Dict, Document, ObjRef, Object};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PageProtection {
    pub pages: Vec<ObjRef>,
    pub allow_insert: bool,
}

/// Unsigned PdfCraft locks stored in the catalog, attached to page identities.
pub fn local_pages(doc: &Document) -> Result<Vec<ObjRef>, SignError> {
    let root = doc.root().ok_or_else(|| SignError::Pdf("The document has no catalog".into()))?;
    let catalog = doc.get(root);
    let d = catalog.as_dict().ok_or_else(|| SignError::Pdf("The document catalog is invalid".into()))?;
    let Some(value) = d.get(b"PdfCraftPageLocks") else { return Ok(Vec::new()) };
    let array = value.as_array().filter(|a| a.len() <= 10000).ok_or_else(|| SignError::Pdf("The page lock list is invalid or too large".into()))?;
    array.iter().map(|v| v.as_ref().ok_or_else(|| SignError::Pdf("A page lock has no page identity".into()))).collect()
}

/// Change unsigned locks only; the engine checks existing signature policies before committing.
pub fn set_local(doc: &mut Document, pages: &[usize], locked: bool) -> Result<(), SignError> {
    if pages.is_empty() || pages.len() > 10000 {
        return Err(SignError::Pdf("Choose between 1 and 10000 pages".into()));
    }
    let refs = pdfcraft_annot::page_refs(doc).map_err(|e| SignError::Pdf(e.to_string()))?;
    let mut locks = local_pages(doc)?;
    for index in pages {
        let page = *refs.get(*index).ok_or_else(|| SignError::Pdf("A selected page does not exist".into()))?;
        if locked {
            if !locks.contains(&page) {
                locks.push(page);
            }
        } else {
            locks.retain(|r| *r != page);
        }
    }
    if locks.len() > 10000 {
        return Err(SignError::Pdf("Choose at most 10000 locked pages".into()));
    }
    let root = doc.root().ok_or_else(|| SignError::Pdf("The document has no catalog".into()))?;
    doc.update_dict(root, |d| {
        if locks.is_empty() {
            d.remove(b"PdfCraftPageLocks");
        } else {
            d.set(b"PdfCraftPageLocks".to_vec(), Object::Array(locks.into_iter().map(Object::Ref).collect()));
        }
    })
    .map_err(|e| SignError::Pdf(e.to_string()))?;
    Ok(())
}

pub(crate) fn write(doc: &Document, pages: &[usize], allow_insert: bool) -> Result<Option<Object>, SignError> {
    let local = local_pages(doc)?;
    if pages.is_empty() && local.is_empty() {
        return Ok(None);
    }
    if pages.len() > 10000 {
        return Err(SignError::Pdf("Choose at most 10000 protected pages".into()));
    }
    let refs = pdfcraft_annot::page_refs(doc).map_err(|e| SignError::Pdf(e.to_string()))?;
    let mut selected: Vec<_> = local.iter().copied().map(Object::Ref).collect();
    let mut seen: HashSet<_> = local.into_iter().collect();
    for page in pages {
        let r = *refs.get(*page).ok_or_else(|| SignError::Pdf(format!("Protected page {} does not exist", page.saturating_add(1))))?;
        if seen.insert(r) {
            selected.push(Object::Ref(r));
        }
    }
    if selected.len() > 10000 {
        return Err(SignError::Pdf("Choose at most 10000 protected pages".into()));
    }
    let mut d = Dict::new();
    d.set(b"Version".to_vec(), Object::Int(1));
    d.set(b"Pages".to_vec(), Object::Array(selected));
    d.set(b"AllowInsert".to_vec(), Object::Bool(allow_insert));
    Ok(Some(Object::Dict(d)))
}

pub(crate) fn read(v: &Dict) -> Option<PageProtection> {
    let d = v.get(b"PdfCraftProtection")?.as_dict()?;
    if d.int(b"Version") != Some(1) {
        return None;
    }
    let array = d.get(b"Pages")?.as_array()?;
    if array.is_empty() || array.len() > 10000 {
        return None;
    }
    let pages: Option<Vec<_>> = array.iter().map(Object::as_ref).collect();
    Some(PageProtection { pages: pages?, allow_insert: matches!(d.get(b"AllowInsert"), Some(Object::Bool(true))) })
}

/// Compare the protected pages and their resources before committing an edit. References, not
/// page numbers, keep protection attached when other pages are inserted or moved.
pub fn check(before: &Document, after: &Document, rules: &[PageProtection]) -> Result<(), SignError> {
    if rules.is_empty() {
        return Ok(());
    }
    let old = pdfcraft_annot::page_refs(before).map_err(|e| SignError::Pdf(e.to_string()))?;
    let new = pdfcraft_annot::page_refs(after).map_err(|e| SignError::Pdf(e.to_string()))?;
    let all: HashSet<_> = old.iter().chain(&new).copied().collect();
    let mut checked = HashSet::new();
    let mut budget = 100000usize;
    for rule in rules {
        if !rule.allow_insert && new.iter().any(|r| !old.contains(r)) {
            return Err(SignError::Pdf("This signature does not permit adding pages".into()));
        }
        let locked: HashSet<_> = rule.pages.iter().copied().collect();
        let order: Vec<_> = old.iter().filter(|r| locked.contains(r)).collect();
        let next_order: Vec<_> = new.iter().filter(|r| locked.contains(r)).collect();
        if order != next_order || rule.pages.iter().any(|r| !new.contains(r)) {
            return Err(SignError::Pdf(
                "A locked page cannot be deleted or reordered; unlock it first (signed locks require signature removal)".into(),
            ));
        }
        for page in &rule.pages {
            if checked.insert(*page) && state(before, *page, &all, &mut budget)? != state(after, *page, &all, &mut budget)? {
                return Err(SignError::Pdf(
                    "This edit changes a locked page or a shared resource; unlock it first (signed locks require signature removal)".into(),
                ));
            }
        }
    }
    Ok(())
}

type State = (Dict, HashMap<ObjRef, Arc<Object>>);
fn state(doc: &Document, page: ObjRef, pages: &HashSet<ObjRef>, budget: &mut usize) -> Result<State, SignError> {
    let o = doc.get(page);
    let mut d = o.as_dict().cloned().ok_or_else(|| SignError::Pdf("A protected page is missing".into()))?;
    let mut parent = d.reference(b"Parent");
    let mut seen = HashSet::new();
    for _ in 0..64 {
        let Some(r) = parent else { break };
        if !seen.insert(r) {
            return Err(SignError::Pdf("A protected page has cyclic inheritance".into()));
        }
        let o = doc.get(r);
        let Some(p) = o.as_dict() else {
            return Err(SignError::Pdf("A protected page has invalid inheritance".into()));
        };
        for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
            if !d.contains(key)
                && let Some(v) = p.get(key)
            {
                d.set(key.to_vec(), v.clone());
            }
        }
        parent = p.reference(b"Parent");
    }
    if parent.is_some() {
        return Err(SignError::Pdf("Protected page inheritance is too deep".into()));
    }
    d.remove(b"Parent");
    let mut objects = HashMap::new();
    walk(doc, &Object::Dict(d.clone()), pages, &mut objects, 0, budget)?;
    Ok((d, objects))
}
fn walk(
    doc: &Document,
    value: &Object,
    pages: &HashSet<ObjRef>,
    objects: &mut HashMap<ObjRef, Arc<Object>>,
    depth: usize,
    budget: &mut usize,
) -> Result<(), SignError> {
    if depth > 64 || *budget == 0 {
        return Err(SignError::Pdf("The protected page's resources exceed the safe inspection limit".into()));
    }
    *budget -= 1;
    match value {
        Object::Ref(r) if !pages.contains(r) && !objects.contains_key(r) => {
            let o = doc.get(*r);
            objects.insert(*r, o.clone());
            walk(doc, &o, pages, objects, depth + 1, budget)?;
        }
        Object::Dict(d) => {
            for (_, v) in d.iter() {
                walk(doc, v, pages, objects, depth + 1, budget)?;
            }
        }
        Object::Array(a) => {
            for v in a {
                walk(doc, v, pages, objects, depth + 1, budget)?;
            }
        }
        Object::Stream(s) => {
            for (_, v) in s.dict.iter() {
                walk(doc, v, pages, objects, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
