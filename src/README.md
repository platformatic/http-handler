# http-handler Source Code Documentation

## Overview

The `http-handler` crate is a request handling framework built on top of the standard Rust [`http`](https://docs.rs/http) crate. It provides a trait-based architecture for processing HTTP requests and generating responses, with support for streaming bodies, WebSocket communication, and optional Node.js integration via NAPI.

### Key Features

- **Standards-Based**: Built on the standard `http` crate types rather than custom implementations
- **Streaming Bodies**: Efficient bidirectional I/O using tokio duplex streams
- **Extension System**: Attach metadata (socket info, logs, exceptions) to requests/responses
- **WebSocket Support**: RFC 6455 compliant WebSocket frame parsing and encoding
- **Node.js Integration**: Optional NAPI bindings for JavaScript interoperability
- **Async-First**: Fully async using tokio runtime

## Architecture

### Core Design Principles

1. **Separation of Concerns**: Request and response bodies are distinct types with clear ownership semantics
2. **Zero-Copy Streaming**: Data flows through duplex streams without intermediate buffering
3. **Type Safety**: Generic over chunk data type, not body type
4. **Standards Compliance**: Implements `http-body::Body` trait for ecosystem compatibility
5. **No Synchronization Overhead**: Channels are moved, not shared via Arc/Mutex (except for body internals)

### Module Structure

```
src/
├── lib.rs              # Public API and re-exports
├── types.rs            # Type aliases (Request, Response)
├── handler.rs          # Handler trait definition
├── body.rs             # RequestBody and ResponseBody implementations
├── extensions.rs       # Extension types and traits
├── napi.rs            # Node.js NAPI bindings (feature-gated)
├── websocket/         # WebSocket frame codec
│   ├── mod.rs
│   ├── frame.rs       # Frame structure and opcodes
│   ├── codec.rs       # Tokio codec implementation
│   └── wrapper.rs     # Encoder/Decoder wrappers
└── test.rs            # Testing utilities (MockRoot)
```

## Key Components

### 1. Handler Trait (`handler.rs`)

The `Handler` trait is the core abstraction for request processing:

```rust
pub trait Handler {
    type Error;

    async fn handle(
        &self,
        request: http::Request<RequestBody>,
    ) -> Result<http::Response<ResponseBody>, Self::Error>;
}
```

**Key Characteristics:**

- Async by design (uses `async_fn_in_trait`)
- Returns `Result` allowing custom error types
- Works with streaming request and response bodies
- Can be composed using middleware patterns

**Implementation Pattern:**

```rust
impl Handler for MyHandler {
    type Error = std::io::Error;

    async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
        let (_parts, body) = request.into_parts();
        let response_body = body.create_response();

        // Spawn task to write response
        let mut writer = response_body.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let _ = writer.write_all(b"Hello!").await;
            let _ = writer.shutdown().await;
        });

        Ok(http::Response::builder()
            .status(200)
            .body(response_body)
            .unwrap())
    }
}
```

### 2. Body Types (`body.rs`)

#### RequestBody

Duplex stream for incoming request data:

```rust
pub struct RequestBody {
    read_side: Arc<Mutex<DuplexStream>>,   // Handler reads from here
    write_side: Arc<Mutex<DuplexStream>>,  // External writes go here
    buffer_size: usize,
}
```

**Key Features:**

- **Bidirectional**: Implements both `AsyncRead` and `AsyncWrite`
- **Clonable**: Shares underlying streams via Arc<Mutex>
- **Backpressure**: Configurable buffer size (default 16KB)
- **Factory Method**: `create_response()` creates a paired ResponseBody

**Important Methods:**

- `new()` / `new_with_buffer_size(size)` - Create new body
- `from_data(bytes)` - Create from buffered data (async)
- `create_response()` - Create paired response body
- Implements `AsyncRead` and `AsyncWrite` traits

#### ResponseBody

Duplex stream for outgoing response data:

```rust
pub struct ResponseBody {
    read_side: Arc<Mutex<DuplexStream>>,   // Body trait polls here
    write_side: Arc<Mutex<DuplexStream>>,  // Handler writes here
    buffer_size: usize,
}
```

**Key Features:**

- **http-body Integration**: Implements `http_body::Body` trait
- **Stream Protocol**: Implements `futures_core::Stream` for iteration
- **Clonable**: Multiple references can write to the same body
- **Frame-Based**: Returns `Frame<Bytes>` chunks when polled

**Important Traits:**

- `Body` - Enables use with http ecosystem (hyper, etc.)
- `Stream` - Enables async iteration with `while let Some(...) = stream.next().await`
- `AsyncRead` / `AsyncWrite` - Direct I/O access

### 3. Extensions System (`extensions.rs`)

Extensions allow attaching metadata to requests and responses using the standard `http::Extensions` API.

#### SocketInfo

Stores local and remote socket addresses:

```rust
pub struct SocketInfo {
    pub local: Option<SocketAddr>,
    pub remote: Option<SocketAddr>,
}
```

**Usage:**

```rust
use http_handler::{RequestExt, SocketInfo};

let mut request = http::Request::new(body);
request.set_socket_info(SocketInfo::new(Some(local_addr), Some(remote_addr)));

// Later access
if let Some(info) = request.socket_info() {
    println!("Remote: {:?}", info.remote);
}
```

#### ResponseLog

Accumulates log messages during request processing:

```rust
pub struct ResponseLog {
    buffer: BytesMut,
}
```

**Key Behavior:**

- Automatically appends newline to each log entry
- Thread-safe when accessed via ResponseExt trait
- Can be converted to `Bytes` for transmission

**Usage:**

```rust
use http_handler::ResponseExt;

response.append_log("Processing started");
response.append_log("Database query completed");

// Retrieve logs
if let Some(log) = response.log() {
    println!("Logs: {}", String::from_utf8_lossy(log.as_bytes()));
}
```

#### ResponseException

Stores exception/error information in responses:

```rust
pub struct ResponseException(pub String);
```

**Usage:**

```rust
response.set_exception("Database connection failed");

if let Some(exc) = response.exception() {
    eprintln!("Error: {}", exc.message());
}
```

#### WebSocketMode

Marker type indicating WebSocket protocol:

```rust
pub struct WebSocketMode;
```

**Usage:**

```rust
// Enable WebSocket mode
request.extensions_mut().insert(WebSocketMode);

// Check if enabled
let is_websocket = request.extensions().get::<WebSocketMode>().is_some();
```

#### BodyBuffer

Utility for accumulating response chunks before creating sync response:

```rust
pub struct BodyBuffer {
    buffer: BytesMut,
}
```

**Usage with ResponseBuilderExt:**

```rust
use http_handler::ResponseBuilderExt;

let mut builder = http::Response::builder();
builder.append_body(b"Hello ");
builder.append_body(b"World");

let body_buffer = builder.body_buffer_mut().clone();
let bytes = body_buffer.into_bytes(); // Get accumulated data
```

### 4. Extension Traits

#### RequestExt

Provides convenient access to request extensions:

- `socket_info()` / `socket_info_mut()` / `set_socket_info()`
- `document_root()` / `document_root_mut()` / `set_document_root()`

#### ResponseExt

Provides convenient access to response extensions:

- `log()` / `log_mut()` / `set_log()` / `append_log()`
- `exception()` / `set_exception()`

#### RequestBuilderExt / ResponseBuilderExt

Provides fluent builder API for extensions:

```rust
let request = http::Request::builder()
    .uri("/api")
    .socket_info(socket_info)
    .body(request_body)?;

let response = http::Response::builder()
    .status(200)
    .log("Request processed successfully")
    .exception("Warning: partial results")
    .body(response_body)?;
```

### 5. WebSocket Support (`websocket/`)

RFC 6455 compliant WebSocket frame handling.

#### WebSocketOpcode

Represents frame types:

```rust
pub enum WebSocketOpcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}
```

**Utility Methods:**

- `is_control()` - Returns true for Close, Ping, Pong
- `is_data()` - Returns true for Text, Binary, Continuation

#### WebSocketFrame

Complete frame structure:

```rust
pub struct WebSocketFrame {
    pub fin: bool,           // Final fragment flag
    pub rsv1: bool,          // Reserved bit 1
    pub rsv2: bool,          // Reserved bit 2
    pub rsv3: bool,          // Reserved bit 3
    pub opcode: WebSocketOpcode,
    pub masked: bool,        // Masking flag
    pub payload: Vec<u8>,    // Frame payload
}
```

#### WebSocketCodec

Tokio codec for framing WebSocket messages:

```rust
pub struct WebSocketCodec;
```

**Integration:**

Used with `tokio_util::codec::Framed` to decode/encode WebSocket frames from/to byte streams.

#### WebSocketEncoder / WebSocketDecoder

Higher-level wrappers for encoding and decoding:

- `WebSocketEncoder` - Encodes Bytes into WebSocket frames
- `WebSocketDecoder` - Decodes WebSocket frames into Bytes

### 6. Type Aliases (`types.rs`)

Convenience aliases for common usage:

```rust
pub type Request = http::Request<RequestBody>;
pub type Response = http::Response<ResponseBody>;
```

**Helper Functions:**

```rust
// Build request with socket info
pub mod request {
    pub fn with_socket_info(
        request: Request,
        local: Option<SocketAddr>,
        remote: Option<SocketAddr>,
    ) -> Request;
}

// Build response with extensions
pub mod response {
    pub fn with_log(response: Response, log: impl Into<Bytes>) -> Response;
    pub fn with_exception(response: Response, exception: impl Into<String>) -> Response;
}
```

### 7. Node.js Integration (`napi.rs`)

When built with `--features napi-support`, provides JavaScript bindings.

#### Key NAPI Types

**HeaderMap:**

```rust
#[napi(transparent)]
pub struct HeaderMap(pub HashMap<String, HeaderMapValue>);

pub type HeaderMapValue = Either<String, Vec<String>>;
```

Converts between JavaScript objects and http::HeaderMap, supporting multi-value headers.

**SocketInfo:**

```rust
#[napi(object)]
pub struct SocketInfo {
    pub local_address: String,
    pub local_port: u16,
    pub local_family: String,  // "IPv4" or "IPv6"
    pub remote_address: String,
    pub remote_port: u16,
    pub remote_family: String,
}
```

Node.js-friendly representation of socket information.

**Integration Pattern:**

The NAPI module provides conversions between Rust types and JavaScript objects, allowing Node.js code to construct requests, invoke handlers, and consume responses.

### 8. Testing Utilities (`test.rs`)

#### MockRoot / MockRootBuilder

Create temporary file systems for testing file-serving handlers:

```rust
use http_handler::MockRoot;

let mock_root = MockRoot::builder()
    .file("index.html", "<html>...</html>")
    .file("styles/main.css", "body { ... }")
    .build()
    .unwrap();

// Use mock_root.deref() to get PathBuf
```

**Key Features:**

- Automatically creates directory structure
- Files created in temp directory
- Deref to PathBuf for easy path manipulation

## Request/Response Flow

### Standard HTTP Flow

```
1. External Source (Node.js, HTTP client)
   └─> Creates RequestBody, writes data via AsyncWrite

2. RequestBody
   └─> Data flows through duplex stream

3. Handler
   ├─> Reads from RequestBody via AsyncRead
   ├─> Creates ResponseBody via body.create_response()
   └─> Writes to ResponseBody via AsyncWrite

4. ResponseBody
   ├─> Implements Body trait
   └─> Data polled via poll_frame()

5. External Consumer
   └─> Reads response frames via Body/Stream trait
```

### Channel Ownership Model

**Key Insight:** Channels are **moved**, not shared.

```
RequestBody Creation:
  ┌─────────────────────────────────────┐
  │ let (read, write) = duplex(16384)  │
  ├─────────────────────────────────────┤
  │ read_side: Arc<Mutex<read>>        │  ← Handler reads
  │ write_side: Arc<Mutex<write>>      │  ← External writes
  └─────────────────────────────────────┘

ResponseBody Creation (via create_response()):
  ┌─────────────────────────────────────┐
  │ let (read, write) = duplex(size)   │
  ├─────────────────────────────────────┤
  │ read_side: Arc<Mutex<read>>        │  ← poll_frame reads
  │ write_side: Arc<Mutex<write>>      │  ← Handler writes
  └─────────────────────────────────────┘
```

## Important Implementation Details

### 1. Body Cloning Behavior

Both `RequestBody` and `ResponseBody` are clonable:

```rust
impl Clone for RequestBody {
    fn clone(&self) -> Self {
        Self {
            read_side: Arc::clone(&self.read_side),
            write_side: Arc::clone(&self.write_side),
            buffer_size: self.buffer_size,
        }
    }
}
```

**Implications:**

- Clones share the same underlying streams
- Multiple references can write concurrently
- Required for NAPI integration (JS can clone Request objects)
- Handler typically spawns tasks that move cloned write handles

### 2. Backpressure and Buffer Sizing

Default buffer size is 16KB, configurable via `new_with_buffer_size()`.

**How Backpressure Works:**

- Duplex streams have bounded buffers
- When buffer fills, write operations block (async)
- This creates natural backpressure through the system
- Consumer must read to unblock producer

**Tuning Considerations:**

- Larger buffers: Better throughput, higher memory usage
- Smaller buffers: Lower latency, tighter backpressure
- WebSocket messages: Consider maximum message size

### 3. Async Write Pattern for Handlers

**Common Pattern:**

```rust
async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
    let (_parts, body) = request.into_parts();
    let response_body = body.create_response();

    // Clone the write handle
    let mut writer = response_body.clone();

    // Spawn task to write asynchronously
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        // Write data
        let _ = writer.write_all(b"data").await;
        // CRITICAL: Always shutdown when done
        let _ = writer.shutdown().await;
    });

    // Return response immediately (streaming)
    Ok(http::Response::builder()
        .status(200)
        .body(response_body)
        .unwrap())
}
```

**Why Spawn a Task?**

- Handler returns immediately with streaming body
- Body writes happen asynchronously
- Enables true streaming responses
- Consumer can start reading while handler still writing

**Critical: Always Shutdown**

Calling `shutdown()` is essential to signal EOF:

```rust
writer.shutdown().await?;
```

Without shutdown, consumers hang waiting for more data.

### 4. HTTP vs WebSocket Mode

**HTTP Mode (Default):**

- Data flows as arbitrary chunks
- No message boundaries
- Consumer sees stream of bytes

**WebSocket Mode:**

Enabled by inserting `WebSocketMode` extension:

```rust
request.extensions_mut().insert(WebSocketMode);
```

**Behavior Changes:**

- Each write represents complete WebSocket message
- Message boundaries preserved
- No explicit end marker (close frames handled at app level)
- Used with WebSocketCodec for frame encoding/decoding

### 5. Extension Lifetime and Access

Extensions are stored in `http::Extensions` (type-map):

```rust
// Set extension
request.extensions_mut().insert(SocketInfo::new(...));

// Get extension (immutable)
if let Some(info) = request.extensions().get::<SocketInfo>() {
    // ...
}

// Get mutable extension
if let Some(info) = request.extensions_mut().get_mut::<SocketInfo>() {
    info.local = Some(new_addr);
}
```

**Extension Traits Simplify This:**

```rust
// Instead of:
request.extensions_mut().insert(SocketInfo::new(...));

// Use:
request.set_socket_info(SocketInfo::new(...));
```

### 6. Error Handling Patterns

**Handler Errors:**

```rust
impl Handler for MyHandler {
    type Error = MyError;  // Custom error type

    async fn handle(&self, req: Request) -> Result<Response, Self::Error> {
        // Return error via Result
        if some_condition {
            return Err(MyError::BadRequest);
        }
        // ...
    }
}
```

**Response Exceptions:**

For errors that still produce valid HTTP responses:

```rust
let mut response = http::Response::builder()
    .status(500)
    .body(response_body)?;

response.set_exception("Database connection failed");
```

**Distinction:**

- Handler errors: Processing failed, no response generated
- Response exceptions: Response generated but contains error condition

## Edge Cases and Gotchas

### 1. Body Not Shutdown

**Problem:**

```rust
// BAD: Missing shutdown
let mut writer = response_body.clone();
tokio::spawn(async move {
    writer.write_all(b"data").await;
    // Missing: writer.shutdown().await;
});
```

**Symptom:** Consumer hangs indefinitely waiting for EOF.

**Solution:** Always call `shutdown()` after final write.

### 2. Reading Request Body Multiple Times

**Problem:**

```rust
// BAD: Body streams are single-use
let (_parts, mut body) = request.into_parts();
let bytes1 = read_all(&mut body).await; // Consumes stream
let bytes2 = read_all(&mut body).await; // Gets nothing!
```

**Solution:** Read once, store result if needed multiple times.

### 3. Dropping Write Handle Too Early

**Problem:**

```rust
// BAD: Writer dropped before data written
{
    let mut writer = response_body.clone();
    tokio::spawn(async move {
        writer.write_all(b"data").await;
    });
} // writer dropped here, but task may not have started!
```

**Solution:** Move writer into spawned task, ensure task completion.

### 4. Concurrent Writes Without Coordination

**Problem:**

```rust
// CAREFUL: Multiple writers can interleave
let writer1 = response_body.clone();
let writer2 = response_body.clone();

tokio::spawn(async move { writer1.write_all(b"AAA").await; });
tokio::spawn(async move { writer2.write_all(b"BBB").await; });

// Output might be: "AAABBB" or "BBBAAA" or "ABABAB"
```

**Solution:** Use single writer, or coordinate with channels/mutexes.

### 5. Buffer Size and Large Messages

**Problem:**

```rust
// Buffer too small for large writes
let body = RequestBody::new_with_buffer_size(64); // Only 64 bytes!
writer.write_all(&large_data).await; // May block for very long time
```

**Solution:** Choose buffer size appropriate for workload. Default 16KB handles most cases.

### 6. Extension Type Mismatches

**Problem:**

```rust
// Set as one type
response.extensions_mut().insert("error string");

// Try to get as different type
let exc = response.exception(); // Returns None!
```

**Solution:** Use typed extension methods (ResponseExt, RequestExt) consistently.

### 7. NAPI Cloning Semantics

When using NAPI bindings, JavaScript can clone objects:

```javascript
const req1 = createRequest(...);
const req2 = req1; // JavaScript reference, but...
```

**In Rust:** Both `req1` and `req2` share underlying streams (Arc<Mutex>).

**Implication:** Writes to either affect the same body.

## Performance Considerations

### 1. Buffer Sizing

- **Default (16KB):** Good for most HTTP workloads
- **Large files:** Consider 64KB+ buffers
- **Low latency:** Consider 4KB buffers
- **WebSocket:** Size to largest expected message

### 2. Spawning Tasks

Spawning tasks has overhead:

```rust
// For simple transformations, inline may be better:
let response_body = body.create_response();

// Inline (lower latency, less overhead)
use tokio::io::AsyncWriteExt;
let _ = response_body.clone().write_all(b"small response").await;
let _ = response_body.clone().shutdown().await;

// Spawned (better for long-running or blocking operations)
tokio::spawn(async move {
    // Complex processing...
});
```

### 3. Extension Access

Extension access is cheap (type-map lookup), but not free:

```rust
// Avoid in tight loops:
for chunk in chunks {
    let info = request.socket_info(); // Repeated lookups
    process(chunk, info);
}

// Better:
let info = request.socket_info();
for chunk in chunks {
    process(chunk, info);
}
```

### 4. Body Cloning

Cloning bodies is cheap (Arc clone), but creates shared state:

```rust
let body1 = request_body.clone(); // Arc clone, cheap
let body2 = request_body.clone();
```

**Mutex Contention:** Multiple simultaneous reads/writes contend on Arc<Mutex>.

## Testing Strategies

### 1. Testing Handlers

```rust
#[tokio::test]
async fn test_my_handler() {
    let handler = MyHandler;

    // Create test request
    let body = RequestBody::from_data(Bytes::from("test input"))
        .await
        .unwrap();
    let request = http::Request::builder()
        .uri("/test")
        .body(body)
        .unwrap();

    // Handle request
    let response = handler.handle(request).await.unwrap();

    // Assert status
    assert_eq!(response.status(), 200);

    // Read response body
    use http_body_util::BodyExt;
    let (_parts, body) = response.into_parts();
    let collected = body.collect().await.unwrap().to_bytes();
    assert_eq!(&collected[..], b"expected output");
}
```

### 2. Testing with Extensions

```rust
#[test]
fn test_socket_info_extension() {
    let mut request = http::Request::new(RequestBody::new());

    let local = SocketAddr::from(([127, 0, 0, 1], 8080));
    let remote = SocketAddr::from(([192, 168, 1, 1], 5000));

    request.set_socket_info(SocketInfo::new(Some(local), Some(remote)));

    let info = request.socket_info().unwrap();
    assert_eq!(info.local, Some(local));
    assert_eq!(info.remote, Some(remote));
}
```

### 3. Testing Streaming Bodies

```rust
#[tokio::test]
async fn test_streaming_response() {
    let request_body = RequestBody::new();
    let response_body = request_body.create_response();

    // Spawn writer
    let mut writer = response_body.clone();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        writer.write_all(b"chunk1").await.unwrap();
        writer.write_all(b"chunk2").await.unwrap();
        writer.shutdown().await.unwrap();
    });

    // Read frames
    use http_body_util::BodyExt;
    let mut collected = BytesMut::new();
    let mut body = response_body;
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame.unwrap().into_data() {
            collected.extend_from_slice(&data);
        }
    }

    assert_eq!(&collected[..], b"chunk1chunk2");
}
```

### 4. Testing with MockRoot

```rust
#[test]
fn test_file_serving() {
    let mock = MockRoot::builder()
        .file("index.html", "<html>Test</html>")
        .file("data/info.json", r#"{"key": "value"}"#)
        .build()
        .unwrap();

    // Use mock.as_ref() to get Path
    let index_path = mock.join("index.html");
    assert!(index_path.exists());
}
```

## Common Patterns

### 1. Echo Handler

```rust
struct EchoHandler;

impl Handler for EchoHandler {
    type Error = std::io::Error;

    async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
        let (_parts, mut body) = request.into_parts();
        let response_body = body.create_response();

        let mut writer = response_body.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buffer = vec![0u8; 8192];
            loop {
                let n = body.read(&mut buffer).await.unwrap_or(0);
                if n == 0 { break; }
                let _ = writer.write_all(&buffer[..n]).await;
            }
            let _ = writer.shutdown().await;
        });

        Ok(http::Response::builder()
            .status(200)
            .body(response_body)?)
    }
}
```

### 2. Logging Middleware

```rust
struct LoggingHandler<H> {
    inner: H,
}

impl<H: Handler> Handler for LoggingHandler<H> {
    type Error = H::Error;

    async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
        let method = request.method().clone();
        let uri = request.uri().clone();

        let mut response = self.inner.handle(request).await?;
        response.append_log(format!("{} {} -> {}", method, uri, response.status()));

        Ok(response)
    }
}
```

### 3. Static Response

```rust
async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
    let (_parts, body) = request.into_parts();
    let response_body = body.create_response();

    let mut writer = response_body.clone();
    let content = b"Static content here";
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let _ = writer.write_all(content).await;
        let _ = writer.shutdown().await;
    });

    Ok(http::Response::builder()
        .status(200)
        .header("Content-Type", "text/plain")
        .header("Content-Length", content.len())
        .body(response_body)?)
}
```

### 4. Error Handling with Exceptions

```rust
async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
    let (_parts, body) = request.into_parts();
    let response_body = body.create_response();

    match process_request(&request).await {
        Ok(data) => {
            let mut writer = response_body.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = writer.write_all(&data).await;
                let _ = writer.shutdown().await;
            });

            Ok(http::Response::builder()
                .status(200)
                .body(response_body)?)
        }
        Err(e) => {
            let mut writer = response_body.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = writer.write_all(b"Internal Server Error").await;
                let _ = writer.shutdown().await;
            });

            let mut response = http::Response::builder()
                .status(500)
                .body(response_body)?;

            response.set_exception(e.to_string());
            Ok(response)
        }
    }
}
```

## Migration Guide

### From StreamHandle to RequestBody/ResponseBody

The project previously used a unified `StreamHandle` type. The new architecture separates concerns:

**Old Pattern:**

```rust
let handle = StreamHandle::new(false);
let (response_body, response_tx) = handle.create_response();
```

**New Pattern:**

```rust
let request_body = RequestBody::new();
let response_body = request_body.create_response();
```

**Key Differences:**

- No more shared buffer (`Arc<Mutex<Option<Bytes>>>`)
- Response channels created on demand, not eagerly
- Clearer ownership: request and response have separate types
- No synchronization primitives exposed in API

## Future Considerations

### 1. Custom Body Types

The architecture is designed to support custom body types beyond the default duplex streams. Future versions might support:

```rust
// Potential future API
impl<B: http_body::Body> Handler<B> {
    async fn handle(
        &self,
        request: http::Request<B>,
    ) -> Result<http::Response<B>, Self::Error>;
}
```

### 2. Trailers Support

HTTP trailers could be added via the `http-body::Body::poll_frame` trailers:

```rust
// In poll_frame implementation
Poll::Ready(Some(Ok(Frame::trailers(header_map))))
```

### 3. HTTP/2 and HTTP/3 Features

The current design is protocol-agnostic and should support HTTP/2 Server Push, HTTP/3 QUIC streams, etc., with appropriate adapters.

## Conclusion

The `http-handler` crate provides a clean, standards-based foundation for HTTP request handling in Rust. Its streaming body architecture enables efficient data flow between Node.js, Rust handlers, and backend services (Python, PHP, etc.) while maintaining type safety and providing natural backpressure.

Key takeaways for developers:

1. **Always shutdown** response writers when done
2. **Use extension traits** for cleaner code
3. **Choose buffer sizes** appropriate for your workload
4. **Spawn tasks** for long-running response generation
5. **Test thoroughly** using the Body trait and test utilities

For questions or contributions, see the main project repository.
