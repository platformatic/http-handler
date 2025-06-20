# TODO: Refactor to use http crate

This document outlines the tasks for refactoring lang_handler to use the standard http crate types instead of custom implementations.

## Overview

Replace custom Request, Response, and Headers types with types from the http crate, preserving custom functionality through the Extensions API and extension traits.

## Phase 1: Setup and Dependencies

- [x] Add dependencies to Cargo.toml:
  - `http = "1.0"`
  - `http-body = "1.0"`
  - `bytes = "1.0"` (already present)
- [x] Create `src/extensions.rs` module for extension types:
  - [x] `SocketInfo` - store local/remote socket addresses
  - [x] `ResponseLog` - log buffer using Bytes
  - [x] `ResponseException` - exception message storage
  - [x] Implement Clone, Debug traits for all extensions

## Phase 2: Extension Traits

Create extension traits that add convenience methods to http types:

- [x] `RequestExt` trait for `http::Request<T>`:
  - [x] `socket_info(&self) -> Option<&SocketInfo>`
  - [x] `socket_info_mut(&mut self) -> &mut SocketInfo`
  - [x] `set_socket_info(&mut self, info: SocketInfo)`

- [x] `ResponseExt` trait for `http::Response<T>`:
  - [x] `log(&self) -> Option<&ResponseLog>`
  - [x] `log_mut(&mut self) -> &mut ResponseLog`
  - [x] `set_log(&mut self, log: impl Into<Bytes>)`
  - [x] `append_log(&mut self, data: impl AsRef<[u8]>)`
  - [x] `exception(&self) -> Option<&ResponseException>`
  - [x] `set_exception(&mut self, exception: impl Into<String>)`

## Phase 3: Core Type Migration

### Request Migration
- [x] Remove custom Request struct and RequestBuilder
- [x] Define type alias: `pub type Request = http::Request<Bytes>`
- [x] Implement `RequestExt` for `http::Request<T>`
- [x] Update all Request construction to use `http::Request::builder()`

### Response Migration
- [x] Remove custom Response struct and ResponseBuilder
- [x] Define type alias: `pub type Response = http::Response<Bytes>`
- [x] Implement `ResponseExt` for `http::Response<T>`
- [x] Update all Response construction to use `http::Response::builder()`

### Headers Migration
- [x] Remove custom Headers type and Header enum
- [x] Use `http::HeaderMap` directly
- [x] Update all header operations to use HeaderMap API

## Phase 4: Handler Trait Update

- [x] Update Handler trait:
  ```rust
  trait Handler {
      type Error;
      fn handle(&self, request: http::Request<Bytes>) -> Result<http::Response<Bytes>, Self::Error>;
  }
  ```
- [x] Update all Handler implementations

## Phase 5: Extract Rewrite Module

- [x] Delete old rewrite module as its functionality has been reproduced in a separate crate.

## Phase 6: NAPI Bindings

Create intermediate types for NAPI that implement Deref/DerefMut:

- [x] `NapiRequest` - wraps `http::Request<Bytes>`
  - [x] Implement Deref/DerefMut to `http::Request<Bytes>`
  - [x] Implement NAPI conversion traits
  - [x] Handle socket info in conversions

- [x] `NapiResponse` - wraps `http::Response<Bytes>`
  - [x] Implement Deref/DerefMut to `http::Response<Bytes>`
  - [x] Implement NAPI conversion traits
  - [x] Handle log/exception in conversions

- [x] `NapiHeaders` - wraps `http::HeaderMap`
  - [x] Implement Deref/DerefMut to `http::HeaderMap`
  - [x] Implement NAPI conversion traits
  - [x] Maintain multi-value support
