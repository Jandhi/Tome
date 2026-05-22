use std::sync::{Arc, RwLock};

use axum::Router;
use axum::routing::{get, post};
use tower_http::cors::{CorsLayer, Any};
use tower_http::services::ServeDir;
use tokio::sync::{broadcast, Notify};

use super::api::*;
use super::ws::{ws_handler, EventSender};
use super::types::{GenerationPhase, VisualizerEvent};

pub struct VisualizerServer {
    pub state: SharedState,
    pub event_tx: EventSender,
    pub generate_notify: Arc<Notify>,
}

impl VisualizerServer {
    pub fn new() -> Self {
        let state = Arc::new(RwLock::new(VisualizerState::new()));
        let (event_tx, _) = broadcast::channel::<VisualizerEvent>(64);
        Self { state, event_tx, generate_notify: Arc::new(Notify::new()) }
    }

    pub fn log(&self, level: &str, message: &str) {
        let entry = super::types::LogEntry {
            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            level: level.to_string(),
            message: message.to_string(),
        };
        if let Ok(mut state) = self.state.write() {
            // Keep last 200 log entries
            if state.logs.len() >= 200 {
                state.logs.remove(0);
            }
            state.logs.push(entry.clone());
        }
        let _ = self.event_tx.send(VisualizerEvent::LogMessage(entry));
    }

    pub fn update_phase(&self, phase: GenerationPhase) {
        self.log("info", &format!("Phase: {:?}", phase));
        if let Ok(mut state) = self.state.write() {
            state.phase = phase.clone();
            if phase != GenerationPhase::Error {
                state.error = None;
            }
        }
        let _ = self.event_tx.send(VisualizerEvent::PhaseChanged(phase));
    }

    pub fn update_error(&self, message: String) {
        self.log("error", &message);
        if let Ok(mut state) = self.state.write() {
            state.phase = GenerationPhase::Error;
            state.error = Some(message.clone());
        }
        let _ = self.event_tx.send(VisualizerEvent::PhaseChanged(GenerationPhase::Error));
    }

    pub fn update_snapshot(&self, snapshot: super::types::WorldSnapshot) {
        if let Ok(mut state) = self.state.write() {
            state.snapshot = Some(snapshot);
        }
        let _ = self.event_tx.send(VisualizerEvent::SnapshotUpdated);
    }

    pub async fn wait_for_generate(&self) {
        self.generate_notify.notified().await;
    }

    pub async fn start(&self) {
        let state = self.state.clone();
        let event_tx = self.event_tx.clone();
        let notify = self.generate_notify.clone();

        let api_routes = Router::new()
            .route("/api/status", get(get_status))
            .route("/api/snapshot", get(get_snapshot))
            .route("/api/heightmap", get(get_heightmap))
            .route("/api/blocks", get(get_blocks))
            .route("/api/biomes", get(get_biomes))
            .route("/api/districts", get(get_districts))
            .route("/api/buildings", get(get_buildings))
            .route("/api/claims", get(get_claims))
            .route("/api/logs", get(get_logs))
            .route("/api/generate", post(post_generate))
            .route("/api/refresh", post(post_refresh))
            .with_state(state)
            .layer(axum::Extension(notify))
            .layer(axum::Extension(event_tx.clone()));

        let ws_routes = Router::new()
            .route("/ws", get(ws_handler))
            .with_state(event_tx);

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // Serve the React build from visualizer/dist/ if it exists,
        // otherwise just serve the API
        let app = Router::new()
            .merge(api_routes)
            .merge(ws_routes)
            .layer(cors)
            .fallback_service(ServeDir::new("visualizer/dist"));

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
            .await
            .expect("Failed to bind visualizer to port 3000");

        log::info!("Visualizer server running at http://localhost:3000");

        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("Visualizer server failed");
        });
    }
}
