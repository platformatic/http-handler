//! This module contains the rewritten implementation using the http crate
//! instead of custom types. It will be lifted to the top level once complete.

#![warn(clippy::dbg_macro, clippy::print_stdout)]
#![warn(missing_docs)]

// Re-export everything from http crate
pub use http::*;

/// Body types for HTTP requests and responses with streaming support
pub mod body;
pub mod extensions;
pub mod handler;
pub mod types;

/// WebSocket frame codec for RFC 6455 compliant framing
pub mod websocket;

/// Provides N-API bindings to expose the `http` crate types to Node.js.
#[cfg(feature = "napi-support")]
pub mod napi;

pub use body::{RequestBody, ResponseBody, StreamError};
pub use extensions::{
    BodyBuffer, RequestBuilderExt, RequestExt, ResponseBuilderExt, ResponseException, ResponseExt,
    ResponseLog, SocketInfo, WebSocketMode,
};
pub use handler::Handler;
pub use types::{Request, Response};
