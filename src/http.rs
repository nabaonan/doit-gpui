//! A desktop `HttpClient` built on gpui's vendored reqwest (`gpui-pre-reqwest`).
//!
//! Key decisions:
//!
//! 1. **No system proxy.** `no_proxy()` — the OS/environment proxy (company
//!    ladder, VPN split tunnel) is the most common reason a private NAS WebDAV
//!    endpoint fails with `error sending request for url`.
//! 2. **Own Tokio handle.** gpui runs on its own executor (not Tokio), so the
//!    reqwest request must be driven on a real Tokio runtime. We create and
//!    keep one runtime, and dispatch every request through its handle.
//! 3. **Optional self-signed trust.** Intranet NAS servers (fnOS, Synology,
//!    ...) frequently use self-signed / privately-signed certificates. When
//!    the user opts in via settings, we use a client that skips certificate
//!    validation. Opt-in only, and clearly surfaced in the UI.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use gpui_kit::gpui::http_client::{
    anyhow, AsyncBody, HttpClient, Inner, Result as HttpResult, http,
};
use futures::future::BoxFuture;

/// Whether to skip server certificate validation (self-signed / private CA NAS).
static TRUST_SELF_SIGNED: AtomicBool = AtomicBool::new(false);

/// Set whether the HTTP client should accept self-signed certificates.
pub fn set_trust_self_signed(trust: bool) {
    TRUST_SELF_SIGNED.store(trust, Ordering::Relaxed);
}

fn build_client(relaxed: bool) -> gpui_pre_reqwest::Client {
    let mut builder = gpui_pre_reqwest::Client::builder()
        .no_proxy()
        .use_rustls_tls()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30));
    if relaxed {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder
        .build()
        .expect("failed to initialize HTTP client")
}

struct NoProxyRequester {
    strict: gpui_pre_reqwest::Client,
    relaxed: gpui_pre_reqwest::Client,
    handle: tokio::runtime::Handle,
    #[allow(dead_code)]
    runtime: tokio::runtime::Runtime,
}

/// Build an HTTP client appropriate for WebDAV sync (no proxy, bounded timeouts).
pub fn new_http_client() -> Arc<dyn HttpClient> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime for HTTP client");
    let handle = runtime.handle().clone();

    Arc::new(NoProxyRequester {
        strict: build_client(false),
        relaxed: build_client(true),
        handle,
        runtime,
    })
}

impl HttpClient for NoProxyRequester {
    fn user_agent(&self) -> Option<&http::HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&gpui_kit::gpui::http_client::Url> {
        None
    }

    fn send(
        &self,
        req: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, HttpResult<http::Response<AsyncBody>>> {
        let client = if TRUST_SELF_SIGNED.load(Ordering::Relaxed) {
            self.relaxed.clone()
        } else {
            self.strict.clone()
        };
        let handle = self.handle.clone();

        Box::pin(async move {
            let (parts, body) = req.into_parts();

            let body_bytes = match body.0 {
                Inner::Empty => Vec::new(),
                Inner::Bytes(cursor) => cursor.into_inner().to_vec(),
                // Our app only ever uploads in-memory bodies; streaming request
                // bodies are not used, so reject them explicitly.
                Inner::AsyncReader(_) => {
                    return Err(anyhow!("不支持的流式请求体"));
                }
            };

            let mut builder = client.request(parts.method, parts.uri.to_string());
            builder = builder.headers(parts.headers.clone());
            let request = builder.body(body_bytes);
            let response = handle
                .spawn(async move { request.send().await })
                .await
                .map_err(|join| anyhow!("HTTP 任务失败：{join}"))?
                .map_err(|e| anyhow!(e))?;

            let status = response.status();
            let headers = response.headers().clone();
            let bytes = response.bytes().await.map_err(|e| anyhow!(e))?;

            let mut out = http::Response::builder().status(status.as_u16());
            *out.headers_mut().ok_or_else(|| anyhow!("no header map"))? = headers;
            out.body(AsyncBody::from_bytes(bytes))
                .map_err(|e| anyhow!(e))
        })
    }
}
