use futures::AsyncReadExt;
use base64::Engine as _;
use gpui_kit::gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use crate::types::{SyncSnapshot, SYNC_FILE};

/// Build a Basic Authorization header value for WebDAV.
fn auth_header(user: &str, pass: &str) -> String {
    let cred = format!("{}:{}", user, pass);
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(cred)
    )
}

/// Join a base WebDAV URL with a file name, keeping one `/` between.
fn join_url(base: &str, file: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), file)
}

/// A readable outcome of a sync action (used to render results in the UI).
#[derive(Clone, Debug, PartialEq)]
pub enum TransferState {
    Idle,
    Busy,
    Ok(String),
    Err(String),
}

/// Validate credentials for a request, yielding a clear message when the URL is empty.
pub(crate) fn check_config(url: &str, user: &str, pass: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("请先配置 WebDAV 地址".to_string());
    }
    let _ = (user, pass);
    Ok(())
}

/// Flatten an error and all of its causes into a readable, multi-line string.
fn chain_text(err: &dyn std::error::Error) -> String {
    let mut lines = vec![err.to_string()];
    let mut cur = err.source();
    while let Some(src) = cur {
        lines.push(src.to_string());
        cur = src.source();
    }
    lines.join("\n  ↳ ")
}

/// Classify a transport-level request error into a concrete, actionable message
/// (DNS, timeout, TLS/certificate, proxy, refused connection, ...).
fn classify_transport(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut cur = err.source();
    while let Some(src) = cur {
        text.push('\n');
        text.push_str(&src.to_string());
        cur = src.source();
    }
    let lower = text.to_lowercase();

    let hint = if lower.contains("certificate")
        || lower.contains("invalid peer")
        || lower.contains("unknownissuer")
        || lower.contains("unable to get local issuer")
        || lower.contains("expired")
    {
        "服务器证书不受信任（可能是自签名或内网证书）"
    } else if lower.contains("failed to lookup")
        || lower.contains("temporary failure in name resolution")
        || lower.contains("dns")
    {
        "域名解析失败，请确认该地址当前可访问"
    } else if lower.contains("timed out")
        || lower.contains("deadline")
        || lower.contains("connection reset")
    {
        "连接超时或被重置，请检查网络或稍后重试"
    } else if lower.contains("connection refused") || lower.contains("econnrefused") {
        "连接被拒绝，请确认服务器已开启且地址/端口正确"
    } else if lower.contains("proxy") {
        "代理连接出错，可尝试关闭系统代理或改用直连"
    } else if lower.contains("tls") {
        "TLS 握手失败（可能出现协议/版本不兼容）"
    } else {
        "网络不可达或请求被中断，请检查地址、网络与防火墙"
    };

    format!("{hint}\n原始信息：\n{}", chain_text(err))
}

/// Turn a response status from a reachability probe into a readable result.
fn status_result(label: &str, status: StatusCode) -> Result<String, String> {
    if status.is_success() {
        Ok(format!("{label}：服务器可达（HTTP {}）", status.as_u16()))
    } else if status == StatusCode::METHOD_NOT_ALLOWED {
        Ok(format!("{label}：服务器可达（HTTP 405）"))
    } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        Err(format!("{label}：认证失败，请检查用户名或密码（HTTP {}）", status.as_u16()))
    } else {
        Err(format!("{label}失败（HTTP {}）", status.as_u16()))
    }
}

