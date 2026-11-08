//! Base64 with the standard alphabet (RFC 4648 section 4) and no padding, as the password hash
//! text uses it. Decoding is strict: every encoding of a byte string is the one [`encode`] gives.

/// The standard Base64 alphabet.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes `bytes` without padding.
pub fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk
            .iter()
            .enumerate()
            .fold(0u32, |group, (index, &byte)| {
                group | u32::from(byte) << (16 - 8 * index)
            });
        // A chunk of n bytes gives n + 1 characters.
        for index in 0..=chunk.len() {
            let sextet = (group >> (18 - 6 * index)) & 0x3F;
            text.push(char::from(ALPHABET[sextet as usize]));
        }
    }
    text
}

/// Decodes unpadded Base64. Returns `None` for any character outside the alphabet (padding
/// included), a length that no byte string encodes to, or unused low bits that are not zero.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3 + 2);
    for chunk in text.as_bytes().chunks(4) {
        // A chunk of one character encodes no whole byte.
        if chunk.len() == 1 {
            return None;
        }
        let mut group = 0u32;
        for (index, &character) in chunk.iter().enumerate() {
            group |= u32::from(sextet(character)?) << (18 - 6 * index);
        }
        let whole = chunk.len() - 1;
        let unused_bits = 24 - 8 * whole;
        if whole < 3 && group & ((1 << unused_bits) - 1) != 0 {
            return None;
        }
        for index in 0..whole {
            bytes.push(group.to_be_bytes()[1 + index]);
        }
    }
    Some(bytes)
}

/// Gets the six-bit value of one alphabet character.
fn sextet(character: u8) -> Option<u8> {
    let value = match character {
        b'A'..=b'Z' => character - b'A',
        b'a'..=b'z' => character - b'a' + 26,
        b'0'..=b'9' => character - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    /// The RFC 4648 section 10 test vectors, without their padding.
    const VECTORS: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg"),
        ("fo", "Zm8"),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg"),
        ("fooba", "Zm9vYmE"),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn encodes_the_rfc_vectors() {
        for (plain, encoded) in VECTORS {
            assert_eq!(encode(plain.as_bytes()), encoded, "{plain}");
        }
    }

    #[test]
    fn decodes_the_rfc_vectors() {
        for (plain, encoded) in VECTORS {
            assert_eq!(
                decode(encoded).as_deref(),
                Some(plain.as_bytes()),
                "{encoded}"
            );
        }
    }

    #[test]
    fn every_byte_round_trips() {
        let bytes: Vec<u8> = (0..=255).collect();
        for length in 0..bytes.len() {
            let encoded = encode(&bytes[..length]);
            assert_eq!(decode(&encoded).as_deref(), Some(&bytes[..length]));
        }
        assert_eq!(encode(&[0xFB, 0xFF]), "+/8");
    }

    #[test]
    fn rejects_text_that_is_not_canonical_unpadded_base64() {
        for text in [
            "Zg==", "Zm8=", "Z", "Zm9vY", "Zh", "Zm9", "Zm-v", "Zm9v Yg", "Zm_v",
        ] {
            assert_eq!(decode(text), None, "{text}");
        }
    }
}
