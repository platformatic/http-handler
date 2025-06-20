//! Extension types for storing additional data in http Request/Response

use bytes::{Bytes, BytesMut};
use std::{
    net::SocketAddr,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

/// Socket information for a request
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocketInfo {
    /// Local socket address
    pub local: Option<SocketAddr>,
    /// Remote socket address
    pub remote: Option<SocketAddr>,
}

impl SocketInfo {
    /// Create a new SocketInfo with both local and remote addresses
    pub fn new(local: Option<SocketAddr>, remote: Option<SocketAddr>) -> Self {
        Self { local, remote }
    }

    /// Create a SocketInfo with only local address
    pub fn with_local(local: SocketAddr) -> Self {
        Self {
            local: Some(local),
            remote: None,
        }
    }

    /// Create a SocketInfo with only remote address
    pub fn with_remote(remote: SocketAddr) -> Self {
        Self {
            local: None,
            remote: Some(remote),
        }
    }
}

/// Document root for a request
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocumentRoot {
    /// The document root path
    pub path: PathBuf,
}

impl DocumentRoot {
    /// Create a new DocumentRoot with the given path
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl Deref for DocumentRoot {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl DerefMut for DocumentRoot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.path
    }
}

impl From<String> for DocumentRoot {
    fn from(path: String) -> Self {
        Self::new(path)
    }
}

/// Response log buffer
#[derive(Clone, Debug, Default)]
pub struct ResponseLog {
    buffer: BytesMut,
}

impl ResponseLog {
    /// Create a new empty log
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a log with initial content
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        let bytes = bytes.into();
        let mut buffer = BytesMut::with_capacity(bytes.len());
        buffer.extend_from_slice(&bytes);
        Self { buffer }
    }

    /// Append data to the log
    pub fn append(&mut self, data: impl AsRef<[u8]>) {
        self.buffer.extend_from_slice(data.as_ref());
    }

    /// Get the log content as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Convert the log to Bytes
    pub fn into_bytes(self) -> Bytes {
        self.buffer.freeze()
    }

    /// Get the length of the log
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the log is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear the log
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Response exception information
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseException(pub String);

impl ResponseException {
    /// Create a new exception
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// Get the exception message
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl From<String> for ResponseException {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ResponseException {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Extension trait for http::Request
///
/// This trait provides methods to access and modify socket information related
/// to the request. This includes the local and remote socket IP addresses,
/// ports, and IP address families.
pub trait RequestExt {
    /// Get socket info from request extensions
    fn socket_info(&self) -> Option<&SocketInfo>;

    /// Get mutable socket info from request extensions
    fn socket_info_mut(&mut self) -> &mut SocketInfo;

    /// Set socket info in request extensions
    fn set_socket_info(&mut self, info: SocketInfo);

    /// Get document root from request extensions
    fn document_root(&self) -> Option<&DocumentRoot>;

    /// Get mutable document root from request extensions
    fn document_root_mut(&mut self) -> &mut DocumentRoot;

    /// Set document root in request extensions
    fn set_document_root(&mut self, root: DocumentRoot);
}

impl<T> RequestExt for http::Request<T> {
    fn socket_info(&self) -> Option<&SocketInfo> {
        self.extensions().get::<SocketInfo>()
    }

    fn socket_info_mut(&mut self) -> &mut SocketInfo {
        if self.extensions().get::<SocketInfo>().is_none() {
            self.extensions_mut().insert(SocketInfo::default());
        }
        self.extensions_mut().get_mut::<SocketInfo>().unwrap()
    }

    fn set_socket_info(&mut self, info: SocketInfo) {
        self.extensions_mut().insert(info);
    }

    fn document_root(&self) -> Option<&DocumentRoot> {
        self.extensions().get::<DocumentRoot>()
    }

    fn document_root_mut(&mut self) -> &mut DocumentRoot {
        if self.extensions().get::<DocumentRoot>().is_none() {
            self.extensions_mut().insert(DocumentRoot::default());
        }
        self.extensions_mut().get_mut::<DocumentRoot>().unwrap()
    }

    fn set_document_root(&mut self, root: DocumentRoot) {
        self.extensions_mut().insert(root);
    }
}

/// Extension trait for http::request::Builder
///
/// This trait provides methods to access and modify socket information related
/// to the request. This includes the local and remote socket IP addresses,
/// ports, and IP address families.
pub trait RequestBuilderExt {
    /// Set socket info in request builder
    fn socket_info(self, info: SocketInfo) -> http::request::Builder;

    /// Set document root in request builder
    fn document_root(self, root: DocumentRoot) -> http::request::Builder;
}

impl RequestBuilderExt for http::request::Builder {
    fn socket_info(self, info: SocketInfo) -> http::request::Builder {
        self.extension(info)
    }

    fn document_root(self, root: DocumentRoot) -> http::request::Builder {
        self.extension(root)
    }
}

/// Extension trait for http::Response
///
/// This trait provides methods to access and modify response logs and
/// exceptions.
pub trait ResponseExt {
    /// Get log from response extensions
    fn log(&self) -> Option<&ResponseLog>;

    /// Get mutable log from response extensions
    fn log_mut(&mut self) -> &mut ResponseLog;

    /// Set log in response extensions
    fn set_log(&mut self, log: impl Into<Bytes>);

    /// Append to the log
    fn append_log(&mut self, data: impl AsRef<[u8]>);

    /// Get exception from response extensions
    fn exception(&self) -> Option<&ResponseException>;

    /// Set exception in response extensions
    fn set_exception(&mut self, exception: impl Into<String>);
}

impl<T> ResponseExt for http::Response<T> {
    fn log(&self) -> Option<&ResponseLog> {
        self.extensions().get::<ResponseLog>()
    }

    fn log_mut(&mut self) -> &mut ResponseLog {
        if self.extensions().get::<ResponseLog>().is_none() {
            self.extensions_mut().insert(ResponseLog::new());
        }
        self.extensions_mut().get_mut::<ResponseLog>().unwrap()
    }

    fn set_log(&mut self, log: impl Into<Bytes>) {
        self.extensions_mut().insert(ResponseLog::from_bytes(log));
    }

    fn append_log(&mut self, data: impl AsRef<[u8]>) {
        self.log_mut().append(data);
    }

    fn exception(&self) -> Option<&ResponseException> {
        self.extensions().get::<ResponseException>()
    }

    fn set_exception(&mut self, exception: impl Into<String>) {
        self.extensions_mut()
            .insert(ResponseException::new(exception));
    }
}

/// Extension trait for http::response::Builder
///
/// This trait provides methods to access and modify response logs and
/// exceptions.
pub trait ResponseBuilderExt {
    /// Set log in response builder
    fn log(self, log: impl Into<Bytes>) -> http::response::Builder;

    /// Set exception in response builder
    fn exception(self, exception: impl Into<String>) -> http::response::Builder;
}

impl ResponseBuilderExt for http::response::Builder {
    fn log(self, log: impl Into<Bytes>) -> http::response::Builder {
        self.extension(ResponseLog::from_bytes(log))
    }

    fn exception(self, exception: impl Into<String>) -> http::response::Builder {
        self.extension(ResponseException::new(exception))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_socket_info() {
        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5000);

        let info = SocketInfo::new(Some(local), Some(remote));
        assert_eq!(info.local, Some(local));
        assert_eq!(info.remote, Some(remote));

        let info = SocketInfo::with_local(local);
        assert_eq!(info.local, Some(local));
        assert_eq!(info.remote, None);

        let info = SocketInfo::with_remote(remote);
        assert_eq!(info.local, None);
        assert_eq!(info.remote, Some(remote));
    }

    #[test]
    fn test_response_log() {
        let mut log = ResponseLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);

        log.append("Hello");
        log.append(" World");
        assert_eq!(log.as_bytes(), b"Hello World");
        assert_eq!(log.len(), 11);
        assert!(!log.is_empty());

        let bytes = log.clone().into_bytes();
        assert_eq!(&bytes[..], b"Hello World");

        log.clear();
        assert!(log.is_empty());

        let log = ResponseLog::from_bytes("Initial content");
        assert_eq!(log.as_bytes(), b"Initial content");
    }

    #[test]
    fn test_response_exception() {
        let exc = ResponseException::new("Error occurred");
        assert_eq!(exc.message(), "Error occurred");

        let exc = ResponseException::from("Another error");
        assert_eq!(exc.message(), "Another error");

        let exc: ResponseException = "String error".into();
        assert_eq!(exc.message(), "String error");
    }

    #[test]
    fn test_request_ext() {
        let mut request = http::Request::builder().uri("/test").body(()).unwrap();

        // Initially no socket info
        assert!(request.socket_info().is_none());

        // Set socket info
        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5000);
        request.set_socket_info(SocketInfo::new(Some(local), Some(remote)));

        // Verify it was set
        let info = request.socket_info().unwrap();
        assert_eq!(info.local, Some(local));
        assert_eq!(info.remote, Some(remote));

        // Modify through mutable reference
        request.socket_info_mut().local = None;
        assert_eq!(request.socket_info().unwrap().local, None);
    }

    #[test]
    fn test_response_ext() {
        let mut response = http::Response::builder().status(200).body(()).unwrap();

        // Initially no log or exception
        assert!(response.log().is_none());
        assert!(response.exception().is_none());

        // Set log
        response.set_log("Initial log");
        assert_eq!(response.log().unwrap().as_bytes(), b"Initial log");

        // Append to log
        response.append_log(" - more data");
        assert_eq!(
            response.log().unwrap().as_bytes(),
            b"Initial log - more data"
        );

        // Set exception
        response.set_exception("Something went wrong");
        assert_eq!(
            response.exception().unwrap().message(),
            "Something went wrong"
        );

        // Access mutable log
        response.log_mut().clear();
        assert!(response.log().unwrap().is_empty());
    }
}
