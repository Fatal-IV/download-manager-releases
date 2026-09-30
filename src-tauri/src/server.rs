//! Tarayıcı eklentisinin indirme göndermesi için yalnızca yerel makinede dinleyen HTTP sunucusu.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::manager::{AddRequest, Manager};

pub const PORT: u16 = 38653;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddBody {
    url: String,
    filename: Option<String>,
    referrer: Option<String>,
    user_agent: Option<String>,
    cookie: Option<String>,
}

/// Web siteleri de localhost'a istek atabilir; yalnızca tarayıcı eklentisi kaynaklarını kabul ederiz.
fn is_extension_origin(o: &HeaderValue) -> bool {
    o.to_str().is_ok_and(|s| {
        s.starts_with("chrome-extension://") || s.starts_with("moz-extension://")
    })
}

/// Tarayıcı, web sitelerinden gelen istekleri `Origin` ile işaretler; başka siteden geleni reddederiz.
/// Eklenti isteklerinde başlık olmayabilir. `/add` ayrıca JSON içerik türü ister; bu da siteleri
/// CORS ön kontrolüne (reddedilir) zorlar.
async fn guard(headers: &HeaderMap) -> Result<(), StatusCode> {
    match headers.get(header::ORIGIN) {
        Some(o) if !is_extension_origin(o) => Err(StatusCode::FORBIDDEN),
        _ => Ok(()),
    }
}

async fn ping(headers: HeaderMap) -> Result<&'static str, StatusCode> {
    guard(&headers).await?;
    Ok("ok")
}

async fn add(
    State(m): State<Manager>,
    headers: HeaderMap,
    Json(b): Json<AddBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    guard(&headers).await.map_err(|c| (c, String::new()))?;
    let mut h = Vec::new();
    for (k, v) in [("Referer", b.referrer), ("User-Agent", b.user_agent), ("Cookie", b.cookie)] {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            h.push((k.to_string(), v));
        }
    }
    m.add(AddRequest { url: b.url, connections: None, headers: h, filename: b.filename, mirrors: Vec::new(), expected_sha256: None })
        .map(|_| StatusCode::OK)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

pub async fn serve(m: Manager) {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|o, _| is_extension_origin(o)))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]);
    let app = Router::new()
        .route("/ping", get(ping))
        .route("/add", post(add))
        .layer(cors)
        .with_state(m);
    match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
        Ok(l) => {
            let _ = axum::serve(l, app).await;
        }
        Err(e) => log::error!("Yerel sunucu başlatılamadı (port {PORT}): {e}"),
    }
}
