//! pdfcraft-engine — the façade every frontend talks to (architecture §4).
//!
//! Holds open documents, their edit history and the tool catalogue. Frontends never touch the
//! parsing, rendering or editing crates directly.
//!
//! **Editing model.** Each document keeps a `pdfcraft_cos::Document` (the object graph with a
//! copy-on-write overlay of edits). An edit runs on a clone, and on success the previous state is
//! pushed onto the undo stack (clones share all unchanged data, so this is cheap). After every
//! edit the *working file* is produced by an incremental write — original bytes plus one
//! appended revision — and the view is refreshed from it, so what you see is exactly what Save
//! will write. Saving rebases onto the written bytes, so the next save appends only new edits.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod actions;
pub mod autotag;
mod bookmarks;
pub mod build_info;
pub mod camera;
pub mod camera_clean;
pub mod camera_edges;
pub mod catalog;
pub mod commands;
mod comments;
pub mod compare;
mod copy_image;
pub mod dates;
mod document;
mod edit;
mod error;
pub mod export;
pub mod field_table;
mod fill_sign;
mod history;
pub mod js;
mod layers;
pub mod links;
pub mod ocr;
mod open;
pub mod optimizer;
mod page_filter;
mod pictures;
pub mod print_raster;
pub mod printcheck;
mod protection;
mod scan_colours;
mod scan_fields;
mod session;
pub mod signature_image;
pub mod signature_scan;
mod sources;
mod stamp_preview;
#[cfg(test)]
mod test_hooks;
pub mod text_fonts;
mod text_preview;
pub mod word_count;
pub mod xfa;
mod xfa_form;

use std::sync::Arc;

pub use links::{
    MAX_REMOTE_DOCUMENT_BYTES, RemoteDocumentPathError, read_remote_document, remote_document_path_is_absolute, resolve_remote_document_path,
};
pub use signature_image::{ImageSignaturePreview, SignatureImage};
pub use signature_scan::{PhotoRect, ScanError, SignaturePhoto, Turn};
pub use stamp_preview::StampPreview;
pub use text_preview::TextEditPreview;

pub use pdfcraft_organize::{BoxSpec, FieldConflicts, PageBox, SplitBy, Transition, TransitionSpec, mime_for_name, split_ranges};

pub use pdfcraft_organize::LabelStyle;
pub use pdfcraft_organize::Standard;
pub use pdfcraft_organize::check_custom_key;
pub use pdfcraft_organize::view::{InitialView, Layout as InitialLayout, Magnification, Navigation};

pub use pdfcraft_cos::Algorithm;
pub use pdfcraft_create::{CONVERTIBLE, CreateError, ImageResolution, SourceKind, decode_text, source_kind};
pub use pdfcraft_edit::added::{
    CHAR_SPACING_LIMIT as ADDED_CHAR_SPACING_LIMIT, paste_place as added_paste_place, paste_rect as added_paste_rect, text_rect as added_text_rect,
    text_width as added_text_width,
};
pub use pdfcraft_edit::{
    Added, AddedImage, AddedText, Align as TextAlign, Background, Content as AddedContent, Family as FontFamily, Hairlines, HairlinesFixed,
    HeaderFooter, MarkKind, PrinterMarks, Watermark,
};
pub use pdfcraft_forms::{
    BorderStyle, CalculatedField, CalculationIssue, CalculationOrder, CheckStyle, Field as FormField, FieldAction, FieldChange, FieldFont,
    FieldKind as FormFieldKind, FieldProps, FieldValue, Look as FieldLook, LookPatch as FieldLookPatch, NewField, TabOrder, Trigger as FieldTrigger,
    Widget as FormWidget, af as form_scripts, appearance as form_appearance, appearance::auto_text_size_by as field_auto_text_size_by,
    calculations_changed as form_calculations_changed, flags as field_flags,
};

pub use pdfcraft_a11y as a11y;
pub use pdfcraft_edit::{BlockKey, BlockStyle, PageImage, RangeStyle, TextBlock, TextLine, block_overlapping, first_undrawable};
pub use pdfcraft_measure as measure;
pub use pdfcraft_xfa::Report as XfaLayout;

