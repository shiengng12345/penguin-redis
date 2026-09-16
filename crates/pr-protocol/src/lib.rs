//! Penguin Redis — `pr-protocol` (v2.1 §31.3).
//!
//! Incremental, bounded, lossless RESP2/RESP3 decoding. This is the kernel boundary
//! (ADR-008): no other crate parses wire bytes.

pub mod decoder;
pub mod value;

pub use decoder::{Budget, DecodeError, Decoder, Step};
pub use value::{NullForm, Value};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn dec(input: &[u8]) -> Result<Step, DecodeError> {
        let mut d = Decoder::with_defaults();
        d.feed(input);
        d.decode()
    }

    fn val(input: &[u8]) -> Value {
        match dec(input) {
            Ok(Step::Value(v)) => v,
            other => panic!("expected a value from {input:?}, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------- scalars
    #[test]
    fn decodes_every_scalar_type() {
        assert_eq!(val(b"+OK\r\n"), Value::Simple(Bytes::from_static(b"OK")));
        assert_eq!(
            val(b"-ERR bad\r\n"),
            Value::Error(Bytes::from_static(b"ERR bad"))
        );
        assert_eq!(val(b":-42\r\n"), Value::Integer(-42));
        assert_eq!(
            val(b"$3\r\nabc\r\n"),
            Value::Bulk(Bytes::from_static(b"abc"))
        );
        assert_eq!(val(b"$-1\r\n"), Value::Null(NullForm::Bulk));
        assert_eq!(val(b"*-1\r\n"), Value::Null(NullForm::Array));
        assert_eq!(val(b"_\r\n"), Value::Null(NullForm::Resp3));
        assert_eq!(val(b"#t\r\n"), Value::Boolean(true));
        assert_eq!(val(b"#f\r\n"), Value::Boolean(false));
        assert_eq!(
            val(b"!5\r\nabcde\r\n"),
            Value::BulkError(Bytes::from_static(b"abcde"))
        );
        assert_eq!(
            val(b"=6\r\ntxt:hi\r\n"),
            Value::Verbatim {
                format: *b"txt",
                data: Bytes::from_static(b"hi")
            }
        );
    }

    #[test]
    fn double_lexemes_are_preserved_not_normalised() {
        // ADR-005: 1.2300 must not become 1.23.
        for lex in [
            "1.2300",
            "-0",
            "0",
            "1e3",
            "1E-3",
            "inf",
            "-inf",
            "nan",
            "3.141592653589793",
        ] {
            let input = format!(",{lex}\r\n");
            assert_eq!(
                val(input.as_bytes()),
                Value::Double(Bytes::copy_from_slice(lex.as_bytes())),
                "{lex}"
            );
        }
    }

    #[test]
    fn big_numbers_keep_full_precision() {
        let n = "9".repeat(200);
        let input = format!("({n}\r\n");
        assert_eq!(
            val(input.as_bytes()),
            Value::BigNumber(Bytes::copy_from_slice(n.as_bytes()))
        );
    }

    #[test]
    fn bulk_strings_are_binary_safe() {
        // DATA-01: NUL, invalid UTF-8 and embedded CRLF all survive.
        assert_eq!(
            val(b"$3\r\na\0b\r\n"),
            Value::Bulk(Bytes::from_static(b"a\0b"))
        );
        assert_eq!(
            val(b"$2\r\n\xff\xfe\r\n"),
            Value::Bulk(Bytes::from_static(b"\xff\xfe"))
        );
        assert_eq!(
            val(b"$4\r\na\r\nb\r\n"),
            Value::Bulk(Bytes::from_static(b"a\r\nb"))
        );
        assert_eq!(val(b"$0\r\n\r\n"), Value::Bulk(Bytes::new()));
    }

    // ---------------------------------------------------------------- aggregates
    #[test]
    fn decodes_aggregates_and_preserves_order_and_duplicates() {
        assert_eq!(
            val(b"*2\r\n:1\r\n:2\r\n"),
            Value::Array(vec![Value::Integer(1), Value::Integer(2)])
        );
        assert_eq!(val(b"*0\r\n"), Value::Array(vec![]));
        assert_eq!(val(b"~1\r\n:7\r\n"), Value::Set(vec![Value::Integer(7)]));
        assert_eq!(
            val(b">1\r\n+m\r\n"),
            Value::Push(vec![Value::Simple(Bytes::from_static(b"m"))])
        );

        // A HashMap would collapse these two entries; an ordered entry list must not.
        let m = val(b"%2\r\n$1\r\na\r\n:1\r\n$1\r\na\r\n:2\r\n");
        let Value::Map(kv) = m else {
            panic!("expected map")
        };
        assert_eq!(kv.len(), 2, "duplicate map keys must survive");
        assert_eq!(kv[0].1, Value::Integer(1));
        assert_eq!(kv[1].1, Value::Integer(2));
    }

    #[test]
    fn attribute_decorates_the_following_value() {
        let v = val(b"|1\r\n+ttl\r\n:3\r\n+OK\r\n");
        let Value::Attribute { attrs, value } = v else {
            panic!("expected attribute")
        };
        assert_eq!(attrs.len(), 1);
        assert_eq!(*value, Value::Simple(Bytes::from_static(b"OK")));
    }

    #[test]
    fn nested_aggregates_decode() {
        let v = val(b"*2\r\n*1\r\n:1\r\n%1\r\n+k\r\n*1\r\n:2\r\n");
        assert_eq!(v.depth(), 4);
        assert_eq!(v.leaf_count(), 3);
    }

    // ---------------------------------------------------------------- streamed (R16)
    #[test]
    fn streamed_bulk_string_reassembles() {
        assert_eq!(
            val(b"$?\r\n;4\r\nHell\r\n;5\r\no wor\r\n;2\r\nld\r\n;0\r\n"),
            Value::Bulk(Bytes::from_static(b"Hello world"))
        );
        assert_eq!(val(b"$?\r\n;0\r\n"), Value::Bulk(Bytes::new()));
    }

    #[test]
    fn streamed_aggregates_decode() {
        assert_eq!(
            val(b"*?\r\n:1\r\n:2\r\n.\r\n"),
            Value::Array(vec![Value::Integer(1), Value::Integer(2)])
        );
        assert_eq!(val(b"*?\r\n.\r\n"), Value::Array(vec![]));
        let m = val(b"%?\r\n+k\r\n:1\r\n.\r\n");
        let Value::Map(kv) = m else {
            panic!("expected map")
        };
        assert_eq!(kv.len(), 1);
    }

    // ---------------------------------------------------------------- incrementality
    #[test]
    fn byte_at_a_time_equals_all_at_once() {
        // The core incremental property: chunk boundaries must not change the result.
        let inputs: [&[u8]; 8] = [
            b"+OK\r\n",
            b"$11\r\nHello world\r\n",
            b"*2\r\n$1\r\na\r\n:2\r\n",
            b"%1\r\n+k\r\n,1.2300\r\n",
            b"$?\r\n;4\r\nHell\r\n;2\r\no!\r\n;0\r\n",
            b"*?\r\n:1\r\n:2\r\n.\r\n",
            b"|1\r\n+a\r\n:1\r\n+OK\r\n",
            b"=9\r\nmkd:hello\r\n",
        ];
        for input in inputs {
            let whole = val(input);
            let mut d = Decoder::with_defaults();
            let mut got = None;
            for (i, b) in input.iter().enumerate() {
                d.feed(&[*b]);
                match d.decode().unwrap() {
                    Step::Value(v) => {
                        assert_eq!(i, input.len() - 1, "completed early for {input:?}");
                        got = Some(v);
                    }
                    Step::Incomplete => {}
                }
            }
            assert_eq!(
                got.as_ref(),
                Some(&whole),
                "byte-wise differs for {input:?}"
            );
        }
    }

    #[test]
    fn split_inside_crlf_and_length_prefix_is_handled() {
        let mut d = Decoder::with_defaults();
        d.feed(b"$1");
        assert_eq!(d.decode().unwrap(), Step::Incomplete);
        d.feed(b"1\r"); // split inside the length, then inside the CRLF
        assert_eq!(d.decode().unwrap(), Step::Incomplete);
        d.feed(b"\nHello world\r");
        assert_eq!(d.decode().unwrap(), Step::Incomplete);
        d.feed(b"\n");
        assert_eq!(
            d.decode().unwrap(),
            Step::Value(Value::Bulk(Bytes::from_static(b"Hello world")))
        );
    }

    #[test]
    fn split_inside_a_utf8_sequence_is_handled() {
        // The decoder is byte-oriented, so a multi-byte char split across chunks is fine.
        let mut d = Decoder::with_defaults();
        let payload = "héllo".as_bytes();
        d.feed(format!("${}\r\n", payload.len()).as_bytes());
        d.feed(&payload[..2]); // splits the é
        assert_eq!(d.decode().unwrap(), Step::Incomplete);
        d.feed(&payload[2..]);
        d.feed(b"\r\n");
        assert_eq!(
            d.decode().unwrap(),
            Step::Value(Value::Bulk(Bytes::copy_from_slice(payload)))
        );
    }

    #[test]
    fn decodes_successive_frames_from_one_buffer() {
        let mut d = Decoder::with_defaults();
        d.feed(b"+A\r\n+B\r\n:3\r\n");
        assert_eq!(
            d.decode().unwrap(),
            Step::Value(Value::Simple(Bytes::from_static(b"A")))
        );
        assert_eq!(
            d.decode().unwrap(),
            Step::Value(Value::Simple(Bytes::from_static(b"B")))
        );
        assert_eq!(d.decode().unwrap(), Step::Value(Value::Integer(3)));
        assert_eq!(d.decode().unwrap(), Step::Incomplete);
        assert_eq!(d.buffered(), 0, "fully consumed input must not linger");
    }

    #[test]
    fn incomplete_input_does_not_consume_the_buffer() {
        let mut d = Decoder::with_defaults();
        d.feed(b"*2\r\n:1\r\n");
        assert_eq!(d.decode().unwrap(), Step::Incomplete);
        let before = d.buffered();
        assert_eq!(d.decode().unwrap(), Step::Incomplete);
        assert_eq!(d.buffered(), before, "a failed parse must not eat bytes");
        d.feed(b":2\r\n");
        assert_eq!(
            d.decode().unwrap(),
            Step::Value(Value::Array(vec![Value::Integer(1), Value::Integer(2)]))
        );
    }

    // ---------------------------------------------------------------- protocol errors
    #[test]
    fn rejects_malformed_framing() {
        let cases: [(&[u8], &str); 12] = [
            (b"@bad\r\n", "unknown type byte"),
            (b"+OK\n", "bare LF"),
            (b":\r\n", "empty integer"),
            (b":+1\r\n", "leading plus"),
            (b":9223372036854775808\r\n", "i64 overflow"),
            (b":1.5\r\n", "not an integer"),
            (b"#x\r\n", "bad boolean"),
            (b"_x\r\n", "null with payload"),
            (b",abc\r\n", "bad double"),
            (b"(12a\r\n", "bad big number"),
            (b"$3\r\nabcd\r\n", "bulk not CRLF terminated"),
            (b"=2\r\nhi\r\n", "verbatim without xxx: prefix"),
        ];
        for (input, why) in cases {
            match dec(input) {
                Err(DecodeError::Protocol(_)) => {}
                other => panic!("{why}: expected protocol error for {input:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn negative_lengths_other_than_minus_one_are_errors() {
        assert!(matches!(dec(b"$-2\r\n"), Err(DecodeError::Protocol(_))));
        assert!(matches!(dec(b"*-5\r\n"), Err(DecodeError::Protocol(_))));
    }

    #[test]
    fn streamed_forms_reject_malformed_chunks() {
        assert!(matches!(
            dec(b"$?\r\n;x\r\n"),
            Err(DecodeError::Protocol(_))
        ));
        assert!(matches!(dec(b"$?\r\nX\r\n"), Err(DecodeError::Protocol(_))));
        assert!(matches!(
            dec(b"*?\r\n:1\r\n.x\r\n"),
            Err(DecodeError::Protocol(_))
        ));
    }

    // ---------------------------------------------------------------- budgets (§24.3)
    #[test]
    fn oversized_declared_length_is_a_budget_error_not_an_allocation() {
        // The key property: a 1 TiB claim costs nothing. If this allocated, the test would die.
        let mut d = Decoder::new(Budget {
            max_bulk_bytes: 1024,
            ..Budget::default()
        });
        d.feed(b"$1099511627776\r\n");
        assert_eq!(d.decode(), Err(DecodeError::Budget("bulk")));
        assert!(
            d.buffered() < 64,
            "must not have buffered the claimed payload"
        );
    }

    #[test]
    fn oversized_element_count_is_a_budget_error() {
        let mut d = Decoder::new(Budget {
            max_elements: 10,
            ..Budget::default()
        });
        d.feed(b"*1000000\r\n");
        assert_eq!(d.decode(), Err(DecodeError::Budget("elements")));
    }

    #[test]
    fn depth_budget_stops_deep_nesting() {
        let mut deep = b"*1\r\n".repeat(100);
        deep.extend_from_slice(b":1\r\n");
        let mut d = Decoder::new(Budget {
            max_depth: 16,
            ..Budget::default()
        });
        d.feed(&deep);
        assert_eq!(d.decode(), Err(DecodeError::Budget("depth")));
    }

    #[test]
    fn budget_errors_are_distinct_from_protocol_errors() {
        // §24.4: a well-formed but oversized stream is refused, not called corrupt.
        let mut d = Decoder::new(Budget {
            max_bulk_bytes: 4,
            ..Budget::default()
        });
        d.feed(b"$100\r\n");
        assert!(matches!(d.decode(), Err(DecodeError::Budget(_))));
        assert!(matches!(dec(b"@x\r\n"), Err(DecodeError::Protocol(_))));
    }

    #[test]
    fn frame_buffer_budget_is_enforced() {
        let mut d = Decoder::new(Budget {
            max_frame_bytes: 32,
            ..Budget::default()
        });
        d.feed(&[b'x'; 64]);
        assert_eq!(d.decode(), Err(DecodeError::Budget("frame buffer")));
    }

    // ---------------------------------------------------------------- hostile corpus
    #[test]
    fn hostile_corpus_never_panics() {
        // Runs the whole V-A03 corpus through the decoder, one byte at a time and whole,
        // asserting only that nothing panics, hangs or allocates without bound.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/protocol");
        if !dir.exists() {
            return; // fixtures are generated by `cargo run -p resp-server -- gen-fixtures`
        }
        let mut checked = 0usize;
        for cat in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            if !cat.path().is_dir() {
                continue;
            }
            for f in std::fs::read_dir(cat.path())
                .unwrap()
                .filter_map(Result::ok)
            {
                let bytes = std::fs::read(f.path()).unwrap();
                let mut d = Decoder::with_defaults();
                d.feed(&bytes);
                let _ = d.decode(); // any Ok/Err is fine; a panic or hang is not
                let mut d2 = Decoder::with_defaults();
                for b in &bytes {
                    d2.feed(&[*b]);
                    if d2.decode().is_err() {
                        break;
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 300, "expected the full corpus, saw {checked}");
    }
}
