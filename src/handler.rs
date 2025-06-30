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
//! use http_handler::Handler;
//! use bytes::BytesMut;
//!
//! struct HelloHandler;
//!
//! #[async_trait::async_trait]
//! impl Handler for HelloHandler {
//!     type Error = std::convert::Infallible;
//!
//!     async fn handle(&self, _request: http::Request<BytesMut>) -> Result<http::Response<BytesMut>, Self::Error> {
//!         Ok(http::Response::builder()
//!             .status(200)
//!             .header("Content-Type", "text/plain")
//!             .body(BytesMut::from("Hello, World!"))
//!             .unwrap())
//!     }
//! }
//! ```
//!
//! ## Handler composition
//!
//! ```
//! use http_handler::Handler;
//! use bytes::BytesMut;
//!
//! // Middleware that adds a header
//! struct AddHeaderHandler<H> {
//!     inner: H,
//!     header_name: &'static str,
//!     header_value: &'static str,
//! }
//!
//! #[async_trait::async_trait]
//! impl<H, B> Handler<B> for AddHeaderHandler<H>
//! where
//!     H: Handler<B> + std::marker::Sync,
//!     for<'async_trait> B: std::marker::Send + 'async_trait
//! {
//!     type Error = H::Error;
//!
//!     async fn handle(&self, request: http::Request<B>) -> Result<http::Response<B>, Self::Error> {
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
//! #[async_trait::async_trait]
//! impl Handler for ApiHandler {
//!     type Error = std::convert::Infallible;
//!     async fn handle(&self, _req: http::Request<BytesMut>) -> Result<http::Response<BytesMut>, Self::Error> {
//!         Ok(http::Response::builder()
//!             .status(200)
//!             .body(BytesMut::from(r#"{"status": "ok"}"#))
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

