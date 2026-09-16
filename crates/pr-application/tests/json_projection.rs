//! V-E04 — §18.3's `--json` projection table, one case per row (v2.1 §18.3, R17, PIPE-02).
//!
//! §18.3 is a table with fourteen rows, and the rows exist because each one is a place where
//! "just serialise it as JSON" gives a wrong answer:
//!
//! - a bulk string that is not UTF-8 has no JSON string form at all
//! - `nan` and `inf` are not JSON numbers
//! - a big number put through a float parser silently loses precision
//! - a RESP3 map with duplicate keys cannot be a JSON object without losing an entry
//! - a RESP2 even-length array is *not* a map, and pretending otherwise changes the meaning
//!   of `-2`
//!
//! Every case here feeds the real decoder real wire bytes, so the test exercises what would
//! actually arrive rather than a `Value` somebody hand-built to match the code.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_application::json_projection::{
    Notice, Projection, Protocol, Strictness, is_error, project, project_many,
};
use pr_protocol::{Decoder, Step, Value};

fn decode(wire: &[u8]) -> Value {
    let mut d = Decoder::with_defaults();
    d.feed(wire);
    match d.decode().expect("the fixture is well formed") {
        Step::Value(v) => v,
        Step::Incomplete => panic!("incomplete fixture: {wire:?}"),
    }
}

fn decode_all(wire: &[u8]) -> Vec<Value> {
    let mut d = Decoder::with_defaults();
    d.feed(wire);
    let mut out = Vec::new();
    while let Ok(Step::Value(v)) = d.decode() {
        out.push(v);
    }
    out
}

fn json(wire: &[u8]) -> String {
    project(&decode(wire), Protocol::Three, Strictness::Lenient).json
}

fn full(wire: &[u8]) -> Projection {
    project(&decode(wire), Protocol::Three, Strictness::Lenient)
}

