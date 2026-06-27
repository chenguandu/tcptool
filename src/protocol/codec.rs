/// JT808 Codec: encode/decode raw byte streams
/// Handles escaping, checksum, and message framing

use crate::protocol::types::constant;

/// Calculate checksum (XOR from msg_id to last byte of body)
pub fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc ^ b)
}

/// Escape message data: replace 0x7E and 0x7D
pub fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 4);
    for &b in data {
        match b {
            0x7E => {
                out.push(constant::ESCAPE);
                out.push(0x02);
            }
            0x7D => {
                out.push(constant::ESCAPE);
                out.push(0x01);
            }
            _ => out.push(b),
        }
    }
    out
}

/// Unescape message data: reverse the escape transform
pub fn unescape(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(data.len());
    let mut escaped = false;
    for &b in data {
        if escaped {
            match b {
                0x01 => out.push(0x7D),
                0x02 => out.push(0x7E),
                _ => return Err(format!("Invalid escape sequence: 0x7D 0x{:02X}", b)),
            }
            escaped = false;
        } else if b == constant::ESCAPE {
            escaped = true;
        } else {
            out.push(b);
        }
    }
    if escaped {
        return Err("Incomplete escape sequence at end of data".into());
    }
    Ok(out)
}

/// Encode a complete JT808 message: header + body -> [0x7E] + escaped(msg) + checksum + [0x7E]
pub fn encode_message(header_and_body: &[u8]) -> Vec<u8> {
    let checksum = calculate_checksum(header_and_body);
    let mut payload = header_and_body.to_vec();
    payload.push(checksum);
    let escaped = escape(&payload);
    let mut frame = Vec::with_capacity(escaped.len() + 2);
    frame.push(constant::DELIMITER);
    frame.extend_from_slice(&escaped);
    frame.push(constant::DELIMITER);
    frame
}

/// Try to decode one complete message from a buffer.
/// Returns (message_bytes, remaining_buffer) on success.
pub fn decode_message(buffer: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    if buffer.len() < 6 {
        return None; // too short
    }
    if buffer[0] != constant::DELIMITER {
        return None; // not starting with delimiter
    }

    // Find closing delimiter
    for i in 1..buffer.len() {
        if buffer[i] == constant::DELIMITER {
            let payload = &buffer[1..i];
            if payload.is_empty() {
                return Some((Vec::new(), &buffer[i + 1..]));
            }
            // Unescape
            let unescaped = match unescape(payload) {
                Ok(d) => d,
                Err(_) => return None,
            };
            if unescaped.len() < 2 {
                return None;
            }
            // Verify checksum
            let msg_len = unescaped.len() - 1; // last byte is checksum
            let computed = calculate_checksum(&unescaped[..msg_len]);
            if computed != unescaped[msg_len] {
                return None; // checksum mismatch
            }
            return Some((unescaped[..msg_len].to_vec(), &buffer[i + 1..]));
        }
    }
    None // no closing delimiter found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_unescape() {
        let data = vec![0x01, 0x7E, 0x7D, 0x03];
        let escaped = escape(&data);
        assert!(escaped.contains(&0x7D));
        let unescaped = unescape(&escaped).unwrap();
        assert_eq!(data, unescaped);
    }

    #[test]
    fn test_checksum() {
        let data = vec![0x01, 0x02, 0x03];
        let cs = calculate_checksum(&data);
        assert_eq!(cs, 0x01 ^ 0x02 ^ 0x03);
    }

    #[test]
    fn test_encode_decode_message() {
        let body = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let frame = encode_message(&body);
        let (decoded, remaining) = decode_message(&frame).unwrap();
        assert_eq!(body, decoded);
        assert!(remaining.is_empty());
    }
}
