//! Multi-object editing through the real shell. Synthetic artwork and standard-font text only.
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::{AddedText, Edit, EditableObject};
use pdfcraft_ui_egui::PdfCraftApp;

fn fixture(clipping: bool) -> Vec<u8> {
    fixture_with_background(clipping, false)
}

fn fixture_with_background(clipping: bool, background: bool) -> Vec<u8> {
    fixture_with_shape(clipping, background, false)
}

fn fixture_with_shape(clipping: bool, background: bool, shape: bool) -> Vec<u8> {
    let stream = |body: &str, dict: &str| format!("<< {dict} /Length {} >>\nstream\n{body}\nendstream", body.len());
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 500] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Figure 6 0 R >> >> >>"
            .into(),
        stream(
            &format!(
                "{}q BT {} Tr /F1 18 Tf 40 430 Td (Group headline) Tj ET Q q 1 0 0 1 40 340 cm /Figure Do Q BT /F1 12 Tf 240 90 Td (Keep here) Tj ET {}",
                if background { "q /Background Do Q " } else { "" },
                if clipping { 4 } else { 0 },
                if shape { "q 0 0.4 0.8 rg 250 340 60 30 re f Q" } else { "" }
            ),
            "",
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        stream("1 0.2 0.1 rg 0 0 60 30 re f", "/Type /XObject /Subtype /Form /BBox [0 0 60 30]"),
    ];
    if background {
        objects[2] = objects[2].replace("/Figure 6 0 R", "/Figure 6 0 R /Background 7 0 R");
        objects.push(stream("1 1 1 rg 0 0 400 500 re f", "/Type /XObject /Subtype /Form /BBox [0 0 400 500]"));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

fn harness() -> Harness<'static, PdfCraftApp> {
    harness_with(false)
}

fn harness_with(clipping: bool) -> Harness<'static, PdfCraftApp> {
    harness_document(clipping, false)
}

fn harness_document(clipping: bool, background: bool) -> Harness<'static, PdfCraftApp> {
    harness_bytes(fixture_with_background(clipping, background))
}

fn harness_bytes(bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.session = pdfcraft_engine::Session::new().with_clock(|| 1_700_000_000);
        app.set_option("language", "en").unwrap();
        app.open_bytes("group.pdf", None, bytes).unwrap();
        let id = app.views[0].id;
        app.session
            .apply(
                id,
                Edit::AddText { page: 0, text: AddedText { text: "Added note".into(), rect: [130.0, 340.0, 230.0, 370.0], ..Default::default() } },
            )
            .unwrap();
        let saved = app.session.save_bytes(id).unwrap();
        app.session.mark_saved(id, saved, None).unwrap();
        app.set_option("zoom", "100").unwrap();
        assert!(app.execute("edit.edit_text"));
        app
    });
    for _ in 0..60 {
        h.run_steps(2);
        if !h.state().render_pending() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h
}

fn at(h: &Harness<'static, PdfCraftApp>, x: f32, y: f32) -> Pos2 {
    let r = h.state().views[0].page_screen_rect(0).unwrap();
    r.min + egui::vec2(x, y) * (r.width() / 400.0)
}
fn objects(h: &Harness<'static, PdfCraftApp>) -> Vec<EditableObject> {
    let app = h.state();
    app.session.get(app.views[0].id).unwrap().editable_objects(0).unwrap()
}
fn click(h: &mut Harness<'static, PdfCraftApp>, p: Pos2, mods: Modifiers) {
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event_modifiers(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: mods }, mods);
    }
    h.run_steps(4);
}
fn marquee(h: &mut Harness<'static, PdfCraftApp>) {
    let (a, b) = (at(h, 25.0, 45.0), at(h, 235.0, 175.0));
    h.hover_at(a);
    h.run_steps(2);
    h.drag_at(a);
    h.run_steps(2);
    h.hover_at(b);
    h.run_steps(2);
    h.drop_at(b);
    h.run_steps(4);
    h.get_by_label("3 objects selected");
}

