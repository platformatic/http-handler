//! StreamHandle - Bidirectional streaming body type for HTTP requests and responses

use bytes::{Bytes, BytesMut};
use std::fmt;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{mpsc, Mutex};

/// Chunk of data in a stream
#[derive(Clone, Debug)]
pub enum StreamChunk {
    /// Data chunk
    Data(Bytes),
    /// End of stream marker
    End,
}

/// Error type for StreamHandle operations
#[derive(Debug, Clone)]
pub enum StreamError {
    /// Stream has been closed
    StreamClosed,
    /// Stream receiver has already been consumed
    StreamAlreadyConsumed,
    /// Error sending data
    SendError(String),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::StreamClosed => write!(f, "Stream closed"),
            StreamError::StreamAlreadyConsumed => write!(f, "Stream already consumed"),
            StreamError::SendError(msg) => write!(f, "Send error: {}", msg),
        }
    }
}

impl std::error::Error for StreamError {}

/// Handle for bidirectional streaming between Node.js, Rust handler, and backend (Python/PHP)
///
/// This is the body type for `http::Request<T>` and `http::Response<T>`.
///
/// # Architecture
///
/// Contains two channel pairs:
/// - **Request channel**: Node.js writes → Handler reads → forwards to Python/PHP
/// - **Response channel**: Handler writes from Python/PHP → Node.js reads
///
/// # Buffering Mode
///
/// When `to_bytes()` is called, all chunks are buffered into the `buffer` field.
/// Subsequent reads return chunks from the buffer.
/// This is used by `handleRequest()` to provide synchronous body access.
///
/// # WebSocket Mode
///
/// When `websocket` is true, each `write()` call represents a complete WebSocket message.
/// No WebSocket framing is performed - the application handles message boundaries.
pub struct StreamHandle {
    // Request body channel (Node.js → Handler → Python)
    /// Sender for Node.js to write request body chunks (Clone to share across tasks)
    request_tx: mpsc::Sender<StreamChunk>,
    /// Receiver for handler to read request body chunks (taken once by handler)
    /// Wrapped in Arc so clones share the same receiver
    request_rx: Arc<Mutex<Option<mpsc::Receiver<StreamChunk>>>>,

    // Response body channel (Python → Handler → Node.js)
    /// Sender for handler to write response body chunks from Python (Clone to share across tasks)
    response_tx: mpsc::Sender<Result<Bytes, String>>,
    /// Receiver for Node.js to read response body chunks
    /// Wrapped in Arc so clones share the same receiver
    response_rx: Arc<Mutex<mpsc::Receiver<Result<Bytes, String>>>>,

    /// Buffered response body (populated by to_bytes())
    /// Uses StdMutex for sync access in NAPI body getter
    /// Wrapped in Arc so clones share the same buffer
    buffer: Arc<StdMutex<Option<Bytes>>>,

    /// WebSocket mode flag (immutable after construction)
    websocket: bool,
}

impl StreamHandle {
    /// Create a new streaming handle with both channel pairs
    ///
    /// # Arguments
    ///
    /// * `websocket` - Whether this is a WebSocket connection
    ///
    /// # Examples
    ///
    /// ```
    /// use http_handler::StreamHandle;
    ///
    /// let handle = StreamHandle::new(false); // HTTP mode
    /// let ws_handle = StreamHandle::new(true); // WebSocket mode
    /// ```
    pub fn new(websocket: bool) -> Self {
        let (request_tx, request_rx) = mpsc::channel(32);
        let (response_tx, response_rx) = mpsc::channel(32);

        Self {
            request_tx,
            request_rx: Arc::new(Mutex::new(Some(request_rx))),
            response_tx,
            response_rx: Arc::new(Mutex::new(response_rx)),
            buffer: Arc::new(StdMutex::new(None)),
            websocket,
        }
    }

