use pdfcraft_automation::{Automation, Content};
use serde_json::{Value, json};

fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args).unwrap().remove(0) {
        Content::Json(v) => v,
        other => panic!("Unexpected {other:?}"),
    }
}

#[test]
fn reusable_templates_keep_text_fields_and_enforce_file_roots() {
    let root = std::env::temp_dir().join(format!("pdfcraft-templates-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut a = Automation::new().with_root(&root).unwrap();
    let list = ok(&mut a, "template_list", json!({}));
    assert!(list["templates"].as_array().unwrap().len() >= 30);
    let id = ok(&mut a, "doc_create", json!({"from":"starter","template":"intake-form","paper":"a4"}))["doc"].as_u64().unwrap();
    ok(&mut a, "form_fill", json!({"doc":id,"values":{"Full name":"Casey Example"}}));
    let fields = ok(&mut a, "form_fields", json!({"doc":id}));
    assert!(fields.to_string().contains("Full name"));
    ok(&mut a, "template_export", json!({"doc":id,"title":"My intake","path":"intake.pdfcraft-template"}));
    let original = std::fs::read(root.join("intake.pdfcraft-template")).unwrap();
    ok(&mut a, "doc_save", json!({"doc":id,"path":"source.pdf"}));
    let saved_source = std::fs::read(root.join("source.pdf")).unwrap();
    assert!(a.call("template_export", &json!({"doc":id,"path":"source.pdf"})).is_err());
    assert_eq!(std::fs::read(root.join("source.pdf")).unwrap(), saved_source);

    let opened = ok(&mut a, "doc_open", json!({"path":"intake.pdfcraft-template"}))["doc"].as_u64().unwrap();
    assert!(a.call("doc_save", &json!({"doc":opened})).is_err(), "opening a template must not adopt its path");

    let next = ok(&mut a, "doc_create", json!({"from":"template","path":"intake.pdfcraft-template"}))["doc"].as_u64().unwrap();
    let found = ok(&mut a, "text_find", json!({"doc":next,"query":"Client intake"}));
    assert!(found["count"].as_u64().unwrap() > 0);
    let fields = ok(&mut a, "form_fields", json!({"doc":next})).to_string();
    assert!(fields.contains("Full name") && fields.contains("Casey Example"));
    ok(&mut a, "doc_save", json!({"doc":next,"path":"working-copy.pdf"}));
    assert_eq!(std::fs::read(root.join("intake.pdfcraft-template")).unwrap(), original);
    assert!(a.call("template_export", &json!({"doc":id,"path":"../escape.pdfcraft-template"})).is_err());
    assert!(a.call("doc_create", &json!({"from":"template","path":"../escape.pdfcraft-template"})).is_err());
    assert!(a.call("doc_create", &json!({"from":"starter","template":"unknown"})).is_err());
    assert!(!root.parent().unwrap().join("escape.pdfcraft-template").exists());
    let outside = root.with_file_name(format!("pdfcraft-template-outside-{}.pdfcraft-template", std::process::id()));
    std::fs::write(&outside, &original).unwrap();
    assert!(a.call("doc_create", &json!({"from":"template","path":outside.to_string_lossy()})).is_err());
    assert!(a.call("doc_open", &json!({"path":outside.to_string_lossy()})).is_err());
    assert_eq!(std::fs::read(&outside).unwrap(), original);
    std::fs::remove_file(outside).unwrap();
}
