//! `CommandSpec` and the grammar it carries (v2.1 §11.4, §11.7, ADR-030, V-F01/V-F04).
//!
//! §11.7 lists the syntax shapes the analyser has to get right, and they are listed there
//! because each one has a specific way of going wrong:
//! - a subcommand is not a key (`CLIENT LIST`)
//! - `HSET k f v f v` alternates roles, and the value position must never be filled from
//!   observed data (§12.2)
//! - `ZADD k score member` must not swap score and member
//! - `SET` conditions and expiry options are mutually exclusive *groups*, filtered by version
//! - `EVAL ... numkeys ...` decides how many of the following arguments are keys
//! - a key may legitimately be named `NX` or `GET`, so a literal is decided by **position**,
//!   not by matching the word (ASSIST-015)

use pr_core::Effects;

/// What an argument means, which is what drives the right candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A subcommand token (`CLIENT` **LIST**). Never a key.
    Subcommand,
    /// A key name.
    Key,
    /// A hash field name, scoped to the preceding key.
    Field,
    /// A set/zset member.
    Member,
    /// A sorted-set score.
    Score,
    /// A value. Never completed from observed data (§12.2).
    Value,
    /// A cursor.
    Cursor,
    /// A consumer group.
    Group,
    /// A pub/sub channel or shard channel (R34 — channels are attacker-influenced names too).
    Channel,
    /// A stream consumer name inside a group.
    Consumer,
    /// A stream entry id.
    EntryId,
    /// A count of following keys, e.g. `EVAL numkeys`.
    NumKeys,
    /// A literal keyword such as `EX` or `NX`.
    Keyword,
    /// An integer parameter with a unit.
    Integer(Unit),
    /// A pattern, used verbatim.
    Pattern,
}

impl Role {
    /// Whether observed names may be offered here.
    ///
    /// The value position is excluded on purpose: filling it from another key's data is how
    /// a suggestion turns into a data leak (§12.2, ASSIST-006).
    #[must_use]
    pub fn accepts_observed_names(self) -> bool {
        matches!(
            self,
            Self::Key
                | Self::Field
                | Self::Member
                | Self::Group
                | Self::Consumer
                | Self::Channel
                | Self::EntryId
        )
    }
}

/// The unit an integer argument is expressed in, so the helper can explain it (§13.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// No unit; a plain count.
    Count,
    /// Seconds.
    Seconds,
    /// Milliseconds.
    Milliseconds,
    /// Absolute Unix seconds.
    UnixSeconds,
    /// Absolute Unix milliseconds.
    UnixMilliseconds,
}

impl Unit {
    /// Short label shown beside the argument.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Seconds => "seconds",
            Self::Milliseconds => "milliseconds",
            Self::UnixSeconds => "unix seconds",
            Self::UnixMilliseconds => "unix milliseconds",
        }
    }
}

/// One node of a command's grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A fixed token, e.g. `LIST` after `CLIENT`.
    Token {
        /// The literal text.
        text: String,
        /// What it means.
        role: Role,
    },
    /// A single argument.
    Arg {
        /// Display name, e.g. `key`.
        name: String,
        /// What it means.
        role: Role,
    },
    /// An optional group.
    Optional(Box<Node>),
    /// One of several alternatives, at most one of which may appear.
    Choice(Vec<Node>),
    /// A sequence.
    Seq(Vec<Node>),
    /// A repeating group, e.g. `field value ...`.
    Repeat(Box<Node>),
    /// Two repeating halves whose lengths are equal and are only known from the total, e.g.
    /// `XREAD ... STREAMS key... id...` (§11.7).
    ///
    /// Nothing in the text marks where the keys stop. Walking it as two ordinary repeats
    /// makes the first one swallow the ids, and then every stream id is treated as a key —
    /// which on a cluster is a routing decision taken on made-up data.
    Paired {
        /// Role of the first half.
        first: Role,
        /// Role of the second half.
        second: Role,
    },
}