#[test]
fn alignment_shortcuts_format_the_open_original_paragraph_and_preserve_its_text() {
    use pdfcraft_engine::TextAlign;
    let mut h = harness();
    let p = at(&h, 55.0, 70.0);
    click(&mut h, p, Modifiers::NONE);
    assert!(h.state().views[0].line_editor.is_some());
    for (key, align) in [(Key::E, TextAlign::Center), (Key::R, TextAlign::Right), (Key::L, TextAlign::Left)] {
        h.key_press_modifiers(Modifiers::COMMAND, key);
        h.run_steps(4);
        let editor = h.state().views[0].line_editor.as_ref().unwrap();
        assert_eq!(editor.look.align, align);
        assert_eq!(editor.text, "Group headline");
        assert!(!h.state().full_screen);
    }
    h.key_press(Key::Escape);
    h.run_steps(4);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert!(objects(&h).iter().any(|o| o.text.as_deref() == Some("Group headline")));
}

#[test]
fn right_click_selected_objects_offers_alignment_and_distribution() {
    let mut h = harness();
    let before = objects(&h);
    marquee(&mut h);
    let p = at(&h, 70.0, 145.0);
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(4);
    h.get_by_label("Distribute ⏵");
    h.get_by_label("Align ⏵").hover();
    h.run_steps(5);
    h.get_by_label("Left").click();
    h.run_steps(5);
    let after = objects(&h);
    for old in before.iter().filter(|o| o.text.as_deref() != Some("Keep here")) {
        let new = after.iter().find(|o| o.text == old.text && o.target.kind == old.target.kind).unwrap();
        assert!((new.rect[0] - 40.0).abs() < 0.01, "{new:?}");
        assert!((new.rect[1] - old.rect[1]).abs() < 0.01);
        assert!(((new.rect[2] - new.rect[0]) - (old.rect[2] - old.rect[0])).abs() < 0.01);
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(objects(&h), before);
}

#[test]
fn square_group_tool_in_edit_mode_moves_original_artwork_and_additions_together() {
    use pdfcraft_engine::{NewAnnotation, NoteIcon, Shape, Style};
    let mut h = harness_bytes(fixture_with_shape(false, false, true));
    h.state_mut().apply_edit(Edit::AddAnnotation(NewAnnotation {
        page: 0,
        shape: Shape::Note { at: [190.0, 460.0], icon: NoteIcon::Note },
        style: Style::default(),
        contents: "Group note".into(),
        author: "Tester".into(),
    }));
    h.state_mut().set_option("mode", "edit").unwrap();
    h.run_steps(4);
    h.get_by_label("Group select").click();
    h.run_steps(4);
    let before = objects(&h);
    let note = h.state().session.get(h.state().views[0].id).unwrap().info.annotations.iter().find(|a| a.subtype == "Text").unwrap().rect;
    let (a, b) = (at(&h, 25.0, 25.0), at(&h, 330.0, 185.0));
    drag(&mut h, a, b);
    assert_eq!(h.state().views[0].group_selection_count(), 5, "original paragraph, picture, path, added text and note");
    for (from, to, total) in [((55.0, 70.0), (65.0, 80.0), 10.0), ((65.0, 80.0), (75.0, 90.0), 20.0)] {
        let (a, b) = (at(&h, from.0, from.1), at(&h, to.0, to.1));
        drag(&mut h, a, b);
        assert_eq!(h.state().views[0].group_selection_count(), 5, "selection follows repeated moves");
        for old in &before {
            let after = objects(&h);
            let new = after.iter().find(|o| o.target.kind == old.target.kind && o.text == old.text).unwrap();
            let shift = if old.text.as_deref() == Some("Keep here") { 0.0 } else { total };
            for i in 0..4 {
                assert!((new.rect[i] - old.rect[i] - if i % 2 == 0 { shift } else { -shift }).abs() < 0.02, "{old:?} -> {new:?}");
            }
        }
        let app = h.state();
        let doc = app.session.get(app.views[0].id).unwrap();
        let moved = doc.info.annotations.iter().find(|a| a.subtype == "Text").unwrap().rect;
        assert!((moved[0] - note[0] - total as f32).abs() < 0.02 && (moved[1] - note[1] + total as f32).abs() < 0.02);
        assert_eq!(doc.can_undo(), Some("Move objects"));
    }
    h.key_press(Key::Delete);
    h.run_steps(4);
    assert_eq!(objects(&h).len(), 1, "only the unselected paragraph remains");
    assert!(h.state().session.get(h.state().views[0].id).unwrap().info.annotations.is_empty());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(objects(&h).len(), 5);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    let restored = objects(&h).into_iter().find(|o| o.text.as_deref() == Some("Group headline")).unwrap();
    let original = before.iter().find(|o| o.text.as_deref() == Some("Group headline")).unwrap();
    assert!((restored.rect[0] - original.rect[0] - 10.0).abs() < 0.02, "one undo per move");
}

#[test]
fn square_group_arrow_keys_nudge_the_selection_and_shift_moves_ten_points() {
    use pdfcraft_engine::{NewAnnotation, NoteIcon, Shape, Style};
    for edit_mode in [false, true] {
        let mut h = harness_bytes(fixture_with_shape(false, false, true));
        h.state_mut().apply_edit(Edit::AddAnnotation(NewAnnotation {
            page: 0,
            shape: Shape::Note { at: [190.0, 460.0], icon: NoteIcon::Note },
            style: Style::default(),
            contents: "Nudge me".into(),
            author: "Tester".into(),
        }));
        h.get_by_label("All tools").click();
        h.run_steps(4);
        h.state_mut().set_option("mode", if edit_mode { "edit" } else { "all" }).unwrap();
        h.state_mut().set_option("left", "closed").unwrap();
        h.state_mut().set_option("zoom", "75").unwrap();
        h.run_steps(4);
        h.get_by_label("Group select").click();
        h.run_steps(4);
        let before = objects(&h);
        let id = h.state().views[0].id;
        let note = h.state().session.get(id).unwrap().info.annotations.iter().find(|a| a.subtype == "Text").unwrap().rect;
        let (a, b) = (at(&h, 25.0, 25.0), at(&h, 330.0, 185.0));
        drag(&mut h, a, b);
        let count = if edit_mode { 5 } else { 2 };
        assert_eq!(h.state().views[0].group_selection_count(), count);
        for (modifiers, key, shift) in [
            (Modifiers::NONE, Key::ArrowRight, [1.0, 0.0]),
            (Modifiers::SHIFT, Key::ArrowDown, [1.0, -10.0]),
            (Modifiers::NONE, Key::ArrowLeft, [0.0, -10.0]),
            (Modifiers::SHIFT, Key::ArrowUp, [0.0, 0.0]),
        ] {
            h.key_press_modifiers(modifiers, key);
            h.run_steps(4);
            assert_eq!(h.state().views[0].group_selection_count(), count);
            let after = objects(&h);
            for old in &before {
                let new = after.iter().find(|o| o.target.kind == old.target.kind && o.text == old.text).unwrap();
                let moving = old.text.as_deref() != Some("Keep here") && (edit_mode || old.target.kind == pdfcraft_engine::ObjectKind::Added);
                let offset = if moving { shift } else { [0.0, 0.0] };
                assert!(new.rect.iter().enumerate().all(|(i, value)| (value - old.rect[i] - offset[i % 2]).abs() < 0.02), "{old:?} -> {new:?}");
            }
            let moved = h.state().session.get(id).unwrap().info.annotations.iter().find(|a| a.subtype == "Text").unwrap().rect;
            assert!(moved.iter().enumerate().all(|(i, value)| (f64::from(value - note[i]) - shift[i % 2]).abs() < 0.02));
        }
        let generation = h.state().session.get(id).unwrap().edit_generation();
        assert!(h.state_mut().execute("app.preferences"));
        h.run_steps(4);
        h.key_press(Key::ArrowRight);
        h.run_steps(4);
        assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation, "a modal owns its arrow keys");
        h.key_press(Key::Escape);
        h.run_steps(4);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(4);
        let added = objects(&h).into_iter().find(|o| o.target.kind == pdfcraft_engine::ObjectKind::Added).unwrap();
        let original = before.iter().find(|o| o.target.kind == pdfcraft_engine::ObjectKind::Added).unwrap();
        assert!((added.rect[1] - original.rect[1] + 10.0).abs() < 0.02, "one undo per nudge");
    }
}

