//! What differs between the Redis and Valkey catalogs (v2.1 §20.3, R29, V-I02).
//!
//! The two projects diverged in 2024 and keep diverging. A client that presents the union of
//! their commands tells a Valkey user that `HEXPIRE` exists; one that presents the
//! intersection hides it from a Redis user. Both are wrong in the same way — they answer a
//! question the user did not ask ("what do these servers have in common?") instead of the one
//! they did ("what can I run *here*?").
//!
//! So the divergence is recorded, not resolved. Four kinds of difference matter, and they have
//! different consequences:
//!
//! | difference | what it breaks if ignored |
//! |---|---|
//! | a command one side lacks | a suggestion the server will reject |
//! | different arguments | a suggestion with the wrong shape |
//! | a different `since` | an option offered to a server too old for it |
//! | different effects | **a policy decision made on the wrong classification** |
//!
//! The last one is the reason this is in Phase 0 rather than in a documentation task.

use crate::spec::{CommandSpec, Node, Role};

/// One command whose grammar differs between the families.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgumentDifference {
    /// Command name.
    pub name: String,
    /// Redis's signature.
    pub redis: String,
    /// Valkey's signature.
    pub valkey: String,
    /// Keywords Redis accepts and Valkey does not.
    pub redis_only_keywords: Vec<String>,
    /// Keywords Valkey accepts and Redis does not.
    pub valkey_only_keywords: Vec<String>,
}

/// One command introduced at different versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SinceDifference {
    /// Command name.
    pub name: String,
    /// Redis's `since`.
    pub redis: Option<String>,
    /// Valkey's `since`.
    pub valkey: Option<String>,
}

/// One command the two families classify differently.
///
/// This is the dangerous category: an approval prompt is driven by the classification, so a
/// mismatch means one of the two servers gets a decision made on the wrong information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectDifference {
    /// Command name.
    pub name: String,
    /// How Redis classifies it.
    pub redis: String,
    /// How Valkey classifies it.
    pub valkey: String,
}

/// The whole comparison.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogDiff {
    /// Commands only Redis has.
    pub redis_only: Vec<String>,
    /// Commands only Valkey has.
    pub valkey_only: Vec<String>,
    /// Shared commands whose grammar differs.
    pub arguments: Vec<ArgumentDifference>,
    /// Shared commands introduced at different versions.
    pub since: Vec<SinceDifference>,
    /// Shared commands classified differently.
    pub effects: Vec<EffectDifference>,
    /// Commands present in both, identical in every respect compared here.
    pub identical: usize,
}

impl CatalogDiff {
    /// Total number of differences of every kind.
    #[must_use]
    pub fn total(&self) -> usize {
        self.redis_only.len()
            + self.valkey_only.len()
            + self.arguments.len()
            + self.since.len()
            + self.effects.len()
    }
}

/// Compare two compiled catalogs.
///
/// Everything is sorted, so the output is a file a human can diff between snapshot refreshes
/// rather than a reshuffle nobody can read.
#[must_use]
pub fn diff(redis: &[CommandSpec], valkey: &[CommandSpec]) -> CatalogDiff {
    let by_name = |v: &[CommandSpec]| -> std::collections::BTreeMap<String, CommandSpec> {
        v.iter().map(|c| (c.name.clone(), c.clone())).collect()
    };
    let (r, v) = (by_name(redis), by_name(valkey));

    let mut out = CatalogDiff {
        redis_only: r.keys().filter(|k| !v.contains_key(*k)).cloned().collect(),
        valkey_only: v.keys().filter(|k| !r.contains_key(*k)).cloned().collect(),
        ..CatalogDiff::default()
    };

    for (name, rc) in &r {
        let Some(vc) = v.get(name) else { continue };
        let mut differed = false;

        let (rs, vs) = (signature(&rc.grammar), signature(&vc.grammar));
        if rs != vs {
            differed = true;
            let rk = rc.grammar.keywords();
            let vk = vc.grammar.keywords();
            out.arguments.push(ArgumentDifference {
                name: name.clone(),
                redis: rs,
                valkey: vs,
                redis_only_keywords: rk.iter().filter(|k| !vk.contains(k)).cloned().collect(),
                valkey_only_keywords: vk.iter().filter(|k| !rk.contains(k)).cloned().collect(),
            });
        }
        if rc.since != vc.since {
            differed = true;
            out.since.push(SinceDifference {
                name: name.clone(),
                redis: rc.since.clone(),
                valkey: vc.since.clone(),
            });
        }
        if rc.effects != vc.effects {
            differed = true;
            out.effects.push(EffectDifference {
                name: name.clone(),
                redis: describe(rc),
                valkey: describe(vc),
            });
        }
        if !differed {
            out.identical += 1;
        }
    }
    out
}

