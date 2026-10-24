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

pub(crate) fn write(doc: &Document, pages: &[usize], allow_insert: bool) -> Result<Option<Object>, SignError> {
    if pages.is_empty() {
        return Ok(None);
    }
    if pages.len() > 10000 {
        return Err(SignError::Pdf("Choose at most 10000 protected pages".into()));
    }
    let refs = pdfcraft_annot::page_refs(doc).map_err(|e| SignError::Pdf(e.to_string()))?;
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for page in pages {
        let r = *refs.get(*page).ok_or_else(|| SignError::Pdf(format!("Protected page {} does not exist", page.saturating_add(1))))?;
        if seen.insert(r) {
            selected.push(Object::Ref(r));
        }
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
            return Err(SignError::Pdf("A signed page cannot be deleted or reordered; remove its signature first".into()));
        }
        for page in &rule.pages {
            if checked.insert(*page) && state(before, *page, &all, &mut budget)? != state(after, *page, &all, &mut budget)? {
                return Err(SignError::Pdf("This edit changes a protected page or a shared resource; remove its signature first".into()));
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
