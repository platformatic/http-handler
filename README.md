# http-handler

> [!WARNING]
> Not yet released

A request handler framework for the [http](https://docs.rs/http) crate.
It provides a `Handler` trait that allows for easy handling of HTTP requests
and responses, along with a few extension types to store additional information
about the request and response including socket information, response logs,
and exceptions.

## Install

```sh
cargo add http-handler
```

## Usage

```rust
use http::{Request, Response, response::Builder as ResponseBuilder};
use http_handler::{Handler, RequestExt, ResponseExt, SocketInfo, ResponseLog, ResponseException};

struct MyHandler;

impl Handler<String> for MyHandler {
    type Error = String;

    fn handle(&self, request: Request<String>) -> Result<Response<String>, Self::Error> {
        ResponseBuilder::new()
            .status(200)
            .header("Content-Type", "text/plain")
            .body("Hello, World!".to_string())
            .map_err(|e| e.to_string())
    }
}

fn main() {
    let handler = MyHandler;

    let request = Request::builder()
        .method("GET")
        .uri("http://example.com")
        .body("".to_string())
        .unwrap();

    match handler.handle(request) {
        Ok(response) => {
            println!("Response: {:?}", response);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}
```
