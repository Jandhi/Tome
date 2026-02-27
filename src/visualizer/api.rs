use std::sync::{Arc, RwLock};

use axum::{extract::State, Json};
use axum::http::StatusCode;
use tokio::sync::broadcast;

use super::types::*;

pub type SharedState = Arc<RwLock<VisualizerState>>;

fn push_log(state: &SharedState, event_tx: &broadcast::Sender<VisualizerEvent>, level: &str, message: &str) {
    let entry = LogEntry {
        timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
        level: level.to_string(),
        message: message.to_string(),
    };
    if let Ok(mut s) = state.write() {
        if s.logs.len() >= 200 {
            s.logs.remove(0);
        }
        s.logs.push(entry.clone());
    }
    let _ = event_tx.send(VisualizerEvent::LogMessage(entry));
}

pub struct VisualizerState {
    pub phase: GenerationPhase,
    pub error: Option<String>,
    pub snapshot: Option<WorldSnapshot>,
    pub logs: Vec<LogEntry>,
}

impl VisualizerState {
    pub fn new() -> Self {
        Self {
            phase: GenerationPhase::Idle,
            error: None,
            snapshot: None,
            logs: Vec::new(),
        }
    }
}

pub async fn get_status(
    State(state): State<SharedState>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(ref snap) = state.snapshot {
        Ok(Json(StatusResponse {
            phase: state.phase.clone(),
            width: snap.width,
            depth: snap.depth,
            origin_x: snap.origin_x,
            origin_z: snap.origin_z,
            error: state.error.clone(),
        }))
    } else {
        Ok(Json(StatusResponse {
            phase: state.phase.clone(),
            width: 0,
            depth: 0,
            origin_x: 0,
            origin_z: 0,
            error: state.error.clone(),
        }))
    }
}

pub async fn get_logs(
    State(state): State<SharedState>,
) -> Result<Json<Vec<LogEntry>>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(state.logs.clone()))
}

pub async fn post_generate(
    State(state): State<SharedState>,
    axum::Extension(notify): axum::Extension<Arc<tokio::sync::Notify>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let phase = {
        let s = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        s.phase.clone()
    };
    match phase {
        GenerationPhase::Idle | GenerationPhase::Done | GenerationPhase::Error => {
            notify.notify_one();
            Ok(Json(serde_json::json!({ "status": "started" })))
        }
        _ => Err(StatusCode::CONFLICT),
    }
}

pub async fn get_snapshot(
    State(state): State<SharedState>,
) -> Result<Json<WorldSnapshot>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match &state.snapshot {
        Some(snap) => Ok(Json(snap.clone())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_heightmap(
    State(state): State<SharedState>,
) -> Result<Json<HeightmapData>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match state.snapshot.as_ref().and_then(|s| s.heightmap.clone()) {
        Some(data) => Ok(Json(data)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_biomes(
    State(state): State<SharedState>,
) -> Result<Json<BiomeMapData>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match state.snapshot.as_ref().and_then(|s| s.biomes.clone()) {
        Some(data) => Ok(Json(data)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_blocks(
    State(state): State<SharedState>,
) -> Result<Json<BlockMapData>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match state.snapshot.as_ref().and_then(|s| s.blocks.clone()) {
        Some(data) => Ok(Json(data)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_districts(
    State(state): State<SharedState>,
) -> Result<Json<DistrictMapData>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match state.snapshot.as_ref().and_then(|s| s.districts.clone()) {
        Some(data) => Ok(Json(data)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_buildings(
    State(state): State<SharedState>,
) -> Result<Json<BuildingsData>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match state.snapshot.as_ref().and_then(|s| s.buildings.clone()) {
        Some(data) => Ok(Json(data)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_claims(
    State(state): State<SharedState>,
) -> Result<Json<ClaimMapData>, StatusCode> {
    let state = state.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match state.snapshot.as_ref().and_then(|s| s.claims.clone()) {
        Some(data) => Ok(Json(data)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn post_refresh(
    State(state): State<SharedState>,
    axum::Extension(event_tx): axum::Extension<broadcast::Sender<VisualizerEvent>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    push_log(&state, &event_tx, "info", "Refresh: loading world from build area...");
    {
        let mut s = state.write().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        s.phase = GenerationPhase::Refreshing;
        s.error = None;
    }
    let _ = event_tx.send(VisualizerEvent::PhaseChanged(GenerationPhase::Refreshing));

    let state_clone = state.clone();
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        use crate::http_mod::GDMCHTTPProvider;
        use crate::editor::World;
        use super::snapshot::extract_full_snapshot;

        let provider = GDMCHTTPProvider::new();
        match World::new(&provider).await {
            Ok(world) => {
                let snap = extract_full_snapshot(&world, &GenerationPhase::Idle);
                if let Ok(mut s) = state_clone.write() {
                    s.snapshot = Some(snap);
                    s.phase = GenerationPhase::Idle;
                }
                let _ = event_tx_clone.send(VisualizerEvent::SnapshotUpdated);
                let _ = event_tx_clone.send(VisualizerEvent::PhaseChanged(GenerationPhase::Idle));
                push_log(&state_clone, &event_tx_clone, "info", "Refresh: world loaded successfully");
            }
            Err(e) => {
                let msg = format!("Refresh failed: {e}");
                push_log(&state_clone, &event_tx_clone, "error", &msg);
                if let Ok(mut s) = state_clone.write() {
                    s.error = Some(msg);
                    s.phase = GenerationPhase::Error;
                }
                let _ = event_tx_clone.send(VisualizerEvent::PhaseChanged(GenerationPhase::Error));
            }
        }
    });

    Ok(Json(serde_json::json!({ "status": "loading" })))
}