    /// Send End marker to indicate empty request body
    ///
    /// This should be called for requests without a body to signal that
    /// no data will be sent through the request channel.
    pub fn send_empty_body(&self) {
        let tx = self.request_tx.clone();
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let _ = tx.send(StreamChunk::End).await;
            });
        } else {
            let _ = tx.try_send(StreamChunk::End);
        }
    }

    /// Create a handle from a buffered body (for constructing requests with bodies)
    ///
    /// Pre-populates the request channel with the body data and an End marker.
    ///
    /// # Arguments
    ///
    /// * `body` - The body bytes to send
    /// * `websocket` - Whether this is a WebSocket connection
    ///
    /// # Examples
    ///
    /// ```
    /// use http_handler::StreamHandle;
    /// use bytes::Bytes;
    ///
    /// let body = Bytes::from("Hello, world!");
    /// let handle = StreamHandle::from_bytes(body, false);
    /// ```
    pub fn from_bytes(body: Bytes, websocket: bool) -> Self {
        let handle = Self::new(websocket);

        // Clone the sender to move into spawned task (or use directly if no runtime)
        let tx = handle.request_tx.clone();

        // Try to spawn in tokio runtime if available, otherwise send synchronously
        if tokio::runtime::Handle::try_current().is_ok() {
            // We're in a tokio runtime context, can spawn
            tokio::spawn(async move {
                let _ = tx.send(StreamChunk::Data(body)).await;
                let _ = tx.send(StreamChunk::End).await;
            });
        } else {
            // No tokio runtime, use synchronous send
            let _ = tx.try_send(StreamChunk::Data(body.clone()));
            let _ = tx.try_send(StreamChunk::End);
        }

        handle
    }

    /// Write a chunk to the request stream (called by Node.js)
    ///
    /// In WebSocket mode, each write is a complete message.
    /// In HTTP mode, chunks are concatenated into the request body.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::StreamClosed` if the receiver has been dropped.
    pub async fn write(&self, chunk: impl Into<Bytes>) -> Result<(), StreamError> {
        self.request_tx
            .send(StreamChunk::Data(chunk.into()))
            .await
            .map_err(|_| StreamError::StreamClosed)
    }

    /// End the request stream (called by Node.js, HTTP mode only)
    ///
    /// In WebSocket mode, this is a no-op. Use close messages instead.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::StreamClosed` if the receiver has been dropped.
    pub async fn end(&self) -> Result<(), StreamError> {
        if !self.websocket {
            self.request_tx
                .send(StreamChunk::End)
                .await
                .map_err(|_| StreamError::StreamClosed)?;
        }
        Ok(())
    }

    /// Read the next chunk from the response stream (called by Node.js AsyncIterator)
    ///
    /// If buffer is populated, reads from buffer. Otherwise reads from channel.
    ///
    /// # Returns
    ///
    /// - `Some(Ok(bytes))` - Next chunk of data
    /// - `Some(Err(error))` - Error from handler
    /// - `None` - End of stream
    pub async fn read(&self) -> Option<Result<Bytes, String>> {
        // Check if we have a buffer
        {
            let buffer = self.buffer.lock().unwrap();
            if let Some(bytes) = buffer.as_ref() {
                // Return buffered bytes once, then None
                if !bytes.is_empty() {
                    let result = Some(Ok(bytes.clone()));
                    drop(buffer);
                    // Clear buffer after first read
                    *self.buffer.lock().unwrap() = Some(Bytes::new());
                    return result;
                } else {
                    return None;
                }
            }
        }
        // Guard is dropped here

        // No buffer, read from channel
        self.response_rx.lock().await.recv().await
    }

    /// Check if this is a WebSocket stream
    pub fn is_websocket(&self) -> bool {
        self.websocket
    }

    /// Check if response has been buffered
    ///
    /// Uses try_lock to avoid async context requirement.
    pub fn is_buffered(&self) -> bool {
        let result = self.buffer
            .try_lock()
            .map(|guard| {
                let has_buffer = guard.is_some();
                has_buffer
            })
            .unwrap_or_else(|_| {
                false
            });
        result
    }

    /// Read all response chunks and buffer them (used by handleRequest)
    ///
    /// This consumes the stream and stores result in buffer field.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::SendError` if any chunk contains an error.
    pub async fn to_bytes(&self) -> Result<Bytes, StreamError> {
        let mut buf = BytesMut::new();

        let mut rx = self.response_rx.lock().await;

        while let Some(result) = rx.recv().await {
            let chunk = result.map_err(StreamError::SendError)?;
            buf.extend_from_slice(&chunk);
        }
        drop(rx);

        let bytes = buf.freeze();
        *self.buffer.lock().unwrap() = Some(bytes.clone());

        Ok(bytes)
    }

    /// Get buffered bytes (returns None if not buffered)
    pub async fn buffered_bytes(&self) -> Option<Bytes> {
        self.buffer.lock().unwrap().clone()
    }

    /// Get buffered bytes synchronously (returns None if not buffered)
    ///
    /// This is a synchronous version of `buffered_bytes()` that can be called
    /// from NAPI getters without requiring a tokio runtime.
    pub fn buffered_bytes_sync(&self) -> Option<Bytes> {
        self.buffer.lock().unwrap().clone()
    }

    /// Take the request receiver (called once by handler to read request body)
    ///
    /// Returns `None` if the receiver has already been taken.
    pub async fn take_request_rx(&self) -> Option<mpsc::Receiver<StreamChunk>> {
        self.request_rx.lock().await.take()
    }

    /// Get a clone of the response sender (called by handler to write response body)
    pub fn response_tx(&self) -> mpsc::Sender<Result<Bytes, String>> {
        self.response_tx.clone()
    }

    /// Create a receiver-only StreamHandle (for responses where sender is external)
    ///
    /// This creates a StreamHandle that only receives data, with the sender
    /// being owned elsewhere. This ensures the channel closes when all external
    /// senders are dropped.
    pub fn receiver_only(response_rx: mpsc::Receiver<Result<Bytes, String>>, websocket: bool) -> Self {
        let (request_tx, _request_rx) = mpsc::channel(32);
        let (response_tx, _dummy_rx) = mpsc::channel(32);

        Self {
            request_tx,
            request_rx: Arc::new(Mutex::new(None)),
            response_tx,  // Dummy sender, never used
            response_rx: Arc::new(Mutex::new(response_rx)),
            buffer: Arc::new(StdMutex::new(None)),
            websocket,
        }
    }
}

impl fmt::Debug for StreamHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamHandle")
            .field("websocket", &self.websocket)
            .field("is_buffered", &self.is_buffered())
            .finish()
    }
}

impl Clone for StreamHandle {
    /// Clone creates a new StreamHandle that shares receivers with the original
    ///
    /// Cloning properly shares the channels via Arc:
    /// - Senders are cloned (they are already Cloneable)
    /// - Receivers are shared via Arc (all clones access the same receivers)
    /// - Buffer is shared via Arc (all clones see the same buffered data)
    ///
    /// This ensures that when a Request is cloned in FromNapiValue, the data
    /// written to the request channels is accessible through the cloned handle.
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
            request_rx: Arc::clone(&self.request_rx),
            response_tx: self.response_tx.clone(),
            response_rx: Arc::clone(&self.response_rx),
            buffer: Arc::clone(&self.buffer),
            websocket: self.websocket,
        }
    }
}