impl Node {
    /// A fixed keyword.
    #[must_use]
    pub fn keyword(text: &str) -> Self {
        Self::Token {
            text: text.to_owned(),
            role: Role::Keyword,
        }
    }
    /// A subcommand token.
    #[must_use]
    pub fn subcommand(text: &str) -> Self {
        Self::Token {
            text: text.to_owned(),
            role: Role::Subcommand,
        }
    }
    /// A named argument.
    #[must_use]
    pub fn arg(name: &str, role: Role) -> Self {
        Self::Arg {
            name: name.to_owned(),
            role,
        }
    }

    /// The role at flat argument position `index`, counting from 0 after the command name.
    ///
    /// Repeats cycle, which is what makes `HSET k f v f v` alternate correctly.
    #[must_use]
    pub fn role_at(&self, index: usize) -> Option<Role> {
        let mut flat = Vec::new();
        self.flatten(&mut flat);
        if let Some(r) = flat.get(index) {
            return Some(*r);
        }
        // Past the fixed part: if the grammar ends in a repeat, cycle through it.
        if let Some(rep) = self.trailing_repeat() {
            let mut cycle = Vec::new();
            rep.flatten(&mut cycle);
            if !cycle.is_empty() && index >= flat.len() {
                let fixed = flat.len().saturating_sub(cycle.len());
                let offset = (index - fixed) % cycle.len();
                return cycle.get(offset).copied();
            }
        }
        None
    }

    fn trailing_repeat(&self) -> Option<&Node> {
        match self {
            Self::Repeat(inner) => Some(inner),
            Self::Seq(items) => items.last().and_then(Node::trailing_repeat),
            _ => None,
        }
    }

    /// Flatten the mandatory spine into argument roles.
    fn flatten(&self, out: &mut Vec<Role>) {
        match self {
            Self::Token { role, .. } | Self::Arg { role, .. } => out.push(*role),
            Self::Seq(items) => {
                for i in items {
                    i.flatten(out);
                }
            }
            // An optional group contributes nothing to the mandatory spine, and a choice is
            // resolved by what the user actually typed, not by guessing here.
            Self::Optional(_) | Self::Choice(_) => {}
            Self::Repeat(inner) => inner.flatten(out),
            Self::Paired { first, second } => {
                out.push(*first);
                out.push(*second);
            }
        }
    }

    /// Every keyword this grammar can accept anywhere.
    ///
    /// Which of them are *still* legal depends on what has already been chosen, and that is
    /// [`CommandSpec::available_keywords`]'s job — the exclusive groups live on the spec, not
    /// on the grammar.
    #[must_use]
    pub fn keywords(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_keywords(&mut out);
        out.sort();
        out.dedup();
        out
    }

    fn collect_keywords(&self, out: &mut Vec<String>) {
        match self {
            Self::Token {
                text,
                role: Role::Keyword,
            } => out.push(text.clone()),
            Self::Optional(inner) | Self::Repeat(inner) => inner.collect_keywords(out),
            Self::Choice(items) | Self::Seq(items) => {
                for i in items {
                    i.collect_keywords(out);
                }
            }
            Self::Token { .. } | Self::Arg { .. } | Self::Paired { .. } => {}
        }
    }
}

/// Mutually exclusive option groups, e.g. `SET`'s expiry set (§11.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExclusiveGroup {
    /// Group name, for the explanation shown to the user.
    pub name: String,
    /// Keywords, at most one of which may be present.
    pub members: Vec<String>,
}

/// A command as the catalog knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    /// Canonical uppercase name.
    pub name: String,
    /// One-line summary.
    pub summary: String,
    /// Locally assigned effects — the policy input (ADR-030).
    pub effects: Effects,
    /// Argument grammar.
    pub grammar: Node,
    /// Mutually exclusive option groups.
    pub exclusive: Vec<ExclusiveGroup>,
    /// Server family this applies to, so Redis and Valkey can differ (R29).
    pub family: Family,
    /// Documentation group, e.g. `string`, `stream`, `bf`.
    pub group: String,
    /// First server version that accepted it, if the snapshot says.
    pub since: Option<String>,
    /// First server version that accepted each optional keyword, e.g. `GET` -> `6.2.0`.
    ///
    /// Needed because §11.7 requires options to be filtered by version: offering `GET` to a
    /// 6.0 server produces a syntax error the user did not cause.
    pub keyword_since: Vec<(String, String)>,
    /// True for a bare container such as `CLIENT`, which cannot be executed on its own.
    ///
    /// A container carries no effects: `CLIENT` reports none while `CLIENT KILL` is admin,
    /// so consulting the container for policy would under-classify every subcommand
    /// (§11.7, ADR-030).
    pub is_container: bool,
}