#[test]
fn square_group_tool_refuses_a_clipped_original_without_moving_the_added_content() {
    use pdfcraft_engine::{NewAnnotation, NoteIcon, Shape, Style};
    let mut h = harness_bytes(fixture_with_shape(true, false, true));
    h.state_mut().apply_edit(Edit::AddAnnotation(NewAnnotation {
        page: 0,
        shape: Shape::Note { at: [190.0, 460.0], icon: NoteIcon::Note },
        style: Style::default(),
        contents: "Group note".into(),
        author: "Tester".into(),
    }));
    h.state_mut().set_option("mode", "edit").unwrap();
    h.run_steps(4);
    h.get_by_label("Group select").click();
    h.run_steps(4);
    let (a, b) = (at(&h, 25.0, 25.0), at(&h, 330.0, 185.0));
    drag(&mut h, a, b);
    assert_eq!(h.state().views[0].group_selection_count(), 5);
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    let bytes = doc.bytes.clone();
    let generation = doc.edit_generation();
    let (a, b) = (at(&h, 55.0, 70.0), at(&h, 65.0, 80.0));
    drag(&mut h, a, b);
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    assert_eq!(doc.edit_generation(), generation);
    assert_eq!(doc.bytes, bytes, "an unsupported original refuses the complete mixed move");
    assert_eq!(h.state().views[0].group_selection_count(), 5);
    h.key_press(Key::ArrowRight);
    h.run_steps(4);
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    assert_eq!(doc.edit_generation(), generation);
    assert_eq!(doc.bytes, bytes, "an unsupported original refuses the complete mixed nudge");
}
fn drag(h: &mut Harness<'static, PdfCraftApp>, a: Pos2, b: Pos2) {
    h.hover_at(a);
    h.run_steps(2);
    h.drag_at(a);
    h.run_steps(2);
    h.hover_at(b);
    h.run_steps(2);
    h.drop_at(b);
    h.run_steps(4);
}

