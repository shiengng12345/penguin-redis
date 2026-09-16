//! Cluster key→slot mapping, including hash tags (v2.1 §21.2, ADR-043/R43).
//!
//! The review found §21.2 described redirects and the same-slot rule but never specified the
//! **hash tag**: only the bytes between the first `{` and the next `}` participate in the
//! CRC16 when that substring is non-empty. Without it, `{user1}:a` and `{user1}:b` land in
//! different slots and every tagged multi-key command misroutes.

/// Number of slots in a Redis Cluster.
pub const SLOT_COUNT: u16 = 16384;

/// CRC16/XMODEM, the polynomial Redis Cluster uses.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// The portion of a key that participates in slot computation.
///
/// Returns the hash tag when the key contains `{`, a following `}`, and at least one byte
/// between them; otherwise the whole key.
#[must_use]
pub fn hash_tag(key: &[u8]) -> &[u8] {
    let Some(open) = key.iter().position(|c| *c == b'{') else {
        return key;
    };
    let Some(rel) = key[open + 1..].iter().position(|c| *c == b'}') else {
        return key;
    };
    if rel == 0 {
        return key; // `{}` is empty: the whole key is used
    }
    &key[open + 1..open + 1 + rel]
}

/// Slot for a key.
#[must_use]
pub fn slot_for(key: &[u8]) -> u16 {
    crc16(hash_tag(key)) % SLOT_COUNT
}

/// Whether every key maps to the same slot, which is what a multi-key command requires.
#[must_use]
pub fn same_slot<'a, I: IntoIterator<Item = &'a [u8]>>(keys: I) -> Option<u16> {
    let mut it = keys.into_iter();
    let first = slot_for(it.next()?);
    for k in it {
        if slot_for(k) != first {
            return None;
        }
    }
    Some(first)
}

/// A parsed `MOVED` / `ASK` redirect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    /// `true` for `ASK` (one-shot, needs `ASKING`), `false` for `MOVED`.
    pub ask: bool,
    /// Slot named in the error.
    pub slot: u16,
    /// Target host as announced by the server.
    pub host: String,
    /// Target port.
    pub port: u16,
}

/// Parse a redirect error reply, e.g. `MOVED 3999 127.0.0.1:6381`.
///
/// Returns `None` for any other error, which must not be treated as a redirect.
#[must_use]
pub fn parse_redirect(err: &str) -> Option<Redirect> {
    let mut parts = err.split_whitespace();
    let kind = parts.next()?;
    let ask = match kind {
        "ASK" => true,
        "MOVED" => false,
        _ => return None,
    };
    let slot: u16 = parts.next()?.parse().ok()?;
    let addr = parts.next()?;
    // IPv6 targets arrive as [::1]:6379.
    let (host, port) = if let Some(rest) = addr.strip_prefix('[') {
        let (h, p) = rest.split_once("]:")?;
        (h.to_owned(), p)
    } else {
        let (h, p) = addr.rsplit_once(':')?;
        (h.to_owned(), p)
    };
    Some(Redirect {
        ask,
        slot,
        host,
        port: port.parse().ok()?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn crc16_matches_the_published_vectors() {
        // Values from the Redis Cluster specification.
        assert_eq!(crc16(b""), 0x0000);
        assert_eq!(crc16(b"123456789"), 0x31C3);
    }

    #[test]
    fn known_keys_map_to_the_documented_slots() {
        // These are the slots a real server reports for these keys.
        assert_eq!(slot_for(b"foo"), 12182);
        assert_eq!(slot_for(b"bar"), 5061);
        assert_eq!(slot_for(b"hello"), 866);
    }

    #[test]
    fn hash_tag_selects_only_the_braced_part() {
        assert_eq!(hash_tag(b"{user1000}.following"), b"user1000");
        assert_eq!(hash_tag(b"foo{bar}baz"), b"bar");
        assert_eq!(hash_tag(b"{a}"), b"a");
    }

    #[test]
    fn keys_sharing_a_tag_share_a_slot() {
        // R43: the whole point. Without tag handling these differ and multi-key misroutes.
        let a = slot_for(b"{user1000}.following");
        let b = slot_for(b"{user1000}.followers");
        assert_eq!(a, b);
        assert_eq!(
            a,
            slot_for(b"user1000"),
            "tagged key hashes as if it were just the tag"
        );
        assert_eq!(
            same_slot([&b"{u}:a"[..], &b"{u}:b"[..], &b"{u}:c"[..]]),
            Some(slot_for(b"u"))
        );
    }

    #[test]
    fn degenerate_braces_fall_back_to_the_whole_key() {
        // Empty tag, unclosed brace, and `}` before `{` all use the full key.
        assert_eq!(hash_tag(b"{}foo"), b"{}foo");
        assert_eq!(hash_tag(b"{foo"), b"{foo");
        assert_eq!(hash_tag(b"}foo{"), b"}foo{");
        assert_eq!(hash_tag(b"foo"), b"foo");
        assert_eq!(slot_for(b"{}foo"), crc16(b"{}foo") % SLOT_COUNT);
    }

    #[test]
    fn only_the_first_brace_pair_counts() {
        assert_eq!(hash_tag(b"{a}{b}"), b"a");
        assert_eq!(hash_tag(b"x{a}y{b}z"), b"a");
    }

    #[test]
    fn untagged_keys_usually_differ() {
        assert_ne!(slot_for(b"user1:a"), slot_for(b"user1:b"));
        assert_eq!(same_slot([&b"user1:a"[..], &b"user1:b"[..]]), None);
    }

    #[test]
    fn slots_are_always_in_range() {
        for i in 0..2000u32 {
            let k = format!("key:{i}");
            assert!(slot_for(k.as_bytes()) < SLOT_COUNT);
        }
        assert!(slot_for(&[0xff, 0x00, 0xfe]) < SLOT_COUNT);
    }

    #[test]
    fn binary_keys_are_supported() {
        // Keys are binary-safe, including inside a tag.
        assert_eq!(hash_tag(b"{\xff\x00}x"), b"\xff\x00");
        let _ = slot_for(b"\x00\x01\x02");
    }

    #[test]
    fn parses_moved_and_ask() {
        assert_eq!(
            parse_redirect("MOVED 3999 127.0.0.1:6381"),
            Some(Redirect {
                ask: false,
                slot: 3999,
                host: "127.0.0.1".into(),
                port: 6381
            })
        );
        assert_eq!(
            parse_redirect("ASK 3999 127.0.0.1:6381"),
            Some(Redirect {
                ask: true,
                slot: 3999,
                host: "127.0.0.1".into(),
                port: 6381
            })
        );
        assert_eq!(
            parse_redirect("MOVED 42 [::1]:7000"),
            Some(Redirect {
                ask: false,
                slot: 42,
                host: "::1".into(),
                port: 7000
            })
        );
    }

    #[test]
    fn other_errors_are_never_redirects() {
        // NET-04: CROSSSLOT is not a redirect and must not trigger one.
        for e in [
            "CROSSSLOT Keys in request don't hash to the same slot",
            "WRONGTYPE Operation against a key",
            "MOVED",
            "MOVED abc 127.0.0.1:1",
            "MOVED 1 nocolon",
            "",
        ] {
            assert_eq!(
                parse_redirect(e),
                None,
                "{e:?} must not parse as a redirect"
            );
        }
    }
}
