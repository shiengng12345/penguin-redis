//! `redis://` URIs, parsed so that a parse *error* cannot contain the password (v2.1 §23.4,
//! V-D06, SEC-01).
//!
//! This lives beside the redaction register rather than beside the transport because its
//! defining property is not that it understands URIs. It is that
//! [`UriError`](enum@UriError) has nowhere to put a secret — the userinfo is consumed before
//! anything can fail on it, and no variant carries the raw input.
//!
//! That is a stronger guarantee than scrubbing the message afterwards. Scrubbing depends on
//! the secret having been registered first, and the first thing a person does with a URI they
//! cannot connect with is paste it into a terminal — at which point the password is in the
//! input and not yet in the register. So the parser is written to have nothing to leak, and
//! [`Redis::redacted`] gives the printable form.
//!
//! §23.4's first named path is "URI parse error", and this is why: the failure case is the one
//! that gets printed.

use crate::secrets;

/// What a URI could not be.
///
/// **No variant carries the input.** A parse error's job is to say what was wrong with the
/// shape, and the shape can be described without quoting the thing that failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UriError {
    /// The scheme is missing or is not one this understands.
    #[error("a connection URI must start with redis:// or rediss://")]
    Scheme,
    /// There is no host.
    #[error("the URI has no host")]
    NoHost,
    /// The port is not a number, or does not fit.
    #[error("the port is not a number between 1 and 65535")]
    Port,
    /// The path is present but is not a database number.
    #[error("the path after the host must be a database number, such as /0")]
    Database,
    /// The userinfo has more than one `:`, so where the password starts is ambiguous.
    ///
    /// Note what this does *not* say: it does not repeat the userinfo. An ambiguous userinfo is
    /// usually one with a `:` in the password.
    #[error("the user:password part contains more than one ':'; percent-encode it")]
    Userinfo,
    /// Percent-decoding failed.
    #[error("a percent escape in the URI is not valid")]
    PercentEscape,
}

/// A parsed connection URI.
///
/// The password is a `Vec<u8>` and the `Debug` below hides it; see the field note.
#[derive(Clone, PartialEq, Eq)]
pub struct Redis {
    /// Whether the scheme was `rediss://`.
    pub tls: bool,
    /// The username, if the URI had one.
    pub user: Option<String>,
    /// The password, if the URI had one.
    ///
    /// Not `String`: a password is bytes, and forcing it through UTF-8 either rejects a valid
    /// one or replaces bytes with `U+FFFD`, which is a silently different password.
    password: Option<Vec<u8>>,
    /// Host, as written.
    pub host: String,
    /// Port, defaulted to 6379.
    pub port: u16,
    /// Database number, defaulted to 0.
    pub db: u32,
}

impl Redis {
    /// The password, for the one caller that has to send it.
    #[must_use]
    pub fn password(&self) -> Option<&[u8]> {
        self.password.as_deref()
    }

    /// Whether a password was present at all — safe to show.
    #[must_use]
    pub fn has_password(&self) -> bool {
        self.password.is_some()
    }

    /// The URI as it may be printed: everything except the password.
    #[must_use]
    pub fn redacted(&self) -> String {
        let scheme = if self.tls { "rediss" } else { "redis" };
        let mut out = format!("{scheme}://");
        if let Some(u) = &self.user {
            out.push_str(u);
            if self.password.is_some() {
                out.push(':');
                out.push_str(secrets::REDACTED);
            }
            out.push('@');
        } else if self.password.is_some() {
            out.push_str(secrets::REDACTED);
            out.push('@');
        }
        out.push_str(&self.host);
        out.push(':');
        out.push_str(&self.port.to_string());
        out.push('/');
        out.push_str(&self.db.to_string());
        out
    }
}

/// Hand-written, because deriving it would print the password.
///
/// The same reasoning as `TlsConfig` (ADR-032): a `#[derive(Debug)]` on a struct with a secret
/// field is a leak that looks like nothing, and it reaches a person through a panic message, a
/// log line or `dbg!`.
#[allow(clippy::missing_fields_in_debug)] // the omitted field is the password. That is the point.
impl std::fmt::Debug for Redis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redis")
            .field("uri", &self.redacted())
            .field("has_password", &self.password.is_some())
            .finish()
    }
}

