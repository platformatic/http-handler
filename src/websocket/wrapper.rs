//! WebSocket decoder and encoder wrappers that use WebSocketCodec.
//!
//! These types provide a clean API for JavaScript bindings while using
//! the WebSocketCodec for frame parsing and encoding.

use super::{WebSocketCodec, WebSocketError, WebSocketFrame};
use bytes::BytesMut;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio_util::codec::{Decoder, Encoder};

/// WebSocket message decoder that reads and assembles frames.
///
/// Uses WebSocketCodec internally to handle frame parsing and message assembly.
pub struct WebSocketDecoder<R> {
    reader: R,
    codec: WebSocketCodec,
    buffer: BytesMut,
}

impl<R: AsyncReadExt + Unpin> WebSocketDecoder<R> {
    /// Create a new WebSocketDecoder from any AsyncRead type.
    pub fn new(reader: R) -> Self {
        WebSocketDecoder {
            reader,
            codec: WebSocketCodec::new(),
            buffer: BytesMut::with_capacity(8192),
        }
    }

    /// Read the next WebSocket message.
    ///
    /// Returns `Ok(Some(frame))` if a complete frame was read,
    /// `Ok(None)` if the stream ended, or `Err` on error.
    pub async fn read_message(&mut self) -> Result<Option<WebSocketFrame>, WebSocketError> {
        loop {
            // Try to decode a frame from the buffer
            match self.codec.decode(&mut self.buffer)? {
                Some(frame) => return Ok(Some(frame)),
                None => {
                    // Need more data - read from stream
                    let mut temp_buf = vec![0u8; 8192];

                    match self.reader.read(&mut temp_buf).await {
                        Ok(0) => return Ok(None), // EOF
                        Ok(n) => {
                            self.buffer.extend_from_slice(&temp_buf[..n]);
                            // Loop to try decoding again
                        }
                        Err(e) => return Err(WebSocketError::IoError(e.to_string())),
                    }
                }
            }
        }
    }
}

/// WebSocket message encoder that generates and writes frames.
///
/// Uses WebSocketCodec internally to handle frame encoding.
pub struct WebSocketEncoder<W> {
    writer: Arc<Mutex<W>>,
    codec: Mutex<WebSocketCodec>,
}

impl<W: AsyncWriteExt + Unpin + Send> WebSocketEncoder<W> {
    /// Create a new WebSocketEncoder from any AsyncWrite type.
    pub fn new(writer: W) -> Self {
        WebSocketEncoder {
            writer: Arc::new(Mutex::new(writer)),
            codec: Mutex::new(WebSocketCodec::new()),
        }
    }

