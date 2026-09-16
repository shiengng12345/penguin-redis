//! Minimal parser for what a *client* sends: RESP arrays of bulk strings (commands), plus
//! inline commands. This is intentionally small and strict; it is **not** the product decoder
//! (`pr-protocol`), it only lets scripts assert what the client transmitted.

use bytes::Bytes;
use thiserror::Error;

/// A decoded client command: raw argv bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command(pub Vec<Bytes>);

/// Parse errors for inbound commands.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum InboundError {
    /// Need more bytes.
    #[error("incomplete")]
    Incomplete,
    /// Not an array-of-bulk-strings command.
    #[error("malformed command at offset {0}")]
    Malformed(usize),
}

fn find_crlf(buf: &[u8], from: usize) -> Option<usize> {
    buf[from..].windows(2).position(|w| w == b"\r\n").map(|p| from + p)
}

fn parse_len(buf: &[u8], from: usize) -> Result<(i64, usize), InboundError> {
    let end = find_crlf(buf, from).ok_or(InboundError::Incomplete)?;
    let s = std::str::from_utf8(&buf[from..end]).map_err(|_| InboundError::Malformed(from))?;
    let n: i64 = s.parse().map_err(|_| InboundError::Malformed(from))?;
    Ok((n, end + 2))
}

/// Try to parse exactly one command from the front of `buf`.
/// Returns the command and the number of bytes consumed.
///
/// # Errors
/// [`InboundError::Incomplete`] if more bytes are needed;
/// [`InboundError::Malformed`] with the byte offset if the framing is invalid.
pub fn parse_command(buf: &[u8]) -> Result<(Command, usize), InboundError> {
    if buf.is_empty() {
        return Err(InboundError::Incomplete);
    }
    if buf[0] != b'*' {
        // inline command: split on whitespace up to CRLF
        let end = find_crlf(buf, 0).ok_or(InboundError::Incomplete)?;
        let args = buf[..end]
            .split(|b| *b == b' ' || *b == b'\t')
            .filter(|s| !s.is_empty())
            .map(Bytes::copy_from_slice)
            .collect();
        return Ok((Command(args), end + 2));
    }
    let (n, mut pos) = parse_len(buf, 1)?;
    if !(0..=1_000_000).contains(&n) {
        return Err(InboundError::Malformed(0));
    }
    let mut args = Vec::with_capacity(usize::try_from(n).unwrap_or(0));
    for _ in 0..n {
        if pos >= buf.len() {
            return Err(InboundError::Incomplete);
        }
        if buf[pos] != b'$' {
            return Err(InboundError::Malformed(pos));
        }
        let (len, data_start) = parse_len(buf, pos + 1)?;
        let len = usize::try_from(len).map_err(|_| InboundError::Malformed(pos))?;
        let data_end = data_start + len;
        if buf.len() < data_end + 2 {
            return Err(InboundError::Incomplete);
        }
        if &buf[data_end..data_end + 2] != b"\r\n" {
            return Err(InboundError::Malformed(data_end));
        }
        args.push(Bytes::copy_from_slice(&buf[data_start..data_end]));
        pos = data_end + 2;
    }
    Ok((Command(args), pos))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_multibulk_and_inline() {
        let (c, n) = parse_command(b"*2\r\n$3\r\nGET\r\n$1\r\nk\r\nleft").unwrap();
        assert_eq!(c.0, vec![Bytes::from_static(b"GET"), Bytes::from_static(b"k")]);
        assert_eq!(n, 20); // *2\r\n(4) + $3\r\nGET\r\n(9) + $1\r\nk\r\n(7)
        let (c, _) = parse_command(b"PING  x\r\n").unwrap();
        assert_eq!(c.0, vec![Bytes::from_static(b"PING"), Bytes::from_static(b"x")]);
    }

    #[test]
    fn incomplete_and_malformed() {
        assert_eq!(parse_command(b"*2\r\n$3\r\nGE").unwrap_err(), InboundError::Incomplete);
        assert_eq!(parse_command(b"*1\r\n:1\r\n").unwrap_err(), InboundError::Malformed(4));
        // declared len 1 but body is "ab": terminator check fails at data_end = 9
        assert_eq!(parse_command(b"*1\r\n$1\r\nab\r\n").unwrap_err(), InboundError::Malformed(9));
    }

    #[test]
    fn binary_safe_args() {
        let (c, _) = parse_command(b"*1\r\n$3\r\n\0\xff\x1b\r\n").unwrap();
        assert_eq!(c.0[0].as_ref(), b"\0\xff\x1b");
    }
}
