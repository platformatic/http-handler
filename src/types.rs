//! Core type aliases and implementations

use super::body::{RequestBody, ResponseBody};
use super::extensions::{RequestExt, ResponseExt, SocketInfo};

/// Type alias for HTTP Request with streaming body
pub type Request = http::Request<RequestBody>;

/// Type alias for HTTP Response with streaming body
pub type Response = http::Response<ResponseBody>;

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
    use bytes::Bytes;
    use http::{Method, StatusCode};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_request_type_alias() {
        let body = RequestBody::from_data(Bytes::from("request body"))
            .await
            .unwrap();
        let request = http::Request::builder()
            .method(Method::GET)
            .uri("/test")
            .body(body)
            .unwrap();

        assert_eq!(request.method(), Method::GET);
        assert_eq!(request.uri().path(), "/test");
    }

    #[test]
    fn test_response_type_alias() {
        let request_body = RequestBody::new();
        let response_body = request_body.create_response();
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain")
            .body(response_body)
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain"
        );
    }

    #[test]
    fn test_request_with_socket_info() {
        let body = RequestBody::new();
        let request = http::Request::builder().uri("/test").body(body).unwrap();

        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5000);

        let request = request::with_socket_info(request, Some(local), Some(remote));

        let info = request.socket_info().unwrap();
        assert_eq!(info.local, Some(local));
        assert_eq!(info.remote, Some(remote));
    }

    #[test]
    fn test_response_with_log() {
        let request_body = RequestBody::new();
        let response_body = request_body.create_response();
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body(response_body)
            .unwrap();

        let response = response::with_log(response, "Test log message");

        let log = response.log().unwrap();
        assert_eq!(log.as_bytes(), b"Test log message");
    }

    #[test]
    fn test_response_with_exception() {
        let request_body = RequestBody::new();
        let response_body = request_body.create_response();
        let response = http::Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(response_body)
            .unwrap();

        let response = response::with_exception(response, "Something went wrong");

        let exception = response.exception().unwrap();
        assert_eq!(exception.message(), "Something went wrong");
    }

    #[test]
    fn test_combined_extensions() {
        // Test that we can use multiple extensions together
        let request_body = RequestBody::new();
        let response_body = request_body.create_response();
        let mut response = http::Response::builder()
            .status(StatusCode::OK)
            .body(response_body)
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