    /// Write a text message.
    pub async fn write_text(&self, text: &str, _masked: bool) -> Result<(), WebSocketError> {
        let frame = WebSocketFrame::new_text(text.to_string(), true);
        let mut buffer = BytesMut::new();

        // Lock the codec to encode the frame
        let mut codec = self.codec.lock().await;
        codec.encode(frame, &mut buffer)?;
        drop(codec); // Release lock early

        let mut writer = self.writer.lock().await;
        writer
            .write_all(&buffer)
            .await
            .map_err(|e| WebSocketError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Write a binary message.
    pub async fn write_binary(&self, data: &[u8], _masked: bool) -> Result<(), WebSocketError> {
        let frame = WebSocketFrame::new_binary(data.to_vec(), true);
        let mut buffer = BytesMut::new();

        // Lock the codec to encode the frame
        let mut codec = self.codec.lock().await;
        codec.encode(frame, &mut buffer)?;
        drop(codec); // Release lock early

        let mut writer = self.writer.lock().await;
        writer
            .write_all(&buffer)
            .await
            .map_err(|e| WebSocketError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Send a close frame with optional code and reason, then close the stream.
    pub async fn write_close(
        &self,
        code: Option<u16>,
        reason: Option<&str>,
    ) -> Result<(), WebSocketError> {
        let frame = WebSocketFrame::new_close(code, reason);
        let mut buffer = BytesMut::new();

        // Lock the codec to encode the frame
        let mut codec = self.codec.lock().await;
        codec.encode(frame, &mut buffer)?;
        drop(codec); // Release lock early

        let mut writer = self.writer.lock().await;
        writer
            .write_all(&buffer)
            .await
            .map_err(|e| WebSocketError::IoError(e.to_string()))?;

        // Shutdown the stream
        writer
            .shutdown()
            .await
            .map_err(|e| WebSocketError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Close the encoder stream without sending a close frame.
    pub async fn end(&self) -> Result<(), WebSocketError> {
        let mut writer = self.writer.lock().await;
        writer
            .shutdown()
            .await
            .map_err(|e| WebSocketError::IoError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_encoder_decoder_creation() {
        let (client, server) = duplex(1024);

        let _encoder = WebSocketEncoder::new(client);
        let _decoder = WebSocketDecoder::new(server);
    }

    #[tokio::test]
    async fn test_write_and_read_text_message() {
        let (client, server) = duplex(1024);

        let encoder = WebSocketEncoder::new(client);
        let mut decoder = WebSocketDecoder::new(server);

        // Write a text message
        encoder.write_text("Hello WebSocket!", false).await.unwrap();

        // Read it back
        let frame = decoder.read_message().await.unwrap().unwrap();
        assert!(frame.is_text());
        assert_eq!(frame.payload_as_text().unwrap(), "Hello WebSocket!");
    }

    #[tokio::test]
    async fn test_write_and_read_binary_message() {
        let (client, server) = duplex(1024);

        let encoder = WebSocketEncoder::new(client);
        let mut decoder = WebSocketDecoder::new(server);

        // Write binary data
        let data = vec![0x01, 0x02, 0x03, 0x04];
        encoder.write_binary(&data, false).await.unwrap();

        // Read it back
        let frame = decoder.read_message().await.unwrap().unwrap();
        assert!(frame.is_binary());
        assert_eq!(frame.payload, data);
    }

    #[tokio::test]
    async fn test_write_close_shuts_down_stream() {
        let (client, server) = duplex(1024);

        let encoder = WebSocketEncoder::new(client);
        let mut decoder = WebSocketDecoder::new(server);

        // Send a close frame
        encoder
            .write_close(Some(1000), Some("Normal closure"))
            .await
            .unwrap();

        // Read the close frame
        let frame = decoder.read_message().await.unwrap().unwrap();
        assert!(frame.is_close());

        // Try to read again - should get None (EOF) because stream was shut down
        let eof = decoder.read_message().await.unwrap();
        assert!(eof.is_none(), "Expected EOF after close frame");

        // Verify we can't write more (stream is closed)
        let write_result = encoder.write_text("Should fail", false).await;
        assert!(write_result.is_err(), "Write should fail after close");
    }

    #[tokio::test]
    async fn test_end_shuts_down_stream_without_close_frame() {
        let (client, server) = duplex(1024);

        let encoder = WebSocketEncoder::new(client);
        let mut decoder = WebSocketDecoder::new(server);

        // Write a message first
        encoder.write_text("Hello", false).await.unwrap();

        // Read it
        let frame = decoder.read_message().await.unwrap().unwrap();
        assert_eq!(frame.payload_as_text().unwrap(), "Hello");

        // Call end() to close stream without sending close frame
        encoder.end().await.unwrap();

        // Should get EOF immediately (no close frame)
        let eof = decoder.read_message().await.unwrap();
        assert!(eof.is_none(), "Expected EOF after end()");
    }

    #[tokio::test]
    async fn test_multiple_messages_then_close() {
        let (client, server) = duplex(2048);

        let encoder = WebSocketEncoder::new(client);
        let mut decoder = WebSocketDecoder::new(server);

        // Send multiple messages
        encoder.write_text("Message 1", false).await.unwrap();
        encoder.write_text("Message 2", false).await.unwrap();
        encoder.write_binary(&[1, 2, 3], false).await.unwrap();

        // Read them back
        let msg1 = decoder.read_message().await.unwrap().unwrap();
        assert_eq!(msg1.payload_as_text().unwrap(), "Message 1");

        let msg2 = decoder.read_message().await.unwrap().unwrap();
        assert_eq!(msg2.payload_as_text().unwrap(), "Message 2");

        let msg3 = decoder.read_message().await.unwrap().unwrap();
        assert_eq!(msg3.payload, vec![1, 2, 3]);

        // Now close
        encoder.write_close(None, None).await.unwrap();

        let close_frame = decoder.read_message().await.unwrap().unwrap();
        assert!(close_frame.is_close());

        // EOF after close
        assert!(decoder.read_message().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_close_cannot_be_called_twice() {
        let (client, _server) = duplex(1024);

        let encoder = WebSocketEncoder::new(client);

        // First close should succeed
        encoder.write_close(Some(1000), None).await.unwrap();

        // Second close should fail (stream already shut down)
        let result = encoder.write_close(Some(1000), None).await;
        assert!(result.is_err(), "Second close should fail");
    }

    #[tokio::test]
    async fn test_end_is_idempotent() {
        let (client, _server) = duplex(1024);

        let encoder = WebSocketEncoder::new(client);

        // First end should succeed
        encoder.end().await.unwrap();

        // Second end should also succeed (shutdown is idempotent)
        encoder.end().await.unwrap();
    }
}
