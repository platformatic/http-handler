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
//! use http_handler::{Handler, Request, Response};
//! use bytes::Bytes;
//! use tokio::io::AsyncWriteExt;
//!
//! struct HelloHandler;
//!
//! impl Handler for HelloHandler {
//!     type Error = std::convert::Infallible;
//!
//!     async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
//!         let (_parts, body) = request.into_parts();
//!         let response_body = body.create_response();
//!
//!         let mut response_writer = response_body.clone();
//!         tokio::spawn(async move {
//!             let _ = response_writer.write_all(b"Hello, World!").await;
//!             let _ = response_writer.shutdown().await;
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
//! use http_handler::{Handler, Request, Response, RequestBody, ResponseBody};
//! use bytes::Bytes;
//! use tokio::io::AsyncWriteExt;
//!
//! // Middleware that adds a header
//! struct AddHeaderHandler<H> {
//!     inner: H,
//!     header_name: &'static str,
//!     header_value: &'static str,
//! }
//!
//! impl<H> Handler for AddHeaderHandler<H>
//! where
//!     H: Handler + std::marker::Sync,
//! {
//!     type Error = H::Error;
//!
//!     async fn handle(
//!         &self,
//!         request: http::Request<RequestBody>
//!     ) -> Result<http::Response<ResponseBody>, Self::Error> {
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
//!         let (_parts, body) = request.into_parts();
//!         let response_body = body.create_response();
//!
//!         let mut response_writer = response_body.clone();
//!         tokio::spawn(async move {
//!             let _ = response_writer.write_all(br#"{"status": "ok"}"#).await;
//!             let _ = response_writer.shutdown().await;
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
/// The handler trait works with duplex stream-based request and response bodies,
/// providing efficient bidirectional I/O with configurable buffer sizes for
/// backpressure control.
///
/// # Examples
///
/// ## Basic handler
///
/// ```
/// use http_handler::{Handler, Request, Response};
/// use bytes::Bytes;
/// use tokio::io::AsyncWriteExt;
///
/// struct MyHandler;
///
/// impl Handler for MyHandler {
///     type Error = std::convert::Infallible;
///
///     async fn handle(&self, request: Request) -> Result<Response, Self::Error> {
///         let (_parts, body) = request.into_parts();
///         let response_body = body.create_response();
///
///         let mut response_writer = response_body.clone();
///         tokio::spawn(async move {
///             let _ = response_writer.write_all(b"Hello, World!").await;
///             let _ = response_writer.shutdown().await;
///         });
///
///         Ok(http::Response::builder()
///             .status(200)
///             .body(response_body)
///             .unwrap())
///     }
/// }
/// ```
pub trait Handler {
    /// The error type returned by the handler
    type Error;

    /// Handle an HTTP request and produce a response
    #[allow(async_fn_in_trait)]
    async fn handle(
        &self,
        request: http::Request<crate::RequestBody>,
    ) -> Result<http::Response<crate::ResponseBody>, Self::Error>;
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
            let response_body = body.create_response();

            // Spawn task to echo request to response
            let mut response_writer = response_body.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                use tokio::io::AsyncWriteExt;
                let mut buffer = vec![0u8; 8192];
                loop {
                    let n = body.read(&mut buffer).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    let _ = response_writer.write_all(&buffer[..n]).await;
                }
                let _ = response_writer.shutdown().await;
            });

            http::Response::builder().status(200).body(response_body)
        }
    }

    #[tokio::test]
    async fn test_echo_handler() {
        let handler = EchoHandler;
        let body = crate::RequestBody::from_data(Bytes::from("Hello, world!"))
            .await
            .unwrap();
        let request = http::Request::builder().uri("/echo").body(body).unwrap();

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

            let response_body = body.create_response();

            // Send OK response
            let mut response_writer = response_body.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = response_writer.write_all(b"OK").await;
                let _ = response_writer.shutdown().await;
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
            let response_body = body.create_response();

            let body_text = match socket_info {
                Some(info) => {
                    format!("Local: {:?}, Remote: {:?}", info.local, info.remote)
                }
                None => "No socket info".to_string(),
            };

            let mut response_writer = response_body.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = response_writer.write_all(body_text.as_bytes()).await;
                let _ = response_writer.shutdown().await;
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
        let request = http::Request::builder().uri("/test").body(body).unwrap();

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
        let mut request = http::Request::builder().uri("/test").body(body).unwrap();

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
        let request = http::Request::builder().uri("/error").body(body).unwrap();

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
            let response_body = body.create_response();

            let mut response_writer = response_body.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let _ = response_writer.write_all(b"Internal Server Error").await;
                let _ = response_writer.shutdown().await;
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
        let request = http::Request::builder().uri("/fail").body(body).unwrap();

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