/// A command's effects, as a stable short string.
fn describe(c: &CommandSpec) -> String {
    let e = c.effects;
    let mut parts = Vec::new();
    for (flag, name) in [
        (e.reads_data, "read"),
        (e.writes_data, "write"),
        (e.consumes, "consumes"),
        (e.blocks, "blocks"),
        (e.admin, "admin"),
        (e.destructive, "destructive"),
        (e.unknown, "unknown"),
    ] {
        if flag {
            parts.push(name);
        }
    }
    if parts.is_empty() {
        "none".to_owned()
    } else {
        parts.join("+")
    }
}

/// Render a grammar as a one-line signature, the way help would show it (§13.2).
///
/// Deterministic and lossless enough to compare: two grammars render the same string only if
/// they accept the same shapes.
#[must_use]
pub fn signature(node: &Node) -> String {
    match node {
        Node::Token { text, .. } => text.clone(),
        Node::Arg { name, role } => match role {
            Role::Key => format!("<{name}:key>"),
            Role::Value => format!("<{name}>"),
            other => format!("<{name}:{}>", role_tag(*other)),
        },
        Node::Optional(inner) => format!("[{}]", signature(inner)),
        Node::Choice(alts) => alts.iter().map(signature).collect::<Vec<_>>().join(" | "),
        Node::Seq(items) => items
            .iter()
            .map(signature)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        Node::Repeat(inner) => format!("{} ...", signature(inner)),
        Node::Paired { first, second } => {
            format!("<{}...> <{}...>", role_tag(*first), role_tag(*second))
        }
    }
}

