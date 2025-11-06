//! WebSocket frame parsing and encoding conforming to RFC 6455.

use std::fmt;

/// WebSocket opcodes as defined in RFC 6455 Section 5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WebSocketOpcode {
    /// Continuation frame (0x0)
    Continuation = 0x0,
    /// Text data frame (0x1)
    Text = 0x1,
    /// Binary data frame (0x2)
    Binary = 0x2,
    /// Connection close frame (0x8)
    Close = 0x8,
    /// Ping frame (0x9)
    Ping = 0x9,
    /// Pong frame (0xA)
    Pong = 0xA,
}

impl WebSocketOpcode {
    /// Parse opcode from 4-bit value.
    fn from_u8(value: u8) -> Result<Self, WebSocketError> {
        match value {
            0x0 => Ok(WebSocketOpcode::Continuation),
            0x1 => Ok(WebSocketOpcode::Text),
            0x2 => Ok(WebSocketOpcode::Binary),
            0x8 => Ok(WebSocketOpcode::Close),
            0x9 => Ok(WebSocketOpcode::Ping),
            0xA => Ok(WebSocketOpcode::Pong),
            _ => Err(WebSocketError::InvalidOpcode(value)),
        }
    }

    /// Check if this is a control frame opcode.
    pub fn is_control(&self) -> bool {
        matches!(
            self,
            WebSocketOpcode::Close | WebSocketOpcode::Ping | WebSocketOpcode::Pong
        )
    }

    /// Check if this is a data frame opcode.
    pub fn is_data(&self) -> bool {
        matches!(
            self,
            WebSocketOpcode::Text | WebSocketOpcode::Binary | WebSocketOpcode::Continuation
        )
    }
}

/// WebSocket frame structure per RFC 6455 Section 5.2.
#[derive(Debug, Clone)]
pub struct WebSocketFrame {
    /// FIN bit: indicates this is the final fragment of a message
    pub fin: bool,
    /// RSV1 bit: reserved for extensions
    pub rsv1: bool,
    /// RSV2 bit: reserved for extensions
    pub rsv2: bool,
    /// RSV3 bit: reserved for extensions
    pub rsv3: bool,
    /// Opcode: identifies the frame type
    pub opcode: WebSocketOpcode,
    /// Mask bit: indicates if payload is masked (always true for client→server)
    pub masked: bool,
    /// Payload data
    pub payload: Vec<u8>,
}

/// Errors that can occur during WebSocket frame parsing/encoding.
#[derive(Debug)]
pub enum WebSocketError {
    /// Invalid opcode value
    InvalidOpcode(u8),
    /// Incomplete frame data
    IncompleteFrame,
    /// Control frame exceeds maximum length (125 bytes)
    ControlFrameTooLarge,
    /// Control frame is fragmented (FIN=0)
    ControlFrameFragmented,
    /// Reserved bits are set without negotiated extension
    ReservedBitsSet,
    /// Invalid UTF-8 in text frame
    InvalidUtf8,
    /// Frame too large
    FrameTooLarge,
    /// I/O error
    IoError(String),
}

impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebSocketError::InvalidOpcode(op) => write!(f, "Invalid WebSocket opcode: {:#x}", op),
            WebSocketError::IncompleteFrame => write!(f, "Incomplete WebSocket frame"),
            WebSocketError::ControlFrameTooLarge => {
                write!(f, "Control frame payload exceeds 125 bytes")
            }
            WebSocketError::ControlFrameFragmented => write!(f, "Control frame is fragmented"),
            WebSocketError::ReservedBitsSet => write!(f, "Reserved bits set without extension"),
            WebSocketError::InvalidUtf8 => write!(f, "Invalid UTF-8 in text frame"),
            WebSocketError::FrameTooLarge => write!(f, "Frame too large"),
            WebSocketError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for WebSocketError {}

impl From<std::io::Error> for WebSocketError {
    fn from(err: std::io::Error) -> Self {
        WebSocketError::IoError(err.to_string())
    }
}

