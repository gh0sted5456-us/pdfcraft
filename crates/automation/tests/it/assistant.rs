use pdfcraft_automation::{Automation, Content, ToolError};
use serde_json::{Value, json};
fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args).unwrap().remove(0) {
        Content::Json(v) => v,
        other => panic!("Unexpected {other:?}"),
    }
}
#[test]
fn assistant_prepares_real_text_without_network_or_edits_and_refuses_unapproved_network() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-assistant-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let session = pdfcraft_engine::Session::new();
    let bytes = session.create_from_text("Synthetic", "The meeting starts at noon. Bring the report.").unwrap();
    std::fs::write(dir.join("synthetic.pdf"), bytes.as_slice()).unwrap();
    let mut a = Automation::new().with_root(&dir).unwrap();
    let doc = ok(&mut a, "doc_open", json!({"path":"synthetic.pdf"}))["doc"].as_u64().unwrap();
    let reviewed = ok(&mut a, "ai_prepare", json!({"doc":doc}));
    assert_eq!(reviewed["sent"], false);
    assert!(reviewed["text"].as_str().unwrap().contains("[Page 1]"));
    assert!(reviewed["text"].as_str().unwrap().contains("meeting starts at noon"));
    for tool in ["ai_assist", "ai_models", "ai_login_start", "ai_claude_login", "ai_copilot_login_start"] {
        let args = match tool {
            "ai_assist" => json!({"doc":doc,"provider":"chatgpt","model":"eligible"}),
            "ai_models" => json!({"provider":"chatgpt"}),
            "ai_copilot_login_start" => {
                json!({"client_id":"11111111-1111-1111-1111-111111111111","tenant_id":"22222222-2222-2222-2222-222222222222"})
            }
            _ => json!({}),
        };
        assert!(matches!(a.call(tool,&args),Err(ToolError::Failed(m)) if m.contains("--network")), "{tool}");
    }
    assert!(a.call("ai_key_connect", &json!({"provider":"openai-api","key_env":"ANTHROPIC_API_KEY"})).is_err());
    assert!(a.call("ai_prepare", &json!({"doc":doc,"pages":[999]})).is_err());
    assert_eq!(ok(&mut a, "doc_info", json!({"doc":doc}))["document"]["dirty"], false);
    assert_eq!(std::fs::read(dir.join("synthetic.pdf")).unwrap().as_slice(), bytes.as_slice());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no key or transcript file is written beside the PDF");
}

#[test]
fn confidential_pages_block_reviewed_text_bypasses_and_persist_after_save() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-ai-privacy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let session = pdfcraft_engine::Session::new();
    let bytes = session.create_from_text("Synthetic", "These words stay private.").unwrap();
    std::fs::write(dir.join("source.pdf"), bytes.as_slice()).unwrap();
    let mut a = Automation::new().with_root(&dir).unwrap().with_network(true);
    let doc = ok(&mut a, "doc_open", json!({"path":"source.pdf"}))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_set_confidential", json!({"doc":doc,"pages":[1],"confidential":true}));
    for (tool, args) in [
        ("ai_prepare", json!({"doc":doc})),
        ("ai_browser_prepare", json!({"doc":doc,"provider":"copilot-personal","reviewed_text":"Old private text"})),
        ("ai_assist", json!({"doc":doc,"provider":"copilot-personal","reviewed_text":"Previously reviewed private text"})),
    ] {
        let error = a.call(tool, &args).unwrap_err().to_string();
        assert!(error.to_lowercase().contains("confidential"), "{tool}: {error}");
    }
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"saved.pdf"}));
    let saved = ok(&mut a, "doc_open", json!({"path":"saved.pdf"}))["doc"].as_u64().unwrap();
    assert!(a.call("ai_prepare", &json!({"doc":saved})).is_err());
    ok(&mut a, "doc_set_confidential", json!({"doc":saved,"pages":[1],"confidential":false}));
    assert!(ok(&mut a, "ai_prepare", json!({"doc":saved}))["text"].as_str().unwrap().contains("stay private"));
}