fn role_tag(r: Role) -> &'static str {
    match r {
        Role::Subcommand => "subcommand",
        Role::Key => "key",
        Role::Field => "field",
        Role::Member => "member",
        Role::Score => "score",
        Role::Value => "value",
        Role::Cursor => "cursor",
        Role::Group => "group",
        Role::Consumer => "consumer",
        Role::Channel => "channel",
        Role::EntryId => "id",
        Role::NumKeys => "numkeys",
        Role::Keyword => "keyword",
        Role::Integer(u) => u.label(),
        Role::Pattern => "pattern",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::spec::{Family, Unit};
    use pr_core::Effects;

    fn spec(name: &str, grammar: Node, effects: Effects, since: Option<&str>) -> CommandSpec {
        CommandSpec {
            name: name.to_owned(),
            summary: String::new(),
            effects,
            grammar,
            exclusive: Vec::new(),
            family: Family::Both,
            group: String::new(),
            since: since.map(str::to_owned),
            keyword_since: Vec::new(),
            is_container: false,
        }
    }

    #[test]
    fn a_signature_reads_like_the_documentation() {
        let set = Node::Seq(vec![
            Node::arg("key", Role::Key),
            Node::arg("value", Role::Value),
            Node::Optional(Box::new(Node::Choice(vec![
                Node::keyword("NX"),
                Node::keyword("XX"),
            ]))),
            Node::Optional(Box::new(Node::Seq(vec![
                Node::keyword("EX"),
                Node::arg("seconds", Role::Integer(Unit::Seconds)),
            ]))),
        ]);
        assert_eq!(
            signature(&set),
            "<key:key> <value> [NX | XX] [EX <seconds:seconds>]"
        );
    }

    #[test]
    fn a_repeat_and_a_paired_list_are_both_visible_in_the_signature() {
        let hset = Node::Seq(vec![
            Node::arg("key", Role::Key),
            Node::Repeat(Box::new(Node::Seq(vec![
                Node::arg("field", Role::Field),
                Node::arg("value", Role::Value),
            ]))),
        ]);
        assert_eq!(signature(&hset), "<key:key> <field:field> <value> ...");

        let xread = Node::Seq(vec![
            Node::keyword("STREAMS"),
            Node::Paired {
                first: Role::Key,
                second: Role::EntryId,
            },
        ]);
        assert_eq!(signature(&xread), "STREAMS <key...> <id...>");
    }

    #[test]
    fn identical_catalogs_produce_no_differences() {
        let a = vec![spec(
            "GET",
            Node::arg("key", Role::Key),
            Effects::read(),
            Some("1.0.0"),
        )];
        let d = diff(&a, &a);
        assert_eq!(d.total(), 0);
        assert_eq!(d.identical, 1);
    }

    #[test]
    fn a_command_only_one_side_has_is_listed_on_that_side() {
        let r = vec![
            spec("GET", Node::arg("k", Role::Key), Effects::read(), None),
            spec("HEXPIRE", Node::arg("k", Role::Key), Effects::write(), None),
        ];
        let v = vec![
            spec("GET", Node::arg("k", Role::Key), Effects::read(), None),
            spec("COMMANDLOG", Node::Seq(vec![]), Effects::read(), None),
        ];
        let d = diff(&r, &v);
        assert_eq!(d.redis_only, ["HEXPIRE"]);
        assert_eq!(d.valkey_only, ["COMMANDLOG"]);
        assert_eq!(d.identical, 1);
        assert_eq!(d.total(), 2);
    }

    #[test]
    fn an_extra_option_on_one_side_is_reported_with_the_keyword_named() {
        // Valkey 8.1's `SET ... IFEQ` is the real case: offering it to a Redis server produces
        // a syntax error the user did not cause.
        let base = Node::Seq(vec![
            Node::arg("key", Role::Key),
            Node::arg("value", Role::Value),
        ]);
        let with_ifeq = Node::Seq(vec![
            Node::arg("key", Role::Key),
            Node::arg("value", Role::Value),
            Node::Optional(Box::new(Node::keyword("IFEQ"))),
        ]);
        let r = vec![spec("SET", base, Effects::write(), None)];
        let v = vec![spec("SET", with_ifeq, Effects::write(), None)];
        let d = diff(&r, &v);
        assert_eq!(d.arguments.len(), 1);
        assert_eq!(d.arguments[0].name, "SET");
        assert_eq!(d.arguments[0].valkey_only_keywords, ["IFEQ"]);
        assert!(d.arguments[0].redis_only_keywords.is_empty());
        assert!(d.arguments[0].valkey.contains("IFEQ"));
        assert_eq!(d.identical, 0);
    }

    #[test]
    fn a_different_since_is_reported_because_an_old_server_would_reject_it() {
        let r = vec![spec(
            "OBJECT|FREQ",
            Node::Seq(vec![]),
            Effects::read(),
            Some("4.0.0"),
        )];
        let v = vec![spec(
            "OBJECT|FREQ",
            Node::Seq(vec![]),
            Effects::read(),
            Some("7.0.0"),
        )];
        let d = diff(&r, &v);
        assert_eq!(d.since.len(), 1);
        assert_eq!(d.since[0].redis.as_deref(), Some("4.0.0"));
        assert_eq!(d.since[0].valkey.as_deref(), Some("7.0.0"));
    }

    #[test]
    fn a_classification_difference_is_reported_in_words_not_as_a_bitfield() {
        // This is the dangerous category: an approval prompt is driven by the classification.
        let r = vec![spec("X", Node::Seq(vec![]), Effects::read(), None)];
        let v = vec![spec(
            "X",
            Node::Seq(vec![]),
            Effects {
                reads_data: true,
                destructive: true,
                ..Effects::default()
            },
            None,
        )];
        let d = diff(&r, &v);
        assert_eq!(d.effects.len(), 1);
        assert_eq!(d.effects[0].redis, "read");
        assert_eq!(d.effects[0].valkey, "read+destructive");
    }

    #[test]
    fn one_command_can_differ_in_several_ways_and_is_counted_once_as_not_identical() {
        let r = vec![spec(
            "X",
            Node::arg("a", Role::Key),
            Effects::read(),
            Some("1.0"),
        )];
        let v = vec![spec(
            "X",
            Node::arg("b", Role::Value),
            Effects::write(),
            Some("2.0"),
        )];
        let d = diff(&r, &v);
        assert_eq!(d.identical, 0);
        assert_eq!(d.arguments.len(), 1);
        assert_eq!(d.since.len(), 1);
        assert_eq!(d.effects.len(), 1);
        assert_eq!(d.total(), 3, "each kind of difference is reported once");
    }

    #[test]
    fn the_output_is_sorted_so_a_refresh_produces_a_readable_diff() {
        let mk = |n: &str| spec(n, Node::Seq(vec![]), Effects::read(), None);
        let r = vec![mk("ZZZ"), mk("AAA"), mk("MMM")];
        let v: Vec<CommandSpec> = vec![];
        let d = diff(&r, &v);
        assert_eq!(d.redis_only, ["AAA", "MMM", "ZZZ"]);
    }
}