pub use pdfcraft_edit::{EditableObject, MAX_MOVE_OBJECTS, ObjectKind, ObjectTarget};

pub use pdfcraft_fonts::{MAX_SIGNATURE_CHARS, ScriptOutline, script_missing, script_outline, undrawable_message};

/// Comment geometry helpers (text-box line breaking) for frontends.
pub use pdfcraft_annot::appearance as annot_text;
pub use pdfcraft_annot::links::{Highlight as LinkHighlight, LinkAction, LinkItem, LinkStyle};
pub use pdfcraft_annot::{
    AppearanceCache, AttachIcon, FillMark, LineEnding, Markup, NOTE_SIZE, NewAnnotation, NoteIcon, OverlayFont, OverlayLook, Props as CommentProps,
    ReviewState, Rgb, Shape, StampGroup, StampKind, Style, rect_quad,
};
pub use pdfcraft_forms::detect;
pub use pdfcraft_optimize as optimize;
pub use pdfcraft_preflight as pdfa;
pub use pdfcraft_print as print;
pub use pdfcraft_redact::codes::{CODE_SETS as REDACTION_CODE_SETS, CodeSet as RedactionCodeSet};
pub use pdfcraft_redact::patterns::{
    MAX_WORDS as REDACT_MAX_WORDS, PATTERNS as REDACT_PATTERNS, Pattern as RedactPattern, find as find_pattern, word_list as redact_word_list,
};
pub use pdfcraft_redact::sanitize::{HIDDEN, Hidden};
pub use pdfcraft_sign as sign;
pub use pdfcraft_sign::{SignOptions, SignatureInfo, Status as SignatureStatus, TrustStore};
pub use pdfcraft_xfdf::Format as DataFormat;

use bookmarks::outline_rows;
pub use bookmarks::{BookmarkPage, BookmarkRow};
use comments::comment_list;
pub use comments::{SummarySort, comment_kind, comment_summary, comment_type_name};
pub use document::{Document, TextNotice, display_bytes_for, render_password_for};
use document::{display_bytes, info_fields, recovered_render_password, same_pages, shown_under_redactions, undrawn_notices};
pub use edit::{BATCH_LABELS, EDIT_LABELS, Edit};
pub use error::{EditError, SourceProblem};
pub use fill_sign::{displayed_offset, text_box_at, typed_signature_graphic, typed_signature_shape};
pub use history::Label;
use layers::{apply_layer_state, set_default_layer_state, use_layer_choices};
pub use open::{OpenContext, PreparedDocument};
pub use page_filter::{PageOrientation, PageParity, filter_pages};
pub use pictures::MarkFile;
use pictures::{embed_image, fit_picture, mark_source};
use protection::unrestricted;
pub use protection::{Changes, Printing, Protection, SecuritySummary};
pub use session::Session;
pub use sources::{CombineSource, Combined, combine_source_check, source_page_count};
use sources::{open_source, open_source_with};
#[cfg(test)]
use test_hooks::{check_refresh_panic, panic_on_refresh, record_link_scan, record_marker_scan, reset_secondary_scan_counts, secondary_scan_counts};
use xfa_form::{
    XFA_REMOVED_PREFIX, XFA_SCRIPTS_OFF, decode_data_file, image_content_type, is_xfa_data_file, note_warnings, strip_xfa, sync_xfa_state,
    undrawn_field_warnings, xfa_after_data_change, xfa_layout, xfa_redraw, xfa_sync_datasets, xfa_values_from_datasets,
};

mod clock;
mod doc_state;

use clock::{local_utc_offset, parse_ymd};
pub use doc_state::{DocId, ImageEdit, OptimizeReport, RecoverySnapshot, SplitPart, guard, merge_data_files};
use doc_state::{LOW_RESOLUTION_PRINT_DPI, render_threads, renderer_password, signatures_of, writable};

#[cfg(test)]
mod tests;

/// What opens a certificate-protected document (§7.6.5): the seed and permissions one of its
/// envelopes gave up to one of the session's identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CertificateKey {
    pub seed: [u8; 20],
    pub permissions: i32,
}

