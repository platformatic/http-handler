use std::{
    fmt, io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use http_body::{Body, Frame};
use tokio::{
    io::{AsyncRead, AsyncWrite, DuplexStream},
    sync::Mutex,
};

/// Error type for stream operations
#[derive(Debug, Clone)]
pub enum StreamError {
    /// The stream has been closed and cannot accept more data
    StreamClosed,
    /// The stream receiver has already been consumed and cannot be taken again
    StreamAlreadyConsumed,
    /// An I/O error occurred
    IoError(String),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::StreamClosed => write!(f, "Stream closed"),
            StreamError::StreamAlreadyConsumed => write!(f, "Stream already consumed"),
            StreamError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for StreamError {}

impl From<io::Error> for StreamError {
    fn from(err: io::Error) -> Self {
        StreamError::IoError(err.to_string())
    }
}

/// Request body with duplex stream for bidirectional I/O
///
/// This type holds both halves of a duplex stream pair. One half is used for polling
/// (by the handler), and the other half is accessible via `stream()` for external writes.
///
/// # Cloning Behavior
///
/// RequestBody is clonable, and clones share the same underlying streams via Arc<Mutex>.
/// This allows NAPI to clone Request objects while preserving the streams.
#[derive(Debug)]
pub struct RequestBody {
    // The half used for polling/reading by the handler
    read_side: Arc<Mutex<DuplexStream>>,
    // The half used by external code to write data into the body
    write_side: Arc<Mutex<DuplexStream>>,
    buffer_size: usize,
}

impl RequestBody {
    /// Create a new request body with specified buffer size
    pub fn new_with_buffer_size(buffer_size: usize) -> Self {
        let (read_side, write_side) = tokio::io::duplex(buffer_size);

        Self {
            read_side: Arc::new(Mutex::new(read_side)),
            write_side: Arc::new(Mutex::new(write_side)),
            buffer_size,
        }
    }

    /// Create a new request body with default buffer size (16KB)
    pub fn new() -> Self {
        Self::new_with_buffer_size(16384)
    }

    /// Create from buffered data (writes data to stream immediately)
    pub async fn from_data(data: Bytes) -> Result<Self, StreamError> {
        let body = Self::new();

        // Write data to the write side of the stream
        use tokio::io::AsyncWriteExt;
        let mut stream = body.write_side.lock().await;
        stream.write_all(&data).await?;
        stream.shutdown().await?;
        drop(stream);

        Ok(body)
    }

    /// Get the buffer size for this request body
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Create response body with the same buffer size
    /// Returns a new ResponseBody that uses a separate duplex stream
    pub fn create_response(&self) -> ResponseBody {
        ResponseBody::new_with_buffer_size(self.buffer_size)
    }
}

impl Default for RequestBody {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RequestBody {
    fn clone(&self) -> Self {
        Self {
            read_side: Arc::clone(&self.read_side),
            write_side: Arc::clone(&self.write_side),
            buffer_size: self.buffer_size,
        }
    }
}

impl AsyncRead for RequestBody {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut stream = match self.read_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for RequestBody {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut stream = match self.write_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut stream = match self.write_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut stream = match self.write_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_shutdown(cx)
    }
}

/// Response body with duplex stream for bidirectional I/O
///
/// This type holds both halves of a duplex stream pair and implements `http-body::Body`.
/// One half is used for polling (reading frames), and the other half is accessible via
/// `stream()` for external writes (e.g., handler writing response data).
///
/// # Cloning Behavior
///
/// ResponseBody is clonable, and clones share the same underlying streams via Arc<Mutex>.
/// This allows NAPI to clone Response objects. The `poll_frame` implementation handles
/// concurrent access via the mutex.
///
/// ## Reading Frames
/// To read frames from this body, use `BodyExt::frame()` from http-body-util.
#[derive(Debug)]
pub struct ResponseBody {
    // The half used for polling/reading frames
    read_side: Arc<Mutex<DuplexStream>>,
    // The half used by handlers to write response data
    write_side: Arc<Mutex<DuplexStream>>,
    buffer_size: usize,
}

impl ResponseBody {
    /// Create a new response body with specified buffer size
    pub fn new_with_buffer_size(buffer_size: usize) -> Self {
        let (read_side, write_side) = tokio::io::duplex(buffer_size);

        Self {
            read_side: Arc::new(Mutex::new(read_side)),
            write_side: Arc::new(Mutex::new(write_side)),
            buffer_size,
        }
    }

    /// Create a new response body with default buffer size (16KB)
    pub fn new() -> Self {
        Self::new_with_buffer_size(16384)
    }

    /// Get the buffer size for this response body
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}

impl Default for ResponseBody {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ResponseBody {
    fn clone(&self) -> Self {
        Self {
            read_side: Arc::clone(&self.read_side),
            write_side: Arc::clone(&self.write_side),
            buffer_size: self.buffer_size,
        }
    }
}

impl AsyncRead for ResponseBody {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut stream = match self.read_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for ResponseBody {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut stream = match self.write_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut stream = match self.write_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut stream = match self.write_side.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };
        Pin::new(&mut *stream).poll_shutdown(cx)
    }
}

impl Body for ResponseBody {
    type Data = Bytes;
    type Error = String;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        // Try to read data from the stream
        let mut buffer = BytesMut::with_capacity(8192);
        unsafe {
            buffer.set_len(8192);
        }

        let mut read_buf = tokio::io::ReadBuf::new(&mut buffer);
        let initial_filled = read_buf.filled().len();

        match self.as_mut().poll_read(cx, &mut read_buf) {
            Poll::Ready(Ok(())) => {
                let filled = read_buf.filled().len();
                if filled == initial_filled {
                    // EOF reached
                    Poll::Ready(None)
                } else {
                    // Data was read
                    buffer.truncate(filled);
                    Poll::Ready(Some(Ok(Frame::data(buffer.freeze()))))
                }
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e.to_string()))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Implement Stream for ResponseBody to enable async iteration in Rust
impl Stream for ResponseBody {
    type Item = Result<Bytes, String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Use poll_frame and extract data
        match self.poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Ok(data) = frame.into_data() {
                    Poll::Ready(Some(Ok(data)))
                } else {
                    // Frame was not data (e.g., trailers) - skip it
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
