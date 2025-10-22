use bytes::{Buf, Bytes};
use futures_core::Stream;
use http_body::{Body, Frame};
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// Chunk of data in a stream
#[derive(Clone, Debug)]
pub enum StreamChunk<T> {
    /// Data chunk
    Data(T),
    /// End of stream marker
    End,
}

/// Error type for stream operations
#[derive(Debug, Clone)]
pub enum StreamError {
    /// The stream has been closed and cannot accept more data
    StreamClosed,
    /// The stream receiver has already been consumed and cannot be taken again
    StreamAlreadyConsumed,
    /// An error occurred while sending data through the channel
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

/// Request body with owned request channels and lazy response channel creation
///
/// This type owns the request-side channels and creates response channels on demand.
/// It is generic over the chunk data type `T` (defaults to `Bytes`).
///
/// # Cloning Behavior
///
/// RequestBody is clonable, and clones share the same underlying receiver via Arc<Mutex>.
/// This allows NAPI to clone Request objects while preserving the receiver. Only one
/// clone can successfully call `take_request_rx()` - subsequent calls return `None`.
#[derive(Debug)]
pub struct RequestBody<T = Bytes> {
    request_tx: mpsc::Sender<StreamChunk<T>>,
    request_rx: Arc<Mutex<Option<mpsc::Receiver<StreamChunk<T>>>>>,
}

impl<T> RequestBody<T> {
    /// Create a new request body with request channels
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel(32);

        Self {
            request_tx,
            request_rx: Arc::new(Mutex::new(Some(request_rx))),
        }
    }

    /// Create from buffered data (pre-populates request channel)
    pub async fn from_data(data: T) -> Result<Self, StreamError> {
        let body = Self::new();

        body.request_tx
            .send(StreamChunk::Data(data))
            .await
            .map_err(|_| StreamError::StreamClosed)?;

        // Send End marker to signal no more data
        body.request_tx
            .send(StreamChunk::End)
            .await
            .map_err(|_| StreamError::StreamClosed)?;

        Ok(body)
    }

    /// Write a chunk to the request stream (async for backpressure)
    pub async fn write(&self, chunk: T) -> Result<(), StreamError> {
        self.request_tx
            .send(StreamChunk::Data(chunk))
            .await
            .map_err(|_| StreamError::StreamClosed)
    }

    /// End the request stream
    /// For HTTP: signals no more body data
    /// For WebSocket: signals connection should close
    pub async fn end(&self) -> Result<(), StreamError> {
        self.request_tx
            .send(StreamChunk::End)
            .await
            .map_err(|_| StreamError::StreamClosed)
    }

    /// Take the request receiver (called once by handler)
    ///
    /// Returns `None` if the receiver was already taken or if the mutex is poisoned.
    /// This is safe to call on cloned RequestBody instances - only the first call
    /// across all clones will return `Some`.
    pub fn take_request_rx(&mut self) -> Option<mpsc::Receiver<StreamChunk<T>>> {
        self.request_rx
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    /// Create response body and sender pair
    /// Returns (ResponseBody, Sender) where Sender writes to ResponseBody's receiver
    pub fn create_response(&self) -> (ResponseBody<T>, mpsc::Sender<Result<StreamChunk<T>, String>>) {
        let (response_tx, response_rx) = mpsc::channel(32);
        let response_body = ResponseBody {
            response_rx: Arc::new(Mutex::new(Some(response_rx))),
        };
        (response_body, response_tx)
    }
}

impl<T> Default for RequestBody<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for RequestBody<T> {
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
            // Clone the Arc to share the same receiver across clones
            request_rx: Arc::clone(&self.request_rx),
        }
    }
}

/// Type alias for the complex response receiver type
type ResponseReceiver<T> = Arc<Mutex<Option<mpsc::Receiver<Result<StreamChunk<T>, String>>>>>;

/// Response body with owned response receiver
///
/// This type owns the response-side receiver and implements `http-body::Body`.
/// It is generic over the chunk data type `T` (defaults to `Bytes`).
///
/// # Cloning Behavior
///
/// ResponseBody is clonable, and clones share the same underlying receiver via Arc<Mutex>.
/// This allows NAPI to clone Response objects. The `poll_frame` implementation handles
/// concurrent access via the mutex.
///
/// ## Reading Frames
/// To read frames from this body, use `BodyExt::frame()` from http-body-util.
#[derive(Debug)]
pub struct ResponseBody<T = Bytes> {
    response_rx: ResponseReceiver<T>,
}

impl<T> ResponseBody<T> {
    /// Create a new empty response body (for buffered responses with BodyBuffer extension)
    pub fn new() -> Self {
        let (_tx, rx) = mpsc::channel(32);
        Self {
            response_rx: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Take the response receiver (for direct access if needed)
    ///
    /// Returns `None` if the receiver was already taken or if the mutex is poisoned.
    pub fn take_response_rx(&mut self) -> Option<mpsc::Receiver<Result<StreamChunk<T>, String>>> {
        self.response_rx
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }
}

impl<T> Default for ResponseBody<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for ResponseBody<T> {
    fn clone(&self) -> Self {
        Self {
            response_rx: Arc::clone(&self.response_rx),
        }
    }
}

impl<T: Buf> Body for ResponseBody<T> {
    type Data = T;
    type Error = String;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        // SAFETY: We're not moving out of the pinned reference
        let this = self.get_mut();

        // Try to lock the mutex (non-blocking to avoid blocking the executor)
        let mut guard = match this.response_rx.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                // Mutex is locked or poisoned - wake task and return Pending
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // If receiver was taken, end the stream
        let receiver = match guard.as_mut() {
            Some(rx) => rx,
            None => return Poll::Ready(None),
        };

        // Poll the underlying receiver
        match Pin::new(receiver).poll_recv(cx) {
            Poll::Ready(Some(Ok(StreamChunk::Data(data)))) => Poll::Ready(Some(Ok(Frame::data(data)))),
            Poll::Ready(Some(Ok(StreamChunk::End))) => {
                // End marker received - take the receiver to signal stream end
                drop(guard.take());
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(e))) => {
                // Error received - take the receiver and return error
                drop(guard.take());
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                // Channel closed without End marker - take the receiver to signal stream end
                drop(guard.take());
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Implement Stream for ResponseBody to enable async iteration in Rust
impl<T: Buf> Stream for ResponseBody<T> {
    type Item = Result<T, String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Try to lock the mutex (non-blocking)
        let mut guard = match this.response_rx.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                // Mutex is locked or poisoned - wake task and return Pending
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // If receiver was taken, end the stream
        let receiver = match guard.as_mut() {
            Some(rx) => rx,
            None => return Poll::Ready(None),
        };

        // Poll the underlying receiver
        match Pin::new(receiver).poll_recv(cx) {
            Poll::Ready(Some(Ok(StreamChunk::Data(data)))) => Poll::Ready(Some(Ok(data))),
            Poll::Ready(Some(Ok(StreamChunk::End))) => {
                // End marker received - take the receiver to signal stream end
                drop(guard.take());
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(e))) => {
                // Error received - take the receiver and return error
                drop(guard.take());
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                // Channel closed without End marker - take the receiver to signal stream end
                drop(guard.take());
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