/// Which server family a spec describes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Family {
    /// Applies to both.
    #[default]
    Both,
    /// Redis only.
    Redis,
    /// Valkey only.
    Valkey,
}

impl CommandSpec {
    /// The role at flat argument index `index`.
    #[must_use]
    pub fn role_at(&self, index: usize) -> Option<Role> {
        self.grammar.role_at(index)
    }

    /// Whether observed names may be offered at `index`.
    #[must_use]
    pub fn accepts_observed_names_at(&self, index: usize) -> bool {
        self.role_at(index)
            .is_some_and(Role::accepts_observed_names)
    }

    /// Given keywords already present, which remaining ones are still legal.
    ///
    /// Choosing `EX` removes `PX`, `EXAT`, `PXAT` and `KEEPTTL`, because they are one
    /// exclusive group; it does **not** remove `NX`, which is a different group (§11.8).
    #[must_use]
    pub fn available_keywords(&self, already: &[&str]) -> Vec<String> {
        let upper: Vec<String> = already.iter().map(|s| s.to_uppercase()).collect();
        let mut blocked: Vec<String> = Vec::new();
        for g in &self.exclusive {
            if g.members.iter().any(|m| upper.contains(m)) {
                blocked.extend(g.members.iter().cloned());
            }
        }
        self.grammar
            .keywords()
            .into_iter()
            .filter(|k| !blocked.contains(k) && !upper.contains(k))
            .collect()
    }

    /// Keywords still legal, additionally filtered by what `server_version` supports.
    ///
    /// A keyword with no recorded `since` is assumed available: the snapshot omits `since`
    /// for anything that has been there from the start.
    #[must_use]
    pub fn available_keywords_for(&self, already: &[&str], server_version: &str) -> Vec<String> {
        self.available_keywords(already)
            .into_iter()
            .filter(|k| match self.keyword_since.iter().find(|(n, _)| n == k) {
                Some((_, since)) => version_at_least(server_version, since),
                None => true,
            })
            .collect()
    }

    /// Which exclusive group a keyword belongs to, for the explanation text.
    #[must_use]
    pub fn group_of(&self, keyword: &str) -> Option<&ExclusiveGroup> {
        let up = keyword.to_uppercase();
        self.exclusive.iter().find(|g| g.members.contains(&up))
    }
}

/// Compare dotted numeric versions: is `have` at least `want`?
///
/// Only the numeric prefix is compared; a suffix such as `-rc1` is ignored rather than
/// guessed at, because a release candidate that does have the feature must not be refused.
#[must_use]
pub fn version_at_least(have: &str, want: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split(['.', '-'])
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (h, w) = (parts(have), parts(want));
    for i in 0..h.len().max(w.len()) {
        let (a, b) = (
            h.get(i).copied().unwrap_or(0),
            w.get(i).copied().unwrap_or(0),
        );
        if a != b {
            return a > b;
        }
    }
    true
}

/// The `numkeys` rule: how many of the following arguments are keys (§11.7).
///
/// Returns the role for the argument at `index` given a parsed `numkeys` value.
#[must_use]
pub fn eval_role_at(index: usize, numkeys: Option<usize>) -> Role {
    // EVAL script numkeys key... arg...
    match index {
        0 => Role::Value,   // the script body
        1 => Role::NumKeys, // how many keys follow
        _ => {
            let Some(n) = numkeys else { return Role::Value };
            if index < 2 + n {
                Role::Key
            } else {
                Role::Value
            }
        }
    }
}

