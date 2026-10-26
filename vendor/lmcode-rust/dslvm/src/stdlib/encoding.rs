//! Strict byte codecs; no text decoding, whitespace skipping, or output cap.
use crate::{Error, Registry, Value};

fn base64_size(n: usize) -> Result<usize, Error> {
    (n / 3 + usize::from(!n.is_multiple_of(3)))
        .checked_mul(4)
        .ok_or_else(|| Error::host("str_base64_encode: size overflow"))
}
fn hex_size(n: usize) -> Result<usize, Error> {
    n.checked_mul(2)
        .ok_or_else(|| Error::host("str_hex_encode: size overflow"))
}
/// RFC 4648 standard alphabet, canonical padding, no line wrapping.
pub fn str_base64_encode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, Error> {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = input.as_ref();
    let mut out = Vec::with_capacity(base64_size(input.len())?);
    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(ABC[(a >> 2) as usize]);
        out.push(ABC[(((a & 3) << 4) | (b >> 4)) as usize]);
        out.push(if chunk.len() > 1 {
            ABC[(((b & 15) << 2) | (c >> 6)) as usize]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            ABC[(c & 63) as usize]
        } else {
            b'='
        });
    }
    Ok(out)
}
fn base64_digit(c: u8) -> Result<u8, Error> {
    match c {
        b'A'..=b'Z' => Ok(c - b'A'),
        b'a'..=b'z' => Ok(c - b'a' + 26),
        b'0'..=b'9' => Ok(c - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(Error::host("str_base64_decode: invalid encoding")),
    }
}
/// Reject noncanonical padding and pad bits, whitespace and URL-safe symbols.
pub fn str_base64_decode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, Error> {
    let input = input.as_ref();
    let invalid = || Error::host("str_base64_decode: invalid encoding");
    if !input.len().is_multiple_of(4) {
        return Err(invalid());
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    for (i, q) in input.chunks_exact(4).enumerate() {
        let last = i + 1 == input.len() / 4;
        let a = base64_digit(q[0])?;
        let b = base64_digit(q[1])?;
        if q[2] == b'=' {
            if !last || q[3] != b'=' || b & 15 != 0 {
                return Err(invalid());
            }
            out.push((a << 2) | (b >> 4));
            continue;
        }
        let c = base64_digit(q[2])?;
        out.push((a << 2) | (b >> 4));
        out.push((b << 4) | (c >> 2));
        if q[3] == b'=' {
            if !last || c & 3 != 0 {
                return Err(invalid());
            }
        } else {
            out.push((c << 6) | base64_digit(q[3])?);
        }
    }
    Ok(out)
}
/// Two lowercase ASCII hex digits for each byte.
pub fn str_hex_encode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, Error> {
    const HEX: &[u8] = b"0123456789abcdef";
    let input = input.as_ref();
    let mut out = Vec::with_capacity(hex_size(input.len())?);
    for &b in input {
        out.extend([HEX[(b >> 4) as usize], HEX[(b & 15) as usize]]);
    }
    Ok(out)
}
fn hex_digit(c: u8) -> Result<u8, Error> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(Error::host("str_hex_decode: invalid encoding")),
    }
}
/// Accept uppercase/lowercase digits only, in complete pairs.
pub fn str_hex_decode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, Error> {
    let input = input.as_ref();
    if !input.len().is_multiple_of(2) {
        return Err(Error::host("str_hex_decode: invalid encoding"));
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    for pair in input.chunks_exact(2) {
        out.push(hex_digit(pair[0])? << 4 | hex_digit(pair[1])?);
    }
    Ok(out)
}
/// Install all four codecs, including compatibility use with an opted-out registry.
pub fn register_encoding(r: &mut Registry) -> Result<&mut Registry, Error> {
    r.begin_compatibility_registration();
    r.function("str_base64_encode", &["string"], "string", |_, a| {
        Ok(Value::string(str_base64_encode(a[0].as_bytes()?)?))
    })?;
    r.function("str_base64_decode", &["string"], "string", |_, a| {
        Ok(Value::string(str_base64_decode(a[0].as_bytes()?)?))
    })?;
    r.function("str_hex_encode", &["string"], "string", |_, a| {
        Ok(Value::string(str_hex_encode(a[0].as_bytes()?)?))
    })?;
    r.function("str_hex_decode", &["string"], "string", |_, a| {
        Ok(Value::string(str_hex_decode(a[0].as_bytes()?)?))
    })?;
    r.end_compatibility_registration();
    Ok(r)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overflow_is_checked_before_any_allocation() {
        assert_eq!(
            base64_size(usize::MAX).unwrap_err().message,
            "str_base64_encode: size overflow"
        );
        assert_eq!(
            hex_size(usize::MAX).unwrap_err().message,
            "str_hex_encode: size overflow"
        );
        let max_hex = usize::MAX / 2;
        assert_eq!(hex_size(max_hex).unwrap(), max_hex * 2);
        assert!(hex_size(max_hex + 1).is_err());
        let max_b64 = usize::MAX / 4 * 3;
        assert_eq!(base64_size(max_b64).unwrap(), usize::MAX / 4 * 4);
        assert!(base64_size(max_b64 + 1).is_err());
    }
}
