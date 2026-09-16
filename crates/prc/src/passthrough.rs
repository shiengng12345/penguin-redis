//! One-shot passthrough execution — V-A02 scaffolding (v2.1 §18.1, §18.6, §32.2).
//!
//! The differential harness needs a `prc` that actually talks to a server before the kernel
//! exists, or the four-layer comparison cannot be built in Phase 0. This is that `prc`: parse
//! argv, send the bytes, read one reply, write it out, exit.
//!
//! Output follows §18.1's non-TTY default, which is raw. Where raw disagrees with
//! `redis-cli`'s raw, the difference is **measured by the harness and recorded in
//! `compatibility/manifest.toml`** rather than guessed at here (ADR-011).

use crate::args::Invocation;
use pr_core::ExitCode;
use pr_protocol::value::{NullForm, Value};
use pr_transport::Oneshot;
use std::io::Write;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Run one command against a directly addressed server.
///
/// Returns the process exit code. Nothing here is reusable by Phase 1 — see the module note in
/// `pr_transport::oneshot`.
pub fn run(inv: &Invocation, host: &str, port: u16) -> ExitCode {
    let mut conn = match Oneshot::connect(host, port, CONNECT_TIMEOUT) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("prc: {host}:{port}: {e}");
            return ExitCode::Connection;
        }
    };

    // `-3` is a real handshake, not a flag we record: RESP3 changes what the server sends, so
    // the differential harness has to be able to ask for it and see the difference.
    if inv.protocol == Some(3) {
        let hello = [
            bytes::Bytes::from_static(b"HELLO"),
            bytes::Bytes::from_static(b"3"),
        ];
        match conn.call(&hello) {
            Ok(Value::Error(e) | Value::BulkError(e)) => {
                eprintln!("prc: HELLO 3: {}", String::from_utf8_lossy(&e));
                return ExitCode::Connection;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("prc: HELLO 3: {e}");
                return ExitCode::Connection;
            }
        }
    }

    if let Some(db) = inv.db {
        let select = [
            bytes::Bytes::from_static(b"SELECT"),
            bytes::Bytes::from(db.to_string()),
        ];
        match conn.call(&select) {
            Ok(Value::Error(e) | Value::BulkError(e)) => {
                eprintln!("prc: SELECT {db}: {}", String::from_utf8_lossy(&e));
                return ExitCode::Connection;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("prc: SELECT {db}: {e}");
                return ExitCode::Connection;
            }
        }
    }

    let reply = match conn.call(&inv.command) {
        Ok(v) => v,
        Err(e) => {
            // The command went out. Whether it took effect is unknown, and §18.6 has a code
            // for exactly that rather than pretending it failed.
            eprintln!("prc: {e}");
            return ExitCode::ResultUnknown;
        }
    };

    if let Value::Error(msg) | Value::BulkError(msg) = &reply {
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(b"(error) ");
        let _ = err.write_all(msg);
        let _ = err.write_all(b"\n");
        return ExitCode::CommandError;
    }

    let mut out = Vec::new();
    render_raw(&reply, &mut out, true);
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(&out).is_err() || stdout.flush().is_err() {
        return ExitCode::LocalFailure;
    }
    ExitCode::Success
}

/// Render one reply the way §18.1's raw mode defines it.
///
/// `top` marks the outermost call: raw output is newline-terminated as a whole, and elements
/// inside an aggregate are newline-*separated*, which is not the same thing when the last
/// element is empty.
fn render_raw(v: &Value, out: &mut Vec<u8>, top: bool) {
    match v {
        Value::Simple(b) | Value::Bulk(b) | Value::Verbatim { data: b, .. } => {
            out.extend_from_slice(b);
        }
        Value::Integer(n) => out.extend_from_slice(n.to_string().as_bytes()),
        Value::Double(lex) | Value::BigNumber(lex) => out.extend_from_slice(lex),
        Value::Boolean(t) => out.extend_from_slice(if *t { b"true" } else { b"false" }),
        // Raw mode has nothing to put here. That CMD-04 cannot tell this apart from an empty
        // string is a property of raw output, not a defect in this renderer — it is the
        // reason §18.1 has a table mode at all, and the harness records it as such.
        Value::Null(NullForm::Bulk | NullForm::Array | NullForm::Resp3) => {}
        Value::Array(items) | Value::Set(items) | Value::Push(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b'\n');
                }
                render_raw(item, out, false);
            }
        }
        Value::Map(entries) => {
            for (i, (k, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(b'\n');
                }
                render_raw(k, out, false);
                out.push(b'\n');
                render_raw(val, out, false);
            }
        }
        Value::Attribute { value, .. } => render_raw(value, out, false),
        Value::Error(b) | Value::BulkError(b) => out.extend_from_slice(b),
    }
    if top {
        out.push(b'\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn raw(v: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        render_raw(v, &mut out, true);
        out
    }

    #[test]
    fn a_status_reply_is_its_bytes_and_a_newline() {
        assert_eq!(raw(&Value::Simple(Bytes::from_static(b"OK"))), b"OK\n");
    }

    #[test]
    fn mget_keeps_duplicates_order_and_the_hole() {
        // CMD-03. Three elements, the third missing: three lines, the last empty.
        let v = Value::Array(vec![
            Value::Bulk(Bytes::from_static(b"v")),
            Value::Bulk(Bytes::from_static(b"v")),
            Value::Null(NullForm::Bulk),
        ]);
        assert_eq!(raw(&v), b"v\nv\n\n");
    }

    #[test]
    fn raw_output_cannot_tell_a_missing_field_from_an_empty_one() {
        // CMD-04, stated as the fact it is rather than worked around. Both render to nothing,
        // so raw mode is the wrong mode for this question and §18.1's table mode is the right
        // one. The harness records this instead of letting a later reader assume raw is enough.
        let missing = Value::Array(vec![Value::Null(NullForm::Bulk)]);
        let empty = Value::Array(vec![Value::Bulk(Bytes::from_static(b""))]);
        assert_eq!(raw(&missing), raw(&empty));
        assert_eq!(raw(&missing), b"\n");
    }

    #[test]
    fn an_integer_zero_is_printed_not_suppressed() {
        // CMD-05: `HSET` overwriting returns 0, and 0 is a real answer, not a failure.
        assert_eq!(raw(&Value::Integer(0)), b"0\n");
    }

    #[test]
    fn a_double_keeps_its_lexeme_through_rendering_too() {
        assert_eq!(
            raw(&Value::Double(Bytes::from_static(b"1.2300"))),
            b"1.2300\n"
        );
    }
}
