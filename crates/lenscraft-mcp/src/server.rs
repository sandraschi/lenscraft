//! L4/L5 lens-server: HTTP + mobile web UI behind `tailscale serve` for iPhone and GPD.
use axum::extract::{DefaultBodyLimit, Path as AxumPath, Query};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use lenscraft_core::{
    answer_followup, build_wikidata_pack, explain, find_museum, ground_result, identify,
    match_collection, nearby_venues, prepare_bytes, resolve_floorplan, search_museums, status,
    CollectionPack, GroundingInfo, HttpKnowledgeClient, IdentifyMode, IdentifyOptions,
    IdentifyResult, LengthPreset, LensError, OllamaHttp, OverpassHttp, QaTurn, SpeechMcpHttp,
    SpeechTransport as _, WorldDb, WorldEntry, DEFAULT_MODEL, DEFAULT_WORLD_DB_PATH,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;

pub const DEFAULT_SERVER_PORT: u16 = 11265;
const MOBILE_HTML: &str = include_str!("mobile_ui.html");
const ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 192 192">
  <rect width="192" height="192" rx="40" fill="#0b0e14"/>
  <circle cx="96" cy="96" r="56" fill="none" stroke="#d4a853" stroke-width="10"/>
  <circle cx="96" cy="96" r="24" fill="#d4a853"/>
  <path d="M96 22v18M96 152v18M22 96h18M152 96h18" stroke="#d4a853" stroke-width="8" stroke-linecap="round"/>
</svg>"##;
const SW_JS: &str = r#"const CACHE = 'lenscraft-pwa-v1';
const SHELL = ['/', '/manifest.webmanifest', '/icon.svg'];
self.addEventListener('install', e => {
  e.waitUntil(caches.open(CACHE).then(c => c.addAll(SHELL)).then(() => self.skipWaiting()));
});
self.addEventListener('activate', e => {
  e.waitUntil(self.clients.claim());
});
self.addEventListener('fetch', e => {
  if (e.request.method !== 'GET' || e.request.url.includes('/api/')) return;
  e.respondWith(
    caches.match(e.request).then(cached => cached || fetch(e.request).then(res => {
      const copy = res.clone();
      caches.open(CACHE).then(c => c.put(e.request, copy));
      return res;
    }))
  );
});"#;

fn err_resp(e: &LensError) -> Response {
    let code = match e {
        LensError::InvalidInput(_) | LensError::Image(_) => StatusCode::BAD_REQUEST,
        LensError::Offline(_) => StatusCode::SERVICE_UNAVAILABLE,
        LensError::Io(_) | LensError::Protocol(_) | LensError::Model(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (
        code,
        Json(json!({"error": e.to_string(), "kind": e.kind()})),
    )
        .into_response()
}

async fn index_html() -> Html<&'static str> {
    Html(MOBILE_HTML)
}

async fn pwa_manifest() -> Response {
    let body = json!({
        "name": "LensCraft Museum Companion",
        "short_name": "LensCraft",
        "start_url": "/",
        "display": "standalone",
        "background_color": "#0b0e14",
        "theme_color": "#0b0e14",
        "icons": [
            { "src": "/icon.svg", "sizes": "any", "type": "image/svg+xml", "purpose": "any maskable" }
        ]
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
        .into_response()
}

async fn pwa_sw() -> Response {
    ([(header::CONTENT_TYPE, "application/javascript")], SW_JS).into_response()
}

async fn pwa_icon() -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml")], ICON_SVG).into_response()
}

async fn api_health() -> Response {
    let res = tokio::task::spawn_blocking(|| {
        let ollama = OllamaHttp::from_env().and_then(|t| status(&t, DEFAULT_MODEL));
        let speech_url = std::env::var("LENSCRAFT_SPEECH_URL")
            .unwrap_or_else(|_| lenscraft_core::DEFAULT_SPEECH_URL.to_string());
        json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "ollama": ollama.map(|s| json!(s)).unwrap_or_else(|e| json!({"error": e.to_string()})),
            "speech_url": speech_url,
        })
    })
    .await;
    match res {
        Ok(v) => Json(v).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct VenuesReq {
    lat: f64,
    lon: f64,
    radius_m: Option<u32>,
}

async fn api_venues(Json(req): Json<VenuesReq>) -> Response {
    let res = tokio::task::spawn_blocking(move || {
        let t = OverpassHttp::from_env()?;
        nearby_venues(&t, req.lat, req.lon, req.radius_m)
    })
    .await;
    match res {
        Ok(Ok(v)) => Json(json!({"venues": v})).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

#[derive(Deserialize)]
struct IdentifyReq {
    image_b64: String,
    #[serde(default)]
    place: Option<String>,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    mode: IdentifyMode,
    #[serde(default)]
    length: Option<String>,
    #[serde(default)]
    ground: Option<bool>,
}

async fn api_identify(Json(req): Json<IdentifyReq>) -> Response {
    let res = tokio::task::spawn_blocking(move || -> Result<Value, LensError> {
        let raw_b64 = req
            .image_b64
            .split_once(',')
            .map(|(_, b)| b)
            .unwrap_or(&req.image_b64);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw_b64.trim())
            .map_err(|e| LensError::InvalidInput(format!("invalid base64 image: {e}")))?;
        let prepared = prepare_bytes(&bytes)?;
        let ollama = OllamaHttp::from_env()?;
        let lang = req.language.clone().unwrap_or_else(|| "en".to_string());
        let opts = IdentifyOptions {
            model: String::new(),
            hint: req.hint,
            place: req.place,
            lat: req.lat,
            lon: req.lon,
            language: Some(lang.clone()),
            mode: req.mode,
        };
        let id_res = identify(&ollama, &prepared, &opts)?;
        let grounding = if req.ground.unwrap_or(true) {
            HttpKnowledgeClient::new()
                .ok()
                .and_then(|kc| ground_result(&kc, &id_res, &lang).ok().flatten())
        } else {
            None
        };
        let explanation = if let Some(ref len_str) = req.length {
            let preset = LengthPreset::parse(len_str)?;
            Some(explain(
                &ollama,
                &id_res,
                grounding.as_ref(),
                preset,
                &lang,
                "",
            )?)
        } else {
            None
        };
        let world = lookup_world(grounding.as_ref(), &id_res);
        Ok(json!({
            "identify": id_res,
            "grounding": grounding,
            "explanation": explanation,
            "world": world,
        }))
    })
    .await;
    match res {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

#[derive(Deserialize)]
struct TtsReq {
    text: String,
    #[serde(default = "default_provider")]
    provider: String,
    #[serde(default = "default_voice")]
    voice_id: String,
}

fn default_provider() -> String {
    "windows".to_string()
}

fn default_voice() -> String {
    "default".to_string()
}

async fn api_tts(Json(req): Json<TtsReq>) -> Response {
    let res = tokio::task::spawn_blocking(move || {
        let speech = SpeechMcpHttp::from_env()?;
        speech.tts_wav(&req.text, &req.provider, &req.voice_id)
    })
    .await;
    match res {
        Ok(Ok(wav)) => ([(header::CONTENT_TYPE, "audio/wav")], wav).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

#[derive(Deserialize)]
struct TranscribeReq {
    audio_b64: String,
    #[serde(default = "default_audio_name")]
    filename: String,
    #[serde(default = "default_lang")]
    language: String,
}

fn default_audio_name() -> String {
    "question.webm".to_string()
}

fn default_lang() -> String {
    "auto".to_string()
}

async fn api_transcribe(Json(req): Json<TranscribeReq>) -> Response {
    let res = tokio::task::spawn_blocking(move || -> Result<Value, LensError> {
        let raw_b64 = req
            .audio_b64
            .split_once(',')
            .map(|(_, b)| b)
            .unwrap_or(&req.audio_b64);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw_b64.trim())
            .map_err(|e| LensError::InvalidInput(format!("invalid base64 audio: {e}")))?;
        let speech = SpeechMcpHttp::from_env()?;
        let text = speech.transcribe_file(&bytes, &req.filename, &req.language)?;
        Ok(json!({"text": text}))
    })
    .await;
    match res {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

#[derive(Deserialize)]
struct AskReq {
    identify: IdentifyResult,
    #[serde(default)]
    grounding: Option<GroundingInfo>,
    #[serde(default)]
    history: Vec<QaTurn>,
    question: String,
    #[serde(default = "default_en")]
    language: String,
}

fn default_en() -> String {
    "en".to_string()
}

async fn api_ask(Json(req): Json<AskReq>) -> Response {
    let res = tokio::task::spawn_blocking(move || {
        let ollama = OllamaHttp::from_env()?;
        answer_followup(
            &ollama,
            &req.identify,
            req.grounding.as_ref(),
            &req.history,
            &req.question,
            &req.language,
            "",
        )
    })
    .await;
    match res {
        Ok(Ok(ans)) => Json(ans).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

#[derive(Deserialize)]
struct PackMatchReq {
    image_b64: String,
    #[serde(default)]
    hint: Option<String>,
    pack: CollectionPack,
}

async fn api_pack_slug(AxumPath(slug): AxumPath<String>) -> Response {
    let slug_clean = slug.trim().to_lowercase();
    let Some(m) = find_museum(&slug_clean) else {
        return err_resp(&LensError::InvalidInput(format!(
            "unknown museum pack '{slug_clean}'"
        )));
    };
    let res = tokio::task::spawn_blocking(move || {
        let local_path = std::path::PathBuf::from(format!("packs/{}.json", m.slug));
        if local_path.exists() {
            if let Ok(pack) = lenscraft_core::load_pack(&local_path) {
                return Ok(pack);
            }
        }
        let kc = HttpKnowledgeClient::new()?;
        build_wikidata_pack(&kc, m.name, m.qid, m.pack_limit)
    })
    .await;
    match res {
        Ok(Ok(pack)) => Json(pack).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

#[derive(Deserialize)]
struct MuseumQuery {
    q: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
}

async fn api_museums_list(Query(mq): Query<MuseumQuery>) -> Response {
    let origin = match (mq.lat, mq.lon) {
        (Some(la), Some(lo)) => Some((la, lo)),
        _ => None,
    };
    let hits = search_museums(mq.q.as_deref(), origin);
    Json(json!({"museums": hits})).into_response()
}

async fn api_museum_floorplan(AxumPath(slug): AxumPath<String>) -> Response {
    let res = tokio::task::spawn_blocking(move || {
        let overpass = OverpassHttp::from_env().ok();
        resolve_floorplan(
            &slug,
            std::path::Path::new("floorplans"),
            overpass
                .as_ref()
                .map(|o| o as &dyn lenscraft_core::VenueTransport),
        )
    })
    .await;
    match res {
        Ok(Ok(fp)) => Json(fp).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

async fn api_pack_khm() -> Response {
    api_pack_slug(AxumPath("khm".to_string())).await
}

async fn api_pack_match(Json(req): Json<PackMatchReq>) -> Response {
    let res = tokio::task::spawn_blocking(move || {
        let raw_b64 = req
            .image_b64
            .split_once(',')
            .map(|(_, b)| b)
            .unwrap_or(&req.image_b64);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw_b64.trim())
            .map_err(|e| LensError::InvalidInput(format!("invalid base64 image: {e}")))?;
        let prepared = prepare_bytes(&bytes)?;
        match_collection(&req.pack, &prepared, req.hint.as_deref(), 5)
    })
    .await;
    match res {
        Ok(Ok(matches)) => Json(json!({"matches": matches})).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

#[derive(Deserialize)]
struct RegroundReq {
    title: String,
    #[serde(default)]
    creator: Option<String>,
    #[serde(default = "default_en")]
    language: String,
    #[serde(default)]
    length: Option<String>,
    identify: IdentifyResult,
}

async fn api_reground(Json(req): Json<RegroundReq>) -> Response {
    let res = tokio::task::spawn_blocking(move || -> Result<Value, LensError> {
        let kc = HttpKnowledgeClient::new()?;
        let grounding =
            lenscraft_core::ground_work(&kc, &req.title, req.creator.as_deref(), &req.language)?;
        let ollama = OllamaHttp::from_env()?;
        let preset = match req.length {
            Some(ref l) => LengthPreset::parse(l)?,
            None => LengthPreset::Standard3m,
        };
        let mut updated_id = req.identify;
        if let Some(ref g) = grounding {
            updated_id.candidates.insert(
                0,
                lenscraft_core::Candidate {
                    title: g.label.clone(),
                    creator: g.creator.clone().unwrap_or_default(),
                    date: g.inception.clone().unwrap_or_default(),
                    certainty: lenscraft_core::Certainty::High,
                    evidence: vec!["visitor correction + Wikidata grounding".to_string()],
                    label_match: None,
                },
            );
            updated_id.verdict = lenscraft_core::Verdict::Likely;
        }
        let explanation = explain(
            &ollama,
            &updated_id,
            grounding.as_ref(),
            preset,
            &req.language,
            "",
        )?;
        let world = lookup_world(grounding.as_ref(), &updated_id);
        Ok(json!({
            "identify": updated_id,
            "grounding": grounding,
            "explanation": explanation,
            "world": world,
        }))
    })
    .await;
    match res {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_resp(&e),
        Err(e) => err_resp(&LensError::Protocol(e.to_string())),
    }
}

fn lookup_world(grounding: Option<&GroundingInfo>, id_res: &IdentifyResult) -> Option<WorldEntry> {
    let db = WorldDb::load(std::path::Path::new(DEFAULT_WORLD_DB_PATH)).ok()?;
    if let Some(g) = grounding {
        if let Some(w) = db.get_by_qid(&g.qid) {
            return Some(w.clone());
        }
    }
    if let Some(top) = id_res.candidates.first() {
        if let Some(w) = db.find_by_title(&top.title) {
            return Some(w.clone());
        }
    }
    None
}

async fn api_worlds_list() -> Response {
    match WorldDb::load(std::path::Path::new(DEFAULT_WORLD_DB_PATH)) {
        Ok(db) => Json(json!({"worlds": db.list_ready()})).into_response(),
        Err(e) => err_resp(&e),
    }
}

async fn api_world_get(AxumPath(qid): AxumPath<String>) -> Response {
    match WorldDb::load(std::path::Path::new(DEFAULT_WORLD_DB_PATH)) {
        Ok(db) => match db.get_by_qid(&qid) {
            Some(w) => Json(w).into_response(),
            None => err_resp(&LensError::InvalidInput(format!(
                "no ready 3d world for '{qid}'"
            ))),
        },
        Err(e) => err_resp(&e),
    }
}

async fn api_world_splat(AxumPath(qid): AxumPath<String>) -> Response {
    let db = match WorldDb::load(std::path::Path::new(DEFAULT_WORLD_DB_PATH)) {
        Ok(d) => d,
        Err(e) => return err_resp(&e),
    };
    let Some(w) = db.get_by_qid(&qid) else {
        return err_resp(&LensError::InvalidInput(format!(
            "no ready 3d world for '{qid}'"
        )));
    };
    match std::fs::read(&w.splat_path) {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
        Err(e) => err_resp(&LensError::Io(format!("read splat failed: {e}"))),
    }
}

pub fn build_router() -> Router {
    Router::new()
        .route("/", get(index_html))
        .route("/manifest.webmanifest", get(pwa_manifest))
        .route("/sw.js", get(pwa_sw))
        .route("/icon.svg", get(pwa_icon))
        .route("/api/health", get(api_health))
        .route("/api/venues", post(api_venues))
        .route("/api/identify", post(api_identify))
        .route("/api/reground", post(api_reground))
        .route("/api/tts", post(api_tts))
        .route("/api/transcribe", post(api_transcribe))
        .route("/api/ask", post(api_ask))
        .route("/api/pack/khm", get(api_pack_khm))
        .route("/api/pack/{slug}", get(api_pack_slug))
        .route("/api/pack/match", post(api_pack_match))
        .route("/api/museums", get(api_museums_list))
        .route("/api/museums/{slug}/floorplan", get(api_museum_floorplan))
        .route("/api/worlds", get(api_worlds_list))
        .route("/api/worlds/{qid}", get(api_world_get))
        .route("/api/worlds/{qid}/splat", get(api_world_splat))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
}

pub fn run_server(host: &str, port: u16) -> Result<(), LensError> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| LensError::InvalidInput(format!("invalid bind address: {e}")))?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| LensError::Io(format!("tokio runtime: {e}")))?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| LensError::Io(format!("bind {addr} failed: {e}")))?;
        eprintln!("lens-server listening on http://{addr}");
        axum::serve(listener, build_router())
            .await
            .map_err(|e| LensError::Io(format!("server error: {e}")))
    })
}
