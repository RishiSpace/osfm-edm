//! WebSocket transport — persistent connection to the server with reconnect logic.

use futures_util::{SinkExt, StreamExt};
use osfm_edm_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::config::AgentConfig;
use crate::transport::protocol;

/// Errors during WebSocket communication.
#[derive(Debug, thiserror::Error)]
pub enum WsError {
    #[error("WebSocket error: {0}")]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Connection closed")]
    ConnectionClosed,
}

/// Run the WebSocket connection loop with exponential backoff reconnection.
/// - `outbound_rx`: receives AgentMessages to send to the server.
/// - `inbound_tx`: sends received ServerMessages to the main loop.
pub async fn run_ws_loop(
    config: &AgentConfig,
    outbound_rx: &mut mpsc::Receiver<AgentMessage>,
    inbound_tx: mpsc::Sender<ServerMessage>,
) {
    let mut backoff_secs = 1u64;
    let max_backoff = 60u64;

    loop {
        let ws_url = format!(
            "{}/ws",
            config
                .server_url
                .replace("https://", "wss://")
                .replace("http://", "ws://")
        );

        tracing::info!(url = %ws_url, "Connecting to server WebSocket");

        match connect_and_run(&ws_url, outbound_rx, &inbound_tx).await {
            Ok(()) => {
                tracing::info!("WebSocket connection closed gracefully");
            }
            Err(e) => {
                tracing::error!(error = %e, "WebSocket connection error");
            }
        }

        // Exponential backoff before reconnecting.
        tracing::info!(backoff = backoff_secs, "Reconnecting in {} seconds", backoff_secs);
        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(max_backoff);
    }
}

async fn connect_and_run(
    ws_url: &str,
    outbound_rx: &mut mpsc::Receiver<AgentMessage>,
    inbound_tx: &mpsc::Sender<ServerMessage>,
) -> Result<(), WsError> {
    // Connect without mTLS for now (mTLS integration will be added when the
    // server WebSocket hub is fully implemented in Phase 6).
    let (ws_stream, _response) = tokio_tungstenite::connect_async(ws_url).await?;
    tracing::info!("WebSocket connected");

    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select! {
            // Receive messages from the server.
            server_msg = read.next() => {
                match server_msg {
                    Some(Ok(Message::Text(text))) => {
                        match protocol::decode_server_message(&text) {
                            Ok(msg) => {
                                if inbound_tx.send(msg).await.is_err() {
                                    tracing::error!("Failed to forward server message — receiver dropped");
                                    return Ok(());
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to parse server message");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Server closed WebSocket connection");
                        return Err(WsError::ConnectionClosed);
                    }
                    Some(Ok(Message::Ping(data))) => {
                        write.send(Message::Pong(data)).await?;
                    }
                    Some(Ok(_)) => {} // Ignore binary, pong, etc.
                    Some(Err(e)) => {
                        return Err(WsError::Tungstenite(e));
                    }
                }
            }
            // Send messages to the server.
            agent_msg = outbound_rx.recv() => {
                match agent_msg {
                    Some(msg) => {
                        let text = protocol::encode_agent_message(&msg)?;
                        write.send(Message::Text(text.into())).await?;
                    }
                    None => {
                        tracing::info!("Outbound channel closed");
                        return Ok(());
                    }
                }
            }
        }
    }
}
