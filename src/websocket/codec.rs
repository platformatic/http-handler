//! WebSocket codec for use with tokio_util::codec::Framed.
//!
//! This codec provides a clean abstraction over DuplexStream, turning raw bytes
//! into a Stream of WebSocket frames.

use super::frame::{WebSocketError, WebSocketFrame, WebSocketOpcode};
use bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

/// WebSocket codec that implements tokio_util's Decoder and Encoder traits.
///
/// This codec handles:
/// - Frame parsing from byte buffers
/// - Message fragmentation and reassembly
/// - Frame encoding to byte buffers
///
/// Use with `tokio_util::codec::Framed` to turn a DuplexStream into a
/// `Stream<Item = WebSocketFrame>` and `Sink<WebSocketFrame>`.
pub struct WebSocketCodec {
    /// Fragments being assembled into a complete message
    fragments: Vec<Vec<u8>>,
    /// Opcode of the first fragment (determines final message type)
    message_opcode: Option<WebSocketOpcode>,
}

impl WebSocketCodec {
    /// Create a new WebSocket codec.
    pub fn new() -> Self {
        Self {
            fragments: Vec::new(),
            message_opcode: None,
        }
    }
}

impl Default for WebSocketCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for WebSocketCodec {
    type Item = WebSocketFrame;
    type Error = WebSocketError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Try to parse a frame from the buffer
        match WebSocketFrame::parse(src) {
            Ok((frame, consumed)) => {
                // Advance the buffer by the number of bytes consumed
                src.advance(consumed);

                // Handle control frames (ping, pong, close)
                // These are never fragmented and should be returned immediately
                if frame.opcode.is_control() {
                    return Ok(Some(frame));
                }

                // Handle data frames (text, binary, continuation)
                match frame.opcode {
                    WebSocketOpcode::Text | WebSocketOpcode::Binary => {
                        // First fragment of a new message
                        self.message_opcode = Some(frame.opcode);
                        self.fragments.push(frame.payload.clone());

                        if frame.fin {
                            // Single-frame message - complete immediately
                            let opcode = self.message_opcode.take().unwrap();
                            let payload = self.fragments.drain(..).flatten().collect();

                            Ok(Some(WebSocketFrame::new_data(opcode, payload, true)))
                        } else {
                            // More fragments coming, wait for them
                            Ok(None)
                        }
                    }
                    WebSocketOpcode::Continuation => {
                        // Continuation of a fragmented message
                        if self.message_opcode.is_none() {
                            // Continuation without initial frame - protocol error
                            // For now, we'll treat this as incomplete
                            return Ok(None);
                        }

                        self.fragments.push(frame.payload.clone());

                        if frame.fin {
                            // Final fragment - assemble complete message
                            let opcode = self.message_opcode.take().unwrap();
                            let payload = self.fragments.drain(..).flatten().collect();

                            Ok(Some(WebSocketFrame::new_data(opcode, payload, true)))
                        } else {
                            // More fragments coming, wait for them
                            Ok(None)
                        }
                    }
                    // Control frames handled above
                    _ => unreachable!(),
                }
            }
            Err(WebSocketError::IncompleteFrame) => {
                // Need more data
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

impl Encoder<WebSocketFrame> for WebSocketCodec {
    type Error = WebSocketError;

    fn encode(&mut self, frame: WebSocketFrame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Encode the frame (no masking for server->client frames)
        let encoded = frame.encode(None);

        // Write to the destination buffer
        dst.extend_from_slice(&encoded);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_decode_single_frame() {
        let mut codec = WebSocketCodec::new();

        // Create a simple text frame
        let frame = WebSocketFrame::new_text("Hello".to_string(), true);
        let encoded = frame.encode(None);

        let mut buffer = BytesMut::from(&encoded[..]);
        let decoded = codec.decode(&mut buffer).unwrap();

        assert!(decoded.is_some());
        let decoded_frame = decoded.unwrap();
        assert_eq!(decoded_frame.opcode, WebSocketOpcode::Text);
        assert_eq!(decoded_frame.payload, b"Hello");
        assert!(decoded_frame.fin);
    }

    #[test]
    fn test_decode_fragmented_message() {
        let mut codec = WebSocketCodec::new();

        // First fragment
        let frame1 = WebSocketFrame::new_text("Hel".to_string(), false);
        let encoded1 = frame1.encode(None);

        let mut buffer = BytesMut::from(&encoded1[..]);
        let result = codec.decode(&mut buffer).unwrap();
        assert!(result.is_none()); // Not complete yet

        // Second fragment (continuation)
        let frame2 = WebSocketFrame::new_continuation(b"lo".to_vec(), true);
        let encoded2 = frame2.encode(None);

        buffer.extend_from_slice(&encoded2);
        let result = codec.decode(&mut buffer).unwrap();

        assert!(result.is_some());
        let decoded_frame = result.unwrap();
        assert_eq!(decoded_frame.opcode, WebSocketOpcode::Text);
        assert_eq!(decoded_frame.payload, b"Hello");
        assert!(decoded_frame.fin);
    }

    #[test]
    fn test_encode_frame() {
        let mut codec = WebSocketCodec::new();
        let mut buffer = BytesMut::new();

        let frame = WebSocketFrame::new_binary(vec![1, 2, 3], true);
        codec.encode(frame, &mut buffer).unwrap();

        assert!(!buffer.is_empty());

        // Decode it back to verify
        let mut decode_codec = WebSocketCodec::new();
        let decoded = decode_codec.decode(&mut buffer).unwrap();

        assert!(decoded.is_some());
        let decoded_frame = decoded.unwrap();
        assert_eq!(decoded_frame.opcode, WebSocketOpcode::Binary);
        assert_eq!(decoded_frame.payload, vec![1, 2, 3]);
    }

    #[test]
    fn test_control_frame_immediate_return() {
        let mut codec = WebSocketCodec::new();

        // Create a ping frame
        let frame = WebSocketFrame::new_ping(b"test".to_vec());
        let encoded = frame.encode(None);

        let mut buffer = BytesMut::from(&encoded[..]);
        let decoded = codec.decode(&mut buffer).unwrap();

        // Control frames should be returned immediately
        assert!(decoded.is_some());
        let decoded_frame = decoded.unwrap();
        assert_eq!(decoded_frame.opcode, WebSocketOpcode::Ping);
        assert_eq!(decoded_frame.payload, b"test");
    }
}