#[test]
fn image_drag_snaps_to_added_text_and_alt_bypasses_it() {
    for (mods, expected) in [(Modifiers::NONE, 130.0), (Modifiers::ALT, 127.0)] {
        let mut h = harness();
        let (a, b) = (at(&h, 60.0, 145.0), at(&h, 147.0, 145.0));
        click(&mut h, a, Modifiers::NONE);
        h.event_modifiers(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: mods }, mods);
        h.run_steps(2);
        for step in 1..=20 {
            h.event_modifiers(egui::Event::PointerMoved(a + (b - a) * (step as f32 / 20.0)), mods);
            h.run_steps(2);
        }
        h.event_modifiers(egui::Event::PointerButton { pos: b, button: egui::PointerButton::Primary, pressed: false, modifiers: mods }, mods);
        h.run_steps(4);
        let image = objects(&h).into_iter().find(|o| o.target.kind == pdfcraft_engine::ObjectKind::Image).unwrap();
        assert!((image.rect[0] - expected).abs() < 0.02, "{mods:?}: {image:?}");
    }
}

#[test]
fn marquee_and_mixed_group_drag_move_once_and_undo_together() {
    let mut h = harness();
    let before = objects(&h);
    marquee(&mut h);
    let (a, b) = (at(&h, 55.0, 70.0), at(&h, 80.0, 100.0));
    drag(&mut h, a, b);
    let after = objects(&h);
    for old in &before {
        let new = after.iter().find(|o| o.target == old.target).unwrap();
        let shift = if old.text.as_deref() == Some("Keep here") { [0.0, 0.0] } else { [25.0, -30.0] };
        for i in 0..4 {
            assert!((new.rect[i] - old.rect[i] - shift[i % 2]).abs() < 0.01, "{old:?} -> {new:?}");
        }
    }
    h.get_by_label("3 objects selected");
    let app = h.state();
    assert_eq!(app.session.get(app.views[0].id).unwrap().can_undo(), Some("Move objects"));
    if let Ok(path) = std::env::var("PDFCRAFT_GROUP_SHOT") {
        let _gpu = crate::gpu();
        h.render().unwrap().save(path).unwrap();
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(objects(&h), before);
    assert!(
        h.query_all_by_label_contains("object selected").next().is_none() && h.query_all_by_label_contains("objects selected").next().is_none(),
        "no objects selected"
    );
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(4);
    assert_eq!(objects(&h), after);
}

#[test]
fn modifier_clicks_extend_an_ordinary_selection_and_toggle_objects() {
    let mut h = harness();
    let p = at(&h, 55.0, 70.0);
    click(&mut h, p, Modifiers::NONE);
    assert!(h.state().views[0].line_editor.is_some());
    let p = at(&h, 60.0, 145.0);
    click(&mut h, p, Modifiers::SHIFT);
    h.get_by_label("2 objects selected");
    assert!(h.state().views[0].line_editor.is_none());
    let p = at(&h, 155.0, 140.0);
    click(&mut h, p, Modifiers::COMMAND);
    h.get_by_label("3 objects selected");
    let p = at(&h, 60.0, 145.0);
    click(&mut h, p, Modifiers::CTRL);
    h.get_by_label("2 objects selected");
    assert!(!h.state().session.get(h.state().views[0].id).unwrap().dirty);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(
        h.query_all_by_label_contains("object selected").next().is_none() && h.query_all_by_label_contains("objects selected").next().is_none(),
        "no objects selected"
    );
}

#[test]
fn escaping_a_group_drag_and_switching_tools_do_not_edit() {
    let mut h = harness();
    let before = objects(&h);
    marquee(&mut h);
    let (a, b) = (at(&h, 55.0, 70.0), at(&h, 90.0, 110.0));
    h.hover_at(a);
    h.run_steps(2);
    h.drag_at(a);
    h.run_steps(2);
    h.hover_at(b);
    h.run_steps(2);
    assert_eq!(objects(&h), before, "only a preview while held");
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.drop_at(b);
    h.run_steps(4);
    assert_eq!(objects(&h), before);
    marquee(&mut h);
    assert!(h.state_mut().execute("edit.text"));
    h.run_steps(4);
    assert_eq!(objects(&h), before);
}

#[test]
fn a_changed_text_editor_is_kept_when_group_selection_is_requested() {
    let mut h = harness();
    let before = objects(&h);
    let p = at(&h, 55.0, 70.0);
    click(&mut h, p, Modifiers::NONE);
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "A draft still being edited".into();
    let p = at(&h, 60.0, 145.0);
    click(&mut h, p, Modifiers::SHIFT);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, "A draft still being edited");
    assert_eq!(objects(&h), before);
}

