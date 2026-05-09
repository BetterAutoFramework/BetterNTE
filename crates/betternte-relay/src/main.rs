//! BetterNTE Relay — minimal WebSocket relay server.
//!
//! Bridges BetterNTE desktop clients with web-based browser editors.
//! Clients register a session; browsers join that session by ID.
//! All non-protocol messages are relayed bidirectionally between the
//! paired client and its connected browsers.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::{mpsc, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// One active session created by a desktop client.
struct Session {
    /// Channel to the desktop client that owns this session.
    client_sender: Option<mpsc::UnboundedSender<String>>,
    /// Channels to every browser currently joined to this session.
    browser_senders: Vec<mpsc::UnboundedSender<String>>,
}

type Sessions = Arc<RwLock<HashMap<String, Session>>>;

// ---------------------------------------------------------------------------
// Message protocol
// ---------------------------------------------------------------------------

/// Envelope for every JSON message that goes over the wire.
#[derive(Debug, Serialize, Deserialize)]
struct RelayMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Connection handler (per WebSocket)
// ---------------------------------------------------------------------------

async fn handle_ws(ws: WebSocket, sessions: Sessions) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    // Channel from the task-local logic back to the WebSocket writer.
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Spawn a writer task that drains `rx` into the WebSocket.
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Wait for the first message to determine the role.
    let first_msg = match ws_rx.next().await {
        Some(Ok(Message::Text(text))) => text.to_string(),
        _ => {
            warn!("Connection closed before sending first message");
            return;
        }
    };

    let envelope: RelayMessage = match serde_json::from_str(&first_msg) {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to parse first message: {e}");
            let _ = tx.send(serde_json::to_string(&RelayMessage {
                msg_type: "error".into(),
                session_id: None,
                msg: Some(format!("invalid message: {e}")),
                payload: None,
            }).unwrap());
            drop(tx);
            let _ = write_task.await;
            return;
        }
    };

    match envelope.msg_type.as_str() {
        // ---------------------------------------------------------------
        // Desktop client registration
        // ---------------------------------------------------------------
        "register" => {
            let session_id = Uuid::new_v4().to_string();
            info!(session_id, "Client registered");

            // Insert the session.
            {
                let mut guard = sessions.write().await;
                guard.insert(
                    session_id.clone(),
                    Session {
                        client_sender: Some(tx.clone()),
                        browser_senders: Vec::new(),
                    },
                );
            }

            // Tell the client its session id.
            let _ = tx.send(serde_json::to_string(&RelayMessage {
                msg_type: "registered".into(),
                session_id: Some(session_id.clone()),
                msg: None,
                payload: None,
            }).unwrap());

            // Read loop — forward every subsequent message to all browsers.
            while let Some(Ok(Message::Text(text))) = ws_rx.next().await {
                let text = text.to_string();
                let guard = sessions.read().await;
                if let Some(session) = guard.get(&session_id) {
                    for browser_tx in &session.browser_senders {
                        let _ = browser_tx.send(text.clone());
                    }
                }
            }

            // Client disconnected — tear down the session.
            info!(session_id, "Client disconnected, removing session");
            sessions.write().await.remove(&session_id);
        }

        // ---------------------------------------------------------------
        // Browser joins an existing session
        // ---------------------------------------------------------------
        "join" => {
            let session_id = match envelope.session_id {
                Some(id) => id,
                None => {
                    let _ = tx.send(serde_json::to_string(&RelayMessage {
                        msg_type: "error".into(),
                        session_id: None,
                        msg: Some("missing session_id".into()),
                        payload: None,
                    }).unwrap());
                    drop(tx);
                    let _ = write_task.await;
                    return;
                }
            };

            // Check that the session exists and register the browser.
            {
                let mut guard = sessions.write().await;
                match guard.get_mut(&session_id) {
                    Some(session) => {
                        session.browser_senders.push(tx.clone());
                    }
                    None => {
                        let _ = tx.send(serde_json::to_string(&RelayMessage {
                            msg_type: "error".into(),
                            session_id: Some(session_id.clone()),
                            msg: Some("session not found".into()),
                            payload: None,
                        }).unwrap());
                        drop(tx);
                        let _ = write_task.await;
                        return;
                    }
                }
            }

            info!(session_id, "Browser joined");
            let _ = tx.send(serde_json::to_string(&RelayMessage {
                msg_type: "joined".into(),
                session_id: Some(session_id.clone()),
                msg: None,
                payload: None,
            }).unwrap());

            // Read loop — forward every subsequent message to the client.
            while let Some(Ok(Message::Text(text))) = ws_rx.next().await {
                let text = text.to_string();
                let guard = sessions.read().await;
                if let Some(session) = guard.get(&session_id) {
                    if let Some(client_tx) = &session.client_sender {
                        let _ = client_tx.send(text.clone());
                    }
                }
            }

            // Browser disconnected — remove from the session.
            info!(session_id, "Browser disconnected");
            let mut guard = sessions.write().await;
            if let Some(session) = guard.get_mut(&session_id) {
                session.browser_senders.retain(|b| !b.is_closed());
                if session.browser_senders.is_empty() && session.client_sender.is_none() {
                    guard.remove(&session_id);
                }
            }
        }

        other => {
            warn!(msg_type = other, "Unknown first message type");
            let _ = tx.send(serde_json::to_string(&RelayMessage {
                msg_type: "error".into(),
                session_id: None,
                msg: Some(format!("unknown message type: {other}")),
                payload: None,
            }).unwrap());
        }
    }

    // Ensure the writer task finishes.
    drop(tx);
    let _ = write_task.await;
}

// ---------------------------------------------------------------------------
// HTTP / WebSocket upgrade
// ---------------------------------------------------------------------------

async fn ws_handler(ws: WebSocketUpgrade, State(sessions): State<Sessions>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, sessions))
}

async fn health() -> &'static str {
    "ok"
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Initialise tracing (reads RUST_LOG env var for level filter).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let sessions: Sessions = Arc::new(RwLock::new(HashMap::new()));

    // CORS — allow everything (dev-friendly).
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Try to serve static files from ../web/dist (relative to the relay crate).
    let static_service = tower_http::services::ServeDir::new("../web/dist")
        .not_found_service(tower_http::services::ServeDir::new("../web/dist"));

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health))
        .fallback_service(static_service)
        .layer(cors)
        .with_state(sessions);

    let addr = SocketAddr::from(([0, 0, 0, 0], 9280));
    info!("BetterNTE Relay listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
