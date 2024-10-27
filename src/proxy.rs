use std::{collections::HashMap, sync::Arc, time::Instant};

use axum::{body::{self, Body}, extract::{Request, State}, http::{response, HeaderMap, Response}, routing::get, Router};
use reqwest::{StatusCode, Url};
use tokio::sync::RwLock;

use crate::plex::Container;

const TOKEN_TTL_MINUTES: u64 = 15;

#[derive(Clone)]
struct ProxyState {
    seen_tokens: Arc<RwLock<HashMap<String, Instant>>>,
    media_map: Arc<RwLock<HashMap<String, String>>>,
    plex_url: Url,
    plex_library_path: String,
    rclone_url: String,
}

impl ProxyState {
    async fn add_token(&self, token: String) {
        let mut seen_tokens = self.seen_tokens.write().await;
        seen_tokens.insert(token, Instant::now());

        let stale_tokens = seen_tokens.iter()
            .filter(|(_, v)| v.elapsed().as_secs() > TOKEN_TTL_MINUTES * 60)
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for token in stale_tokens {
            seen_tokens.remove(&token);
        }
    }

    async fn add_media(&self, key: String, file: String) {
        let mut media_map = self.media_map.write().await;
        media_map.insert(key, file);
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .gzip(true)
        .build()
        .unwrap()
}

trait ResponseBuilderExt {
    fn headers(self, headers: HeaderMap) -> Self;
}

impl ResponseBuilderExt for response::Builder {
    fn headers(mut self, headers: HeaderMap) -> Self {
        *self.headers_mut().unwrap() = headers;
        self
    }
}

async fn pass_to_plex(State(ProxyState { plex_url, .. }): State<ProxyState>, request: Request) -> Result<Response<Body>, (StatusCode, &'static str)> {
    let mut url = plex_url.clone();
    url.set_path(request.uri().path());
    url.set_query(request.uri().query());

    let plex_resp = client()
        .request(request.method().clone(), url)
        .headers(request.headers().clone())
        .body(reqwest::Body::wrap_stream(request.into_body().into_data_stream()))
        .send()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Failed to proxy request"))?;

    let response = Response::builder()
        .status(plex_resp.status())
        .headers(plex_resp.headers().clone())
        .body(Body::from_stream(plex_resp.bytes_stream()))
        .unwrap();

    Ok(response)
}

async fn pass_to_rclone(rclone_url: &str) -> Result<Response<Body>, (StatusCode, &'static str)> {
    log::info!("Proxying to rclone: {}", rclone_url);

    let rclone_resp = client()
        .get(rclone_url)
        .send()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Failed to proxy request"))?;

    let response = Response::builder()
        .status(rclone_resp.status())
        .headers(rclone_resp.headers().clone())
        .body(Body::from_stream(rclone_resp.bytes_stream()))
        .unwrap();

    Ok(response)
}

async fn fallback(state: State<ProxyState>, request: Request) -> Result<Response<Body>, (StatusCode, &'static str)> {
    // Check if this is a request for a media file and proxy it if so
    // else just pass through

    // only try to proxy if there's a token that we know about
    if let Some(token) = request.headers().get("x-plex-token").and_then(|v| v.to_str().ok()) {
        let seen_tokens = state.seen_tokens.read().await;
        if seen_tokens.contains_key(token) {
            // and it's for a known media file
            if let Some(path) = request.uri().path_and_query().map(|pq| pq.path()) {
                let media_map = state.media_map.read().await;
                if let Some(file) = media_map.get(path) {
                    if let Some(path) = file.strip_prefix(&state.plex_library_path) {
                        let rclone_url = state.rclone_url.trim_end_matches('/');
                        let path = path.trim_start_matches('/');
                        let url = format!("{}/{}", rclone_url, path);
                        return pass_to_rclone(&url).await;
                    } else {
                        log::info!("Not proxying unknown media file: {}", file);
                    }
                }
            }
        }
    }

    pass_to_plex(state.clone(), request).await
}

async fn capture_metadata(state: State<ProxyState>, headers: HeaderMap, request: Request<Body>) -> Result<Response<Body>, (StatusCode, &'static str)> {
    let response = pass_to_plex(state.clone(), request).await?;

    if matches!(response.status(), StatusCode::OK) {
        let plex_token = headers.get("x-plex-token").and_then(|v| v.to_str().ok().map(|s| s.to_string()));
        if let Some(plex_token) = plex_token {
            state.add_token(plex_token).await;
        }

        // take the whole body, we need it for parsing anyway
        let status = response.status();
        let headers = response.headers().clone();
        let data = body::to_bytes(response.into_body(), 1_048_576).await
            .map_err(|_| (StatusCode::BAD_GATEWAY, "Failed to read response body"))?;

        match serde_json::from_slice::<Container>(&data) {
            Ok(container) => {
                let parts = container.media_container.metadata.into_iter()
                .flat_map(|md| md.media.into_iter()
                    .flat_map(|media| media.parts.into_iter()
                        .map(|p| (p.key, p.file))));
                for (key, file) in parts {
                    state.add_media(key, file).await;
                }
            },
            Err(e) => {
                log::warn!("Failed to parse metadata: {}", e);
            }
        }

        let response = Response::builder()
            .status(status)
            .headers(headers)
            .body(Body::from(data))
            .unwrap();
        
        return Ok(response);
    }

    Ok(response)
}

pub fn make_proxy(plex_url: String, plex_library_path: String, rclone_url: String) -> Router {
    
    let plex_url: Url = plex_url.parse().unwrap();

    let state = ProxyState {
        seen_tokens: Arc::new(RwLock::new(HashMap::new())),
        media_map: Arc::new(RwLock::new(HashMap::new())),
        plex_url,
        plex_library_path,
        rclone_url,
    };

    Router::new()
        .route("/library/metadata/:id/children", get(capture_metadata))
        .fallback(fallback)
        .with_state(state)
}