#[test]
fn a_document_change_invalidates_old_selection_before_the_next_drag() {
    let mut h = harness();
    marquee(&mut h);
    let id = h.state().views[0].id;
    h.state_mut()
        .session
        .apply(id, Edit::AddText { page: 0, text: AddedText { text: "A new item".into(), rect: [40.0, 210.0, 150.0, 240.0], ..Default::default() } })
        .unwrap();
    h.run_steps(4);
    assert!(
        h.query_all_by_label_contains("object selected").next().is_none() && h.query_all_by_label_contains("objects selected").next().is_none(),
        "no objects selected"
    );
    assert_eq!(objects(&h).len(), 5);
}

#[test]
fn refusing_one_object_keeps_the_whole_group_and_its_selection() {
    let mut h = harness_with(true);
    let before = objects(&h);
    marquee(&mut h);
    let (a, b) = (at(&h, 55.0, 70.0), at(&h, 80.0, 100.0));
    drag(&mut h, a, b);
    assert_eq!(objects(&h), before);
    h.get_by_label("3 objects selected");
    assert!(h.state().toast.as_ref().unwrap().0.contains("Text used as a clipping path cannot be moved with this selection."));
    assert!(!h.state().session.get(h.state().views[0].id).unwrap().dirty);
}

#[test]
fn a_group_drag_released_beyond_the_page_finishes_once() {
    let mut h = harness();
    let before = objects(&h);
    marquee(&mut h);
    let (a, b) = (at(&h, 55.0, 70.0), at(&h, 415.0, 100.0));
    drag(&mut h, a, b);
    let after = objects(&h);
    for (old, new) in before.iter().zip(&after) {
        let delta = if old.text.as_deref() == Some("Keep here") { 0.0 } else { 360.0 };
        assert!((new.rect[0] - old.rect[0] - delta).abs() < 0.01);
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(objects(&h), before);
}

#[test]
fn group_selection_can_be_extended_by_an_additive_marquee() {
    let mut h = harness();
    let p = at(&h, 55.0, 70.0);
    click(&mut h, p, Modifiers::SHIFT);
    h.get_by_label("1 object selected");
    let (a, b) = (at(&h, 25.0, 120.0), at(&h, 235.0, 175.0));
    h.hover_at(a);
    h.run_steps(2);
    h.event_modifiers(
        egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::SHIFT },
        Modifiers::SHIFT,
    );
    h.run_steps(2);
    h.hover_at(b);
    h.run_steps(2);
    h.drop_at(b);
    h.run_steps(4);
    h.get_by_label("3 objects selected");
    assert!(!h.state().session.get(h.state().views[0].id).unwrap().dirty);
}