/// Whether the document's next save is certificate security (`/Adobe.PubSec`): its working
/// bytes cannot be read by the renderer, which gets a decrypted copy instead.
fn certificate_secured(cos: &pdfcraft_cos::Document) -> bool {
    cos.output_handler().is_some_and(|h| h.dict().filter == pdfcraft_cos::PUBSEC_FILTER)
}

/// The document as it is now, with no encryption: what the renderer and inspector read when
/// the file itself is certificate-protected.
fn plain_copy(cos: &pdfcraft_cos::Document) -> Result<Arc<Vec<u8>>, EditError> {
    let mut plain = cos.clone();
    plain.remove_encryption();
    write_full(&plain, &SaveOptions::default()).map(Arc::new).map_err(|e| EditError::Write(e.to_string()))
}

/// The working file: the file as opened or last saved, plus one appended update with the unsaved
/// edits. A certificate-protected document's working file stays encrypted; its
/// [`Document::bytes`] are a decrypted copy that must not leave memory.
fn working_file(cos: &pdfcraft_cos::Document) -> Result<Arc<Vec<u8>>, EditError> {
    if cos.is_modified() {
        write_incremental(cos, &SaveOptions::default()).map(Arc::new).map_err(|e| EditError::Write(e.to_string()))
    } else {
        Ok(cos.bytes().clone())
    }
}

/// What one of `identities` makes of the envelopes: the seed and permissions of the first
/// envelope addressed to it.
fn unlock_with_identities(identities: &[Arc<pdfcraft_sign::DigitalId>], envelopes: &[Vec<u8>]) -> Option<CertificateKey> {
    for id in identities {
        for env in envelopes {
            if let Ok(Some(content)) = pdfcraft_sign::envelope::open(env, id)
                && let Some((seed, permissions)) = pdfcraft_cos::permissions_from_envelope(&content)
            {
                return Some(CertificateKey { seed, permissions });
            }
        }
    }
    None
}

/// Certificate security to apply (§7.6.5): the file encrypted for `recipients`, each with the
/// permissions their envelope carries (`-1`: everything, what the author gives themself).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateProtection {
    pub recipients: Vec<CertificateRecipient>,
    /// AES-256 (Acrobat X and later) or AES-128 (Acrobat 7 and later).
    pub algorithm: pdfcraft_cos::Algorithm,
    pub encrypt_metadata: bool,
    /// The file key's seed, fresh from [`Session::new_seed`].
    pub seed: [u8; 20],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateRecipient {
    /// The certificate, DER.
    pub certificate: Vec<u8>,
    /// Permission bits as `/P` (§7.6.4.2, Table 22); `-1` for everything.
    pub permissions: i32,
}

/// Whether the opening password allows an edit (§7.6.4.2, Table 22).
fn check_certified_edit(edit: &Edit, permission: Option<u8>) -> Result<(), EditError> {
    let Some(p) = permission else { return Ok(()) };
    if let Edit::Batch { edits, .. } = edit {
        for e in edits {
            check_certified_edit(e, permission)?;
        }
        return Ok(());
    }
    let allowed = matches!(edit, Edit::ClearSignature { .. })
        || (p >= 2 && matches!(scope_of(edit), Scope::Form))
        || (p == 3 && matches!(scope_of(edit), Scope::Comments));
    if allowed {
        Ok(())
    } else {
        Err(EditError::Invalid("The document's certification does not permit this edit; remove the signature first".into()))
    }
}

/// Whether the edit, or any edit inside a batch, deletes pages.
fn deletes_pages(edit: &Edit) -> bool {
    match edit {
        Edit::DeletePages { .. } => true,
        Edit::Batch { edits, .. } => edits.iter().any(deletes_pages),
        _ => false,
    }
}

pub mod assistant;

pub mod clear_signature;

mod ocr_pdf;

pub mod outputcheck;

mod page_workflows;

pub mod spelling;

pub mod templates;

use history::*;

use pdfcraft_cos::{SaveOptions, write_full, write_incremental};

use pdfcraft_render::DocInfo;

pub use pdfcraft_edit::PathOutline;

pub use pdfcraft_edit::PathStyle;

pub use pdfcraft_edit::ObjectAlign;