impl WebSocketFrame {
    /// Parse a WebSocket frame from bytes.
    ///
    /// Returns the parsed frame and the number of bytes consumed.
    /// Returns `Err(WebSocketError::IncompleteFrame)` if more data is needed.
    ///
    /// # RFC 6455 Frame Format
    ///
    /// ```text
    ///  0                   1                   2                   3
    ///  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
    /// +-+-+-+-+-------+-+-------------+-------------------------------+
    /// |F|R|R|R| opcode|M| Payload len |    Extended payload length    |
    /// |I|S|S|S|  (4)  |A|     (7)     |             (16/64)           |
    /// |N|V|V|V|       |S|             |   (if payload len==126/127)   |
    /// | |1|2|3|       |K|             |                               |
    /// +-+-+-+-+-------+-+-------------+ - - - - - - - - - - - - - - - +
    /// |     Extended payload length continued, if payload len == 127  |
    /// + - - - - - - - - - - - - - - - +-------------------------------+
    /// |                               |Masking-key, if MASK set to 1  |
    /// +-------------------------------+-------------------------------+
    /// | Masking-key (continued)       |          Payload Data         |
    /// +-------------------------------- - - - - - - - - - - - - - - - +
    /// :                     Payload Data continued ...                :
    /// + - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - +
    /// |                     Payload Data continued ...                |
    /// +---------------------------------------------------------------+
    /// ```
    pub fn parse(data: &[u8]) -> Result<(Self, usize), WebSocketError> {
        // Need at least 2 bytes for header
        if data.len() < 2 {
            return Err(WebSocketError::IncompleteFrame);
        }

        // Parse first byte: FIN, RSV1-3, Opcode
        let byte1 = data[0];
        let fin = (byte1 & 0b1000_0000) != 0;
        let rsv1 = (byte1 & 0b0100_0000) != 0;
        let rsv2 = (byte1 & 0b0010_0000) != 0;
        let rsv3 = (byte1 & 0b0001_0000) != 0;
        let opcode = WebSocketOpcode::from_u8(byte1 & 0b0000_1111)?;

        // Parse second byte: MASK, Payload length
        let byte2 = data[1];
        let masked = (byte2 & 0b1000_0000) != 0;
        let mut payload_len = (byte2 & 0b0111_1111) as u64;

        let mut offset = 2;

        // Parse extended payload length if needed
        if payload_len == 126 {
            if data.len() < offset + 2 {
                return Err(WebSocketError::IncompleteFrame);
            }
            payload_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as u64;
            offset += 2;
        } else if payload_len == 127 {
            if data.len() < offset + 8 {
                return Err(WebSocketError::IncompleteFrame);
            }
            payload_len = u64::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            offset += 8;
        }

        // Validate payload length
        if payload_len > usize::MAX as u64 {
            return Err(WebSocketError::FrameTooLarge);
        }
        let payload_len = payload_len as usize;

        // Validate control frames
        if opcode.is_control() {
            if payload_len > 125 {
                return Err(WebSocketError::ControlFrameTooLarge);
            }
            if !fin {
                return Err(WebSocketError::ControlFrameFragmented);
            }
        }

        // Validate reserved bits (must be 0 unless extension is negotiated)
        if rsv1 || rsv2 || rsv3 {
            return Err(WebSocketError::ReservedBitsSet);
        }

        // Parse masking key if present
        let masking_key = if masked {
            if data.len() < offset + 4 {
                return Err(WebSocketError::IncompleteFrame);
            }
            let key = [
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ];
            offset += 4;
            Some(key)
        } else {
            None
        };

        // Parse payload
        if data.len() < offset + payload_len {
            return Err(WebSocketError::IncompleteFrame);
        }

        let mut payload = data[offset..offset + payload_len].to_vec();
        offset += payload_len;

        // Unmask payload if masked
        if let Some(mask) = masking_key {
            Self::apply_mask(&mut payload, &mask);
        }

        // Validate UTF-8 for text frames
        if opcode == WebSocketOpcode::Text && fin && std::str::from_utf8(&payload).is_err() {
            return Err(WebSocketError::InvalidUtf8);
        }

        Ok((
            WebSocketFrame {
                fin,
                rsv1,
                rsv2,
                rsv3,
                opcode,
                masked,
                payload,
            },
            offset,
        ))
    }

