//! Core type aliases and implementations for v2

use super::extensions::{RequestExt, ResponseExt, SocketInfo};
use bytes::BytesMut;

/// Type alias for HTTP Request with BytesMut body
pub type Request = http::Request<BytesMut>;

/// Type alias for HTTP Response with BytesMut body
pub type Response = http::Response<BytesMut>;

/// Helper functions for building requests with extensions
pub mod request {
    use super::*;
    use std::net::SocketAddr;

    /// Build a request with socket info
    pub fn with_socket_info(
        mut request: Request,
        local: Option<SocketAddr>,
        remote: Option<SocketAddr>,
    ) -> Request {
        request.set_socket_info(SocketInfo::new(local, remote));
        request
    }
}

/// Helper functions for building responses with extensions
pub mod response {
    use super::*;
    use bytes::Bytes;

    /// Build a response with log data
    pub fn with_log(mut response: Response, log: impl Into<Bytes>) -> Response {
        response.set_log(log);
        response
    }

    /// Build a response with exception
    pub fn with_exception(mut response: Response, exception: impl Into<String>) -> Response {
        response.set_exception(exception);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Method, StatusCode};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn test_request_type_alias() {
        let request = http::Request::builder()
            .method(Method::GET)
            .uri("/test")
            .body(BytesMut::from("request body"))
            .unwrap();

        assert_eq!(request.method(), Method::GET);
        assert_eq!(request.uri().path(), "/test");
        assert_eq!(request.body(), &BytesMut::from("request body"));
    }

    #[test]
    fn test_response_type_alias() {
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain")
            .body(BytesMut::from("response body"))
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain"
        );
        assert_eq!(response.body(), &BytesMut::from("response body"));
    }

    #[test]
    fn test_request_with_socket_info() {
        let request = http::Request::builder()
            .uri("/test")
            .body(BytesMut::new())
            .unwrap();

        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5000);

        let request = request::with_socket_info(request, Some(local), Some(remote));

        let info = request.socket_info().unwrap();
        assert_eq!(info.local, Some(local));
        assert_eq!(info.remote, Some(remote));
    }

    #[test]
    fn test_response_with_log() {
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body(BytesMut::new())
            .unwrap();

        let response = response::with_log(response, "Test log message");

        let log = response.log().unwrap();
        assert_eq!(log.as_bytes(), b"Test log message");
    }

    #[test]
    fn test_response_with_exception() {
        let response = http::Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(BytesMut::new())
            .unwrap();

        let response = response::with_exception(response, "Something went wrong");

        let exception = response.exception().unwrap();
        assert_eq!(exception.message(), "Something went wrong");
    }

    #[test]
    fn test_combined_extensions() {
        // Test that we can use multiple extensions together
        let mut response = http::Response::builder()
            .status(StatusCode::OK)
            .body(BytesMut::from("body"))
            .unwrap();

        response.set_log("Initial log");
        response.append_log(" - more info");
        response.set_exception("Warning: something happened");

        assert_eq!(
            response.log().unwrap().as_bytes(),
            b"Initial log - more info\n"
        );
        assert_eq!(
            response.exception().unwrap().message(),
            "Warning: something happened"
        );
    }
}
