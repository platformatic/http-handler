//! WebSocket frame codec implementation conforming to RFC 6455.
//!
//! This module provides WebSocket frame parsing, encoding, and message assembly
//! for bidirectional WebSocket communication using tokio_util::codec.

mod codec;
mod frame;
mod wrapper;

pub use codec::WebSocketCodec;
pub use frame::{WebSocketError, WebSocketFrame, WebSocketOpcode};
pub use wrapper::{WebSocketDecoder, WebSocketEncoder};