#[test]
fn group_drag_respects_document_rotation_and_view_rotation() {
    for document_rotation in [0, 90, 180, 270] {
        let mut h = harness();
        assert!(h.state_mut().apply_edit(Edit::RotatePages { pages: vec![0], degrees: document_rotation }));
        h.state_mut().views[0].rotate_view(true);
        h.run_steps(4);
        let before = objects(&h);
        let targets: Vec<_> = before.iter().filter(|o| o.text.as_deref() != Some("Keep here")).map(|o| o.target).collect();
        for target in &targets {
            let app = h.state();
            let doc = app.session.get(app.views[0].id).unwrap();
            let o = before.iter().find(|o| o.target == *target).unwrap();
            let p = pdfcraft_ui_egui::canvas::PageXform {
                rect: app.views[0].page_screen_rect(0).unwrap(),
                rot: 90,
                pw: doc.info.pages[0].width,
                ph: doc.info.pages[0].height,
            }
            .user_rect(&doc.info, 0, o.rect.map(|v| v as f32))
            .center();
            click(&mut h, p, Modifiers::SHIFT);
        }
        h.get_by_label("3 objects selected");
        let app = h.state();
        let doc = app.session.get(app.views[0].id).unwrap();
        let xf = pdfcraft_ui_egui::canvas::PageXform {
            rect: app.views[0].page_screen_rect(0).unwrap(),
            rot: 90,
            pw: doc.info.pages[0].width,
            ph: doc.info.pages[0].height,
        };
        let start =
            xf.user_rect(&doc.info, 0, before.iter().find(|o| o.text.as_deref() == Some("Group headline")).unwrap().rect.map(|v| v as f32)).center();
        let end = start + egui::vec2(21.0, 33.0);
        let from = xf.screen_to_view(start);
        let to = xf.screen_to_view(end);
        let delta = doc.object_move_offset(0, [f64::from(to.0 - from.0), f64::from(to.1 - from.1)]).unwrap();
        drag(&mut h, start, end);
        for (old, new) in before.iter().zip(objects(&h)) {
            let expected = if targets.contains(&old.target) { delta } else { [0.0, 0.0] };
            for i in 0..4 {
                assert!((new.rect[i] - old.rect[i] - expected[i % 2]).abs() < 0.01, "rotation {document_rotation}");
            }
        }
    }
}

#[test]
fn group_drags_cancel_on_focus_loss_tab_switch_home_and_modal_dialogs() {
    for case in 0..5 {
        let mut h = harness();
        let before = objects(&h);
        marquee(&mut h);
        let (a, b) = (at(&h, 55.0, 70.0), at(&h, 90.0, 110.0));
        h.hover_at(a);
        h.run_steps(2);
        h.drag_at(a);
        h.run_steps(2);
        h.hover_at(b);
        h.run_steps(2);
        match case {
            0 => h.input_mut().focused = false,
            1 => {
                h.state_mut().open_bytes("other.pdf", None, fixture(false)).unwrap();
            }
            2 => h.state_mut().active = None,
            3 => h.state_mut().dialog = Some(pdfcraft_ui_egui::Dialog::About),
            _ => h.state_mut().request_document_url("https://example.invalid/", pdfcraft_ui_egui::LinkOrigin::Link),
        }
        h.run_steps(2);
        h.drop_at(b);
        h.run_steps(2);
        h.input_mut().focused = true;
        h.state_mut().active = Some(0);
        h.state_mut().dialog = None;
        h.state_mut().pending_link = None;
        h.run_steps(4);
        assert_eq!(objects(&h), before, "cancelled gesture case {case}");
        assert!(!h.state().session.get(h.state().views[0].id).unwrap().dirty, "case {case}");
        h.get_by_label("3 objects selected");
    }
}