    /// Encode a WebSocket frame to bytes.
    ///
    /// # Arguments
    ///
    /// * `mask` - Optional masking key. If provided, the payload will be masked.
    pub fn encode(&self, mask: Option<[u8; 4]>) -> Vec<u8> {
        let mut frame = Vec::new();

        // First byte: FIN, RSV1-3, Opcode
        let mut byte1 = self.opcode as u8;
        if self.fin {
            byte1 |= 0b1000_0000;
        }
        if self.rsv1 {
            byte1 |= 0b0100_0000;
        }
        if self.rsv2 {
            byte1 |= 0b0010_0000;
        }
        if self.rsv3 {
            byte1 |= 0b0001_0000;
        }
        frame.push(byte1);

        // Second byte: MASK, Payload length
        let payload_len = self.payload.len();
        let mut byte2 = if mask.is_some() {
            0b1000_0000
        } else {
            0b0000_0000
        };

        if payload_len < 126 {
            byte2 |= payload_len as u8;
            frame.push(byte2);
        } else if payload_len <= 65535 {
            byte2 |= 126;
            frame.push(byte2);
            frame.extend_from_slice(&(payload_len as u16).to_be_bytes());
        } else {
            byte2 |= 127;
            frame.push(byte2);
            frame.extend_from_slice(&(payload_len as u64).to_be_bytes());
        }

        // Masking key if present
        if let Some(masking_key) = mask {
            frame.extend_from_slice(&masking_key);
        }

        // Payload
        if let Some(masking_key) = mask {
            let mut masked_payload = self.payload.clone();
            Self::apply_mask(&mut masked_payload, &masking_key);
            frame.extend_from_slice(&masked_payload);
        } else {
            frame.extend_from_slice(&self.payload);
        }

        frame
    }

    /// Apply XOR mask to payload data per RFC 6455 Section 5.3.
    ///
    /// This operation is reversible (applying the same mask twice yields the original data).
    fn apply_mask(payload: &mut [u8], mask: &[u8; 4]) {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }

    /// Create a new data frame (text or binary).
    pub fn new_data(opcode: WebSocketOpcode, payload: Vec<u8>, fin: bool) -> Self {
        debug_assert!(opcode.is_data());
        WebSocketFrame {
            fin,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode,
            masked: false,
            payload,
        }
    }

    /// Create a new text frame.
    pub fn new_text(text: String, fin: bool) -> Self {
        Self::new_data(WebSocketOpcode::Text, text.into_bytes(), fin)
    }

    /// Create a new binary frame.
    pub fn new_binary(data: Vec<u8>, fin: bool) -> Self {
        Self::new_data(WebSocketOpcode::Binary, data, fin)
    }

    /// Create a new continuation frame.
    pub fn new_continuation(data: Vec<u8>, fin: bool) -> Self {
        Self::new_data(WebSocketOpcode::Continuation, data, fin)
    }

