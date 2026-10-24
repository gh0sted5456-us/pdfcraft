//! Opt-in RFC 3161 transport. Creating a server never opens a connection.

use pdfcraft_sign::{SignError, TimestampAuthority};

pub const DEFAULT_SERVER: &str = "https://timestamp.sectigo.com";

/// Existing OS trust anchors, read locally and used only for TSA validation. No store is changed.
pub fn trusted_certificates() -> Vec<pdfcraft_sign::Certificate> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static ROOTS: std::sync::OnceLock<Vec<pdfcraft_sign::Certificate>> = std::sync::OnceLock::new();
        ROOTS
            .get_or_init(|| {
                rustls_native_certs::load_native_certs().certs.iter().filter_map(|c| pdfcraft_sign::Certificate::parse(c.as_ref()).ok()).collect()
            })
            .clone()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Vec::new()
    }
}

#[derive(Clone)]
pub struct TimestampServer(String);

impl TimestampServer {
    pub fn new(url: &str) -> Result<Self, SignError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let url = url.trim();
            if url.len() > 2048 {
                return Err(invalid());
            }
            let uri: ureq::http::Uri = url.parse().map_err(|_| invalid())?;
            let host = uri.host().ok_or_else(invalid)?;
            let loopback = matches!(host, "127.0.0.1" | "[::1]" | "localhost");
            if url.len() > 2048 || url.contains(['@', '#']) || !(uri.scheme_str() == Some("https") || uri.scheme_str() == Some("http") && loopback) {
                return Err(invalid());
            }
            Ok(Self(url.to_string()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = url;
            Err(invalid())
        }
    }
}

fn invalid() -> SignError {
    SignError::Malformed("Choose an HTTPS timestamp server URL (HTTP is allowed only on loopback); timestamps need the desktop app.".into())
}

impl TimestampAuthority for TimestampServer {
    fn timestamp(&self, request: &[u8]) -> Result<Vec<u8>, SignError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if request.len() > 1024 * 1024 {
                return Err(SignError::Malformed("timestamp request is too large".into()));
            }
            let roots: Vec<ureq::tls::Certificate<'static>> =
                rustls_native_certs::load_native_certs().certs.iter().map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned()).collect();
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(20)))
                .max_redirects(0)
                .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::new_with_certs(&roots)).build())
                .build()
                .new_agent();
            let failed = |e| SignError::Crypto(format!("Timestamp server request failed: {e}"));
            let mut response = agent
                .post(&self.0)
                .header("Content-Type", "application/timestamp-query")
                .header("Accept", "application/timestamp-reply")
                .send(request)
                .map_err(failed)?;
            if response.status() != 200 {
                return Err(SignError::Crypto(format!("Timestamp server returned HTTP {}", response.status())));
            }
            response.body_mut().with_config().limit(4 * 1024 * 1024).read_to_vec().map_err(failed)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (&self.0, request);
            Err(invalid())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "contacts the public Sectigo TSA; run explicitly to check the default service"]
    fn default_service_returns_a_verified_timestamp_chained_to_an_os_root() {
        use pdfcraft_sign::{
            DigestAlg,
            timestamp::{TimestampQuery, parse_response, token_certs},
        };
        let digest = DigestAlg::Sha256.digest(&[b"PdfCraft synthetic timestamp service check"]);
        let mut query = TimestampQuery::new(DigestAlg::Sha256, digest).unwrap();
        query.nonce = Some(854349732);
        let reply = TimestampServer::new(DEFAULT_SERVER).unwrap().timestamp(&query.encode().unwrap()).unwrap();
        let token = parse_response(&reply, &query).unwrap();
        let certificate = token.signer_certificate().unwrap();
        assert!(certificate.may_timestamp() && certificate.valid_at(token.gen_time));
        let roots = trusted_certificates();
        let mut pool = token_certs(&token.raw);
        pool.extend(roots.iter().cloned());
        let chain = pdfcraft_sign::x509::build_chain(&certificate, &pool, Some(token.gen_time));
        assert!(chain.iter().any(|cert| roots.iter().any(|root| root.raw == cert.raw)), "TSA must chain to an existing OS trust anchor");
        println!("Verified timestamp from {} at {}", certificate.display_name(), token.gen_time);
    }

    #[test]
    fn server_urls_require_tls_and_no_credentials_or_fragments() {
        for url in ["", "ftp://example.com", "http://example.com", "https://user:password@example.com", "https://example.com/#token"] {
            assert!(TimestampServer::new(url).is_err(), "{url}");
        }
        for url in ["https://example.com/tsa", "http://127.0.0.1:1234/", "http://[::1]/tsa"] {
            assert!(TimestampServer::new(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn redirects_are_not_followed() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..n]).contains("application/timestamp-query"));
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        assert!(TimestampServer::new(&url).unwrap().timestamp(b"test").is_err());
        worker.join().unwrap();
    }
}