#[test]
fn modifier_selection_picks_foreground_text_and_added_items_over_background_artwork() {
    let mut h = harness_document(false, true);
    let before = objects(&h);
    assert_eq!(before.len(), 5);
    for (x, y) in [(55.0, 70.0), (150.0, 135.0), (60.0, 145.0)] {
        let p = at(&h, x, y);
        click(&mut h, p, Modifiers::SHIFT);
    }
    h.get_by_label("3 objects selected");
    let (a, b) = (at(&h, 55.0, 70.0), at(&h, 80.0, 100.0));
    drag(&mut h, a, b);
    let after = objects(&h);
    for old in before {
        let new = after.iter().find(|o| o.target == old.target).unwrap();
        let stationary = old.text.as_deref() == Some("Keep here") || (old.target.kind == pdfcraft_engine::ObjectKind::Image && old.target.index == 0);
        let shift = if stationary { [0.0, 0.0] } else { [25.0, -30.0] };
        assert!(new.rect.iter().enumerate().all(|(i, value)| (value - old.rect[i] - shift[i % 2]).abs() < 0.01), "{old:?} -> {new:?}");
    }
}

#[test]
fn delete_removes_a_mixed_selection_as_one_undo_step_and_survives_reopen() {
    for key in [Key::Delete, Key::Backspace] {
        let mut h = harness_bytes(fixture_with_shape(false, false, true));
        let before = objects(&h);
        assert_eq!(before.len(), 5);
        let (a, b) = (at(&h, 25.0, 45.0), at(&h, 325.0, 175.0));
        drag(&mut h, a, b);
        h.get_by_label("4 objects selected");
        if let Ok(path) = std::env::var("PDFCRAFT_DELETE_SHOT") {
            let _gpu = crate::gpu();
            h.render().unwrap().save(path).unwrap();
        }
        h.hover_at(at(&h, 370.0, 250.0));
        h.key_press(key);
        h.run_steps(4);
        let remaining = objects(&h);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].text.as_deref(), Some("Keep here"));
        let id = h.state().views[0].id;
        assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Delete content"));
        assert_eq!(h.state().session.get(id).unwrap().info.pages.len(), 1);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(4);
        assert_eq!(objects(&h), before);
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
        h.run_steps(4);
        assert_eq!(objects(&h), remaining);
        let bytes = h.state_mut().session.save_full_bytes(id).unwrap();
        let mut session = pdfcraft_engine::Session::new();
        let reopened = session.open("deleted.pdf", None, bytes, None).unwrap();
        assert_eq!(session.get(reopened).unwrap().editable_objects(0).unwrap(), remaining);
    }
}

#[test]
fn clicking_a_shape_then_delete_removes_only_that_shape() {
    let mut h = harness_bytes(fixture_with_shape(false, false, true));
    let before = objects(&h);
    let p = at(&h, 280.0, 145.0);
    click(&mut h, p, Modifiers::NONE);
    h.get_by_label("1 object selected");
    h.hover_at(at(&h, 370.0, 250.0));
    h.key_press(Key::Delete);
    h.run_steps(4);
    let after = objects(&h);
    assert_eq!(after.len(), before.len() - 1);
    assert!(after.iter().all(|o| o.target.kind != pdfcraft_engine::ObjectKind::Path));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(objects(&h), before);
}

#[test]
fn a_selected_image_deletes_even_after_the_pointer_leaves_it() {
    let mut h = harness();
    let before = objects(&h);
    let p = at(&h, 60.0, 145.0);
    click(&mut h, p, Modifiers::NONE);
    assert!(h.state().views[0].image_selection.is_some());
    h.hover_at(at(&h, 370.0, 250.0));
    h.run_steps(2);
    h.key_press(Key::Delete);
    h.run_steps(4);
    let after = objects(&h);
    assert_eq!(after.len(), before.len() - 1);
    assert!(after.iter().all(|o| o.target.kind != pdfcraft_engine::ObjectKind::Image));
    assert!(h.state().views[0].image_selection.is_none(), "a deleted image cannot leave a stale selected index");
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(objects(&h), after, "a second Delete cannot remove an unselected object");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(objects(&h), before);
}

#[test]
fn delete_inside_the_paragraph_editor_does_not_delete_the_box() {
    let mut h = harness();
    let before = objects(&h);
    let p = at(&h, 55.0, 70.0);
    click(&mut h, p, Modifiers::NONE);
    assert!(h.state().views[0].line_editor.is_some());
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert!(h.state().views[0].line_editor.is_some());
    assert_eq!(objects(&h), before, "the text draft has not been applied as an object deletion");
}
