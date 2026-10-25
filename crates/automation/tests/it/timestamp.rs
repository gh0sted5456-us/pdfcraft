use std::{
    io::{Read, Write},
    sync::Arc,
};

use pdfcraft_automation::{Automation, Content};
use pdfcraft_engine::sign::{self, Certificate, DigitalId, Name, PrivateKey, Time, der};
use serde_json::{Value, json};

fn call(app: &mut Automation, tool: &str, args: Value) -> Value {
    let result = app.call(tool, &args).unwrap();
    let Content::Json(value) = result.into_iter().next().unwrap() else { panic!("expected JSON") };
    value
}

fn tsa() -> DigitalId {
    let key = PrivateKey::generate_p256().unwrap();
    let name = Name::build("Synthetic timestamp authority", "", "PdfCraft Tests", "", "US");
    let alg = key.preferred_digest();
    let sig_alg = key.signature_algorithm(alg);
    let from = Time { year: 2026, month: 10, day: 2, hour: 0, minute: 0, second: 0 };
    let until = Time { year: 2036, ..from };
    let eku = der::seq(&[&der::oid("2.5.29.37"), &der::boolean(true), &der::octets(&der::seq(&[&der::oid("1.3.6.1.5.5.7.3.8")]))]);
    let tbs = der::seq(&[
        &der::explicit(0, &der::int(2)),
        &der::uint(&[49]),
        &sig_alg,
        &name.raw,
        &der::seq(&[&from.encode(), &until.encode()]),
        &name.raw,
        &key.public_key().spki(),
        &der::explicit(3, &der::seq(&[&eku])),
    ]);
    let signature = key.sign(alg, &tbs).unwrap();
    let certificate = Certificate::parse(&der::seq(&[&tbs, &sig_alg, &der::bit_string(&signature)])).unwrap();
    DigitalId { key, certificate, chain: Vec::new(), friendly_name: None }
}

fn server(id: DigitalId) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/tsa", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        // Two valid requests followed by one malformed response.
        for index in 0..3 {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(10))
                    }
                    Err(error) => panic!("timestamp test server: {error}"),
                }
            };
            // Accepted sockets can inherit the listener's nonblocking mode on Windows.
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut bytes = Vec::new();
            let (start, length) = loop {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0 && bytes.len() < 65536);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                    assert!(headers.contains("content-type: application/timestamp-query"));
                    let length: usize = headers.lines().find_map(|l| l.strip_prefix("content-length:")).unwrap().trim().parse().unwrap();
                    break (end + 4, length);
                }
            };
            while bytes.len() < start + length {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0 && bytes.len() < 65536);
                bytes.extend_from_slice(&chunk[..count]);
            }
            let query = sign::timestamp::parse_request(&bytes[start..start + length]).unwrap();
            let body = if index == 2 {
                b"malformed timestamp response".to_vec()
            } else {
                sign::timestamp::respond(
                    &id.key,
                    &id.certificate,
                    &[],
                    sign::DigestAlg::Sha256,
                    &query,
                    "1.2.3.4",
                    Time { year: 2026, month: 10, day: 24, hour: 12, minute: 0, second: 0 },
                    7,
                )
                .unwrap()
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/timestamp-reply\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (url, worker)
}

#[test]
fn trusted_timestamps_work_through_tools_and_fail_without_replacing_files() {
    let dir = tempfile::tempdir().unwrap();
    let tsa = tsa();
    std::fs::write(dir.path().join("tsa.cer"), &tsa.certificate.raw).unwrap();
    let (url, worker) = server(tsa);
    let mut app = Automation::new().with_root(dir.path()).unwrap();
    let doc = call(&mut app, "doc_create", json!({"from":"blank"}))["doc"].clone();
    let args = json!({"doc":doc, "out":"timestamped.pdf", "timestamp_server":url});
    assert!(app.call("doc_timestamp", &args).unwrap_err().to_string().contains("--network"));
    assert!(!dir.path().join("timestamped.pdf").exists());
    app = app.with_network(true);
    assert!(app.call("doc_timestamp", &json!({"doc":doc,"out":"../escape.pdf","timestamp_server":url})).is_err());
    call(&mut app, "sign_trust", json!({"paths":["tsa.cer"]}));
    call(&mut app, "doc_timestamp", args.clone());
    let list = call(&mut app, "sign_list", json!({"doc":doc}));
    assert_eq!(list["count"], 1);
    assert_eq!(list["signatures"][0]["status"], "valid", "{list}");
    call(&mut app, "sign_id_create", json!({"name":"Synthetic signer","key":"p256","password":"test-password","path":"signer.p12"}));
    assert!(
        app.call(
            "sign_document",
            &json!({"doc":doc,"id":"signer.p12","password":"test-password",
        "rect":[20,20,180,60],"background":{"image":"../outside.png"},"out":"refused.pdf"})
        )
        .is_err()
    );
    assert!(!dir.path().join("refused.pdf").exists());
    call(
        &mut app,
        "sign_document",
        json!({"doc":doc,"id":"signer.p12","password":"test-password","timestamp_server":url,"rect":[20,20,180,60],"background":"pdfcraft","out":"signed.pdf"}),
    );
    let list = call(&mut app, "sign_list", json!({"doc":doc}));
    assert!(list["signatures"].as_array().unwrap().iter().any(|s| s["timestamp"] == true));
    let before = std::fs::read(dir.path().join("timestamped.pdf")).unwrap();
    let generation = app.session().get(pdfcraft_engine::DocId(doc.as_u64().unwrap())).unwrap().edit_generation();
    assert!(app.call("doc_timestamp", &args).is_err());
    assert_eq!(std::fs::read(dir.path().join("timestamped.pdf")).unwrap(), before);
    assert_eq!(app.session().get(pdfcraft_engine::DocId(doc.as_u64().unwrap())).unwrap().edit_generation(), generation);
    let bytes = Arc::new(std::fs::read(dir.path().join("signed.pdf")).unwrap());
    assert!(bytes.windows(b"/Watermark".len()).any(|s| s == b"/Watermark"), "the headless background reaches the signed PDF");
    assert!(bytes.starts_with(&before), "signing keeps the earlier timestamped revision");
    worker.join().unwrap();
}
