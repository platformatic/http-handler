//! Handler trait for processing HTTP requests
//!
//! This module provides the core `Handler` trait for building HTTP request handlers.
//! The trait is generic over the request body type, allowing handlers to work with
//! different body representations while maintaining type safety.
//!
//! # Examples
//!
//! ## Basic handler implementation
//!
//! ```
//! use http_handler::{Handler, Request, Response, StreamChunk};
//! use bytes::Bytes;
//!
//! struct HelloHandler;
//!
//! impl Handler for HelloHandler {
//!     type Error = std::convert::Infallible;
//!
//!     async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
//!         let (_parts, mut body) = request.into_parts();
//!         let (response_body, response_tx) = body.create_response();
//!
//!         tokio::spawn(async move {
//!             let _ = response_tx.send(Ok(StreamChunk::Data(Bytes::from("Hello, World!")))).await;
//!             let _ = response_tx.send(Ok(StreamChunk::End)).await;
//!         });
//!
//!         Ok(http::Response::builder()
//!             .status(200)
//!             .header("Content-Type", "text/plain")
//!             .body(response_body)
//!             .unwrap())
//!     }
//! }
//! ```
//!
//! ## Handler composition
//!
//! ```
//! use http_handler::{Handler, Request, Response, RequestBody, ResponseBody, StreamChunk};
//! use bytes::Bytes;
//!
//! // Middleware that adds a header
//! struct AddHeaderHandler<H> {
//!     inner: H,
//!     header_name: &'static str,
//!     header_value: &'static str,
//! }
//!
//! impl<H, B> Handler<B> for AddHeaderHandler<H>
//! where
//!     H: Handler<B> + std::marker::Sync,
//!     B: std::marker::Send
//! {
//!     type Error = H::Error;
//!
//!     async fn handle(
//!         &self,
//!         request: http::Request<RequestBody<B>>
//!     ) -> Result<http::Response<ResponseBody<B>>, Self::Error> {
//!         let mut response = self.inner.handle(request).await?;
//!         response.headers_mut().insert(
//!             self.header_name,
//!             self.header_value.parse().unwrap()
//!         );
//!         Ok(response)
//!     }
//! }
//!
//! // Usage
//! struct ApiHandler;
//!
//! impl Handler for ApiHandler {
//!     type Error = std::convert::Infallible;
//!     async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
//!         let (_parts, mut body) = request.into_parts();
//!         let (response_body, response_tx) = body.create_response();
//!
//!         tokio::spawn(async move {
//!             let _ = response_tx.send(Ok(StreamChunk::Data(Bytes::from(r#"{"status": "ok"}"#)))).await;
//!             let _ = response_tx.send(Ok(StreamChunk::End)).await;
//!         });
//!
//!         Ok(http::Response::builder()
//!             .status(200)
//!             .body(response_body)
//!             .unwrap())
//!     }
//! }
//!
//! let handler = AddHeaderHandler {
//!     inner: ApiHandler,
//!     header_name: "X-API-Version",
//!     header_value: "1.0",
//! };
//! ```


/// Trait for types that can handle HTTP requests and produce responses
///
/// The handler trait is generic over the request body type `B`, allowing
/// handlers to work with different body representations such as `Bytes`,
/// `String`, streaming bodies, or custom types.
///
/// The response body type is fixed to `Bytes` for simplicity, but handlers
/// can be composed with body transformers if different response types are needed.
///
/// # Examples
///
/// ## Handler for Bytes body (default)
///
/// ```
/// use http_handler::{Handler, Request, Response, StreamChunk};
/// use bytes::Bytes;
///
/// struct MyHandler;
///
/// impl Handler for MyHandler {
///     type Error = std::convert::Infallible;
///
///     async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
///         let (_parts, mut body) = request.into_parts();
///         let (response_body, response_tx) = body.create_response();
///
///         tokio::spawn(async move {
///             let _ = response_tx.send(Ok(StreamChunk::Data(Bytes::from("Hello, World!")))).await;
///             let _ = response_tx.send(Ok(StreamChunk::End)).await;
///         });
///
///         Ok(http::Response::builder()
///             .status(200)
///             .body(response_body)
///             .unwrap())
///     }
/// }
/// ```
///
/// ## Handler for String body
///
/// ```
/// use http_handler::{Handler, RequestBody, ResponseBody, StreamChunk};
///
/// struct StringHandler;
///
/// impl Handler<String> for StringHandler {
///     type Error = std::convert::Infallible;
///
///     async fn handle(
///         &self,
///         request: http::Request<RequestBody<String>>
///     ) -> Result<http::Response<ResponseBody<String>>, Self::Error> {
///         let (_parts, mut body) = request.into_parts();
///         let (response_body, response_tx) = body.create_response();
///
///         tokio::spawn(async move {
///             let mut collected = String::new();
///             if let Some(mut rx) = body.take_request_rx() {
///                 while let Some(chunk) = rx.recv().await {
///                     match chunk {
///                         StreamChunk::Data(data) => collected.push_str(&data),
///                         StreamChunk::End => break,
///                     }
///                 }
///             }
///             let response_text = format!("You sent: {}", collected);
///             let _ = response_tx.send(Ok(StreamChunk::Data(response_text))).await;
///             let _ = response_tx.send(Ok(StreamChunk::End)).await;
///         });
///
///         Ok(http::Response::builder()
///             .status(200)
///             .body(response_body)
///             .unwrap())
///     }
/// }
/// ```
pub trait Handler<T = bytes::Bytes> {
    /// The error type returned by the handler
    type Error;

