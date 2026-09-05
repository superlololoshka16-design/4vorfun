use core_utils::base64::{AsOut, STANDARD};
use smallvec::SmallVec;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum B64Error {
    #[error("malformed base64")]
    Malformed,
    #[error("decoded blob too large: {0}")]
    TooLarge(usize),
}

pub fn decode_b64_field(input: &[u8], out: &mut SmallVec<[u8; 4096]>) -> Result<usize, B64Error> {
    let mut trimmed = input;
    while let Some((f, rest)) = trimmed.split_first() {
        if f.is_ascii_whitespace() || *f == b'"' || *f == b'\'' {
            trimmed = rest;
        } else {
            break;
        }
    }
    let mut end = trimmed.len();
    while end > 0 {
        let b = trimmed[end - 1];
        if b.is_ascii_whitespace() || b == b'"' || b == b'\'' {
            end -= 1;
        } else {
            break;
        }
    }
    let src = &trimmed[..end];
    let need = STANDARD
        .decoded_length(src)
        .map_err(|_| B64Error::Malformed)?;
    if need > 64 * 1024 {
        return Err(B64Error::TooLarge(need));
    }
    out.clear();
    out.resize(need, 0);
    let written = STANDARD
        .decode(src, out.as_mut_slice().as_out())
        .map_err(|_| B64Error::Malformed)?;
    Ok(written.len())
}
