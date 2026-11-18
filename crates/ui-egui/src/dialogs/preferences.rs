//! Upstream's segmented pill controls and preference search matching, which Lion PDF's own
//! Preferences (`preferences_ui`) and the hotbar use. Lion PDF keeps its own Preferences layout.

use egui::{Color32, CornerRadius, Margin, Sense, Stroke, vec2};

use crate::theme::{self, Tokens};

/// A segmented pill control for mutually exclusive choices.
///
/// Draws a rounded container with options inside. The active option is highlighted
/// with the accent colour. Emits AccessKit RadioButton widget info with the option's label.
pub fn segmented_pill_choice<T: PartialEq + Copy>(ui: &mut egui::Ui, t: &Tokens, current: &mut T, options: &[(T, &str)]) -> bool {
    let with_a11y: Vec<(T, &str, &str)> = options.iter().map(|&(v, l)| (v, l, l)).collect();
    segmented_pill_choice_a11y(ui, t, current, &with_a11y)
}

/// A segmented pill selection bar with separate accessibility labels.
pub fn segmented_pill_choice_a11y<T: PartialEq + Copy>(ui: &mut egui::Ui, t: &Tokens, current: &mut T, options: &[(T, &str, &str)]) -> bool {
    segmented_pill_choice_a11y_query(ui, t, current, options, "").0
}

/// A segmented pill selection bar with separate accessibility labels and optional query highlighting.
/// Returns whether the selection changed and the Response of the first matching option, if any.
pub fn segmented_pill_choice_a11y_query<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    t: &Tokens,
    current: &mut T,
    options: &[(T, &str, &str)],
    query: &str,
) -> (bool, Option<egui::Response>) {
    let mut changed = false;
    let mut first_match_resp: Option<egui::Response> = None;
    egui::Frame::new()
        .fill(t.field)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(2, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for &(value, label, a11y_label) in options {
                    let is_active = *current == value;
                    let font = theme::medium(12.0);
                    let text_color = if is_active { Color32::WHITE } else { t.text };
                    let label_galley = layout_pill_label(ui, label, query, font, text_color);
                    let w = label_galley.size().x;
                    let (rect, resp) = ui.allocate_exact_size(vec2(w + 18.0, 24.0), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, is_active, a11y_label));

                    if first_match_resp.is_none() && !find_match_ranges(label, query).is_empty() {
                        first_match_resp = Some(resp.clone());
                    }

                    if is_active {
                        ui.painter().rect(rect, CornerRadius::same(12), t.accent, Stroke::NONE, egui::StrokeKind::Inside);
                    } else {
                        let press = theme::Press::track(ui, &resp);
                        let fill = press.fill(Color32::TRANSPARENT, t.hover, t.pressed);
                        ui.painter().rect(rect, CornerRadius::same(12), fill, Stroke::NONE, egui::StrokeKind::Inside);
                    }
                    ui.painter().galley(rect.center() - label_galley.size() / 2.0, label_galley, text_color);

                    if resp.has_focus() {
                        ui.painter().rect_stroke(
                            rect.expand(1.0),
                            CornerRadius::same(13),
                            Stroke::new(2.0, t.accent_text),
                            egui::StrokeKind::Outside,
                        );
                    }

                    let resp = theme::hand(resp);
                    let key_select = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter));
                    if (resp.clicked() || key_select) && *current != value {
                        *current = value;
                        changed = true;
                    }
                }
            });
        });
    (changed, first_match_resp)
}

fn layout_pill_label(ui: &egui::Ui, label: &str, query: &str, font: egui::FontId, normal_color: Color32) -> std::sync::Arc<egui::Galley> {
    let ranges = find_match_ranges(label, query);
    if ranges.is_empty() {
        crate::bidi::layout_ui(ui, label.to_owned(), font, normal_color)
    } else {
        let (job, _) = create_highlight_job(label, &ranges, font, normal_color);
        ui.painter().layout_job(job)
    }
}

fn fold_search_char(c: char, mut f: impl FnMut(char)) {
    match c {
        'I' | 'i' | '\u{130}' | '\u{131}' => f('i'),
        'Σ' | 'σ' | 'ς' => f('σ'),
        _ => {
            for lc in c.to_lowercase() {
                f(lc);
            }
        }
    }
}