use bytes::BytesMut;

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
/// use http_handler::Handler;
/// use bytes::BytesMut;
///
/// struct MyHandler;
///
/// #[async_trait::async_trait]
/// impl Handler for MyHandler {
///     type Error = std::convert::Infallible;
///
///     async fn handle(&self, request: http::Request<BytesMut>) -> Result<http::Response<BytesMut>, Self::Error> {
///         Ok(http::Response::builder()
///             .status(200)
///             .body(BytesMut::from("Hello, World!"))
///             .unwrap())
///     }
/// }
/// ```
///
/// ## Handler for String body
///
/// ```
/// use http_handler::Handler;
/// use bytes::BytesMut;
///
/// struct StringHandler;
///
/// #[async_trait::async_trait]
/// impl Handler<String> for StringHandler {
///     type Error = std::convert::Infallible;
///
///     async fn handle(&self, request: http::Request<String>) -> Result<http::Response<String>, Self::Error> {
///         let body = request.body();
///         let response_body = format!("You sent: {}", body);
///
///         Ok(http::Response::builder()
///             .status(200)
///             .body(response_body)
///             .unwrap())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait Handler<B = BytesMut> {
    /// The error type returned by the handler
    type Error;

    /// Handle an HTTP request and produce a response
    async fn handle(&self, request: http::Request<B>) -> Result<http::Response<B>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::SocketInfo;
    use crate::extensions::{RequestExt, ResponseExt};
    use bytes::Bytes;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    /// Example handler that echoes the request body
    pub struct EchoHandler;

    #[async_trait::async_trait]
    impl Handler<Bytes> for EchoHandler {
        type Error = http::Error;

        async fn handle(
            &self,
            request: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, Self::Error> {
            http::Response::builder()
                .status(200)
                .body(request.body().clone())
        }
    }

    #[tokio::test]
    async fn test_echo_handler() {
        let handler = EchoHandler;
        let request = http::Request::builder()
            .uri("/echo")
            .body(Bytes::from("Hello, world!"))
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), &Bytes::from("Hello, world!"));
    }

    /// Test handler that adds logging
    struct LoggingHandler;

    #[async_trait::async_trait]
    impl Handler<Bytes> for LoggingHandler {
        type Error = String;

        async fn handle(
            &self,
            request: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, Self::Error> {
            let method = request.method();
            let uri = request.uri();

            let mut response = http::Response::builder()
                .status(200)
                .body(Bytes::from("OK"))
                .unwrap();

            response.append_log(format!("{} {}", method, uri));

            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_logging_handler() {
        let handler = LoggingHandler;
        let request = http::Request::builder()
            .method("POST")
            .uri("/api/users")
            .body(Bytes::new())
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), &Bytes::from("OK"));

        let log = response.log().unwrap();
        assert_eq!(log.as_bytes(), b"POST /api/users");
    }

    /// Test handler that uses socket info
    struct SocketAwareHandler;

    #[async_trait::async_trait]
    impl Handler<Bytes> for SocketAwareHandler {
        type Error = String;

        async fn handle(
            &self,
            request: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, Self::Error> {
            let socket_info = request.socket_info();

            let body = match socket_info {
                Some(info) => {
                    format!("Local: {:?}, Remote: {:?}", info.local, info.remote)
                }
                None => "No socket info".to_string(),
            };

            Ok(http::Response::builder()
                .status(200)
                .body(Bytes::from(body))
                .unwrap())
        }
    }

    #[tokio::test]
    async fn test_socket_aware_handler() {
        let handler = SocketAwareHandler;

        // Test without socket info
        let request = http::Request::builder()
            .uri("/test")
            .body(Bytes::new())
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.body(), &Bytes::from("No socket info"));

        // Test with socket info
        let mut request = http::Request::builder()
            .uri("/test")
            .body(Bytes::new())
            .unwrap();

        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5000);
        request.set_socket_info(SocketInfo::new(Some(local), Some(remote)));

        let response = handler.handle(request).await.unwrap();
        let body_str = std::str::from_utf8(response.body()).unwrap();
        assert!(body_str.contains("127.0.0.1:8080"));
        assert!(body_str.contains("192.168.1.1:5000"));
    }

    /// Test handler that returns errors
    struct ErrorHandler;

    #[async_trait::async_trait]
    impl Handler<Bytes> for ErrorHandler {
        type Error = String;

        async fn handle(
            &self,
            _request: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, Self::Error> {
            Err("Something went wrong".to_string())
        }
    }

    #[tokio::test]
    async fn test_error_handler() {
        let handler = ErrorHandler;
        let request = http::Request::builder()
            .uri("/error")
            .body(Bytes::new())
            .unwrap();

        let result = handler.handle(request).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Something went wrong");
    }

    /// Test handler that sets an exception
    struct ExceptionHandler;

    #[async_trait::async_trait]
    impl Handler<Bytes> for ExceptionHandler {
        type Error = std::convert::Infallible;

        async fn handle(
            &self,
            _request: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, Self::Error> {
            let mut response = http::Response::builder()
                .status(500)
                .body(Bytes::from("Internal Server Error"))
                .unwrap();

            response.set_exception("Database connection failed");

            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_exception_handler() {
        let handler = ExceptionHandler;
        let request = http::Request::builder()
            .uri("/fail")
            .body(Bytes::new())
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(response.body(), &Bytes::from("Internal Server Error"));

        let exception = response.exception().unwrap();
        assert_eq!(exception.message(), "Database connection failed");
    }

    /// Test handler that works with String bodies
    struct StringBodyHandler;

    #[async_trait::async_trait]
    impl Handler<String> for StringBodyHandler {
        type Error = std::convert::Infallible;

        async fn handle(
            &self,
            request: http::Request<String>,
        ) -> Result<http::Response<String>, Self::Error> {
            let body = request.body();
            let response_body = format!("Received: {}", body.to_uppercase());

            Ok(http::Response::builder()
                .status(200)
                .body(response_body)
                .unwrap())
        }
    }

    #[tokio::test]
    async fn test_string_body_handler() {
        let handler = StringBodyHandler;
        let request = http::Request::builder()
            .uri("/string")
            .body("hello world".to_string())
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), &Bytes::from("Received: HELLO WORLD"));
    }

    // /// Test generic handler with different body types
    // struct TypeAwareHandler;

    // impl<B: std::fmt::Debug> Handler<B> for TypeAwareHandler {
    //     type Error = std::convert::Infallible;

    //     fn handle(&self, request: http::Request<B>) -> Result<http::Response<B>, Self::Error> {
    //         let type_name = std::any::type_name::<B>();
    //         let body_debug = format!("{:?}", request.body());
    //         let response_body = format!("Type: {}\nBody: {}", type_name, body_debug);

    //         Ok(http::Response::builder()
    //             .status(200)
    //             .body(response_body)
    //             .unwrap())
    //     }
    // }

    // #[test]
    // fn test_type_aware_handler() {
    //     let handler = TypeAwareHandler;

    //     // Test with String body
    //     let request = http::Request::builder()
    //         .uri("/type")
    //         .body("test string".to_string())
    //         .unwrap();

    //     let response = handler.handle(request).unwrap();
    //     let body_str = std::str::from_utf8(response.body()).unwrap();
    //     assert!(body_str.contains("alloc::string::String"));
    //     assert!(body_str.contains("test string"));

    //     // Test with Vec<u8> body
    //     let request = http::Request::builder()
    //         .uri("/type")
    //         .body(vec![1u8, 2, 3, 4])
    //         .unwrap();

    //     let response = handler.handle(request).unwrap();
    //     let body_str = std::str::from_utf8(response.body()).unwrap();
    //     assert!(body_str.contains("vec::Vec<u8>"));
    //     assert!(body_str.contains("[1, 2, 3, 4]"));
    // }

    /// Generic echo handler that works with any cloneable body type
    pub struct GenericEchoHandler;

    #[async_trait::async_trait]
    impl Handler<Bytes> for GenericEchoHandler {
        type Error = http::Error;

        async fn handle(
            &self,
            request: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, Self::Error> {
            http::Response::builder()
                .status(200)
                .body(request.into_body())
        }
    }

    #[async_trait::async_trait]
    impl Handler<Vec<u8>> for GenericEchoHandler {
        type Error = http::Error;

        async fn handle(
            &self,
            request: http::Request<Vec<u8>>,
        ) -> Result<http::Response<Vec<u8>>, Self::Error> {
            http::Response::builder()
                .status(200)
                .body(request.into_body())
        }
    }

    #[tokio::test]
    async fn test_generic_echo_handler() {
        let handler = GenericEchoHandler;

        // Test with Bytes
        let request = http::Request::builder()
            .uri("/echo")
            .body(Bytes::from("echo bytes"))
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.body(), &Bytes::from("echo bytes"));

        // Test with Vec<u8>
        let request = http::Request::builder()
            .uri("/echo")
            .body(vec![72, 101, 108, 108, 111]) // "Hello" in ASCII
            .unwrap();

        let response = handler.handle(request).await.unwrap();
        assert_eq!(response.body(), &Bytes::from("Hello"));
    }
}
