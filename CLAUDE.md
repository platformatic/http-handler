# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build Commands
```bash
cargo build                    # Build in debug mode
cargo build --release         # Build in release mode
cargo build --features napi   # Build with Node.js bindings support
```

### Test Commands
```bash
cargo test                    # Run all tests
cargo test -- --nocapture     # Run tests with output
cargo test test_name          # Run specific test by name
```

### Lint and Format Commands
```bash
cargo fmt                     # Format code
cargo fmt -- --check          # Check formatting without changes
cargo clippy                  # Run linter
cargo clippy -- -D warnings   # Treat warnings as errors
```

### Documentation Commands
```bash
cargo doc                     # Generate documentation
cargo doc --open             # Generate and open documentation
```

## Architecture Overview

This is a Rust library for managing HTTP requests across multiple language runtimes. The crate is a standard Rust library (`rlib`) with optional Node.js bindings via NAPI.

### Core Components

- **Headers** (`src/headers.rs`): HTTP header management with support for multiple values per header name
- **Request** (`src/request.rs`): HTTP request representation with builder pattern API
- **Response** (`src/response.rs`): HTTP response representation with builder pattern, includes logging and exception fields
- **Handler** (`src/handler.rs`): Trait for implementing request handlers that transform requests into responses

### Key Design Patterns

1. **Builder Pattern**: Both Request and Response use builders for construction
2. **Trait-based Handler**: The Handler trait allows different implementations for request processing

### Node.js Integration

When built with `--features napi`, the library can be used from Node.js. The build.rs script handles NAPI setup during compilation.

### Testing Support

The library includes `MockRoot` and `MockRootBuilder` utilities for creating temporary file systems during testing, useful for testing file-serving handlers.