/// Verify the WebDAV server is reachable and the credentials work.
///
/// Probe order:
/// 1. `OPTIONS` — a plain reachability probe many servers answer without auth.
/// 2. If OPTIONS fails at the transport level, retry with `PROPFIND` (some
///    gateways mishandle OPTIONS).
/// 3. `PROPFIND` with `Depth: 0` actually triggers authentication, so its
///    `401/403` is a real credential check, and `2xx` confirms the account works.
pub async fn test_connection(
    client: &dyn HttpClient,
    url: &str,
    user: &str,
    pass: &str,
) -> Result<String, String> {
    check_config(url, user, pass)?;
    let url = url.trim();

    let options = Request::builder()
        .method(Method::OPTIONS)
        .uri(url)
        .header("Authorization", auth_header(user, pass))
        .body(AsyncBody::empty())
        .map_err(|e| e.to_string())?;

    // 1) OPTIONS reachability probe.
    match client.send(options).await {
        Ok(resp) => {
            // Reachable — continue to PROPFIND to also validate credentials.
            let _ = resp;
        }
        Err(_e) => {
            // Some servers/gateways break on OPTIONS; fall back to PROPFIND
            // before concluding the host is unreachable.
            let propfind = propfind_request(url, user, pass)?;
            match client.send(propfind).await {
                Ok(_) => {}
                Err(e2) => return Err(format!("连接失败：{}", classify_transport(e2.as_ref()))),
            }
        }
    }

    // 2) PROPFIND — real WebDAV + credentials check.
    let propfind = propfind_request(url, user, pass)?;
    match client.send(propfind).await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                Ok(format!("连接正常，凭据有效（HTTP {}）", status.as_u16()))
            } else if status == StatusCode::METHOD_NOT_ALLOWED {
                Result::<String, String>::Ok(format!(
                    "服务器可达，但该服务不支持目录探测（HTTP 405）；可在云备份中直接上传试试"
                ))
            } else {
                status_result("连接", status)
            }
        }
        Err(e) => Err(format!("连接失败：{}", classify_transport(e.as_ref()))),
    }
}

fn propfind_request(
    url: &str,
    user: &str,
    pass: &str,
) -> Result<Request<AsyncBody>, String> {
    Request::builder()
        .method(Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri(url)
        .header("Authorization", auth_header(user, pass))
        .header("Depth", "0")
        .body(AsyncBody::empty())
        .map_err(|e| e.to_string())
}

/// Upload a snapshot to `{webdav_url}/doit-snapshot.json`.
pub async fn upload_snapshot(
    client: &dyn HttpClient,
    url: &str,
    user: &str,
    pass: &str,
    snapshot: &SyncSnapshot,
) -> Result<String, String> {
    check_config(url, user, pass)?;
    let body = serde_json::to_string(snapshot).map_err(|e| e.to_string())?;
    let target = join_url(url, SYNC_FILE);
    let req = Request::builder()
        .method(Method::PUT)
        .uri(&target)
        .header("Authorization", auth_header(user, pass))
        .header("Content-Type", "application/json")
        .body(AsyncBody::from(body))
        .map_err(|e| e.to_string())?;

    let resp = client
        .send(req)
        .await
        .map_err(|e| format!("上传失败：{}", classify_transport(e.as_ref())))?;
    let status = resp.status();
    if status.is_success() {
        Ok(format!("上传成功（HTTP {}）", status.as_u16()))
    } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        Err(format!("上传失败：认证失败，请检查用户名或密码（HTTP {}）", status.as_u16()))
    } else {
        Err(format!("上传失败（HTTP {}）", status.as_u16()))
    }
}

/// Download the snapshot from `{webdav_url}/doit-snapshot.json`.
pub async fn download_snapshot(
    client: &dyn HttpClient,
    url: &str,
    user: &str,
    pass: &str,
) -> Result<SyncSnapshot, String> {
    check_config(url, user, pass)?;
    let target = join_url(url, SYNC_FILE);
    let req = Request::builder()
        .method(Method::GET)
        .uri(&target)
        .header("Authorization", auth_header(user, pass))
        .body(AsyncBody::empty())
        .map_err(|e| e.to_string())?;

    let resp = client
        .send(req)
        .await
        .map_err(|e| format!("下载失败：{}", classify_transport(e.as_ref())))?;
    let status = resp.status();
    if status == StatusCode::NOT_FOUND {
        return Err("云端还没有备份文件（doit-snapshot.json），请先上传".to_string());
    }
    if !status.is_success() {
        return Err(format!("下载失败（HTTP {}）", status.as_u16()));
    }
    let mut body = resp.into_body();
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("读取数据失败：{e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("解析云端数据失败：{e}"))
}

/// The current local time, ISO-8601 without timezone suffix (for snapshot stamps).
pub(crate) fn local_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}