    /// Handle an HTTP request and produce a response
    #[allow(async_fn_in_trait)]
    async fn handle(
        &self,
        request: http::Request<crate::RequestBody<T>>,
    ) -> Result<http::Response<crate::ResponseBody<T>>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::SocketInfo;
    use crate::extensions::{RequestExt, ResponseExt};
    use bytes::{Bytes, BytesMut};
    use http_body_util::BodyExt;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    /// Example handler that echoes the request body
    pub struct EchoHandler;

    impl Handler for EchoHandler {
        type Error = http::Error;

        async fn handle(&self, request: crate::Request) -> Result<crate::Response, Self::Error> {
            let (_parts, mut body) = request.into_parts();
            let (response_body, response_tx) = body.create_response();

            // Spawn task to echo request to response
            tokio::spawn(async move {
                if let Some(mut rx) = body.take_request_rx() {
                    while let Some(chunk) = rx.recv().await {
                        match chunk {
                            crate::StreamChunk::Data(data) => {
                                let _ = response_tx.send(Ok(crate::StreamChunk::Data(data))).await;
                            }
                            crate::StreamChunk::End => break,
                        }
                    }
                }
                // Send End marker
                let _ = response_tx.send(Ok(crate::StreamChunk::End)).await;
                // Drop response_tx to close the channel
                drop(response_tx);
            });

            http::Response::builder()
                .status(200)
                .body(response_body)
        }
    }

    #[tokio::test]
    async fn test_echo_handler() {
        let handler = EchoHandler;
        let body = crate::RequestBody::from_data(Bytes::from("Hello, world!"))
            .await
            .unwrap();
        let request = http::Request::builder()
            .uri("/echo")
            .body(body)
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.status(), 200);

