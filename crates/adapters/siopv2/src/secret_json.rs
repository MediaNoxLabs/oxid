// SPDX-License-Identifier: Apache-2.0

use oxid_protocol_application::SelfIssuedProtocolError;
use zeroize::Zeroizing;

pub(super) fn decode_json_string(
    raw: &[u8],
    max_bytes: usize,
    reject_empty: bool,
) -> Result<Zeroizing<String>, SelfIssuedProtocolError> {
    if raw.len() < 2 || raw.first() != Some(&b'"') || raw.last() != Some(&b'"') {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }

    let end = raw.len() - 1;
    let mut decoded = Zeroizing::new(String::with_capacity(end.saturating_sub(1)));
    let mut index = 1;
    while index < end {
        let byte = raw[index];
        let character = match byte {
            b'"' | 0x00..=0x1f => return Err(SelfIssuedProtocolError::InvalidProof),
            b'\\' => {
                index += 1;
                let escape = raw
                    .get(index)
                    .copied()
                    .ok_or(SelfIssuedProtocolError::InvalidProof)?;
                index += 1;
                match escape {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\u{0008}',
                    b'f' => '\u{000c}',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'u' => {
                        let first = decode_json_hex_quad(raw, &mut index, end)?;
                        let scalar = if (0xd800..=0xdbff).contains(&first) {
                            if raw.get(index..index.saturating_add(2)) != Some(b"\\u") {
                                return Err(SelfIssuedProtocolError::InvalidProof);
                            }
                            index += 2;
                            let second = decode_json_hex_quad(raw, &mut index, end)?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err(SelfIssuedProtocolError::InvalidProof);
                            }
                            0x1_0000
                                + ((u32::from(first) - 0xd800) << 10)
                                + (u32::from(second) - 0xdc00)
                        } else if (0xdc00..=0xdfff).contains(&first) {
                            return Err(SelfIssuedProtocolError::InvalidProof);
                        } else {
                            u32::from(first)
                        };
                        char::from_u32(scalar).ok_or(SelfIssuedProtocolError::InvalidProof)?
                    }
                    _ => return Err(SelfIssuedProtocolError::InvalidProof),
                }
            }
            0x20..=0x7f => {
                index += 1;
                char::from(byte)
            }
            _ => {
                let width = match byte {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Err(SelfIssuedProtocolError::InvalidProof),
                };
                let encoded = raw
                    .get(index..index.saturating_add(width))
                    .ok_or(SelfIssuedProtocolError::InvalidProof)?;
                let character = std::str::from_utf8(encoded)
                    .ok()
                    .and_then(|text| text.chars().next())
                    .ok_or(SelfIssuedProtocolError::InvalidProof)?;
                index += width;
                character
            }
        };
        decoded.push(character);
        if decoded.len() > max_bytes {
            return Err(SelfIssuedProtocolError::InvalidProof);
        }
    }
    if reject_empty && decoded.is_empty() {
        return Err(SelfIssuedProtocolError::InvalidProof);
    }
    Ok(decoded)
}

fn decode_json_hex_quad(
    raw: &[u8],
    index: &mut usize,
    end: usize,
) -> Result<u16, SelfIssuedProtocolError> {
    let digits = raw
        .get(*index..index.saturating_add(4))
        .filter(|digits| digits.len() == 4 && index.saturating_add(4) <= end)
        .ok_or(SelfIssuedProtocolError::InvalidProof)?;
    let mut value = 0_u16;
    for digit in digits {
        value = (value << 4)
            | u16::from(match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => return Err(SelfIssuedProtocolError::InvalidProof),
            });
    }
    *index += 4;
    Ok(value)
}