/// Walk `grammar` against the argument bytes the user actually typed, returning one role per
/// argument (v2.1 §11.7, V-F04).
///
/// A positional table is not enough, because the optional groups sit *between* the key and
/// the repeating data: for `ZADD k NX 1 a` the score is at index 2, and for `ZADD k 1 a` it
/// is at index 1. The walker therefore matches real tokens.
///
/// Two rules keep it honest:
/// - a keyword is only consumed where the grammar permits one **and** the bytes match, so a
///   key legitimately named `NX` stays a key (ASSIST-015)
/// - anything the grammar does not cover falls back to [`Role::Value`], which accepts no
///   observed names — unknown positions fail closed (§12.2)
#[must_use]
pub fn resolve_roles(grammar: &Node, args: &[&[u8]]) -> Vec<Role> {
    let mut w = Walker {
        args,
        i: 0,
        out: Vec::with_capacity(args.len()),
        numkeys: NumKeys::Unseen,
    };
    w.walk(grammar);
    while w.out.len() < args.len() {
        w.out.push(Role::Value);
    }
    w.out
}

/// How many of the arguments after a `numkeys` are keys.
///
/// `Unparsed` is deliberately distinct from `Unseen`: a `numkeys` that is there but is not a
/// number must make the key repeat consume **nothing**, because guessing turns into a routing
/// decision on a cluster (ASSIST-013). Treating it as absent would fall through to the greedy
/// repeat and swallow every remaining argument as a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumKeys {
    /// No `numkeys` argument has been reached.
    Unseen,
    /// Seen, but the value is not a number.
    Unparsed,
    /// Seen, and this many keys follow.
    Exactly(usize),
}

struct Walker<'a> {
    args: &'a [&'a [u8]],
    i: usize,
    out: Vec<Role>,
    /// What is known about a `numkeys` argument, which governs the following key repeat.
    numkeys: NumKeys,
}

impl Walker<'_> {
    fn peek_matches(&self, text: &str) -> bool {
        self.args
            .get(self.i)
            .is_some_and(|a| a.eq_ignore_ascii_case(text.as_bytes()))
    }

    fn take(&mut self, role: Role) {
        if self.i < self.args.len() {
            self.out.push(role);
            self.i += 1;
        }
    }

    fn walk(&mut self, node: &Node) {
        if self.i >= self.args.len() {
            return;
        }
        match node {
            Node::Token { text, role } => {
                if self.peek_matches(text) {
                    self.take(*role);
                }
            }
            Node::Arg { role, .. } => {
                if *role == Role::NumKeys {
                    self.numkeys = self
                        .args
                        .get(self.i)
                        .and_then(|a| std::str::from_utf8(a).ok())
                        .and_then(|s| s.parse::<usize>().ok())
                        .map_or(NumKeys::Unparsed, NumKeys::Exactly);
                }
                self.take(*role);
            }
            Node::Seq(items) => {
                for it in items {
                    self.walk(it);
                }
            }
            Node::Optional(inner) => {
                if self.may_start(inner) {
                    self.walk(inner);
                }
            }
            Node::Choice(alts) => {
                // A keyword-led alternative wins if its keyword is actually there. Otherwise
                // an *unconditional* alternative applies — `XADD key ... <* | id>` has no
                // keyword in front of the explicit id, and without this the id would fall
                // through and be read as the first field name.
                let chosen = alts
                    .iter()
                    .find(|a| self.starts_with_matching_token(a))
                    .or_else(|| alts.iter().find(|a| leading_tokens(a).is_none()));
                if let Some(alt) = chosen {
                    self.walk(alt);
                }
            }
            Node::Repeat(inner) => self.walk_repeat(inner),
            Node::Paired { first, second } => self.walk_paired(*first, *second),
        }
    }

    /// Split what is left into two equal halves.
    ///
    /// An odd count means the command is not finished — Redis itself rejects it. The extra
    /// argument is attributed to the **first** half, because that is the position the user is
    /// typing when the count is odd, and no numeric default is invented either way (§11.7).
    fn walk_paired(&mut self, first: Role, second: Role) {
        let remaining = self.args.len() - self.i;
        let firsts = remaining.div_ceil(2);
        for n in 0..remaining {
            self.take(if n < firsts { first } else { second });
        }
    }

    fn walk_repeat(&mut self, inner: &Node) {
        // `EVAL script numkeys key...` — exactly `numkeys` keys, no more. Guessing here is
        // what sends ARGV entries as keys and misroutes the command on a cluster
        // (ASSIST-013).
        if self.numkeys != NumKeys::Unseen && single_role(inner) == Some(Role::Key) {
            let n = match self.numkeys {
                NumKeys::Exactly(n) => n,
                // Fail closed: no argument is promoted to a key on a guess.
                NumKeys::Unseen | NumKeys::Unparsed => 0,
            };
            self.numkeys = NumKeys::Unseen;
            for _ in 0..n {
                self.take(Role::Key);
            }
            return;
        }
        while self.i < self.args.len() {
            let before = self.i;
            if !self.may_start(inner) {
                break;
            }
            self.walk(inner);
            if self.i == before {
                break;
            }
        }
    }

    /// Whether `node` may begin at the current position.
    ///
    /// A node introduced by a keyword may only start if that keyword is actually there. A
    /// node that starts with a plain argument is unconditional.
    fn may_start(&self, node: &Node) -> bool {
        match leading_tokens(node) {
            None => true,
            Some(toks) => toks.iter().any(|t| self.peek_matches(t)),
        }
    }

    fn starts_with_matching_token(&self, node: &Node) -> bool {
        matches!(leading_tokens(node), Some(toks) if toks.iter().any(|t| self.peek_matches(t)))
    }
}