        // Read the response body
        let (_, mut response_body) = response.into_parts();
        let mut collected = BytesMut::new();
        while let Some(result) = response_body.frame().await {
            match result {
                Ok(frame) => {
                    if let Ok(data) = frame.into_data() {
                        collected.extend_from_slice(&data);
                    }
                }
                Err(_) => break,
            }
        }
        assert_eq!(&collected[..], b"Hello, world!");
    }

    /// Test handler that adds logging
    struct LoggingHandler;

    impl Handler for LoggingHandler {
        type Error = String;

        async fn handle(&self, request: crate::Request) -> Result<crate::Response, Self::Error> {
            let method = request.method().clone();
            let uri = request.uri().clone();
            let (_, body) = request.into_parts();

            let (response_body, response_tx) = body.create_response();

            // Send OK response
            tokio::spawn(async move {
                let _ = response_tx.send(Ok(crate::StreamChunk::Data(Bytes::from("OK")))).await;
                let _ = response_tx.send(Ok(crate::StreamChunk::End)).await;
                drop(response_tx);
            });

            let mut response = http::Response::builder()
                .status(200)
                .body(response_body)
                .unwrap();

            response.append_log(format!("{} {}", method, uri));

            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_logging_handler() {
        let handler = LoggingHandler;
        let body = crate::RequestBody::new();
        let request = http::Request::builder()
            .method("POST")
            .uri("/api/users")
            .body(body)
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.status(), 200);

        let log = response.log().unwrap();
        assert_eq!(log.as_bytes(), b"POST /api/users\n");

        // Read the response body
        let (_, mut response_body) = response.into_parts();
        let mut collected = BytesMut::new();
        while let Some(result) = response_body.frame().await {
            match result {
                Ok(frame) => {
                    if let Ok(data) = frame.into_data() {
                        collected.extend_from_slice(&data);
                    }
                }
                Err(_) => break,
            }
        }
        assert_eq!(&collected[..], b"OK");
    }

    /// Test handler that uses socket info
    struct SocketAwareHandler;

    impl Handler for SocketAwareHandler {
        type Error = String;

        async fn handle(&self, request: crate::Request) -> Result<crate::Response, Self::Error> {
            let socket_info = request.socket_info().cloned();
            let (_, body) = request.into_parts();
            let (response_body, response_tx) = body.create_response();

            let body_text = match socket_info {
                Some(info) => {
                    format!("Local: {:?}, Remote: {:?}", info.local, info.remote)
                }
                None => "No socket info".to_string(),
            };

            tokio::spawn(async move {
                let _ = response_tx.send(Ok(crate::StreamChunk::Data(Bytes::from(body_text)))).await;
                let _ = response_tx.send(Ok(crate::StreamChunk::End)).await;
                drop(response_tx);
            });

            Ok(http::Response::builder()
                .status(200)
                .body(response_body)
                .unwrap())
        }
    }

    #[tokio::test]
    async fn test_socket_aware_handler() {
        let handler = SocketAwareHandler;

        // Test without socket info
        let body = crate::RequestBody::new();
        let request = http::Request::builder()
            .uri("/test")
            .body(body)
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        let (_, mut response_body) = response.into_parts();
        let mut collected = BytesMut::new();
        while let Some(result) = response_body.frame().await {
            match result {
                Ok(frame) => {
                    if let Ok(data) = frame.into_data() {
                        collected.extend_from_slice(&data);
                    }
                }
                Err(_) => break,
            }
        }
        assert_eq!(&collected[..], b"No socket info");

        // Test with socket info
        let body = crate::RequestBody::new();
        let mut request = http::Request::builder()
            .uri("/test")
            .body(body)
            .unwrap();

        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5000);
        request.set_socket_info(SocketInfo::new(Some(local), Some(remote)));

        let response = handler.handle(request).await.unwrap();
        let (_, mut response_body) = response.into_parts();
        let mut collected = BytesMut::new();
        while let Some(result) = response_body.frame().await {
            match result {
                Ok(frame) => {
                    if let Ok(data) = frame.into_data() {
                        collected.extend_from_slice(&data);
                    }
                }
                Err(_) => break,
            }
        }
        let body_str = std::str::from_utf8(&collected).unwrap();
        assert!(body_str.contains("127.0.0.1:8080"));
        assert!(body_str.contains("192.168.1.1:5000"));
    }

    /// Test handler that returns errors
    struct ErrorHandler;

    impl Handler for ErrorHandler {
        type Error = String;

        async fn handle(&self, _request: crate::Request) -> Result<crate::Response, Self::Error> {
            Err("Something went wrong".to_string())
        }
    }

    #[tokio::test]
    async fn test_error_handler() {
        let handler = ErrorHandler;
        let body = crate::RequestBody::new();
        let request = http::Request::builder()
            .uri("/error")
            .body(body)
            .unwrap();

        let result = handler.handle(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Something went wrong");
    }

    /// Test handler that sets an exception
    struct ExceptionHandler;

    impl Handler for ExceptionHandler {
        type Error = std::convert::Infallible;

        async fn handle(&self, request: crate::Request) -> Result<crate::Response, Self::Error> {
            let (_, body) = request.into_parts();
            let (response_body, response_tx) = body.create_response();

            tokio::spawn(async move {
                let _ = response_tx.send(Ok(crate::StreamChunk::Data(Bytes::from("Internal Server Error")))).await;
                let _ = response_tx.send(Ok(crate::StreamChunk::End)).await;
                drop(response_tx);
            });

            let mut response = http::Response::builder()
                .status(500)
                .body(response_body)
                .unwrap();

            response.set_exception("Database connection failed");

            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_exception_handler() {
        let handler = ExceptionHandler;
        let body = crate::RequestBody::new();
        let request = http::Request::builder()
            .uri("/fail")
            .body(body)
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.status(), 500);

        let exception = response.exception().unwrap();
        assert_eq!(exception.message(), "Database connection failed");

        let (_, mut response_body) = response.into_parts();
        let mut collected = BytesMut::new();
        while let Some(result) = response_body.frame().await {
            match result {
                Ok(frame) => {
                    if let Ok(data) = frame.into_data() {
                        collected.extend_from_slice(&data);
                    }
                }
                Err(_) => break,
            }
        }
        assert_eq!(&collected[..], b"Internal Server Error");
    }
}