/// Parse a `redis://` or `rediss://` URI.
///
/// The password, if there is one, is registered with [`secrets`] before this returns: from that
/// moment every other sink in the process scrubs it, which is the point of doing the parsing
/// here.
///
/// # Errors
/// [`UriError`], which never contains any part of the input.
pub fn parse(raw: &str) -> Result<Redis, UriError> {
    let (scheme, rest) = raw.split_once("://").ok_or(UriError::Scheme)?;
    let tls = match scheme.to_ascii_lowercase().as_str() {
        "redis" => false,
        "rediss" => true,
        _ => return Err(UriError::Scheme),
    };

    // Split userinfo off first, and do it from the *last* `@`: a password may contain one.
    let (userinfo, authority) = match rest.rfind('@') {
        Some(i) => (Some(&rest[..i]), &rest[i + 1..]),
        None => (None, rest),
    };

    let (user, password) = match userinfo {
        None => (None, None),
        Some(ui) => {
            let mut parts = ui.splitn(2, ':');
            let u = parts.next().unwrap_or("");
            let p = parts.next();
            if p.is_some_and(|p| p.contains(':')) {
                return Err(UriError::Userinfo);
            }
            let user = if u.is_empty() {
                None
            } else {
                Some(percent_decode_str(u)?)
            };
            let password = match p {
                None => None,
                Some(p) => Some(percent_decode(p)?),
            };
            (user, password)
        }
    };

    let (hostport, path) = match authority.find('/') {
        Some(i) => (&authority[..i], Some(&authority[i + 1..])),
        None => (authority, None),
    };

    // IPv6 literals are bracketed, and the colons inside them are not the port separator.
    let (host, port) = if let Some(close) = hostport.strip_prefix('[').and_then(|r| r.find(']')) {
        let host = &hostport[1..=close];
        let after = &hostport[close + 2..];
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(UriError::Port),
        };
        (host.to_owned(), port.map(str::to_owned))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_owned(), Some(p.to_owned())),
            None => (hostport.to_owned(), None),
        }
    };

    if host.is_empty() {
        return Err(UriError::NoHost);
    }
    let port = match port {
        None => 6379,
        Some(p) => p
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or(UriError::Port)?,
    };
    let db = match path {
        None | Some("") => 0,
        Some(p) => p.parse::<u32>().map_err(|_| UriError::Database)?,
    };

    if let Some(p) = &password {
        let _ = secrets::remember(p);
    }

    Ok(Redis {
        tls,
        user,
        password,
        host,
        port,
        db,
    })
}

/// Percent-decode to bytes.
fn percent_decode(s: &str) -> Result<Vec<u8>, UriError> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = b.get(i + 1..i + 3).ok_or(UriError::PercentEscape)?;
            let s = std::str::from_utf8(hex).map_err(|_| UriError::PercentEscape)?;
            out.push(u8::from_str_radix(s, 16).map_err(|_| UriError::PercentEscape)?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}

/// Percent-decode to text.
fn percent_decode_str(s: &str) -> Result<String, UriError> {
    String::from_utf8(percent_decode(s)?).map_err(|_| UriError::PercentEscape)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const PW: &str = "hunter2-VeryS3cret-c0ffee";

    #[test]
    fn a_full_uri_parses() {
        let u = parse(&format!("rediss://alice:{PW}@db.example:6380/3")).unwrap();
        assert!(u.tls);
        assert_eq!(u.user.as_deref(), Some("alice"));
        assert_eq!(u.password(), Some(PW.as_bytes()));
        assert_eq!(u.host, "db.example");
        assert_eq!(u.port, 6380);
        assert_eq!(u.db, 3);
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let u = parse("redis://localhost").unwrap();
        assert_eq!((u.port, u.db, u.tls), (6379, 0, false));
        assert!(u.user.is_none() && !u.has_password());
    }

    #[test]
    fn an_ipv6_literal_is_not_mistaken_for_a_port() {
        let u = parse("redis://[2001:db8::1]:7000/1").unwrap();
        assert_eq!(u.host, "2001:db8::1");
        assert_eq!(u.port, 7000);
    }

    #[test]
    fn no_error_ever_contains_the_password() {
        // §23.4's first path, as an assertion rather than a promise. Every way this parser can
        // fail, with a password present.
        let cases = [
            format!("http://alice:{PW}@host"),
            format!("redis://alice:{PW}@host:notaport"),
            format!("redis://alice:{PW}@host:0"),
            format!("redis://alice:{PW}@host/notadb"),
            format!("redis://alice:{PW}:extra@host"),
            format!("redis://alice:{PW}@:6379"),
            format!("redis://alice:%zz{PW}@host"),
        ];
        for c in &cases {
            let e = parse(c).unwrap_err();
            let rendered = format!("{e} {e:?}");
            assert!(
                !rendered.contains(PW) && !rendered.contains("hunter2"),
                "the error for {c:?} contains the password: {rendered}"
            );
        }
    }

    #[test]
    fn the_printable_form_keeps_everything_except_the_password() {
        let u = parse(&format!("redis://alice:{PW}@db:6380/2")).unwrap();
        let shown = u.redacted();
        assert_eq!(shown, "redis://alice:[redacted]@db:6380/2");
        assert!(!shown.contains(PW));
        // And `Debug` is the same story: deriving it would have printed the field.
        let debugged = format!("{u:?}");
        assert!(!debugged.contains(PW), "{debugged}");
        assert!(debugged.contains("has_password: true"));
    }

    #[test]
    fn a_password_with_an_at_sign_splits_at_the_last_one() {
        let u = parse("redis://alice:p@ss-word-1@host:6379").unwrap();
        assert_eq!(u.host, "host");
        assert_eq!(u.password(), Some(&b"p@ss-word-1"[..]));
    }

    #[test]
    fn parsing_registers_the_password_so_every_other_sink_scrubs_it() {
        let unique = "uri-registered-secret-9f3a";
        parse(&format!("redis://u:{unique}@h")).unwrap();
        assert!(!secrets::appears(b"nothing here"));
        assert!(secrets::appears(format!("log line: {unique}").as_bytes()));
        assert!(!secrets::scrub_str(&format!("log line: {unique}")).contains(unique));
    }

    #[test]
    fn a_percent_encoded_password_decodes_to_bytes() {
        let u = parse("redis://u:a%3Ab%00c@h").unwrap();
        assert_eq!(u.password(), Some(&b"a:b\x00c"[..]));
    }
}
