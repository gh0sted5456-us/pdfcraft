//! Search page sets and transfers between documents already open in the session.

use crate::{DocId, Edit, EditError, Session};

impl Session {
    /// Unique matching pages in document order (zero-based). A failed page refuses the
    /// complete search, so extraction never quietly omits an unreadable page.
    pub fn matching_pages(&self, id: DocId, query: &str, case_sensitive: bool, whole_words: bool) -> Result<Vec<usize>, EditError> {
        if query.trim().is_empty() {
            return Err(EditError::Invalid("enter a search phrase first".into()));
        }
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        if !doc.allows_copying() {
            return Err(EditError::NotPermitted("searching document text"));
        }
        let mut renderer = pdfcraft_render::PageRenderer::new(doc.display.clone(), doc.config.clone());
        let mut pages = Vec::new();
        for page in 0..doc.info.pages.len() {
            let out = renderer.render(pdfcraft_render::RenderRequest { page, kind: pdfcraft_render::RequestKind::Text, ..Default::default() });
            if let Some(error) = out.error {
                return Err(EditError::Invalid(format!("couldn't search page {}: {error}", page.saturating_add(1))));
            }
            if out.text.is_some_and(|text| !text.find_opts_limited(query, case_sensitive, whole_words, 1).is_empty()) {
                pages.push(page);
            }
        }
        Ok(pages)
    }

    /// Copy pages between open documents, optionally removing them from the source afterwards.
    /// Each document has its own Undo. A failed removal keeps both copies and returns a warning;
    /// source pages are never removed before the destination accepts them. No file is written.
    pub fn transfer_pages(
        &mut self,
        source: DocId,
        target: DocId,
        pages: &[usize],
        at: usize,
        remove_source: bool,
    ) -> Result<Option<String>, EditError> {
        if source == target {
            return Err(EditError::Invalid("use Move pages or Duplicate pages within one document".into()));
        }
        let from = self.get(source).ok_or(EditError::NoDocument)?;
        let to = self.get(target).ok_or(EditError::NoDocument)?;
        if pages.is_empty() || pages.len() > 1000 {
            return Err(EditError::Invalid("select between 1 and 1000 pages".into()));
        }
        let mut pages = pages.to_vec();
        pages.sort_unstable();
        pages.dedup();
        if pages.iter().any(|p| *p >= from.info.pages.len()) || at > to.info.pages.len() {
            return Err(EditError::Invalid("a page or insertion position is outside the document".into()));
        }
        if !from.allows_assembly() || !to.allows_assembly() {
            return Err(EditError::NotPermitted("transferring pages"));
        }
        if remove_source && (pages.len() >= from.info.pages.len() || from.read_only_reason.is_some()) {
            return Err(EditError::Invalid("the source must remain editable with at least one page; copy instead".into()));
        }
        let name = from.name.clone();
        // Extract from the working document, including unsaved edits and unlocked encryption.
        let bytes = self.extract(source, &pages)?;
        self.apply(target, Edit::InsertPagesFrom { name, bytes, pages: None, at, field_conflicts: crate::FieldConflicts::Rename })?;
        if remove_source && let Err(error) = self.apply(source, Edit::DeletePages { pages }) {
            return Ok(Some(format!("Pages were copied, but couldn't be removed from the source: {error}. Both copies were kept.")));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn documents() -> (Session, DocId, DocId) {
        let mut s = Session::new();
        let bytes = s.create_blank(300.0, 400.0, 3).unwrap();
        let source = s.open_new("source", bytes.clone()).unwrap();
        let target = s.open_new("target", bytes).unwrap();
        for (page, label) in ["Invoice Invoice", "Invoices", "INVOICE"].iter().enumerate() {
            s.apply(
                source,
                Edit::AddText { page, text: crate::AddedText { rect: [20.0, 300.0, 240.0, 325.0], text: label.to_string(), ..Default::default() } },
            )
            .unwrap();
        }
        (s, source, target)
    }

    #[test]
    fn matching_pages_are_unique_ordered_and_obey_search_options() {
        let (s, source, _) = documents();
        assert_eq!(s.matching_pages(source, "invoice", false, false).unwrap(), [0, 1, 2]);
        assert_eq!(s.matching_pages(source, "Invoice", true, true).unwrap(), [0]);
        assert_eq!(s.matching_pages(source, "invoice", false, true).unwrap(), [0, 2]);
        assert!(s.matching_pages(source, "absent", false, false).unwrap().is_empty());
        assert!(s.matching_pages(source, "  ", false, false).is_err());
        let bytes = s.extract(source, &[0, 2]).unwrap();
        let mut extracted = Session::new();
        let id = extracted.open_new("results", bytes).unwrap();
        assert_eq!(extracted.get(id).unwrap().info.pages.len(), 2);
        assert_eq!(s.get(source).unwrap().info.pages.len(), 3);
    }

    #[test]
    fn transfers_keep_unsaved_text_in_page_order_and_undo_separately() {
        let (mut s, source, target) = documents();
        let original = s.get(source).unwrap().bytes.clone();
        assert_eq!(s.transfer_pages(source, target, &[2, 0, 2], 1, false).unwrap(), None);
        assert_eq!(s.get(target).unwrap().info.pages.len(), 5);
        assert_eq!(s.matching_pages(target, "invoice", false, true).unwrap(), [1, 2]);
        assert_eq!(s.get(source).unwrap().bytes, original);
        s.undo(target).unwrap();
        assert_eq!(s.get(target).unwrap().info.pages.len(), 3);
        assert_eq!(s.transfer_pages(source, target, &[0, 2], 3, true).unwrap(), None);
        assert_eq!(s.get(source).unwrap().info.pages.len(), 1);
        assert_eq!(s.get(target).unwrap().info.pages.len(), 5);
        s.undo(source).unwrap();
        s.undo(target).unwrap();
        assert_eq!(s.get(source).unwrap().bytes, original);
    }

    #[test]
    fn invalid_transfers_leave_both_documents_and_history_intact() {
        let (mut s, source, target) = documents();
        let before: Vec<_> = [source, target]
            .into_iter()
            .map(|id| {
                let d = s.get(id).unwrap();
                (Arc::clone(&d.bytes), d.edit_generation(), d.can_undo().map(str::to_owned))
            })
            .collect();
        for (pages, at, moving) in [(vec![], 0, false), (vec![3], 0, false), (vec![0], 4, false), (vec![0, 1, 2], 0, true)] {
            assert!(s.transfer_pages(source, target, &pages, at, moving).is_err());
        }
        assert!(s.transfer_pages(source, source, &[0], 0, false).is_err());
        for (id, expected) in [source, target].into_iter().zip(before) {
            let d = s.get(id).unwrap();
            assert_eq!((Arc::clone(&d.bytes), d.edit_generation(), d.can_undo().map(str::to_owned)), expected);
        }
    }
}
