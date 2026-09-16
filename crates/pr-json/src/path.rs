//! Occurrence-aware paths (v2.1 §8.3, ADR-028).
//!
//! `$.a`, `$.a[2]`, `$["a"]` and — the part that matters — `$.a#2` selecting the second
//! member named `a`. When a name is duplicated, a path without `#n` is **rejected** rather
//! than silently resolving to the first or last, because that silent choice is exactly how
//! an editor would overwrite the wrong member.

use crate::dom::{JsonError, JsonNode, NodeKind};

/// One step of a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Object member, optionally disambiguated by 1-based occurrence.
    Member {
        /// Member name.
        name: String,
        /// Which occurrence, 1-based. `None` means "must be unique".
        occurrence: Option<u32>,
    },
    /// Array index, 0-based.
    Index(usize),
}

/// Parse a path expression.
///
/// # Errors
/// [`JsonError::BadPath`] on malformed syntax.
pub fn parse(p: &str) -> Result<Vec<Step>, JsonError> {
    let mut steps = Vec::new();
    let b = p.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'$') {
        i = 1;
    }
    while i < b.len() {
        match b[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < b.len() && !matches!(b[i], b'.' | b'[' | b'#') {
                    i += 1;
                }
                if start == i {
                    return Err(JsonError::BadPath("empty member name"));
                }
                let name = p[start..i].to_owned();
                let occurrence = parse_occurrence(p, b, &mut i)?;
                steps.push(Step::Member { name, occurrence });
            }
            b'[' => {
                i += 1;
                if i < b.len() && b[i] == b'"' {
                    i += 1;
                    let start = i;
                    while i < b.len() && b[i] != b'"' {
                        i += 1;
                    }
                    if i >= b.len() {
                        return Err(JsonError::BadPath("unterminated quoted member"));
                    }
                    let name = p[start..i].to_owned();
                    i += 1; // closing quote
                    if i >= b.len() || b[i] != b']' {
                        return Err(JsonError::BadPath("expected ]"));
                    }
                    i += 1;
                    let occurrence = parse_occurrence(p, b, &mut i)?;
                    steps.push(Step::Member { name, occurrence });
                } else {
                    let start = i;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    if start == i || i >= b.len() || b[i] != b']' {
                        return Err(JsonError::BadPath("expected [n]"));
                    }
                    let n = p[start..i]
                        .parse::<usize>()
                        .map_err(|_| JsonError::BadPath("bad index"))?;
                    i += 1;
                    steps.push(Step::Index(n));
                }
            }
            _ => return Err(JsonError::BadPath("expected '.' or '['")),
        }
    }
    Ok(steps)
}

fn parse_occurrence(p: &str, b: &[u8], i: &mut usize) -> Result<Option<u32>, JsonError> {
    if *i < b.len() && b[*i] == b'#' {
        *i += 1;
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        if start == *i {
            return Err(JsonError::BadPath("expected digits after '#'"));
        }
        let n = p[start..*i]
            .parse::<u32>()
            .map_err(|_| JsonError::BadPath("bad occurrence"))?;
        if n == 0 {
            return Err(JsonError::BadPath("occurrence is 1-based"));
        }
        return Ok(Some(n));
    }
    Ok(None)
}

/// Apply one step.
///
/// # Errors
/// [`JsonError::AmbiguousMember`] if the name is duplicated and no occurrence was given;
/// [`JsonError::NoSuchPath`] otherwise.
pub fn step<'a>(node: &'a JsonNode, s: &Step) -> Result<&'a JsonNode, JsonError> {
    match (&node.kind, s) {
        (NodeKind::Object(ms), Step::Member { name, occurrence }) => {
            let matching: Vec<_> = ms.iter().filter(|m| &m.key == name).collect();
            if matching.is_empty() {
                return Err(JsonError::NoSuchPath);
            }
            match occurrence {
                None => {
                    if matching.len() > 1 {
                        // The whole point of ADR-028.
                        return Err(JsonError::AmbiguousMember(
                            name.clone(),
                            u32::try_from(matching.len()).unwrap_or(u32::MAX),
                        ));
                    }
                    Ok(&matching[0].value)
                }
                Some(n) => matching
                    .iter()
                    .find(|m| m.occurrence == *n)
                    .map(|m| &m.value)
                    .ok_or(JsonError::NoSuchPath),
            }
        }
        (NodeKind::Array(items), Step::Index(i)) => items.get(*i).ok_or(JsonError::NoSuchPath),
        _ => Err(JsonError::NoSuchPath),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_forms() {
        assert_eq!(
            parse("$.a").unwrap(),
            vec![Step::Member {
                name: "a".into(),
                occurrence: None
            }]
        );
        assert_eq!(
            parse(".a").unwrap(),
            vec![Step::Member {
                name: "a".into(),
                occurrence: None
            }]
        );
        assert_eq!(
            parse("$.a#2").unwrap(),
            vec![Step::Member {
                name: "a".into(),
                occurrence: Some(2)
            }]
        );
        assert_eq!(
            parse("$[\"a b\"]").unwrap(),
            vec![Step::Member {
                name: "a b".into(),
                occurrence: None
            }]
        );
        assert_eq!(
            parse("$[\"a\"]#3").unwrap(),
            vec![Step::Member {
                name: "a".into(),
                occurrence: Some(3)
            }]
        );
        assert_eq!(parse("$[2]").unwrap(), vec![Step::Index(2)]);
        assert_eq!(
            parse("$.a[1].b#2").unwrap(),
            vec![
                Step::Member {
                    name: "a".into(),
                    occurrence: None
                },
                Step::Index(1),
                Step::Member {
                    name: "b".into(),
                    occurrence: Some(2)
                },
            ]
        );
        assert_eq!(parse("$").unwrap(), vec![]);
    }

    #[test]
    fn rejects_bad_syntax() {
        assert!(parse("$.").is_err());
        assert!(parse("$.a#").is_err());
        assert!(parse("$.a#0").is_err(), "occurrence is 1-based");
        assert!(parse("$[x]").is_err());
        assert!(parse("$[\"a").is_err());
        assert!(parse("a").is_err());
    }
}
