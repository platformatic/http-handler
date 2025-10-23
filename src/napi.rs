use std::{
    collections::HashMap,
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

use bytes::BytesMut;
use http::{
    HeaderMap as HttpHeaderMap, HeaderName, HeaderValue as HttpHeaderValue,
    request::Builder as RequestBuilder, response::Builder as ResponseBuilder,
};
use napi::{Either, Error, Result, Status, bindgen_prelude::*};
use napi_derive::napi;

use crate::{
    Request as InnerRequest, RequestBuilderExt, RequestExt, Response as InnerResponse,
    ResponseBuilderExt, ResponseExt, SocketInfo as InnerSocketInfo,
};

//
// HeaderMap
//

/// A header entry value, which can be either a string or array of strings.
#[napi]
pub type HeaderMapValue = Either<String, Vec<String>>;

/// A multi-map of HTTP headers. Any given header key can have multiple values.
#[napi(transparent)]
#[derive(Default)]
pub struct HeaderMap(pub HashMap<String, HeaderMapValue>);

impl TryFrom<HeaderMap> for HttpHeaderMap {
    type Error = Error;

    fn try_from(map: HeaderMap) -> std::result::Result<Self, Self::Error> {
        let mut headers = HttpHeaderMap::new();

        for (key, value) in map.0 {
            let header_name = HeaderName::try_from(key).map_err(|e| {
                Error::new(Status::InvalidArg, format!("Invalid header name: {}", e))
            })?;

            match value {
                Either::A(value) => {
                    let header_value = HttpHeaderValue::try_from(value).map_err(|e| {
                        Error::new(Status::InvalidArg, format!("Invalid header value: {}", e))
                    })?;
                    headers.insert(header_name, header_value);
                }
                Either::B(values) => {
                    for value in values {
                        let header_value = HttpHeaderValue::try_from(value).map_err(|e| {
                            Error::new(Status::InvalidArg, format!("Invalid header value: {}", e))
                        })?;
                        headers.append(header_name.clone(), header_value);
                    }
                }
            }
        }

        Ok(headers)
    }
}

//
// HeaderValue
//

struct HeaderValue(HttpHeaderValue);

impl Deref for HeaderValue {
    type Target = HttpHeaderValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for HeaderValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TryFrom<String> for HeaderValue {
    type Error = Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        HttpHeaderValue::try_from(value)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid header value: {}", e)))
            .map(HeaderValue)
    }
}

//
// SocketInfo
//

/// Input options for creating a SocketInfo.
#[napi(object)]
#[derive(Default)]
pub struct SocketInfo {
    /// The string representation of the local IP address the remote client is connecting on.
    pub local_address: String,
    /// The numeric representation of the local port. For example, 80 or 21.
    pub local_port: u16,
    /// The string representation of the local IP family, e.g., "IPv4" or "IPv6".
    pub local_family: String,
    /// The string representation of the remote IP address.
    pub remote_address: String,
    /// The numeric representation of the remote port. For example, 80 or 21.
    pub remote_port: u16,
    /// The string representation of the remote IP family, e.g., "IPv4" or "IPv6".
    pub remote_family: String,
}

impl TryFrom<InnerSocketInfo> for SocketInfo {
    type Error = Error;

    fn try_from(socket: InnerSocketInfo) -> Result<Self> {
        let local = socket.local.ok_or(Error::new(
            Status::InvalidArg,
            "Local socket address is required",
        ))?;
        let remote = socket.remote.ok_or(Error::new(
            Status::InvalidArg,
            "Remote socket address is required",
        ))?;

        fn socket_info_tuple(socket: &SocketAddr) -> (String, u16, String) {
            (
                socket.ip().to_string(),
                socket.port(),
                if socket.is_ipv4() { "IPv4" } else { "IPv6" }.to_string(),
            )
        }

        let (local_address, local_port, local_family) = socket_info_tuple(&local);
        let (remote_address, remote_port, remote_family) = socket_info_tuple(&remote);

        Ok(SocketInfo {
            local_address,
            local_port,
            local_family,
            remote_address,
            remote_port,
            remote_family,
        })
    }
}

impl TryFrom<SocketInfo> for InnerSocketInfo {
    type Error = Error;

    fn try_from(socket: SocketInfo) -> std::result::Result<Self, Self::Error> {
        fn sock_addr(family: &str, address: &str, port: u16) -> Result<SocketAddr> {
            if family == "IPv6" {
                format!("[{}]:{}", address, port)
            } else {
                format!("{}:{}", address, port)
            }
            .parse()
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid socket address: {}", e)))
        }

        let local = sock_addr(
            &socket.local_family,
            &socket.local_address,
            socket.local_port,
        )?;
        let remote = sock_addr(
            &socket.remote_family,
            &socket.remote_address,
            socket.remote_port,
        )?;
        Ok(Self {
            local: Some(local),
            remote: Some(remote),
        })
    }
}

//
// Headers
//

/// Wraps an http::HeaderMap instance to expose it to JavaScript.
///
/// It provides methods to access and modify HTTP headers, iterate over them,
/// and convert them to a JSON object representation.
#[napi]
#[derive(Debug, Clone, Default)]
pub struct Headers(HttpHeaderMap);

impl Deref for Headers {
    type Target = HttpHeaderMap;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Headers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromNapiValue for Headers {
    unsafe fn from_napi_value(env: sys::napi_env, value: sys::napi_value) -> Result<Self> {
        // Try to convert from ClassInstance<Headers>
        if let Ok(instance) = unsafe { ClassInstance::<Headers>::from_napi_value(env, value) } {
            return Ok(Headers(instance.0.clone()));
        }

        // If that fails, try to convert from HeaderMap
        if let Ok(header_map) = unsafe { HeaderMap::from_napi_value(env, value) } {
            return Ok(Headers(header_map.try_into()?));
        }

        // If both conversions fail, return an error
        Err(Error::new(
            Status::InvalidArg,
            "Expected Headers or HeaderMap",
        ))
    }
}

#[napi]
impl Headers {
    /// Create a new Headers instance.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers({
    ///   'Content-Type': 'application/json',
    ///   'Accept': ['text/html', 'application/json']
    /// });
    ///
    /// console.log(headers.get('Content-Type')); // application/json
    /// for (const mime of headers.getAll('Accept')) {
    ///   console.log(mime); // text/html, application/json
    /// }
    /// ```
    #[napi(constructor)]
    pub fn new(options: Option<HeaderMap>) -> Result<Self> {
        Ok(Self(options.unwrap_or_default().try_into()?))
    }

    /// Get the last set value for a given header key.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Accept', 'application/json');
    /// headers.set('Accept', 'text/html');
    ///
    /// console.log(headers.get('Accept')); // text/html
    /// ```
    #[napi]
    pub fn get(&self, key: String) -> Option<String> {
        // Return the last value for this key (HTTP headers can have multiple values)
        self.0
            .get_all(&key)
            .iter()
            .last()
            .and_then(|v| v.to_str().map(|s| s.to_string()).ok())
    }

    /// Get all values for a given header key.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Accept', 'application/json');
    /// headers.set('Accept', 'text/html');
    ///
    /// for (const mime of headers.getAll('Accept')) {
    ///   console.log(mime);
    /// }
    /// ```
    #[napi]
    pub fn get_all(&self, key: String) -> Vec<String> {
        self.0
            .get_all(&key)
            .iter()
            .filter_map(|v| v.to_str().map(|s| s.to_string()).ok())
            .collect()
    }

    /// Get all values for a given header key as a comma-separated string.
    ///
    /// This is useful for headers that can have multiple values, such as `Accept`.
    /// But note that some headers like `Set-Cookie`, expect separate lines.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Accept', 'application/json');
    /// headers.set('Accept', 'text/html');
    ///
    /// console.log(headers.getLine('Accept')); // application/json,text/html
    /// ```
    #[napi]
    pub fn get_line(&self, key: String) -> Option<String> {
        let values = self.get_all(key);
        if values.is_empty() {
            None
        } else {
            Some(values.join(","))
        }
    }

    /// Clear all header entries.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// headers.set('Accept', 'application/json');
    /// headers.clear();
    ///
    /// console.log(headers.has('Content-Type')); // false
    /// console.log(headers.has('Accept')); // false
    /// ```
    #[napi]
    pub fn clear(&mut self) {
        self.0.clear()
    }

    /// Check if a header key exists.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    ///
    /// console.log(headers.has('Content-Type')); // true
    /// console.log(headers.has('Accept')); // false
    /// ```
    #[napi]
    pub fn has(&self, key: String) -> bool {
        self.0.contains_key(&key)
    }

    /// Set a header key/value pair.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// ```
    #[napi]
    pub fn set(&mut self, key: String, value: HeaderMapValue) -> Result<bool> {
        let key = HeaderName::try_from(key)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid header name: {}", e)))?;

        let had_value = self.0.remove(&key).is_some();

        match value {
            Either::A(value) => {
                let value = HttpHeaderValue::try_from(value).map_err(|e| {
                    Error::new(Status::InvalidArg, format!("Invalid header value: {}", e))
                })?;
                self.0.insert(key, value);
            }
            Either::B(values) => {
                for value in values {
                    let value = HttpHeaderValue::try_from(value).map_err(|e| {
                        Error::new(Status::InvalidArg, format!("Invalid header value: {}", e))
                    })?;
                    self.0.append(key.clone(), value);
                }
            }
        }

        Ok(had_value)
    }

    /// Add a value to a header key.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Accept', 'application/json');
    /// headers.add('Accept', 'text/html');
    ///
    /// console.log(headers.get('Accept')); // application/json, text/html
    /// ```
    #[napi]
    pub fn add(&mut self, key: String, value: String) -> Result<bool> {
        let key = HeaderName::try_from(key)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid header name: {}", e)))?;

        let value = HttpHeaderValue::try_from(value)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid header value: {}", e)))?;

        Ok(self.0.append(key, value))
    }

    /// Delete a header key/value pair.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// headers.delete('Content-Type');
    /// ```
    #[napi]
    pub fn delete(&mut self, key: String) -> bool {
        self.0.remove(&key).is_some()
    }

    /// Get the number of header entries.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// headers.set('Accept', 'application/json');
    ///
    /// console.log(headers.size); // 2
    /// ```
    #[napi(getter)]
    pub fn size(&self) -> u32 {
        self.0.keys_len() as u32
    }

    /// Get an iterator over the header entries.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// headers.set('Accept', 'application/json');
    ///
    /// for (const [name, value] of headers.entries()) {
    ///   console.log(`${name}: ${value}`);
    /// }
    /// ```
    #[napi]
    pub fn entries(&self) -> Vec<(String, String)> {
        self.0
            .iter()
            .map(|(name, value)| {
                let name = name.as_str().to_string();
                let value = value.to_str().unwrap_or("").to_string();
                (name, value)
            })
            .collect()
    }

    /// Get an iterator over the header keys.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// headers.set('Accept', 'application/json');
    ///
    /// for (const name of headers.keys()) {
    ///   console.log(name);
    /// }
    /// ```
    #[napi]
    pub fn keys(&self) -> Vec<String> {
        self.0
            .keys()
            .map(|name| name.as_str().to_string())
            .collect()
    }

    /// Get an iterator over the header values.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// headers.set('Accept', 'application/json');
    ///
    /// for (const value of headers.values()) {
    ///   console.log(value);
    /// }
    /// ```
    #[napi]
    pub fn values(&self) -> Vec<String> {
        self.0
            .values()
            .map(|value| value.to_str().unwrap_or("").to_string())
            .collect()
    }

    /// Execute a callback for each header entry.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers();
    /// headers.set('Content-Type', 'application/json');
    /// headers.set('Accept', 'application/json');
    ///
    /// headers.forEach((value, name, headers) => {
    ///   console.log(`${name}: ${value}`);
    /// });
    /// ```
    #[napi]
    pub fn for_each<F: Fn(String, String, This) -> Result<()>>(
        &self,
        this: This,
        callback: F,
    ) -> Result<()> {
        for entry in self.entries() {
            callback(entry.1, entry.0, this)?;
        }
        Ok(())
    }

    /// Convert the headers to a JSON object representation.
    ///
    /// # Examples
    ///
    /// ```js
    /// const headers = new Headers({
    ///   'Content-Type': 'application/json',
    ///   'Accept': ['text/html', 'application/json']
    /// });
    ///
    /// console.log(headers.toJSON());
    /// ```
    #[napi(js_name = "toJSON")]
    pub fn to_json(&self, env: &Env) -> Result<Object> {
        let mut obj = Object::new(env)?;

        for key in self.keys() {
            let values = self.get_all(key.clone());
            if values.len() == 1 {
                obj.set(&key, values[0].clone())?;
            } else {
                let mut array = env.create_array(values.len() as u32)?;
                for (i, value) in values.iter().enumerate() {
                    array.set(i as u32, value.clone())?;
                }
                obj.set(&key, array)?;
            };
        }

        Ok(obj)
    }
}

//
// Request
//

/// Input options for creating a Request.
#[napi(object)]
#[derive(Default)]
pub struct RequestOptions {
    /// The HTTP method for the request.
    pub method: Option<String>,
    /// The URL for the request.
    pub url: String,
    /// The headers for the request.
    #[napi(ts_type = "Headers | HeaderMap")]
    pub headers: Option<Headers>,
    /// The body for the request.
    pub body: Option<Buffer>,
    /// The socket information for the request.
    pub socket: Option<SocketInfo>,
    /// Document root for the request, if applicable.
    pub docroot: Option<String>,
}

/// Wraps an http::Request instance to expose it to JavaScript.
///
/// It provides methods to access the HTTP method, URI, headers, and body of
/// the request along with a toJSON method to convert it to a JSON object.
#[napi]
#[derive(Debug)]
pub struct Request(InnerRequest);

#[napi]
impl Request {
    /// Create a new Request from a Request instance.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   method: 'GET',
    ///   url: 'http://example.com',
    ///   headers: {
    ///     'Accept': ['application/json', 'text/html']
    ///   },
    ///   body: Buffer.from(JSON.stringify({
    ///     message: 'Hello, world!'
    ///   })),
    /// });
    /// ```
    #[napi(constructor)]
    pub fn new(options: Option<RequestOptions>) -> Result<Self> {
        // This is just to make the error message clearer when no options are provided.
        let options = match options {
            Some(opts) => opts,
            None => return Err(Error::new(Status::InvalidArg, "Missing `options` argument")),
        };

        // Parse the initial URI to check if it's a full URL or just a path
        let initial_uri: http::Uri = options
            .url
            .parse()
            .map_err(|_| Error::new(Status::InvalidArg, "Invalid URL"))?;

        let mut final_uri = initial_uri.clone();

        // If we only have a path (no scheme/authority), try to reconstruct from Host header
        if initial_uri.scheme().is_none() && initial_uri.authority().is_none() {
            if let Some(ref headers) = options.headers {
                if let Some(host_value) = headers.get("host".to_string()) {
                    // Reconstruct the full URI using the Host header
                    let scheme = "https"; // Default to HTTPS
                    let full_url = format!(
                        "{}://{}{}",
                        scheme,
                        host_value,
                        initial_uri
                            .path_and_query()
                            .map(|pq| pq.as_str())
                            .unwrap_or("/")
                    );

                    final_uri = full_url.parse().map_err(|_| {
                        Error::new(
                            Status::InvalidArg,
                            "Invalid reconstructed URL from Host header",
                        )
                    })?;
                }
            }
        }

        let mut request = RequestBuilder::new()
            .method(options.method.unwrap_or_else(|| "GET".to_string()).as_str())
            .uri(final_uri);

        if let Some(headers) = options.headers {
            for (key, value) in headers.iter() {
                request = request.header(key, value);
            }
        }

        if let Some(socket_info) = options.socket {
            request = request.socket_info(socket_info.try_into()?);
        }

        if let Some(docroot) = options.docroot {
            request = request.document_root(docroot.into());
        }

        let body = options
            .body
            .map(|body| BytesMut::from(body.deref()))
            .unwrap_or_default();

        let request = request.body(body).expect("Failed to build request");

        Ok(Request(request))
    }

    /// Get the HTTP method for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   method: "GET",
    ///   url: "/index.php"
    /// });
    ///
    /// console.log(request.method); // GET
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn method(&self) -> String {
        self.0.method().to_string()
    }

    /// Set the HTTP method for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///  url: "/index.php"
    /// });
    ///
    /// request.method = "POST";
    /// console.log(request.method); // POST
    /// ```
    #[napi(setter, enumerable = true, js_name = "method")]
    pub fn set_method(&mut self, method: String) -> Result<()> {
        *self.0.method_mut() = method
            .parse()
            .map_err(|_| Error::new(Status::InvalidArg, "Invalid `method` name"))?;

        Ok(())
    }

    /// Get the full URL for the request, including scheme and authority.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   url: "https://example.com/index.php"
    /// });
    ///
    /// console.log(request.url); // https://example.com/index.php
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn url(&self) -> String {
        self.0.uri().to_string()
    }

    /// Set the URL for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///  url: "https://example.com/index.php"
    /// });
    ///
    /// request.url = "https://example.com/new-url";
    /// console.log(request.url); // https://example.com/new-url
    /// ```
    #[napi(setter, enumerable = true, js_name = "url")]
    pub fn set_url(&mut self, url: String) -> Result<()> {
        *self.0.uri_mut() = url
            .parse()
            .map_err(|_| Error::new(Status::InvalidArg, "Invalid URL"))?;

        Ok(())
    }

    /// Get the path portion of the URL for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   url: "https://example.com/api/users?id=123"
    /// });
    ///
    /// console.log(request.path); // /api/users
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn path(&self) -> String {
        self.0.uri().path().to_string()
    }

    /// Get the headers for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   url: "/index.php",
    ///   headers: {
    ///     'Content-Type': ['application/json']
    ///   }
    /// });
    ///
    /// for (const mime of request.headers.getAll('Content-Type')) {
    ///   console.log(mime); // application/json
    /// }
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn headers(&self) -> Headers {
        Headers(self.0.headers().clone())
    }

    /// Set the headers for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///  url: "/index.php"
    /// });
    ///
    /// request.headers = new Headers({
    ///  'Content-Type': ['application/json']
    /// });
    ///
    /// for (const mime of request.headers.getAll('Content-Type')) {
    ///  console.log(mime); // application/json
    /// }
    /// ```
    #[napi(setter, enumerable = true, js_name = "headers")]
    pub fn set_headers(&mut self, headers: Headers) {
        *self.0.headers_mut() = headers.deref().clone();
    }

    /// Get the document root for the request, if applicable.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   url: "/index.php",
    ///   docroot: "/var/www/html"
    /// });
    ///
    /// console.log(request.docroot); // /var/www/html
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn docroot(&self) -> Option<String> {
        self.0.document_root().map(|s| s.path.display().to_string())
    }

    /// Set the document root for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///  url: "/index.php"
    /// });
    ///
    /// request.docroot = "/var/www/html";
    /// console.log(request.docroot); // /var/www/html
    /// ```
    #[napi(setter, enumerable = true, js_name = "docroot")]
    pub fn set_docroot(&mut self, docroot: String) {
        *self.0.document_root_mut() = docroot.into();
    }

    /// Get the body of the request as a Buffer.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   url: "/v2/api/thing",
    ///   body: Buffer.from(JSON.stringify({
    ///     message: 'Hello, world!'
    ///   }))
    /// });
    ///
    /// console.log(request.body.toString()); // {"message":"Hello, world!"}
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn body(&self) -> Buffer {
        Buffer::from(self.0.body().to_vec())
    }

    /// Set the body of the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///  url: "/v2/api/thing"
    /// });
    ///
    /// request.body = Buffer.from(JSON.stringify({
    ///   message: 'Hello, world!'
    /// }));
    ///
    /// console.log(request.body.toString()); // {"message":"Hello, world!"}
    /// ```
    #[napi(setter, enumerable = true, js_name = "body")]
    pub fn set_body(&mut self, body: Buffer) {
        *self.0.body_mut() = BytesMut::from(body.deref());
    }

    /// Convert the response to a JSON object representation.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   method: "GET",
    ///   url: "https://example.com/index.php",
    ///   headers: {
    ///     'Content-Type': ['application/json']
    ///   },
    ///   body: Buffer.from(JSON.stringify({
    ///     message: 'Hello, world!'
    ///   }))
    /// });
    ///
    /// console.log(request.toJSON());
    /// ```
    #[napi(js_name = "toJSON")]
    pub fn to_json(&self, env: &Env) -> Result<Object> {
        let mut obj = Object::new(env)?;
        obj.set("method", self.method())?;
        obj.set("url", self.url())?;
        obj.set("headers", self.headers().to_json(env)?)?;
        obj.set("body", self.body())?;
        Ok(obj)
    }
}

// Rust-only methods (not exposed to JavaScript)
impl Request {
    /// Consume this Request and return the inner HTTP request.
    pub fn into_inner(self) -> InnerRequest {
        self.0
    }
}

impl Clone for Request {
    fn clone(&self) -> Self {
        use crate::RequestExt;

        // Build a new request with all fields cloned
        let mut builder = http::request::Builder::new()
            .method(self.0.method().clone())
            .uri(self.0.uri().clone())
            .version(self.0.version());

        for (key, value) in self.0.headers() {
            builder = builder.header(key.clone(), value.clone());
        }

        let mut req = builder
            .body(self.0.body().clone())
            .expect("Failed to build request");

        // Copy extensions manually
        if let Some(docroot) = self.0.document_root() {
            req.set_document_root(docroot.clone());
        }
        if let Some(socket) = self.0.socket_info() {
            req.set_socket_info(socket.clone());
        }

        Request(req)
    }
}

impl Deref for Request {
    type Target = InnerRequest;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<InnerRequest> for Request {
    fn from(request: InnerRequest) -> Self {
        Request(request)
    }
}

impl FromNapiValue for Request {
    unsafe fn from_napi_value(env: sys::napi_env, value: sys::napi_value) -> Result<Self> {
        // Try to convert from ClassInstance<Request>
        if let Ok(instance) = unsafe { ClassInstance::<Request>::from_napi_value(env, value) } {
            return Ok(instance.deref().clone());
        }

        // If both conversions fail, return an error
        Err(Error::new(Status::InvalidArg, "Expected Request"))
    }
}

//
// Response
//

/// Input options for creating a Response.
#[napi(object)]
#[derive(Default)]
pub struct ResponseOptions {
    /// The HTTP method for the request.
    pub status: Option<u16>,
    /// The headers for the request.
    #[napi(ts_type = "Headers | HeaderMap")]
    pub headers: Option<Headers>,
    /// The body for the request.
    pub body: Option<Buffer>,
    /// The log output for the request.
    pub log: Option<Buffer>,
    /// The exception output for the request.
    pub exception: Option<String>,
}

/// Wraps an http::Response instance to expose it to JavaScript.
///
/// It provides methods to access the status code, headers, and body of the
/// response along with a toJSON method to convert it to a JSON object.
///
/// # Examples
///
/// ```js
/// const response = new Response({
///   status: 200,
///   headers: {
///     'Content-Type': ['application/json']
///   },
///   body: Buffer.from(JSON.stringify({
///     message: 'Hello, world!'
///   }))
/// });
///
/// console.log(response.status); // 200
/// for (const mime of response.headers.getAll('Content-Type')) {
///   console.log(mime); // application/json
/// }
/// console.log(response.body.toString()); // {"message":"Hello, world!"}
/// ```
#[napi]
pub struct Response(InnerResponse);

#[napi]
impl Response {
    /// Create a new Response from a Response instance.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response({
    ///   status: 200,
    ///   headers: {
    ///     'Content-Type': ['application/json']
    ///   },
    ///   body: Buffer.from(JSON.stringify({ message: 'Hello, world!' }))
    /// });
    /// ```
    #[napi(constructor)]
    pub fn new(options: Option<ResponseOptions>) -> Result<Self> {
        let options = options.unwrap_or_default();
        let mut builder = ResponseBuilder::new();

        if let Some(status) = options.status {
            builder = builder.status(status);
        }

        if let Some(headers) = options.headers {
            for (key, value) in headers.iter() {
                builder = builder.header(key, value);
            }
        }

        if let Some(log) = options.log {
            builder = builder.log(BytesMut::from(log.deref()));
        }

        if let Some(exception) = options.exception {
            builder = builder.exception(exception);
        }

        let body = options
            .body
            .map(|body| BytesMut::from(body.deref()))
            .unwrap_or_default();

        let response = builder.body(body).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to build response: {}", e),
            )
        })?;

        Ok(Response(response))
    }

    /// Get the HTTP status code for the response.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response({
    ///   status: 200,
    ///   headers: {
    ///     'Content-Type': ['application/json']
    ///   },
    ///   body: Buffer.from(JSON.stringify({
    ///     message: 'Hello, world!'
    ///   }))
    /// });
    ///
    /// console.log(response.status); // 200
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn status(&self) -> u16 {
        self.0.status().as_u16()
    }

    /// Set the HTTP status code for the response.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response();
    ///
    /// response.status = 404;
    /// console.log(response.status); // 404
    /// ```
    #[napi(setter, enumerable = true, js_name = "status")]
    pub fn set_status(&mut self, status: u16) -> Result<()> {
        *self.0.status_mut() = http::StatusCode::from_u16(status)
            .map_err(|_| Error::new(Status::InvalidArg, "Invalid status code"))?;
        Ok(())
    }

    /// Get the headers for the response.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response({
    ///   headers: {
    ///     'Content-Type': ['application/json']
    ///   },
    ///   body: Buffer.from(JSON.stringify({
    ///     message: 'Hello, world!'
    ///   }))
    /// });
    ///
    /// for (const mime of response.headers.get('Content-Type')) {
    ///   console.log(mime); // application/json
    /// }
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn headers(&self) -> Headers {
        Headers(self.0.headers().clone())
    }

    /// Set the headers for the response.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response();
    ///
    /// response.headers = new Headers({
    ///  'Content-Type': ['application/json']
    /// });
    ///
    /// for (const mime of response.headers.getAll('Content-Type')) {
    ///  console.log(mime); // application/json
    /// }
    /// ```
    #[napi(setter, enumerable = true, js_name = "headers")]
    pub fn set_headers(&mut self, headers: Headers) {
        *self.0.headers_mut() = headers.deref().clone();
    }

    /// Get the body of the response as a Buffer.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response({
    ///   body: Buffer.from(JSON.stringify({
    ///     message: 'Hello, world!'
    ///   }))
    /// });
    ///
    /// console.log(response.body.toString()); // {"message":"Hello, world!"}
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn body(&self) -> Buffer {
        Buffer::from(self.0.body().to_vec())
    }

    /// Set the body of the response.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response();
    ///
    /// response.body = Buffer.from(JSON.stringify({
    ///   message: 'Hello, world!'
    /// }));
    ///
    /// console.log(response.body.toString()); // {"message":"Hello, world!"}
    /// ```
    #[napi(setter, enumerable = true, js_name = "body")]
    pub fn set_body(&mut self, body: Buffer) {
        *self.0.body_mut() = BytesMut::from(body.deref());
    }

    /// Get the log of the response as a Buffer.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response({
    ///   log: Buffer.from('Log message')
    /// });
    ///
    /// console.log(response.log.toString()); // Log message
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn log(&self) -> Buffer {
        self.0
            .log()
            .map(|log| Buffer::from(log.as_bytes().to_vec()))
            .unwrap_or_else(|| Buffer::from(vec![]))
    }

    /// Get the exception of the response.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response({
    ///   exception: 'Error message'
    /// });
    ///
    /// console.log(response.exception); // Error message
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn exception(&self) -> Option<String> {
        self.0.exception().map(|e| e.0.clone())
    }

    /// Convert the response to a JSON object representation.
    ///
    /// # Examples
    ///
    /// ```js
    /// const response = new Response({
    ///   status: 200,
    ///   headers: {
    ///     'Content-Type': ['application/json']
    ///   },
    ///   body: Buffer.from(JSON.stringify({
    ///     message: 'Hello, world!'
    ///   }))
    /// });
    ///
    /// console.log(response.toJSON());
    /// ```
    #[napi(js_name = "toJSON")]
    pub fn to_json(&self, env: &Env) -> Result<Object> {
        let mut obj = Object::new(env)?;
        obj.set("status", self.status())?;
        obj.set("headers", self.headers().to_json(env)?)?;
        obj.set("body", self.body())?;

        // Only include log if it has content
        if let Some(log) = self.0.log() {
            if !log.is_empty() {
                obj.set("log", Buffer::from(log.as_bytes().to_vec()))?;
            }
        }

        // Include exception if present
        if let Some(exception) = self.0.exception() {
            obj.set("exception", exception.0.clone())?;
        }

        Ok(obj)
    }
}

impl Deref for Response {
    type Target = InnerResponse;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Response {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<InnerResponse> for Response {
    fn from(response: InnerResponse) -> Self {
        Response(response)
    }
}
