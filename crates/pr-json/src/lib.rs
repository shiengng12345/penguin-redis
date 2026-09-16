//! Penguin Redis — `pr-json` (v2.1 §8.3/§8.5, ADR-028).
//!
//! The authoritative model for **server business JSON**. `serde_json` is permitted only for
//! Penguin's own schemas (config, typed output); it can never carry a Redis value, because
//! its map drops duplicate members and its number normalises the lexeme.
//!
//! What this crate guarantees:
//! - duplicate object members are all retained, in source order, each addressable as `a#n`
//! - number lexemes survive verbatim: `1.2300`, `-0`, `1e3`, `9007199254740993`
//! - a path that names a duplicated member without `#n` is an error, never a silent pick
//! - editing rewrites only the target span; every other byte is untouched

pub mod contract;
pub mod dom;
pub mod path;

pub use dom::{Document, JsonError, JsonNode, Limits, Member, NodeKind, decode_string};
pub use path::{Step, parse as parse_path};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn doc(s: &str) -> Document {
        Document::parse(Bytes::copy_from_slice(s.as_bytes()), Limits::default()).unwrap()
    }

    // ---------------------------------------------------------------- DATA-04 duplicates
    #[test]
    fn duplicate_members_are_all_retained_and_addressable() {
        let d = doc(r#"{"a":1,"a":2}"#);
        let NodeKind::Object(ms) = &d.root().kind else {
            panic!("expected object")
        };
        assert_eq!(ms.len(), 2, "serde_json would have dropped one");
        assert_eq!((ms[0].occurrence, ms[0].occurrence_total), (1, 2));
        assert_eq!((ms[1].occurrence, ms[1].occurrence_total), (2, 2));
        assert_eq!(d.number_lexeme(d.get("$.a#1").unwrap()), Some("1"));
        assert_eq!(d.number_lexeme(d.get("$.a#2").unwrap()), Some("2"));
        assert_eq!(d.duplicate_members(), vec![("a".to_string(), 2)]);
    }

    #[test]
    fn ambiguous_path_is_refused_not_silently_resolved() {
        // JSON-05: the failure mode ADR-028 exists to prevent.
        let d = doc(r#"{"a":1,"a":2}"#);
        assert_eq!(
            d.get("$.a").unwrap_err(),
            JsonError::AmbiguousMember("a".into(), 2)
        );
        assert!(d.get("$.a#3").is_err());
    }

    #[test]
    fn unique_member_needs_no_occurrence() {
        let d = doc(r#"{"a":1,"b":2}"#);
        assert_eq!(d.number_lexeme(d.get("$.a").unwrap()), Some("1"));
        assert_eq!(d.number_lexeme(d.get("$.a#1").unwrap()), Some("1"));
    }

    // ---------------------------------------------------------------- DATA-03 lexemes
    #[test]
    fn number_lexemes_survive_verbatim() {
        let d = doc(r#"{"big":9007199254740993,"amt":1.2300,"e":1e3,"neg":-0,"z":0}"#);
        assert_eq!(
            d.number_lexeme(d.get("$.big").unwrap()),
            Some("9007199254740993")
        );
        assert_eq!(d.number_lexeme(d.get("$.amt").unwrap()), Some("1.2300"));
        assert_eq!(d.number_lexeme(d.get("$.e").unwrap()), Some("1e3"));
        assert_eq!(d.number_lexeme(d.get("$.neg").unwrap()), Some("-0"));
        assert_eq!(d.number_lexeme(d.get("$.z").unwrap()), Some("0"));
        // f64 would collapse the first three; prove we never went through one.
        assert_ne!(
            d.number_lexeme(d.get("$.big").unwrap()),
            Some("9007199254740992")
        );
    }

    // ---------------------------------------------------------------- DATA-02 strings
    #[test]
    fn numeric_looking_strings_stay_strings() {
        let d = doc(r#"{"id":"000123","flag":"false","n":"1e5"}"#);
        assert!(matches!(d.get("$.id").unwrap().kind, NodeKind::String));
        assert_eq!(
            d.string_value(d.get("$.id").unwrap()).as_deref(),
            Some("000123")
        );
        assert_eq!(
            d.string_value(d.get("$.flag").unwrap()).as_deref(),
            Some("false")
        );
        assert_eq!(d.number_lexeme(d.get("$.id").unwrap()), None);
    }

    #[test]
    fn escapes_and_surrogate_pairs_decode() {
        // The escape sequences are assembled at runtime so that nothing in the authoring
        // pipeline can pre-decode them: the JSON we feed the parser really does contain
        // backslash-u, which is the whole point of the test.
        let b = char::from_u32(92).unwrap();
        let src = format!(
            "{{{q}s{q}:{q}a{b}u00e9{b}n{b}t{b}{q}{b}{b} {b}ud83d{b}udc27{q}}}",
            q = char::from_u32(34).unwrap(),
            b = b
        );
        let d = doc(&src);

        // Decoded: escapes become real characters, surrogate pair becomes one scalar.
        let want = format!(
            "a{e}{nl}{tab}{q}{b} {peng}",
            e = char::from_u32(0xE9).unwrap(),
            nl = char::from_u32(10).unwrap(),
            tab = char::from_u32(9).unwrap(),
            q = char::from_u32(34).unwrap(),
            b = b,
            peng = char::from_u32(0x1_F427).unwrap()
        );
        assert_eq!(
            d.string_value(d.get("$.s").unwrap()).as_deref(),
            Some(want.as_str())
        );

        // Raw bytes are byte-identical to what arrived: still escaped, never normalised.
        let raw = std::str::from_utf8(d.raw(d.get("$.s").unwrap())).unwrap();
        assert!(
            raw.contains(&format!("{b}u00e9")),
            "raw kept the escape: {raw}"
        );
        assert!(
            raw.contains(&format!("{b}ud83d{b}udc27")),
            "raw kept the pair: {raw}"
        );
        assert!(
            !raw.contains(char::from_u32(0xE9).unwrap()),
            "raw must not be decoded"
        );
        assert!(
            !raw.contains(char::from_u32(0x1_F427).unwrap()),
            "raw must not be decoded"
        );
    }

    #[test]
    fn lone_surrogate_is_preserved_but_not_decodable() {
        // ADR-005: raw bytes are authoritative and decoding is a *view*. A lone surrogate is
        // syntactically well-formed JSON that denotes no Unicode scalar, so we keep the
        // document (the user can still copy the original bytes) and simply decline to decode
        // that string, rather than rejecting data Redis happily stores.
        for src in [
            br#"{"s":"\ud800"}"#.as_slice(),
            br#"{"s":"\udc00"}"#.as_slice(),
        ] {
            let d = Document::parse(Bytes::copy_from_slice(src), Limits::default())
                .expect("syntactically valid JSON must parse");
            let n = d.get("$.s").unwrap();
            assert!(matches!(n.kind, NodeKind::String));
            assert_eq!(
                d.raw(n),
                &src[5..src.len() - 1],
                "raw token preserved verbatim"
            );
            assert_eq!(
                d.string_value(n),
                None,
                "must decline to decode a lone surrogate"
            );
        }
    }

    // ---------------------------------------------------------------- editing
    #[test]
    fn replace_touches_only_the_target_span() {
        let src = r#"{"a":1,"a":2,"b":  "keep me"}"#;
        let d = doc(src);
        let out = d.replace("$.a#2", b"99").unwrap();
        assert_eq!(
            std::str::from_utf8(&out).unwrap(),
            r#"{"a":1,"a":99,"b":  "keep me"}"#
        );
        // whitespace, key order and the untouched duplicate all survive
        assert!(out.windows(2).any(|w| w == b"  "));
    }

    #[test]
    fn replace_refuses_an_ambiguous_path() {
        let d = doc(r#"{"a":1,"a":2}"#);
        assert!(d.replace("$.a", b"9").is_err());
    }

    #[test]
    fn nested_and_array_paths_resolve() {
        let d = doc(r#"{"cfg":{"ch":["FCM","HUAWEI"],"on":true},"cfg":{"ch":[]}}"#);
        assert_eq!(
            d.string_value(d.get("$.cfg#1.ch[1]").unwrap()).as_deref(),
            Some("HUAWEI")
        );
        assert!(matches!(
            d.get("$.cfg#1.on").unwrap().kind,
            NodeKind::Bool(true)
        ));
        let NodeKind::Array(items) = &d.get("$.cfg#2.ch").unwrap().kind else {
            panic!()
        };
        assert!(items.is_empty());
    }

    // ---------------------------------------------------------------- parser hygiene
    #[test]
    fn rejects_invalid_documents() {
        for bad in [
            "{",
            "}",
            "[1,]",
            "{\"a\":}",
            "{a:1}",
            "{'a':1}",
            "01",
            "1.",
            ".1",
            "+1",
            "1e",
            "nul",
            "tru",
            "{\"a\":1}x",
            "\"\u{1}\"",
            "[1 2]",
            "{\"a\" 1}",
            "--1",
            "1e+",
            "{\"a\":1,}",
        ] {
            assert!(
                Document::parse(Bytes::copy_from_slice(bad.as_bytes()), Limits::default()).is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn accepts_valid_edge_documents() {
        for good in [
            "{}",
            "[]",
            "0",
            "-0",
            "1e-3",
            "\"\"",
            "null",
            "true",
            "[[[]]]",
            " {\n\t\"a\" : 1 }\r\n",
        ] {
            assert!(
                Document::parse(Bytes::copy_from_slice(good.as_bytes()), Limits::default()).is_ok(),
                "should accept {good:?}"
            );
        }
    }

    #[test]
    fn depth_budget_is_enforced() {
        // v2.1 §24.3: deep nesting must hit a budget, not blow the stack.
        let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
        let e = Document::parse(
            Bytes::copy_from_slice(deep.as_bytes()),
            Limits { max_depth: 64 },
        )
        .unwrap_err();
        assert_eq!(e, JsonError::DepthExceeded(64));
        let ok = format!("{}{}", "[".repeat(50), "]".repeat(50));
        assert!(
            Document::parse(
                Bytes::copy_from_slice(ok.as_bytes()),
                Limits { max_depth: 64 }
            )
            .is_ok()
        );
    }

    #[test]
    fn auto_detection_only_accepts_complete_object_or_array() {
        // v2.1 §8.1: plain `70`, `true`, `null` are NOT auto-detected as JSON.
        assert!(Document::looks_like_json(br#"{"a":1}"#));
        assert!(Document::looks_like_json(b"  [1,2]  "));
        assert!(!Document::looks_like_json(b"70"));
        assert!(!Document::looks_like_json(b"true"));
        assert!(!Document::looks_like_json(b"null"));
        assert!(!Document::looks_like_json(b"\"str\""));
        assert!(
            !Document::looks_like_json(br#"{"a":1"#),
            "incomplete must not count"
        );
        assert!(!Document::looks_like_json(b"ACTIVE"));
    }

    #[test]
    fn source_bytes_round_trip_exactly() {
        let src = r#"{"a":1,"a":2,"s":"é","n":1.2300}"#;
        let d = doc(src);
        assert_eq!(d.source().as_ref(), src.as_bytes());
        assert_eq!(d.raw(d.root()), src.as_bytes());
    }
}
