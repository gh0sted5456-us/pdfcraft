//! PdfCraft's saved AI opt-out label. This is not PDF encryption or an external-reader policy.
use crate::{Document, Object, OrganizeError};
const KEY: &[u8] = b"PdfCraftAIConfidential";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Confidentiality {
    pub document: bool,
    /// Page-specific labels; the document label applies in addition to these.
    pub pages: Vec<usize>,
}
pub(crate) fn marked(doc: &Document, dict: &pdfcraft_cos::Dict) -> bool {
    // Unknown future or malformed label values fail closed instead of permitting AI access.
    dict.get(KEY).is_some_and(|v| !matches!(doc.resolve(v).as_ref(), Object::Bool(false)))
}
pub fn document_confidential(doc: &Document) -> Result<bool, OrganizeError> {
    let root = doc.root().ok_or_else(|| OrganizeError::Invalid("Cannot verify the document's Confidential label".into()))?;
    let value = doc.get(root);
    let dict = value.as_dict().ok_or_else(|| OrganizeError::Invalid("Cannot verify the document's Confidential label".into()))?;
    Ok(marked(doc, dict))
}
pub fn confidentiality(doc: &Document) -> Result<Confidentiality, OrganizeError> {
    let pages = crate::walk(doc)?;
    Ok(Confidentiality {
        document: document_confidential(doc)?,
        pages: pages
            .iter()
            .enumerate()
            .filter_map(|(i, (r, _))| {
                let value = doc.get(*r);
                value.as_dict().is_none_or(|d| marked(doc, d)).then_some(i)
            })
            .collect(),
    })
}
pub fn check_ai_pages(doc: &Document, pages: &[usize]) -> Result<(), OrganizeError> {
    let policy = confidentiality(doc)?;
    let count = crate::page_count(doc)?;
    if let Some(page) = pages.iter().find(|p| **p >= count) {
        return Err(OrganizeError::NoSuchPage(*page));
    }
    if policy.document {
        return Err(OrganizeError::Invalid("This document is Confidential. AI access is disabled; nothing was shared.".into()));
    }
    if let Some(page) = pages.iter().find(|p| policy.pages.contains(p)) {
        return Err(OrganizeError::Invalid(format!(
            "Page {} is Confidential. Choose pages that allow AI access; nothing was shared.",
            page.saturating_add(1)
        )));
    }
    Ok(())
}
/// `None` changes the document label; selected page labels remain independent.
pub fn set_confidential(doc: &mut Document, pages: Option<&[usize]>, confidential: bool) -> Result<(), OrganizeError> {
    let targets = match pages {
        None => vec![doc.root().ok_or_else(|| OrganizeError::Invalid("The document has no catalog".into()))?],
        Some(selected) => {
            let all = crate::walk(doc)?;
            if selected.is_empty() || selected.len() > 10000 {
                return Err(OrganizeError::Invalid("Select 1–10000 pages for the Confidential label".into()));
            }
            let mut targets = Vec::new();
            for page in selected {
                let target = all.get(*page).ok_or(OrganizeError::NoSuchPage(*page))?.0;
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            targets
        }
    };
    for target in targets {
        doc.update_dict(target, |d| {
            if confidential {
                d.set(KEY.to_vec(), Object::Bool(true));
            } else {
                d.remove(KEY);
            }
        })?;
    }
    Ok(())
}
/// A copied page keeps the source's document-level label even outside that document.
pub(crate) fn inherit_confidential(source_confidential: bool, page_confidential: bool, page: &mut pdfcraft_cos::Dict) {
    if source_confidential && !page_confidential {
        page.set(KEY.to_vec(), Object::Bool(true));
    }
}
