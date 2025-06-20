use std::{
    collections::HashMap,
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

use bytes::BytesMut;
use http::{HeaderMap, HeaderName, HeaderValue};
use napi::{Either, Error, Result, Status, bindgen_prelude::*};
use napi_derive::napi;

use crate::{Request, RequestBuilderExt, RequestExt, Response, ResponseBuilderExt, SocketInfo};

//
// NapiHeaderMap
//

// TODO: How can we handle both ClassInstance<NapiHeaders> and NapiHeaderMap?
// pub type NapiHeadersInput<'a> = Either<ClassInstance<'a, NapiHeaders>, NapiHeaderMap>;

/// A header entry value, which can be either a string or array of strings.
#[napi]
pub type NapiHeaderMapValue = Either<String, Vec<String>>;

/// A multi-map of HTTP headers. Any given header key can have multiple values.
#[napi(transparent)]
#[derive(Default)]
pub struct NapiHeaderMap(pub HashMap<String, NapiHeaderMapValue>);

impl TryFrom<NapiHeaderMap> for HeaderMap {
    type Error = Error;

    fn try_from(map: NapiHeaderMap) -> std::result::Result<Self, Self::Error> {
        let mut headers = HeaderMap::new();

        for (key, value) in map.0 {
            let header_name = HeaderName::try_from(key).map_err(|e| {
                Error::new(Status::InvalidArg, format!("Invalid header name: {}", e))
            })?;

            match value {
                Either::A(value) => {
                    let header_value = HeaderValue::try_from(value).map_err(|e| {
                        Error::new(Status::InvalidArg, format!("Invalid header value: {}", e))
                    })?;
                    headers.insert(header_name, header_value);
                }
                Either::B(values) => {
                    for value in values {
                        let header_value = HeaderValue::try_from(value).map_err(|e| {
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

#[napi]
struct NapiHeaderValue(HeaderValue);

impl Deref for NapiHeaderValue {
    type Target = HeaderValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NapiHeaderValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TryFrom<String> for NapiHeaderValue {
    type Error = Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        HeaderValue::try_from(value)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid header value: {}", e)))
            .map(NapiHeaderValue)
    }
}

//
// SocketInfo
//

/// Input options for creating a NapiSocketInfo.
#[napi(object)]
#[derive(Default)]
pub struct NapiSocketInfo {
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

impl TryFrom<SocketInfo> for NapiSocketInfo {
    type Error = Error;

    fn try_from(socket: SocketInfo) -> Result<Self> {
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

        Ok(NapiSocketInfo {
            local_address,
            local_port,
            local_family,
            remote_address,
            remote_port,
            remote_family,
        })
    }
}

impl TryFrom<NapiSocketInfo> for SocketInfo {
    type Error = Error;

    fn try_from(socket: NapiSocketInfo) -> std::result::Result<Self, Self::Error> {
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
// NapiHeaders
//

/// A NapiHeaders wraps an http::HeaderMap instance to expose it to JavaScript.
///
/// It provides methods to access and modify HTTP headers, iterate over them,
/// and convert them to a JSON object representation.
#[napi(js_name = "Headers")]
#[derive(Debug, Clone, Default)]
pub struct NapiHeaders(HeaderMap);

impl Deref for NapiHeaders {
    type Target = HeaderMap;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NapiHeaders {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromNapiValue for NapiHeaders {
    unsafe fn from_napi_value(env: sys::napi_env, value: sys::napi_value) -> Result<Self> {
        // Try to convert from ClassInstance<NapiHeaders>
        if let Ok(instance) = unsafe { ClassInstance::<NapiHeaders>::from_napi_value(env, value) } {
            return Ok(NapiHeaders(instance.0.clone()));
        }

        // If that fails, try to convert from NapiHeaderMap
        if let Ok(header_map) = unsafe { NapiHeaderMap::from_napi_value(env, value) } {
            return Ok(NapiHeaders(header_map.try_into()?));
        }

        // If both conversions fail, return an error
        Err(Error::new(
            Status::InvalidArg,
            "Expected Headers or NapiHeaderMap",
        ))
    }
}

#[napi]
impl NapiHeaders {
    // TODO: accept Either<ClassInstance<NapiHeaders>, NapiHeaderMap>
    /// Create a new NapiHeaders instance.
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
    pub fn new(options: Option<NapiHeaderMap>) -> Result<Self> {
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
        self.0
            .get(&key)
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
    /// console.log(headers.getLine('Accept')); // application/json, text/html
    /// ```
    #[napi]
    pub fn get_line(&self, key: String) -> Option<String> {
        let values = self.get_all(key);
        if values.is_empty() {
            None
        } else {
            Some(values.join(", "))
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
    pub fn set(&mut self, key: String, value: NapiHeaderMapValue) -> Result<bool> {
        let key = HeaderName::try_from(key)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Invalid header name: {}", e)))?;

        let had_value = self.0.remove(&key).is_some();

        match value {
            Either::A(value) => {
                let value = HeaderValue::try_from(value).map_err(|e| {
                    Error::new(Status::InvalidArg, format!("Invalid header value: {}", e))
                })?;
                self.0.insert(key, value);
            }
            Either::B(values) => {
                for value in values {
                    let value = HeaderValue::try_from(value).map_err(|e| {
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

        let value = HeaderValue::try_from(value)
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
        self.0.len() as u32
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

/// Input options for creating a NapiRequest.
#[napi(object)]
#[derive(Default)]
pub struct NapiRequestOptions {
    /// The HTTP method for the request.
    pub method: Option<String>,
    /// The URI for the request.
    pub uri: String,
    /// The headers for the request.
    #[napi(ts_type = "Headers | NapiHeaderMap")]
    pub headers: Option<NapiHeaders>,
    /// The body for the request.
    pub body: Option<Buffer>,
    /// The socket information for the request.
    pub socket: Option<NapiSocketInfo>,
    /// Document root for the request, if applicable.
    pub docroot: Option<String>,
}

/// A NapiRequest wraps an http::Request instance to expose it to JavaScript.
///
/// It provides methods to access the HTTP method, URI, headers, and body of
/// the request along with a toJSON method to convert it to a JSON object.
#[napi(js_name = "Request")]
#[derive(Debug, Clone)]
pub struct NapiRequest(Request);

#[napi]
impl NapiRequest {
    /// Create a new NapiRequest from a Request instance.
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
    pub fn new(options: NapiRequestOptions) -> Result<Self> {
        let mut request = http::request::Builder::new()
            .method(options.method.unwrap_or_else(|| "GET".to_string()).as_str())
            .uri(&options.uri);

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

        let request = request
            .uri(options.uri.as_str())
            .body(body)
            .expect("Failed to build request");

        Ok(NapiRequest(request))
    }

    /// Get the HTTP method for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   method: "GET",
    ///   uri: "/index.php"
    /// });
    ///
    /// console.log(request.method); // GET
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn method(&self) -> String {
        self.0.method().to_string()
    }

    /// Get the URI for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   uri: "/index.php"
    /// });
    ///
    /// console.log(request.uri); // /index.php
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn uri(&self) -> String {
        self.0.uri().to_string()
    }

    /// Get the headers for the request.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   uri: "/index.php",
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
    pub fn headers(&self) -> NapiHeaders {
        NapiHeaders(self.0.headers().clone())
    }

    /// Get the document root for the request, if applicable.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   uri: "/index.php",
    ///   docroot: "/var/www/html"
    /// });
    ///
    /// console.log(request.docroot); // /var/www/html
    /// ```
    #[napi(getter, enumerable = true)]
    pub fn docroot(&self) -> Option<String> {
        self.0.document_root().map(|s| s.path.display().to_string())
    }

    /// Get the body of the request as a Buffer.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   uri: "/v2/api/thing",
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

    /// Convert the response to a JSON object representation.
    ///
    /// # Examples
    ///
    /// ```js
    /// const request = new Request({
    ///   method: "GET",
    ///   uri: "/index.php",
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
        obj.set("uri", self.uri())?;
        obj.set("headers", self.headers().to_json(env)?)?;
        obj.set("body", self.body())?;
        Ok(obj)
    }
}

impl Deref for NapiRequest {
    type Target = Request;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NapiRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// TODO: Get rid of these and use derefs instead
// impl From<&NapiRequest> for Request {
//     fn from(request: &NapiRequest) -> Self {
//         request.0.to_owned()
//     }
// }

impl From<Request> for NapiRequest {
    fn from(request: Request) -> Self {
        NapiRequest(request)
    }
}

impl FromNapiValue for NapiRequest {
    unsafe fn from_napi_value(env: sys::napi_env, value: sys::napi_value) -> Result<Self> {
        // Try to convert from ClassInstance<NapiRequest>
        if let Ok(instance) = unsafe { ClassInstance::<NapiRequest>::from_napi_value(env, value) } {
            return Ok(instance.deref().clone());
        }

        // If both conversions fail, return an error
        Err(Error::new(Status::InvalidArg, "Expected NapiRequest"))
    }
}

//
// Response
//

/// Input options for creating a NapiResponse.
#[napi(object)]
#[derive(Default)]
pub struct NapiResponseOptions {
    /// The HTTP method for the request.
    pub status: Option<u16>,
    /// The headers for the request.
    #[napi(ts_type = "Headers | NapiHeaderMap")]
    pub headers: Option<NapiHeaders>,
    /// The body for the request.
    pub body: Option<Buffer>,
    /// The log output for the request.
    pub log: Option<Buffer>,
}

/// A NapiResponse wraps an http::Response instance to expose it to JavaScript.
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
#[napi(js_name = "Response")]
pub struct NapiResponse(Response);

#[napi]
impl NapiResponse {
    /// Create a new NapiResponse from a Response instance.
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
    pub fn new(options: NapiResponseOptions) -> Result<Self> {
        let mut builder = http::response::Builder::new();

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

        Ok(NapiResponse(response))
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
    pub fn status(&self) -> i32 {
        self.0.status().as_u16() as i32
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
    pub fn headers(&self) -> NapiHeaders {
        NapiHeaders(self.0.headers().clone())
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
        Ok(obj)
    }
}

impl Deref for NapiResponse {
    type Target = Response;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NapiResponse {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// TODO: Get rid of these and use derefs instead
impl From<&NapiResponse> for Response {
    fn from(response: &NapiResponse) -> Self {
        response.0.to_owned()
    }
}

impl From<Response> for NapiResponse {
    fn from(response: Response) -> Self {
        NapiResponse(response)
    }
}