/// Finds non-overlapping (start_byte, end_byte) slices in `text` matching `query` case-insensitively.
/// All byte indices are guaranteed to align to valid char boundaries.
pub fn find_match_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let mut q_chars = Vec::new();
    for c in q.chars() {
        fold_search_char(c, |ch| q_chars.push(ch));
    }
    if q_chars.is_empty() {
        return Vec::new();
    }
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < char_indices.len() {
        let mut matched = true;
        let mut text_char_count = 0;
        let mut q_idx = 0;

        while q_idx < q_chars.len() {
            if i + text_char_count >= char_indices.len() {
                matched = false;
                break;
            }
            let c = char_indices.get(i + text_char_count).map(|(_, ch)| *ch).unwrap_or('\0');
            let mut c_folded = ['\0'; 4];
            let mut len = 0;
            fold_search_char(c, |lc| {
                if len < c_folded.len() {
                    c_folded[len] = lc;
                    len += 1;
                }
            });
            if len == 0 || q_idx + len > q_chars.len() || q_chars.get(q_idx..q_idx + len) != Some(&c_folded[..len]) {
                matched = false;
                break;
            }
            q_idx += len;
            text_char_count += 1;
        }

        if matched
            && q_idx == q_chars.len()
            && text_char_count > 0
            && let Some(start_byte) = char_indices.get(i).map(|(pos, _)| *pos)
        {
            let end_byte = if let Some((pos, _)) = char_indices.get(i + text_char_count) { *pos } else { text.len() };
            ranges.push((start_byte, end_byte));
            i += text_char_count;
            continue;
        }
        i += 1;
    }
    ranges
}

/// Builds an `egui::text::LayoutJob` with yellow background for matching spans.
pub fn create_highlight_job(
    text: &str,
    ranges: &[(usize, usize)],
    normal_font: egui::FontId,
    normal_color: Color32,
) -> (egui::text::LayoutJob, usize) {
    let mut job = egui::text::LayoutJob::default();
    let normal_format = egui::text::TextFormat { font_id: normal_font.clone(), color: normal_color, ..Default::default() };
    let highlight_format = egui::text::TextFormat {
        font_id: egui::FontId::new(normal_font.size, egui::FontFamily::Proportional),
        color: Color32::BLACK,
        background: Color32::from_rgb(255, 220, 60),
        ..Default::default()
    };

    let mut cursor = 0;
    for &(start, end) in ranges {
        if start > cursor
            && let Some(sub) = text.get(cursor..start)
        {
            job.append(sub, 0.0, normal_format.clone());
        }
        if let Some(matched) = text.get(start..end) {
            job.append(matched, 0.0, highlight_format.clone());
        }
        cursor = end;
    }
    if cursor < text.len()
        && let Some(sub) = text.get(cursor..)
    {
        job.append(sub, 0.0, normal_format);
    }
    let count = ranges.len();
    (job, count)
}

/// The date picker's displayed name.
pub(crate) fn date_language_picker_name(l: &pdfcraft_engine::dates::DateLanguage, drawable: impl FnOnce(&str) -> bool) -> &'static str {
    match crate::i18n::Lang::from_code(l.code) {
        Some(language) => language.picker_name(drawable),
        None => l.name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_match_ranges_case_insensitive() {
        let text = "Hide hover previews for highlights without comment text";
        let ranges = find_match_ranges(text, "hide hover");
        assert_eq!(ranges, vec![(0, 10)]);
        assert_eq!(text.get(ranges[0].0..ranges[0].1), Some("Hide hover"));

        let text2 = "Highlight fields and highlight text";
        let ranges2 = find_match_ranges(text2, "highlight");
        assert_eq!(ranges2.len(), 2);
        assert_eq!(text2.get(ranges2[0].0..ranges2[0].1), Some("Highlight"));
        assert_eq!(text2.get(ranges2[1].0..ranges2[1].1), Some("highlight"));

        let ranges3 = find_match_ranges(text, "not found");
        assert!(ranges3.is_empty());

        let ranges4 = find_match_ranges(text, "");
        assert!(ranges4.is_empty());
    }

    #[test]
    fn test_find_match_ranges_turkish_i() {
        let text = "Doldur ve İmzala";
        let ranges = find_match_ranges(text, "imzala");
        assert_eq!(ranges.len(), 1);
        assert_eq!(text.get(ranges[0].0..ranges[0].1), Some("İmzala"));

        let ranges2 = find_match_ranges(text, "İMZALA");
        assert_eq!(ranges2.len(), 1);
        assert_eq!(text.get(ranges2[0].0..ranges2[0].1), Some("İmzala"));

        let text2 = "Doldur ve imzala";
        let ranges3 = find_match_ranges(text2, "İmzala");
        assert_eq!(ranges3.len(), 1);
        assert_eq!(text2.get(ranges3[0].0..ranges3[0].1), Some("imzala"));

        let ranges4 = find_match_ranges(text, "ımzala");
        assert_eq!(ranges4.len(), 1);
        assert_eq!(text.get(ranges4[0].0..ranges4[0].1), Some("İmzala"));
    }
}
