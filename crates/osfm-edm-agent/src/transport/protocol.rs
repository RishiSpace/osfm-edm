//! Protocol helpers — serialize and deserialize WebSocket messages.

use osfm_edm_common::protocol::{AgentMessage, ServerMessage};

/// Serialize an AgentMessage to a JSON string for sending over WebSocket.
pub fn encode_agent_message(msg: &AgentMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(msg)
}

/// Deserialize a ServerMessage from a JSON string received over WebSocket.
pub fn decode_server_message(text: &str) -> Result<ServerMessage, serde_json::Error> {
    serde_json::from_str(text)
}