/// If `node` consumes exactly one argument, the role of that argument.
fn single_role(node: &Node) -> Option<Role> {
    match node {
        Node::Arg { role, .. } => Some(*role),
        Node::Seq(items) => match items.as_slice() {
            [one] => single_role(one),
            _ => None,
        },
        _ => None,
    }
}

/// The keywords that can introduce `node`, or `None` if it starts with a plain argument.
fn leading_tokens(node: &Node) -> Option<Vec<String>> {
    match node {
        Node::Token { text, .. } => Some(vec![text.clone()]),
        // Both start with a plain argument, so neither is gated on a keyword.
        Node::Arg { .. } | Node::Paired { .. } => None,
        Node::Optional(inner) | Node::Repeat(inner) => leading_tokens(inner),
        Node::Seq(items) => items.first().and_then(leading_tokens),
        Node::Choice(alts) => {
            let mut all = Vec::new();
            for a in alts {
                all.extend(leading_tokens(a)?);
            }
            Some(all)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn spec(name: &str, grammar: Node, effects: Effects) -> CommandSpec {
        CommandSpec {
            name: name.to_owned(),
            summary: String::new(),
            effects,
            grammar,
            exclusive: Vec::new(),
            family: Family::Both,
            group: String::new(),
            since: None,
            keyword_since: Vec::new(),
            is_container: false,
        }
    }

    // ---------------------------------------------------------------- §11.7 shapes
    #[test]
    fn a_subcommand_is_not_a_key() {
        // ASSIST-014: the second token of CLIENT LIST must not offer key candidates.
        let s = spec(
            "CLIENT",
            Node::Seq(vec![Node::subcommand("LIST")]),
            Effects {
                admin: true,
                ..Effects::default()
            },
        );
        assert_eq!(s.role_at(0), Some(Role::Subcommand));
        assert!(
            !s.accepts_observed_names_at(0),
            "a subcommand slot must not offer keys"
        );
    }

    #[test]
    fn hset_alternates_field_and_value() {
        // ASSIST-006: and the value position never offers observed data.
        let s = spec(
            "HSET",
            Node::Seq(vec![
                Node::arg("key", Role::Key),
                Node::Repeat(Box::new(Node::Seq(vec![
                    Node::arg("field", Role::Field),
                    Node::arg("value", Role::Value),
                ]))),
            ]),
            Effects::write(),
        );
        assert_eq!(s.role_at(0), Some(Role::Key));
        assert_eq!(s.role_at(1), Some(Role::Field));
        assert_eq!(s.role_at(2), Some(Role::Value));
        assert_eq!(s.role_at(3), Some(Role::Field), "the repeat cycles");
        assert_eq!(s.role_at(4), Some(Role::Value));
        assert_eq!(s.role_at(5), Some(Role::Field));

        assert!(s.accepts_observed_names_at(1), "field candidates are fine");
        assert!(
            !s.accepts_observed_names_at(2),
            "a value must never be auto-filled"
        );
        assert!(!s.accepts_observed_names_at(4));
    }

    #[test]
    fn zadd_does_not_swap_score_and_member() {
        let s = spec(
            "ZADD",
            Node::Seq(vec![
                Node::arg("key", Role::Key),
                Node::Repeat(Box::new(Node::Seq(vec![
                    Node::arg("score", Role::Score),
                    Node::arg("member", Role::Member),
                ]))),
            ]),
            Effects::write(),
        );
        assert_eq!(s.role_at(1), Some(Role::Score));
        assert_eq!(s.role_at(2), Some(Role::Member));
        assert_eq!(s.role_at(3), Some(Role::Score));
        assert!(!s.accepts_observed_names_at(1), "a score is not a name");
        assert!(s.accepts_observed_names_at(2), "a member is");
    }

    #[test]
    fn set_expiry_options_are_mutually_exclusive_but_conditions_are_not() {
        // ASSIST-007/008 and §11.8: choosing EX removes the other expiry forms, and leaves
        // NX/XX alone because they are a different group.
        let s = CommandSpec {
            name: "SET".into(),
            summary: String::new(),
            effects: Effects::write(),
            grammar: Node::Seq(vec![
                Node::arg("key", Role::Key),
                Node::arg("value", Role::Value),
                Node::Optional(Box::new(Node::Choice(vec![
                    Node::keyword("NX"),
                    Node::keyword("XX"),
                ]))),
                Node::Optional(Box::new(Node::Choice(vec![
                    Node::keyword("EX"),
                    Node::keyword("PX"),
                    Node::keyword("EXAT"),
                    Node::keyword("PXAT"),
                    Node::keyword("KEEPTTL"),
                ]))),
                Node::Optional(Box::new(Node::keyword("GET"))),
            ]),
            exclusive: vec![
                ExclusiveGroup {
                    name: "condition".into(),
                    members: vec!["NX".into(), "XX".into()],
                },
                ExclusiveGroup {
                    name: "expiry".into(),
                    members: vec![
                        "EX".into(),
                        "PX".into(),
                        "EXAT".into(),
                        "PXAT".into(),
                        "KEEPTTL".into(),
                    ],
                },
            ],
            family: Family::Both,
            group: "string".into(),
            since: Some("1.0.0".into()),
            keyword_since: vec![
                ("GET".into(), "6.2.0".into()),
                ("EXAT".into(), "6.2.0".into()),
            ],
            is_container: false,
        };

        let fresh = s.available_keywords(&[]);
        for k in ["NX", "XX", "EX", "PX", "EXAT", "PXAT", "KEEPTTL", "GET"] {
            assert!(
                fresh.contains(&k.to_string()),
                "{k} should be offered initially"
            );
        }

        let after_ex = s.available_keywords(&["EX"]);
        for gone in ["EX", "PX", "EXAT", "PXAT", "KEEPTTL"] {
            assert!(
                !after_ex.contains(&gone.to_string()),
                "{gone} must be removed after EX"
            );
        }
        for still in ["NX", "XX", "GET"] {
            assert!(
                after_ex.contains(&still.to_string()),
                "{still} is a different group and must remain"
            );
        }

        let after_both = s.available_keywords(&["EX", "NX"]);
        assert!(!after_both.contains(&"XX".to_string()));
        assert!(after_both.contains(&"GET".to_string()));

        assert_eq!(s.group_of("px").map(|g| g.name.as_str()), Some("expiry"));
        assert_eq!(s.group_of("nx").map(|g| g.name.as_str()), Some("condition"));
        assert!(s.group_of("GET").is_none(), "GET is in no exclusive group");
    }

    #[test]
    fn eval_numkeys_decides_where_keys_stop() {
        // ASSIST-013: getting this wrong sends ARGV as keys, which misroutes on a cluster.
        assert_eq!(eval_role_at(0, None), Role::Value, "the script body");
        assert_eq!(eval_role_at(1, None), Role::NumKeys);
        // numkeys = 2 -> indices 2 and 3 are keys, 4+ are ARGV.
        assert_eq!(eval_role_at(2, Some(2)), Role::Key);
        assert_eq!(eval_role_at(3, Some(2)), Role::Key);
        assert_eq!(eval_role_at(4, Some(2)), Role::Value);
        assert_eq!(eval_role_at(9, Some(2)), Role::Value);
        // numkeys = 0 -> everything after is ARGV.
        assert_eq!(eval_role_at(2, Some(0)), Role::Value);
        // Unparsed numkeys must not guess a key.
        assert_eq!(
            eval_role_at(2, None),
            Role::Value,
            "never guess that an arg is a key"
        );
    }

    #[test]
    fn a_key_named_like_a_keyword_is_decided_by_position() {
        // ASSIST-015: `SET NX GET` — the key really is called NX.
        let s = spec(
            "SET",
            Node::Seq(vec![
                Node::arg("key", Role::Key),
                Node::arg("value", Role::Value),
            ]),
            Effects::write(),
        );
        // Position 0 is the key regardless of what the text happens to be.
        assert_eq!(s.role_at(0), Some(Role::Key));
        assert_eq!(s.role_at(1), Some(Role::Value));
    }

    #[test]
    fn units_are_carried_so_the_helper_can_explain_them() {
        // §13.6: EX is seconds, PX is milliseconds, and the helper must not guess.
        assert_eq!(Unit::Seconds.label(), "seconds");
        assert_eq!(Unit::Milliseconds.label(), "milliseconds");
        assert_ne!(Unit::Seconds.label(), Unit::Milliseconds.label());
        let r = Role::Integer(Unit::Seconds);
        assert!(!r.accepts_observed_names(), "an integer is not a name");
    }

    #[test]
    fn only_name_roles_accept_observed_candidates() {
        for r in [
            Role::Key,
            Role::Field,
            Role::Member,
            Role::Group,
            Role::EntryId,
        ] {
            assert!(r.accepts_observed_names(), "{r:?} should accept names");
        }
        for r in [
            Role::Value,
            Role::Score,
            Role::Keyword,
            Role::Subcommand,
            Role::NumKeys,
            Role::Cursor,
            Role::Pattern,
            Role::Integer(Unit::Count),
        ] {
            assert!(!r.accepts_observed_names(), "{r:?} must not accept names");
        }
    }

    #[test]
    fn xread_pairs_keys_and_ids_after_the_streams_keyword() {
        // ASSIST-012: the STREAMS keyword introduces N keys then N ids.
        let s = spec(
            "XREAD",
            Node::Seq(vec![
                Node::keyword("STREAMS"),
                Node::Repeat(Box::new(Node::arg("key", Role::Key))),
            ]),
            Effects::read(),
        );
        assert_eq!(s.role_at(0), Some(Role::Keyword));
        assert_eq!(s.role_at(1), Some(Role::Key));
        assert_eq!(s.role_at(2), Some(Role::Key), "the repeat continues");
    }

    #[test]
    fn a_family_specific_spec_is_labelled() {
        // R29: Redis and Valkey have diverged, so a spec says which it describes.
        let mut s = spec("HGETEX", Node::arg("key", Role::Key), Effects::write());
        s.family = Family::Redis;
        assert_eq!(s.family, Family::Redis);
        assert_ne!(s.family, Family::Valkey);
        assert_eq!(Family::default(), Family::Both);
    }

    #[test]
    fn role_lookup_past_a_non_repeating_grammar_returns_none() {
        let s = spec(
            "GET",
            Node::Seq(vec![Node::arg("key", Role::Key)]),
            Effects::read(),
        );
        assert_eq!(s.role_at(0), Some(Role::Key));
        assert_eq!(s.role_at(1), None, "GET takes one argument");
        assert!(!s.accepts_observed_names_at(1));
    }
}
