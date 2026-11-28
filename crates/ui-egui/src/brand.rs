//! Lion PDF's mark: the lion of `assets/app-icon/pdfcraft.svg`. The picture comes from the framework's SVG source,
//! which bounds the decode and draws nothing for bytes it cannot read.

use craft::ui::contract::Show as _;
use craft::ui::display::{Picture, PictureSize};

const MARK: &[u8] = include_bytes!("../../../assets/app-icon/pdfcraft.svg");

/// The square Lion PDF mark at `size`.
pub(crate) fn mark(ui: &mut egui::Ui, size: PictureSize) {
    let _ = Picture::svg("Lion PDF".to_string(), MARK).extent(size).show(ui);
}