// ============================================================ row 1: UTF-8 strings
#[test]
fn row1_a_valid_utf8_string_is_a_json_string() {
    assert_eq!(json(b"+OK\r\n"), r#""OK""#);
    assert_eq!(json(b"$5\r\nhello\r\n"), r#""hello""#);
    assert_eq!(json("$6\r\n中文\r\n".as_bytes()), r#""中文""#);
    assert_eq!(json(b"$0\r\n\r\n"), r#""""#);
    // And the JSON escaping is real escaping, not a hope.
    assert_eq!(json(b"$4\r\na\"\\b\r\n"), r#""a\"\\b""#);
    assert_eq!(json(b"$3\r\na\nb\r\n"), r#""a\nb""#);
    assert_eq!(json(b"$2\r\na\x01\r\n"), r#""a\u0001""#);
}

// ============================================================ row 2: invalid UTF-8
#[test]
fn row2_invalid_utf8_becomes_a_bytes_object_and_says_so() {
    // There is no JSON string for these bytes. Producing one by replacing them would hand the
    // user a value that is not what the server holds.
    let p = full(b"$3\r\n\xff\xfe\x80\r\n");
    assert_eq!(p.json, r#"{"$bytes":"//6A"}"#);
    assert_eq!(p.notices, vec![Notice::NonUtf8 { len: 3 }]);
    assert!(p.notices[0].message().contains("not valid UTF-8"));
    assert!(p.ok(), "lenient mode still produces a document");
}

#[test]
fn row2_strict_mode_refuses_instead_of_projecting() {
    // §18.3: "`--json-strict` 下改为非零退出". A pipeline that would rather stop than receive a
    // value it cannot round-trip.
    let v = decode(b"$3\r\n\xff\xfe\x80\r\n");
    let p = project(&v, Protocol::Three, Strictness::Strict);
    assert!(!p.ok());
    assert_eq!(p.refused, Some(Notice::NonUtf8 { len: 3 }));
}

#[test]
fn row2_valid_utf8_is_never_refused_even_in_strict_mode() {
    // Strictness is about bytes with no JSON form, not about caution.
    for wire in [&b"$5\r\nhello\r\n"[..], "$6\r\n中文\r\n".as_bytes()] {
        let p = project(&decode(wire), Protocol::Three, Strictness::Strict);
        assert!(p.ok(), "{wire:?} was refused");
    }
}

// ============================================================ row 3: integers
#[test]
fn row3_an_integer_is_a_json_number() {
    assert_eq!(json(b":42\r\n"), "42");
    assert_eq!(json(b":-1\r\n"), "-1");
    assert_eq!(json(b":0\r\n"), "0");
    // i64 extremes survive: they are exactly representable, and a client that rounded them
    // would report a different number than the server holds.
    assert_eq!(json(b":9223372036854775807\r\n"), "9223372036854775807");
    assert_eq!(json(b":-9223372036854775808\r\n"), "-9223372036854775808");
}

// ============================================================ row 4: doubles
#[test]
fn row4_a_finite_double_keeps_its_lexeme() {
    // `1.2300` is a valid JSON number, and the trailing zeros were the server's choice.
    // Reparsing and reprinting would quietly change the value the user sees.
    assert_eq!(json(b",1.2300\r\n"), "1.2300");
    assert_eq!(json(b",3.141592653589793\r\n"), "3.141592653589793");
    assert_eq!(json(b",-0\r\n"), "-0");
    assert_eq!(json(b",0\r\n"), "0");
}

#[test]
fn row4_a_non_finite_double_becomes_a_string_and_says_so() {
    // JSON has no `inf` or `nan`. Emitting `null` would make "no value" and "not a number"
    // indistinguishable, which is exactly the kind of thing a script gets wrong.
    for (wire, want) in [
        (&b",inf\r\n"[..], r#""inf""#),
        (b",-inf\r\n", r#""-inf""#),
        (b",nan\r\n", r#""nan""#),
    ] {
        let p = full(wire);
        assert_eq!(p.json, want);
        assert_eq!(p.notices.len(), 1, "{wire:?}");
        assert!(matches!(p.notices[0], Notice::NonFiniteDouble { .. }));
        assert!(p.notices[0].message().contains("no JSON number form"));
    }
}

// ============================================================ row 5: big numbers
#[test]
fn row5_a_big_number_stays_a_string_so_precision_survives() {
    // A JSON number here would be read by most parsers as a float and lose digits.
    let p = full(b"(3492890328409238509324850943850943825024385\r\n");
    assert_eq!(p.json, r#""3492890328409238509324850943850943825024385""#);
    assert!(
        p.notices.is_empty(),
        "nothing was lost, so nothing is announced"
    );
}

// ============================================================ row 6: booleans and null
#[test]
fn row6_booleans_and_nulls_are_json_literals() {
    assert_eq!(json(b"#t\r\n"), "true");
    assert_eq!(json(b"#f\r\n"), "false");
    // All three null spellings project the same: the difference is a protocol detail, not
    // something a `jq` filter should have to know.
    for wire in [&b"_\r\n"[..], b"$-1\r\n", b"*-1\r\n"] {
        assert_eq!(json(wire), "null", "{wire:?}");
    }
}

// ============================================================ row 7: arrays and sets
#[test]
fn row7_an_array_is_a_json_array() {
    assert_eq!(json(b"*0\r\n"), "[]");
    assert_eq!(json(b"*2\r\n:1\r\n:2\r\n"), "[1,2]");
    assert_eq!(
        json(b"*2\r\n$1\r\na\r\n*2\r\n:1\r\n:2\r\n"),
        r#"["a",[1,2]]"#,
        "nesting survives"
    );
}

#[test]
fn row7_a_set_keeps_arrival_order() {
    // §18.3: "set 保持到达顺序". Sorting would make two runs of the same command differ for no
    // reason the user can see.
    assert_eq!(
        json(b"~3\r\n$1\r\nc\r\n$1\r\na\r\n$1\r\nb\r\n"),
        r#"["c","a","b"]"#
    );
}

// ============================================================ row 8: RESP3 maps
#[test]
fn row8_a_map_with_unique_string_keys_is_a_json_object() {
    assert_eq!(
        json(b"%2\r\n$4\r\nname\r\n$3\r\nabc\r\n$3\r\nage\r\n:30\r\n"),
        r#"{"name":"abc","age":30}"#
    );
    assert_eq!(json(b"%0\r\n"), "{}");
}

#[test]
fn row8_duplicate_keys_become_pairs_rather_than_losing_an_entry() {
    // Redis map keys can repeat. A JSON object cannot, so one entry would silently vanish.
    let p = full(b"%2\r\n$1\r\na\r\n:1\r\n$1\r\na\r\n:2\r\n");
    assert_eq!(p.json, r#"[["a",1],["a",2]]"#);
    assert_eq!(p.notices, vec![Notice::DuplicateMapKeys]);
    assert!(p.notices[0].message().contains("duplicate keys"));
}

#[test]
fn row8_a_non_string_key_becomes_pairs_too() {
    // A JSON object key must be a string. An integer key is not one, and coercing it would
    // make `1` and `"1"` the same key.
    let p = full(b"%1\r\n:1\r\n$1\r\nv\r\n");
    assert_eq!(p.json, r#"[[1,"v"]]"#);
    assert_eq!(p.notices, vec![Notice::NonStringMapKey]);
}

#[test]
fn row8_a_key_that_is_not_utf8_becomes_pairs() {
    let p = full(b"%1\r\n$2\r\n\xff\xfe\r\n$1\r\nv\r\n");
    assert!(p.json.starts_with("[["), "{}", p.json);
    assert!(p.notices.contains(&Notice::NonStringMapKey));
    assert!(
        p.notices
            .iter()
            .any(|n| matches!(n, Notice::NonUtf8 { .. }))
    );
}

// ============================================================ row 9: RESP2 flat arrays
#[test]
fn row9_a_resp2_even_array_is_not_turned_into_a_map() {
    // §18.3: "**不**自动 map 化（与 `-2` 语义一致）". `HGETALL` under RESP2 returns a flat
    // array, and a user who asked for `-2` asked for that shape. Inventing an object would
    // make `-2` mean something other than RESP2.
    let wire = b"*4\r\n$4\r\nname\r\n$3\r\nabc\r\n$3\r\nage\r\n$2\r\n30\r\n";
    let v = decode(wire);
    for protocol in [Protocol::Two, Protocol::Three] {
        let p = project(&v, protocol, Strictness::Lenient);
        assert_eq!(
            p.json, r#"["name","abc","age","30"]"#,
            "an even-length array was map-ified under {protocol:?}"
        );
        assert!(p.notices.is_empty());
    }
}

#[test]
fn pipe_02_the_same_command_projects_differently_under_the_two_protocols() {
    // The whole point of PIPE-02: `-2` and `-3` are not cosmetic. `HGETALL` gives a flat array
    // on one and a map on the other, and `--json` must not paper over the difference.
    let resp2 = json(b"*2\r\n$1\r\na\r\n:1\r\n");
    let resp3 = json(b"%1\r\n$1\r\na\r\n:1\r\n");
    assert_eq!(resp2, r#"["a",1]"#);
    assert_eq!(resp3, r#"{"a":1}"#);
    assert_ne!(resp2, resp3);
}

// ============================================================ row 10: verbatim
#[test]
fn row10_a_verbatim_string_keeps_its_content_and_announces_the_lost_tag() {
    let p = full(b"=15\r\ntxt:Some string\r\n");
    assert_eq!(p.json, r#""Some string""#);
    assert_eq!(
        p.notices,
        vec![Notice::VerbatimFormatDiscarded {
            format: "txt".into()
        }]
    );
    assert!(
        p.notices[0].message().contains("typed-json"),
        "and says where to find it"
    );
}

// ============================================================ row 11: attributes
#[test]
fn row11_an_attribute_is_discarded_and_counted() {
    let p = full(b"|1\r\n$3\r\nttl\r\n:100\r\n$5\r\nhello\r\n");
    assert_eq!(
        p.json, r#""hello""#,
        "the decorated value is what is projected"
    );
    assert_eq!(p.notices, vec![Notice::AttributeDiscarded]);
    assert!(p.notices[0].message().contains("typed-json"));
}

#[test]
fn row11_an_attribute_on_an_error_is_still_an_error() {
    // The classification must see through the wrapper, or an attributed error would exit 0.
    let v = decode(b"|1\r\n$1\r\na\r\n:1\r\n-ERR bad\r\n");
    assert!(is_error(&v));
}

// ============================================================ row 12: errors
#[test]
fn row12_an_error_reply_becomes_an_error_object() {
    let p = full(b"-ERR unknown command 'FOO'\r\n");
    assert_eq!(p.json, r#"{"error":"ERR unknown command 'FOO'"}"#);
    assert!(is_error(&decode(b"-ERR bad\r\n")));
    assert!(is_error(&decode(b"!9\r\nERR bad x\r\n")));
    assert!(!is_error(&decode(b"+OK\r\n")));
}

#[test]
fn row12_an_error_message_is_escaped_like_any_other_string() {
    // A server error carries server-controlled text, including the key name the user asked
    // for. It cannot be allowed to break out of the JSON string.
    let p = full(b"-ERR bad \"quoted\" and \\ backslash\r\n");
    assert!(p.json.contains(r#"\"quoted\""#), "{}", p.json);
    assert!(p.json.contains(r"\\"), "{}", p.json);
    // The document still parses as one object.
    assert!(p.json.starts_with(r#"{"error":""#) && p.json.ends_with(r#""}"#));
}

// ============================================================ row 13: multiple replies
#[test]
fn row13_several_replies_are_ndjson_and_one_reply_is_a_single_document() {
    let many = decode_all(b"+OK\r\n:1\r\n$3\r\nabc\r\n");
    assert_eq!(many.len(), 3);
    let p = project_many(&many, Protocol::Three, Strictness::Lenient);
    assert_eq!(p.json, "\"OK\"\n1\n\"abc\"");
    assert_eq!(p.json.lines().count(), 3);

    // A single reply is a single document, not a one-line stream: `jq .` and `jq -s .` are
    // different invocations and the shape has to match what was asked for.
    let one = project_many(&[decode(b"+OK\r\n")], Protocol::Three, Strictness::Lenient);
    assert_eq!(one.json, "\"OK\"");
    assert!(!one.json.contains('\n'));
}

#[test]
fn row13_notices_from_every_reply_are_collected() {
    let many = decode_all(b",inf\r\n$2\r\n\xff\xfe\r\n");
    let p = project_many(&many, Protocol::Three, Strictness::Lenient);
    assert_eq!(p.notices.len(), 2, "{:?}", p.notices);
    assert!(
        p.notices
            .iter()
            .any(|n| matches!(n, Notice::NonFiniteDouble { .. }))
    );
    assert!(
        p.notices
            .iter()
            .any(|n| matches!(n, Notice::NonUtf8 { .. }))
    );
}

// ============================================================ the table as a whole
#[test]
fn every_row_of_the_table_has_a_case_and_none_of_them_panics() {
    // A sweep over every shape the decoder can produce, so a new `Value` variant cannot be
    // added without this file being considered.
    let shapes: &[&[u8]] = &[
        b"+OK\r\n",
        b"-ERR x\r\n",
        b":1\r\n",
        b"$3\r\nabc\r\n",
        b"$2\r\n\xff\xfe\r\n",
        b"_\r\n",
        b"$-1\r\n",
        b"*-1\r\n",
        b"*2\r\n:1\r\n:2\r\n",
        b"#t\r\n",
        b",1.5\r\n",
        b",nan\r\n",
        b"(12345678901234567890\r\n",
        b"!5\r\nERR x\r\n",
        b"=9\r\ntxt:hello\r\n",
        b"%1\r\n$1\r\na\r\n:1\r\n",
        b"~2\r\n:1\r\n:2\r\n",
        b"|1\r\n:1\r\n:2\r\n:3\r\n",
        b">2\r\n$7\r\nmessage\r\n$1\r\nx\r\n",
    ];
    for wire in shapes {
        let p = full(wire);
        assert!(
            !p.json.is_empty(),
            "{} produced nothing",
            String::from_utf8_lossy(wire)
        );
        // Every document is balanced, which is the cheapest check that it is JSON at all.
        let opens = p.json.matches(['{', '[']).count();
        let closes = p.json.matches(['}', ']']).count();
        assert_eq!(
            opens,
            closes,
            "{} produced unbalanced JSON: {}",
            String::from_utf8_lossy(wire),
            p.json
        );
    }
}

#[test]
fn nothing_is_lost_without_a_notice() {
    // The rule the whole module rests on: every lossy row announces itself, and every lossless
    // row stays quiet. A silent loss is how a script computes the wrong answer and nobody
    // finds out.
    let lossless: &[&[u8]] = &[
        b"+OK\r\n",
        b":1\r\n",
        b"$3\r\nabc\r\n",
        b"_\r\n",
        b"#t\r\n",
        b",1.5\r\n",
        b"(123\r\n",
        b"*2\r\n:1\r\n:2\r\n",
        b"~2\r\n:1\r\n:2\r\n",
        b"%1\r\n$1\r\na\r\n:1\r\n",
        b"-ERR x\r\n",
    ];
    for wire in lossless {
        let p = full(wire);
        assert!(
            p.notices.is_empty(),
            "{} announced a loss it did not make: {:?}",
            String::from_utf8_lossy(wire),
            p.notices
        );
    }

    let lossy: &[&[u8]] = &[
        b"$2\r\n\xff\xfe\r\n",
        b",inf\r\n",
        b",nan\r\n",
        b"=9\r\ntxt:hello\r\n",
        b"|1\r\n:1\r\n:2\r\n:3\r\n",
        b"%2\r\n$1\r\na\r\n:1\r\n$1\r\na\r\n:2\r\n",
    ];
    for wire in lossy {
        let p = full(wire);
        assert!(
            !p.notices.is_empty(),
            "{} lost information silently",
            String::from_utf8_lossy(wire)
        );
    }
}

// ============================================================ the recorded differences
/// The manifest section V-E04 writes the measured baseline differences into.
const MANIFEST: &str = include_str!("../../../compatibility/manifest.toml");

#[test]
fn every_recorded_difference_is_what_the_projection_actually_does() {
    // The manifest is only worth having if it describes this code. Each row below is checked
    // against the projection, so a change here that is not recorded there fails.
    let m: toml::Table = MANIFEST.parse().expect("the manifest is valid TOML");
    let jp = m["json_projection"]
        .as_table()
        .expect("a json_projection section");
    let diffs = jp["difference"].as_array().expect("differences");
    assert_eq!(diffs.len(), 6, "the measured difference count changed");

    let case = |name: &str| -> &toml::Table {
        diffs
            .iter()
            .filter_map(toml::Value::as_table)
            .find(|d| d["case"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("{name} is not recorded in the manifest"))
    };

    // Error replies become an object, not the baseline's bare `error:` prefix.
    let d = case("error reply");
    assert!(d["penguin"].as_str().unwrap().starts_with(r#"{"error""#));
    assert!(json(b"-ERR unknown command 'x'\r\n").starts_with(r#"{"error":"#));

    // A big number keeps every digit.
    let d = case("RESP3 big number");
    let expected = d["penguin"].as_str().unwrap();
    assert_eq!(
        json(b"(3492890328409238509324850943850943825024385\r\n"),
        expected
    );

    // A map with non-string keys becomes pairs.
    let d = case("RESP3 map with non-string keys");
    assert_eq!(
        json(b"%3\r\n:0\r\n#f\r\n:1\r\n#t\r\n:2\r\n#f\r\n"),
        d["penguin"].as_str().unwrap()
    );

    // Invalid UTF-8 becomes $bytes.
    let d = case("bulk string that is not valid UTF-8");
    assert_eq!(
        json(b"$3\r\n\xff\xfe\x80\r\n"),
        d["penguin"].as_str().unwrap()
    );

    // An attribute is discarded rather than fatal.
    let _ = case("RESP3 attribute");
    let p = full(b"|1\r\n$1\r\na\r\n:1\r\n$2\r\nok\r\n");
    assert_eq!(p.json, r#""ok""#);
    assert!(p.notices.contains(&Notice::AttributeDiscarded));

    let _ = case("error reply exit code");
}

#[test]
fn every_difference_says_which_authority_governs_and_why() {
    // A recorded difference with no reason is a difference nobody decided; it just happened.
    let m: toml::Table = MANIFEST.parse().unwrap();
    let diffs = m["json_projection"]["difference"].as_array().unwrap();
    for d in diffs {
        let t = d.as_table().unwrap();
        let case = t["case"].as_str().unwrap();
        let governs = t["governs"].as_str().unwrap();
        assert!(
            governs == "18.3" || governs == "baseline",
            "{case}: {governs:?} is not an authority"
        );
        let reason = t["reason"].as_str().unwrap();
        assert!(
            reason.len() > 60,
            "{case}: the reason is too short to be one: {reason:?}"
        );
        assert!(t.contains_key("baseline") && t.contains_key("penguin"));
    }
}

#[test]
fn the_baseline_is_pinned_by_digest() {
    // A difference measured against a floating tag is a difference against nothing in
    // particular.
    let m: toml::Table = MANIFEST.parse().unwrap();
    let jp = m["json_projection"].as_table().unwrap();
    assert!(jp["baseline_image"].as_str().unwrap().contains("@sha256:"));
    assert_eq!(jp["baseline_version"].as_str(), Some("8.0.6"));
    assert!(
        jp["method"].as_str().unwrap().contains("DEBUG PROTOCOL"),
        "the manifest says how the comparison was made"
    );
}
