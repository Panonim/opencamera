//! Secure local web server that hands out the phone website and its live connection channel.

use crate::engine::Inner;
use crate::net::Tls;
use crate::{session, Result};
use axum::body::Body;
use axum::extract::{ws::WebSocketUpgrade, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(RustEmbed)]
#[folder = "../../apps/mobile/dist"]
#[allow_missing = true]
struct Mobile;

pub fn start(inner: Arc<Inner>, runtime: &tokio::runtime::Runtime, port: u16, tls: Tls) -> Result<axum_server::Handle<SocketAddr>> {
    let cert = CertificateDer::from_pem_slice(tls.cert_pem.as_bytes())?;
    let key = PrivateKeyDer::from_pem_slice(tls.key_pem.as_bytes())?;
    let mut config = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(vec![cert], key)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    let listener = std::net::TcpListener::bind(("0.0.0.0", port))?;
    listener.set_nonblocking(true)?;
    let _guard = runtime.enter();
    let handle = axum_server::Handle::new();
    let server = axum_server::from_tcp_rustls(listener, axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(config)))?
        .handle(handle.clone());
    let app = Router::new().route("/ws", get(ws)).fallback(get(asset)).with_state(inner.clone());
    runtime.spawn(async move {
        if let Err(e) = server.serve(app.into_make_service_with_connect_info::<SocketAddr>()).await {
            inner.log(format!("server stopped: {e}"));
        }
    });
    Ok(handle)
}

fn header_str(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Checks that the request comes from a page this server delivered itself.
fn same_origin(headers: &HeaderMap) -> bool {
    let host = header_str(headers, header::HOST);
    let origin = header_str(headers, header::ORIGIN);
    matches!((host, origin), (Some(h), Some(o)) if o == format!("https://{h}"))
}

async fn ws(State(inner): State<Arc<Inner>>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    upgrade.max_message_size(256 * 1024).on_upgrade(move |socket| session::run(inner, socket))
}

async fn asset(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (path, file) = match Mobile::get(path) {
        Some(f) if !path.is_empty() => (path, f),
        _ => match Mobile::get("index.html") {
            Some(f) => ("index.html", f),
            None => return (StatusCode::SERVICE_UNAVAILABLE, "Mobile site not built").into_response(),
        },
    };
    let mut res = Response::new(Body::from(file.data.into_owned()));
    let h = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(file.metadata.mimetype()) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    if path == "index.html" {
        let host = header_str(&headers, header::HOST).unwrap_or("");
        let csp = format!(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
             media-src 'self' blob: mediastream:; connect-src 'self' wss://{host}; frame-ancestors 'none'"
        );
        if let Ok(v) = HeaderValue::from_str(&csp) {
            h.insert(header::CONTENT_SECURITY_POLICY, v);
        }
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    } else {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_must_match_host() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "192.168.1.5:47823".parse().unwrap());
        assert!(!same_origin(&h));
        h.insert(header::ORIGIN, "https://192.168.1.5:47823".parse().unwrap());
        assert!(same_origin(&h));
        h.insert(header::ORIGIN, "https://evil.example".parse().unwrap());
        assert!(!same_origin(&h));
    }
}