    /// Create a new close frame with optional status code and reason.
    pub fn new_close(code: Option<u16>, reason: Option<&str>) -> Self {
        let mut payload = Vec::new();
        if let Some(code) = code {
            payload.extend_from_slice(&code.to_be_bytes());
            if let Some(reason) = reason {
                payload.extend_from_slice(reason.as_bytes());
            }
        }
        WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Close,
            masked: false,
            payload,
        }
    }

    /// Create a new ping frame.
    pub fn new_ping(data: Vec<u8>) -> Self {
        WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Ping,
            masked: false,
            payload: data,
        }
    }

    /// Create a new pong frame.
    pub fn new_pong(data: Vec<u8>) -> Self {
        WebSocketFrame {
            fin: true,
            rsv1: false,
            rsv2: false,
            rsv3: false,
            opcode: WebSocketOpcode::Pong,
            masked: false,
            payload: data,
        }
    }

    /// Parse close frame payload to extract status code and reason.
    pub fn parse_close_payload(&self) -> Option<(u16, String)> {
        if self.opcode != WebSocketOpcode::Close {
            return None;
        }
        if self.payload.len() < 2 {
            return None;
        }
        let code = u16::from_be_bytes([self.payload[0], self.payload[1]]);
        let reason = String::from_utf8_lossy(&self.payload[2..]).to_string();
        Some((code, reason))
    }

    /// Check if this is a text frame.
    pub fn is_text(&self) -> bool {
        self.opcode == WebSocketOpcode::Text
    }

    /// Check if this is a binary frame.
    pub fn is_binary(&self) -> bool {
        self.opcode == WebSocketOpcode::Binary
    }

    /// Check if this is a close frame.
    pub fn is_close(&self) -> bool {
        self.opcode == WebSocketOpcode::Close
    }

    /// Get the payload as a UTF-8 text string.
    /// Returns None if the frame is not a text frame or contains invalid UTF-8.
    pub fn payload_as_text(&self) -> Option<String> {
        if !self.is_text() {
            return None;
        }
        String::from_utf8(self.payload.clone()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_text_frame() {
        // Simple unmasked text frame: "Hello"
        let data = vec![
            0b1000_0001, // FIN=1, RSV=0, Opcode=Text
            5,           // Payload length=5
            b'H',
            b'e',
            b'l',
            b'l',
            b'o',
        ];

        let (frame, consumed) = WebSocketFrame::parse(&data).unwrap();
        assert_eq!(consumed, 7);
        assert!(frame.fin);
        assert_eq!(frame.opcode, WebSocketOpcode::Text);
        assert_eq!(frame.payload, b"Hello");
    }

    #[test]
    fn test_parse_masked_frame() {
        // Masked text frame
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut payload = b"Hello".to_vec();
        WebSocketFrame::apply_mask(&mut payload, &mask);

        let mut data = vec![
            0b1000_0001, // FIN=1, RSV=0, Opcode=Text
            0b1000_0101, // MASK=1, Payload length=5
        ];
        data.extend_from_slice(&mask);
        data.extend_from_slice(&payload);

        let (frame, consumed) = WebSocketFrame::parse(&data).unwrap();
        assert_eq!(consumed, 11);
        assert!(frame.fin);
        assert_eq!(frame.payload, b"Hello");
    }

    #[test]
    fn test_encode_frame() {
        let frame = WebSocketFrame::new_text("Hello".to_string(), true);
        let encoded = frame.encode(None);

        let expected = vec![
            0b1000_0001, // FIN=1, Opcode=Text
            5,           // Payload length=5
            b'H',
            b'e',
            b'l',
            b'l',
            b'o',
        ];
        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_extended_length_16bit() {
        let payload = vec![0u8; 200];
        let mut data = vec![
            0b1000_0010, // FIN=1, Opcode=Binary
            126,         // Extended 16-bit length indicator
            0x00,
            0xC8, // Length = 200
        ];
        data.extend_from_slice(&payload);

        let (frame, consumed) = WebSocketFrame::parse(&data).unwrap();
        assert_eq!(consumed, 204);
        assert_eq!(frame.payload.len(), 200);
    }

    #[test]
    fn test_close_frame() {
        let frame = WebSocketFrame::new_close(Some(1000), Some("Normal closure"));
        let encoded = frame.encode(None);

        let (parsed, _) = WebSocketFrame::parse(&encoded).unwrap();
        let (code, reason) = parsed.parse_close_payload().unwrap();
        assert_eq!(code, 1000);
        assert_eq!(reason, "Normal closure");
    }

    #[test]
    fn test_control_frame_too_large() {
        // Control frame with payload > 125 bytes
        let data = vec![
            0b1000_1000, // FIN=1, Opcode=Close
            126,         // Extended length
            0x00,
            0x7F, // Length = 127 (> 125)
        ];

        let result = WebSocketFrame::parse(&data);
        assert!(matches!(result, Err(WebSocketError::ControlFrameTooLarge)));
    }

    #[test]
    fn test_incomplete_frame() {
        let data = vec![0b1000_0001]; // Only first byte
        let result = WebSocketFrame::parse(&data);
        assert!(matches!(result, Err(WebSocketError::IncompleteFrame)));
    }
}
