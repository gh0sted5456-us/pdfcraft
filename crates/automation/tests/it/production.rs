use pdfcraft_automation::{Automation, Content};
use serde_json::{Value, json};
fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args).unwrap().remove(0) {
        Content::Json(v) => v,
        other => panic!("Unexpected {other:?}"),
    }
}

#[test]
fn persistent_document_actions_survive_save_reopen_and_remain_undoable() {
    let root = std::env::temp_dir().join(format!("pdfcraft-js-lifecycle-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut a = Automation::new().with_root(&root).unwrap();
    let id = ok(&mut a, "doc_create", json!({"from":"blank"}))["doc"].as_u64().unwrap();
    let run = |a: &mut Automation, source: &str| ok(a, "js_run", json!({"doc":id,"script":source}));
    assert_eq!(run(&mut a, "var n = 2; n")["result"], "2");
    assert_eq!(run(&mut a, "++n")["result"], "3");
    let field = ok(&mut a, "form_add_field", json!({"doc":id,"page":1,"type":"text","rect":[20,20,150,50]}))["field"].as_str().unwrap().to_string();
    for (event, script) in [
        ("Open", "var opened = 7;".to_owned()),
        ("WillSave", format!("getField('{}').value = 'Saved by action';", field)),
        ("WillPrint", "console.println('printing');".to_owned()),
    ] {
        ok(&mut a, "js_set_document_action", json!({"doc":id,"event":event,"script":script}));
    }
    assert!(a.call("js_set_document_action", &json!({"doc":id,"event":"Unknown","script":"1"})).is_err());
    ok(&mut a, "doc_save", json!({"doc":id,"path":"actions.pdf"}));
    let opened = ok(&mut a, "doc_open", json!({"path":"actions.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "js_run", json!({"doc":opened,"script":"opened"}))["result"], "7");
    assert!(ok(&mut a, "form_fields", json!({"doc":opened})).to_string().contains("Saved by action"));
    let actions = ok(&mut a, "js_document_actions", json!({"doc":opened}));
    assert!(actions.to_string().contains("printing"));
    ok(&mut a, "js_set_document_action", json!({"doc":opened,"event":"WillPrint"}));
    ok(&mut a, "edit_undo", json!({"doc":opened}));
    assert_eq!(ok(&mut a, "js_document_actions", json!({"doc":opened})), actions);
}

#[test]
fn production_reports_are_read_only_and_validate_page_ranges() {
    let root = std::env::temp_dir().join(format!("pdfcraft-prepress-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut a = Automation::new().with_root(&root).unwrap();
    let id = ok(&mut a, "doc_create", json!({"from":"starter","template":"business-card"}))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "doc_info", json!({"doc":id}));
    let output = ok(&mut a, "output_inspection", json!({"doc":id,"page":1}));
    assert_eq!(output["complete"], true);
    assert!(output["spaces"].as_array().unwrap().iter().any(|v| v == "DeviceRGB"));
    assert!(output["report"].as_str().unwrap().contains("not simulated"));
    let report = ok(&mut a, "print_readiness", json!({"doc":id}));
    assert_eq!(report["pages"].as_array().unwrap().len(), 1);
    assert!(a.call("output_inspection", &json!({"doc":id,"page":0})).is_err());
    assert!(a.call("output_inspection", &json!({"doc":id,"page":99})).is_err());
    assert_eq!(ok(&mut a, "doc_info", json!({"doc":id})), before);
}

#[test]
fn dynamic_choices_border_and_widget_scripts_survive_roundtrip() {
    let root = std::env::temp_dir().join(format!("pdfcraft-dynamic-js-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut a = Automation::new().with_root(&root).unwrap();
    let id = ok(&mut a, "doc_create", json!({"from":"blank"}))["doc"].as_u64().unwrap();
    let name = ok(&mut a, "form_add_field", json!({"doc":id,"page":1,"type":"combo","rect":[20,20,180,55],"options":["old"]}))["field"]
        .as_str()
        .unwrap()
        .to_owned();
    let script = format!(
        r#"var choice = getField('{name}'); choice.setItems([['Small','S'], ['Large','L']]); choice.value = 'L'; choice.strokeColor = color.red; choice.setAction('OnFocus', "console.println('focused');");"#
    );
    assert!(ok(&mut a, "js_run", json!({"doc":id,"script":script}))["error"].is_null());
    assert_eq!(ok(&mut a, "js_field_event", json!({"doc":id,"field":name,"trigger":"on_focus"}))["console"], json!(["focused"]));
    let mouse = format!(r#"getField('{name}').setAction('MouseUp', "console.println('clicked');");"#);
    assert!(ok(&mut a, "js_run", json!({"doc":id,"script":mouse}))["error"].is_null());
    assert_eq!(ok(&mut a, "js_field_event", json!({"doc":id,"field":name,"trigger":"mouse_up"}))["console"], json!(["clicked"]));
    ok(&mut a, "doc_save", json!({"doc":id,"path":"choices.pdf"}));
    let opened = ok(&mut a, "doc_open", json!({"path":"choices.pdf"}))["doc"].as_u64().unwrap();
    let fields = ok(&mut a, "form_fields", json!({"doc":opened})).to_string();
    assert!(fields.contains("Small") && fields.contains("Large"), "{fields}");
    assert_eq!(ok(&mut a, "js_field_event", json!({"doc":opened,"field":name,"trigger":"on_focus"}))["console"], json!(["focused"]));
    assert_eq!(ok(&mut a, "js_field_event", json!({"doc":opened,"field":name,"trigger":"mouse_up"}))["console"], json!(["clicked"]));
    let check = format!("getField('{}').strokeColor.join(',')", name);
    assert_eq!(ok(&mut a, "js_run", json!({"doc":opened,"script":check}))["result"], "RGB,1,0,0");
}
