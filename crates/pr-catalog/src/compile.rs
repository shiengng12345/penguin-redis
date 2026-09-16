//! Compile a pinned snapshot into `CommandSpec`s (v2.1 §11.4, §11.7, ADR-030, V-F01).
//!
//! The snapshot describes arguments as a tree (`key`, `oneof`, `block`, `pure-token`, …).
//! Compilation turns that into the [`Node`] grammar the analyser walks, and — separately —
//! into [`Effects`].
//!
//! Effects are the part that needs care, so they are derived in two stages:
//!
//! 1. the server's own command flags give the base classification (`write`, `readonly`,
//!    `admin`, `blocking`)
//! 2. [`OVERRIDES`] — a hand-maintained table in this file — is applied on top
//!
//! Stage 2 exists because the flags are not a policy vocabulary. `FLUSHALL` is flagged only
//! `write`, which is true and useless: it says nothing about the fact that it empties the
//! instance. `XREADGROUP` is flagged `write` but what matters is that it *advances a
//! consumer group*, which cannot be undone by writing the value back. The overrides are
//! deliberately in Rust rather than in the snapshot so that changing one shows up in a code
//! review of this crate, not in a 300 KB data diff.
//!
//! An override may only ever make a command **more** restricted. That is enforced by
//! [`apply_override`] and asserted in the tests, so the table cannot become a way to quietly
//! downgrade something to a read.

use crate::snapshot::{RawArg, RawCommand, Snapshot};
use crate::spec::{CommandSpec, ExclusiveGroup, Family, Node, Role, Unit};
use pr_core::Effects;

/// Extra effects a command carries that its server flags do not express.
///
/// Fields are *additive*: `true` sets the bit, `false` leaves whatever the flags decided.
// Six independent bits, each naming a distinct property of a command. They are not states of
// one enum — a command can be admin *and* destructive *and* blocking — so a bitflag-style
// struct is the honest shape.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Override {
    /// Empties or resets state at database or instance scope.
    pub destructive: bool,
    /// Advances state that cannot be restored by writing the old value back.
    pub consumes: bool,
    /// Administrative even though the flags do not say so.
    pub admin: bool,
    /// Blocks the connection even though the flags do not say so.
    pub blocks: bool,
    /// Reads server or data state. Only meaningful together with [`Override::classified`].
    pub reads_data: bool,
    /// Has an effect other clients can observe, even though no key changes — `PUBLISH`.
    pub writes_data: bool,
    /// A human has reviewed this command and confirms the classification is complete.
    ///
    /// This is the **only** way `unknown` is ever cleared, and it is a separate field rather
    /// than an implied consequence of the other four so that relaxing a command is a visible,
    /// deliberate edit in a code review (§20.2, ADR-030).
    pub classified: bool,
}

// Writing an entry in `OVERRIDES` *is* the review, so each of these clears `unknown`. The
// field stays separate so that the intent is legible at the point of use.
const DESTRUCTIVE: Override = Override {
    destructive: true,
    classified: true,
    ..NONE
};
const DESTRUCTIVE_ADMIN: Override = Override {
    destructive: true,
    admin: true,
    classified: true,
    ..NONE
};
const ADMIN: Override = Override {
    admin: true,
    classified: true,
    ..NONE
};
const CONSUMES: Override = Override {
    consumes: true,
    classified: true,
    ..NONE
};
const NONE: Override = Override {
    destructive: false,
    consumes: false,
    admin: false,
    blocks: false,
    reads_data: false,
    writes_data: false,
    classified: false,
};
/// Reviewed and harmless: reports state, changes nothing.
const DIAGNOSTIC: Override = Override {
    reads_data: true,
    classified: true,
    ..NONE
};
/// Reviewed: administrative, and the flags did not say so.
const ADMIN_CLASSIFIED: Override = Override {
    admin: true,
    classified: true,
    ..NONE
};
/// Reviewed: affects this connection only — protocol, DB selection, subscription mode,
/// transaction state. No keyspace effect, and nothing another client can see.
const SESSION: Override = Override {
    classified: true,
    ..NONE
};
/// Reviewed: no key changes, but other clients observe it (`PUBLISH`, `SPUBLISH`).
const PUBLISHES: Override = Override {
    writes_data: true,
    classified: true,
    ..NONE
};
/// Reviewed: blocks the connection without touching the keyspace (`WAIT`).
const BLOCKING: Override = Override {
    blocks: true,
    classified: true,
    ..NONE
};

/// The human authority layer over server flags.
///
/// Every entry here is a case where the flags are accurate but insufficient for a decision a
/// user is about to be asked to confirm (§20, ADR-030).
pub const OVERRIDES: &[(&str, Override)] = &[
    // Emptying data. `write` is true but hopelessly under-states it.
    ("FLUSHALL", DESTRUCTIVE),
    ("FLUSHDB", DESTRUCTIVE),
    ("SWAPDB", DESTRUCTIVE_ADMIN),
    ("SCRIPT|FLUSH", DESTRUCTIVE_ADMIN),
    ("FUNCTION|FLUSH", DESTRUCTIVE_ADMIN),
    ("CLUSTER|RESET", DESTRUCTIVE_ADMIN),
    ("CLUSTER|FLUSHSLOTS", DESTRUCTIVE_ADMIN),
    ("CLUSTER|FORGET", DESTRUCTIVE_ADMIN),
    ("CLUSTER|DELSLOTS", DESTRUCTIVE_ADMIN),
    ("CLUSTER|DELSLOTSRANGE", DESTRUCTIVE_ADMIN),
    ("CLUSTER|SETSLOT", DESTRUCTIVE_ADMIN),
    ("CLUSTER|FAILOVER", DESTRUCTIVE_ADMIN),
    ("ACL|DELUSER", DESTRUCTIVE_ADMIN),
    ("ACL|LOAD", ADMIN),
    ("ACL|SETUSER", ADMIN),
    // Topology and lifecycle. Not a data write at all, which is exactly why `write` misses
    // them; several are flagged `admin` already and are listed for completeness of intent.
    ("SHUTDOWN", DESTRUCTIVE_ADMIN),
    ("FAILOVER", DESTRUCTIVE_ADMIN),
    ("REPLICAOF", DESTRUCTIVE_ADMIN),
    ("SLAVEOF", DESTRUCTIVE_ADMIN),
    ("DEBUG", DESTRUCTIVE_ADMIN),
    ("MIGRATE", ADMIN),
    ("CLIENT|KILL", ADMIN),
    ("CLIENT|PAUSE", ADMIN),
    ("CLIENT|UNPAUSE", ADMIN),
    ("CONFIG|SET", ADMIN),
    ("CONFIG|REWRITE", ADMIN),
    ("CONFIG|RESETSTAT", ADMIN),
    ("BGREWRITEAOF", ADMIN),
    ("BGSAVE", ADMIN),
    ("SAVE", ADMIN),
    ("LASTSAVE", DIAGNOSTIC),
    // Reviewed diagnostics whose containers report no flags at all. Without an entry here
    // they stay `unknown`, which is the correct default but is unhelpful for a plain read.
    ("MEMORY|DOCTOR", DIAGNOSTIC),
    ("MEMORY|STATS", DIAGNOSTIC),
    ("MEMORY|MALLOC-STATS", DIAGNOSTIC),
    ("MEMORY|USAGE", DIAGNOSTIC),
    ("MEMORY|PURGE", ADMIN_CLASSIFIED),
    ("LATENCY|DOCTOR", DIAGNOSTIC),
    ("LATENCY|HISTORY", DIAGNOSTIC),
    ("LATENCY|LATEST", DIAGNOSTIC),
    ("LATENCY|GRAPH", DIAGNOSTIC),
    ("LATENCY|RESET", ADMIN_CLASSIFIED),
    ("SLOWLOG|GET", DIAGNOSTIC),
    ("SLOWLOG|LEN", DIAGNOSTIC),
    ("SLOWLOG|HELP", DIAGNOSTIC),
    ("SLOWLOG|RESET", ADMIN_CLASSIFIED),
    ("OBJECT|ENCODING", DIAGNOSTIC),
    ("OBJECT|FREQ", DIAGNOSTIC),
    ("OBJECT|IDLETIME", DIAGNOSTIC),
    ("OBJECT|REFCOUNT", DIAGNOSTIC),
    ("OBJECT|HELP", DIAGNOSTIC),
    ("PUBSUB|CHANNELS", DIAGNOSTIC),
    ("PUBSUB|NUMSUB", DIAGNOSTIC),
    ("PUBSUB|NUMPAT", DIAGNOSTIC),
    ("PUBSUB|SHARDCHANNELS", DIAGNOSTIC),
    ("PUBSUB|SHARDNUMSUB", DIAGNOSTIC),
    ("PUBSUB|HELP", DIAGNOSTIC),
    ("MODULE|LIST", DIAGNOSTIC),
    ("MODULE|HELP", DIAGNOSTIC),
    ("MODULE|LOAD", ADMIN_CLASSIFIED),
    ("MODULE|LOADEX", ADMIN_CLASSIFIED),
    ("MODULE|UNLOAD", ADMIN_CLASSIFIED),
    ("MONITOR", ADMIN),
    // Consuming: the reply is the only copy of what was consumed (§12.2, §20.4).
    ("XREADGROUP", CONSUMES),
    ("XCLAIM", CONSUMES),
    ("XAUTOCLAIM", CONSUMES),
    ("XACK", CONSUMES),
    ("XGROUP|SETID", CONSUMES),
    ("XGROUP|DESTROY", CONSUMES),
    ("SPOP", CONSUMES),
    ("LPOP", CONSUMES),
    ("RPOP", CONSUMES),
    ("LMPOP", CONSUMES),
    ("ZMPOP", CONSUMES),
    ("BLPOP", CONSUMES),
    ("BRPOP", CONSUMES),
    ("BLMPOP", CONSUMES),
    ("BZMPOP", CONSUMES),
    ("BZPOPMIN", CONSUMES),
    ("BZPOPMAX", CONSUMES),
    ("ZPOPMIN", CONSUMES),
    ("ZPOPMAX", CONSUMES),
    ("GETDEL", CONSUMES),
    ("HGETDEL", CONSUMES),
    // ---------------------------------------------------------------- connection scope
    // These carry no server flags because they affect only this connection. Left alone they
    // stay `unknown`, which would put an approval prompt in front of `PING`.
    ("ASKING", SESSION),
    ("AUTH", SESSION),
    ("CLIENT|CACHING", SESSION),
    ("CLIENT|CAPA", SESSION),
    ("CLIENT|NO-TOUCH", SESSION),
    ("CLIENT|REPLY", SESSION),
    ("CLIENT|SETINFO", SESSION),
    ("CLIENT|SETNAME", SESSION),
    ("CLIENT|TRACKING", SESSION),
    ("DISCARD", SESSION),
    ("ECHO", SESSION),
    ("HELLO", SESSION),
    ("MULTI", SESSION),
    ("PING", SESSION),
    ("PSUBSCRIBE", SESSION),
    ("PUNSUBSCRIBE", SESSION),
    ("QUIT", SESSION),
    ("READONLY", SESSION),
    ("READWRITE", SESSION),
    ("RESET", SESSION),
    ("SELECT", SESSION),
    ("SSUBSCRIBE", SESSION),
    ("SUBSCRIBE", SESSION),
    ("SUNSUBSCRIBE", SESSION),
    ("UNSUBSCRIBE", SESSION),
    ("UNWATCH", SESSION),
    ("WATCH", SESSION),
    // Observable by other clients without changing a key.
    ("PUBLISH", PUBLISHES),
    ("SPUBLISH", PUBLISHES),
    // Waits on replication; no keyspace effect, but it does block.
    ("WAIT", BLOCKING),
    ("WAITAOF", BLOCKING),
    // ---------------------------------------------------------------- introspection
    ("ACL|CAT", DIAGNOSTIC),
    ("ACL|GENPASS", DIAGNOSTIC),
    ("ACL|HELP", DIAGNOSTIC),
    ("ACL|WHOAMI", DIAGNOSTIC),
    ("CLIENT|GETNAME", DIAGNOSTIC),
    ("CLIENT|GETREDIR", DIAGNOSTIC),
    ("CLIENT|HELP", DIAGNOSTIC),
    ("CLIENT|ID", DIAGNOSTIC),
    ("CLIENT|INFO", DIAGNOSTIC),
    ("CLIENT|TRACKINGINFO", DIAGNOSTIC),
    ("CLUSTER|COUNTKEYSINSLOT", DIAGNOSTIC),
    // Returns key names, so it reads data even though it is a CLUSTER subcommand.
    ("CLUSTER|GETKEYSINSLOT", DIAGNOSTIC),
    ("CLUSTER|HELP", DIAGNOSTIC),
    ("CLUSTER|INFO", DIAGNOSTIC),
    ("CLUSTER|KEYSLOT", DIAGNOSTIC),
    ("CLUSTER|LINKS", DIAGNOSTIC),
    ("CLUSTER|MYID", DIAGNOSTIC),
    ("CLUSTER|MYSHARDID", DIAGNOSTIC),
    ("CLUSTER|NODES", DIAGNOSTIC),
    ("CLUSTER|SHARDS", DIAGNOSTIC),
    ("CLUSTER|SLOTS", DIAGNOSTIC),
    ("CLUSTER|SLOT-STATS", DIAGNOSTIC),
    ("COMMAND", DIAGNOSTIC),
    ("COMMAND|COUNT", DIAGNOSTIC),
    ("COMMAND|DOCS", DIAGNOSTIC),
    ("COMMAND|GETKEYS", DIAGNOSTIC),
    ("COMMAND|GETKEYSANDFLAGS", DIAGNOSTIC),
    ("COMMAND|HELP", DIAGNOSTIC),
    ("COMMAND|INFO", DIAGNOSTIC),
    ("COMMAND|LIST", DIAGNOSTIC),
    ("COMMANDLOG|HELP", DIAGNOSTIC),
    ("CONFIG|HELP", DIAGNOSTIC),
    ("FUNCTION|DUMP", DIAGNOSTIC),
    ("FUNCTION|HELP", DIAGNOSTIC),
    ("FUNCTION|LIST", DIAGNOSTIC),
    ("FUNCTION|STATS", DIAGNOSTIC),
    ("INFO", DIAGNOSTIC),
    ("LATENCY|HELP", DIAGNOSTIC),
    ("MEMORY|HELP", DIAGNOSTIC),
    ("ROLE", DIAGNOSTIC),
    ("SCRIPT|EXISTS", DIAGNOSTIC),
    ("SCRIPT|HELP", DIAGNOSTIC),
    ("SCRIPT|SHOW", DIAGNOSTIC),
    ("TIME", DIAGNOSTIC),
    ("XGROUP|HELP", DIAGNOSTIC),
    ("XINFO|HELP", DIAGNOSTIC),
    // ---------------------------------------------------------------- server management
    ("CLIENT|IMPORT-SOURCE", ADMIN_CLASSIFIED),
    ("FUNCTION|KILL", ADMIN_CLASSIFIED),
    ("SCRIPT|DEBUG", ADMIN_CLASSIFIED),
    ("SCRIPT|KILL", ADMIN_CLASSIFIED),
    ("SCRIPT|LOAD", ADMIN_CLASSIFIED),
];

/// Commands left `unknown` on purpose (§20.2).
///
/// These are not gaps in [`OVERRIDES`]. Each one's effect is *whatever it is asked to do*,
/// and no static classification can be honest about that:
///
/// - `EVAL` / `EVALSHA` / `FCALL` run arbitrary server-side code. Redis itself declines to
///   flag them `readonly` or `write` for exactly this reason; the `_RO` variants are flagged
///   `readonly` and classify normally.
/// - `EXEC` executes whatever `MULTI` queued, so its risk is the union of the queued
///   commands and has to be computed at approval time, not looked up here.
///
/// Leaving them `unknown` makes them max-risk, which is the fail-closed answer: the user is
/// asked, and the approval can show what is actually being run.
pub const DELIBERATELY_UNKNOWN: &[&str] = &["EVAL", "EVALSHA", "EXEC", "FCALL"];

/// Effects implied by the server's own command flags.
///
/// Flags that classify nothing leave `unknown` set. That is the fail-closed direction §20.2
/// asks for: a command nobody has classified is treated as maximum risk rather than as
/// harmless, and the only way out is a reviewed [`OVERRIDES`] entry marked `classified`.
fn effects_from_flags(cmd: &RawCommand) -> Effects {
    let has = |f: &str| cmd.flags.iter().any(|x| x == f);
    let (read, write, admin, blocks) =
        (has("readonly"), has("write"), has("admin"), has("blocking"));
    Effects {
        reads_data: read,
        writes_data: write,
        consumes: false,
        blocks,
        admin,
        destructive: false,
        unknown: !(read || write || admin),
    }
}

/// Apply an override, which may only ever add restriction.
fn apply_override(base: Effects, ov: Override) -> Effects {
    Effects {
        reads_data: base.reads_data || ov.reads_data,
        writes_data: base.writes_data || ov.writes_data,
        consumes: base.consumes || ov.consumes,
        blocks: base.blocks || ov.blocks,
        admin: base.admin || ov.admin,
        destructive: base.destructive || ov.destructive,
        // Restriction is never removed; `unknown` is the single exception, and only when the
        // entry says a human classified it.
        unknown: base.unknown && !ov.classified,
    }
}

fn override_for(name: &str) -> Override {
    OVERRIDES
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(NONE, |(_, o)| *o)
}

/// Role for a leaf argument, from its declared type and its name.
///
/// The type alone is not enough: a hash field and a value are both `string`, and offering
/// observed names in the value slot is the leak §12.2 forbids. The name is what distinguishes
/// them, and it comes from the same reviewed snapshot as everything else.
fn leaf_role(arg: &RawArg, group: &str) -> Role {
    let n = arg.name.to_ascii_lowercase();
    match arg.kind.as_str() {
        "key" => return Role::Key,
        "pattern" => return Role::Pattern,
        "unix-time" => {
            return Role::Integer(if n.contains("milliseconds") {
                Unit::UnixMilliseconds
            } else {
                Unit::UnixSeconds
            });
        }
        _ => {}
    }
    match n.as_str() {
        "numkeys" => Role::NumKeys,
        "field" => Role::Field,
        "member" => Role::Member,
        "group" | "groupname" => Role::Group,
        "channel" | "shardchannel" => Role::Channel,
        "consumer" => Role::Consumer,
        "cursor" => Role::Cursor,
        "score" if arg.kind == "double" => Role::Score,
        // `id` means an entry id inside a stream command and a numeric client id elsewhere,
        // so the command's own group decides.
        "id" | "entry-id" | "start-id" | "end-id" if group == "stream" => Role::EntryId,
        // `timeout` is seconds everywhere it appears (BLPOP and friends), so it shares the
        // arm rather than being spelled out again.
        "seconds" | "timeout" => Role::Integer(Unit::Seconds),
        "milliseconds" => Role::Integer(Unit::Milliseconds),
        _ => match arg.kind.as_str() {
            "integer" => Role::Integer(Unit::Count),
            _ => Role::Value,
        },
    }
}

/// Compile one argument node.
fn node_from(arg: &RawArg, group: &str) -> Node {
    let inner = match arg.kind.as_str() {
        "pure-token" => {
            let text = arg
                .token
                .clone()
                .unwrap_or_else(|| arg.name.to_ascii_uppercase());
            return wrap(arg, Node::keyword(&text), true);
        }
        "oneof" => Node::Choice(
            arg.arguments
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|a| node_from(a, group))
                .collect(),
        ),
        "block" => {
            let kids = arg.arguments.as_deref().unwrap_or_default();
            paired(kids, group)
                .unwrap_or_else(|| Node::Seq(kids.iter().map(|a| node_from(a, group)).collect()))
        }
        _ => Node::arg(&arg.name, leaf_role(arg, group)),
    };
    wrap(arg, inner, false)
}

/// Recognise the `key... id...` shape: two adjacent repeating leaves, neither introduced by a
/// keyword, whose halves must be the same length (§11.7).
fn paired(kids: &[RawArg], group: &str) -> Option<Node> {
    let [a, b] = kids else { return None };
    let plain =
        |x: &RawArg| x.multiple && x.token.is_none() && !x.optional && x.arguments.is_none();
    if plain(a) && plain(b) {
        Some(Node::Paired {
            first: leaf_role(a, group),
            second: leaf_role(b, group),
        })
    } else {
        None
    }
}

/// Apply `token`, `multiple` and `optional` around a compiled node, in the order the snapshot
/// means them.
fn wrap(arg: &RawArg, inner: Node, is_pure_token: bool) -> Node {
    let mut n = inner;
    // A token in front of a non-pure-token argument introduces it: `EX seconds`.
    if !is_pure_token && let Some(t) = &arg.token {
        {
            if arg.multiple && !arg.multiple_token {
                // `ID 1 2 3` — the keyword appears once, the argument repeats.
                return finish_optional(
                    arg,
                    Node::Seq(vec![Node::keyword(t), Node::Repeat(Box::new(n))]),
                );
            }
            n = Node::Seq(vec![Node::keyword(t), n]);
        }
    }
    if arg.multiple {
        n = Node::Repeat(Box::new(n));
    }
    finish_optional(arg, n)
}

fn finish_optional(arg: &RawArg, n: Node) -> Node {
    if arg.optional {
        Node::Optional(Box::new(n))
    } else {
        n
    }
}

/// Collect the mutually exclusive option groups from a `oneof` of keywords.
fn collect_groups(args: &[RawArg], out: &mut Vec<ExclusiveGroup>) {
    for a in args {
        if a.kind == "oneof" {
            let kids = a.arguments.as_deref().unwrap_or_default();
            let members: Vec<String> = kids
                .iter()
                .filter_map(|k| {
                    k.token
                        .clone()
                        .or_else(|| (k.kind == "pure-token").then(|| k.name.to_ascii_uppercase()))
                })
                .collect();
            if members.len() == kids.len() && members.len() > 1 {
                out.push(ExclusiveGroup {
                    name: a.name.clone(),
                    members,
                });
            }
        }
        if let Some(kids) = &a.arguments {
            collect_groups(kids, out);
        }
    }
}

/// Record the first version that accepted each optional keyword.
///
/// A child inherits its parent's `since` when it has none of its own: in `SET`, the
/// `condition` group carries 2.6.12 while `NX` and `XX` carry nothing.
fn collect_since(args: &[RawArg], parent: Option<&str>, out: &mut Vec<(String, String)>) {
    for a in args {
        let since = a.since.as_deref().or(parent);
        if let Some(tok) = a
            .token
            .clone()
            .or_else(|| (a.kind == "pure-token").then(|| a.name.to_ascii_uppercase()))
            && let Some(v) = since
        {
            out.push((tok, v.to_owned()));
        }
        if let Some(kids) = &a.arguments {
            collect_since(kids, since, out);
        }
    }
}

/// Compile a single command.
#[must_use]
pub fn compile_command(cmd: &RawCommand, family: Family) -> CommandSpec {
    let group = cmd.group.clone().unwrap_or_default();
    let args = cmd.arguments.as_deref().unwrap_or_default();
    let grammar = Node::Seq(args.iter().map(|a| node_from(a, &group)).collect());
    let mut exclusive = Vec::new();
    collect_groups(args, &mut exclusive);
    let mut keyword_since = Vec::new();
    collect_since(args, None, &mut keyword_since);
    keyword_since.sort();
    keyword_since.dedup();

    // A bare container (`CLIENT`) reports no flags of its own. Giving it `Effects::default()`
    // would classify it as harmless; it is simply not a command, and `is_container` says so.
    let is_container = cmd.container.is_none()
        && cmd.flags.is_empty()
        && cmd.arguments.is_none()
        && cmd.arity.is_some_and(|a| a == -2);

    let effects = if is_container {
        Effects::unknown()
    } else {
        apply_override(effects_from_flags(cmd), override_for(&cmd.name))
    };

    CommandSpec {
        name: cmd.name.clone(),
        summary: cmd.summary.clone().unwrap_or_default(),
        effects,
        grammar,
        exclusive,
        family,
        group,
        since: cmd.since.clone(),
        keyword_since,
        is_container,
    }
}

/// Compile every command in a snapshot.
#[must_use]
pub fn compile(snapshot: &Snapshot, family: Family) -> Vec<CommandSpec> {
    snapshot
        .commands
        .iter()
        .map(|c| compile_command(c, family))
        .collect()
}

/// The result of merging the Redis and Valkey branches (R29).
#[derive(Clone, Debug)]
pub struct Merged {
    /// Every command, with [`Family`] recording which servers have it.
    pub commands: Vec<CommandSpec>,
    /// Names only Redis has.
    pub redis_only: Vec<String>,
    /// Names only Valkey has.
    pub valkey_only: Vec<String>,
    /// Names present in both but whose effects disagree. An empty list is the expectation;
    /// a non-empty one is a review item, not something to average away.
    pub effect_conflicts: Vec<String>,
}

/// Merge two compiled branches into one catalog.
///
/// Where a command exists in both and the effects agree, it becomes [`Family::Both`]. Where
/// they disagree, the **more restrictive** classification wins and the name is reported, so
/// the divergence is visible rather than silently resolved.
#[must_use]
pub fn merge(redis: Vec<CommandSpec>, valkey: Vec<CommandSpec>) -> Merged {
    let mut out: Vec<CommandSpec> = Vec::new();
    let mut redis_only = Vec::new();
    let mut valkey_only = Vec::new();
    let mut conflicts = Vec::new();

    let mut v_by_name: std::collections::BTreeMap<String, CommandSpec> =
        valkey.into_iter().map(|c| (c.name.clone(), c)).collect();

    for mut r in redis {
        if let Some(v) = v_by_name.remove(&r.name) {
            if r.effects != v.effects {
                conflicts.push(r.name.clone());
                r.effects = more_restrictive(r.effects, v.effects);
            }
            r.family = Family::Both;
        } else {
            redis_only.push(r.name.clone());
            r.family = Family::Redis;
        }
        out.push(r);
    }
    for (name, mut v) in v_by_name {
        valkey_only.push(name);
        v.family = Family::Valkey;
        out.push(v);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Merged {
        commands: out,
        redis_only,
        valkey_only,
        effect_conflicts: conflicts,
    }
}

/// The union of two classifications — restriction never cancels out.
fn more_restrictive(a: Effects, b: Effects) -> Effects {
    Effects {
        reads_data: a.reads_data || b.reads_data,
        writes_data: a.writes_data || b.writes_data,
        consumes: a.consumes || b.consumes,
        blocks: a.blocks || b.blocks,
        admin: a.admin || b.admin,
        destructive: a.destructive || b.destructive,
        unknown: a.unknown || b.unknown,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn raw(name: &str, flags: &[&str]) -> RawCommand {
        RawCommand {
            name: name.to_owned(),
            flags: flags.iter().map(|s| (*s).to_owned()).collect(),
            ..RawCommand::default()
        }
    }

    #[test]
    fn flags_map_onto_effects() {
        assert!(effects_from_flags(&raw("GET", &["readonly", "fast"])).reads_data);
        assert!(effects_from_flags(&raw("SET", &["write", "denyoom"])).writes_data);
        assert!(effects_from_flags(&raw("BLPOP", &["write", "blocking"])).blocks);
        assert!(effects_from_flags(&raw("CLIENT|KILL", &["admin"])).admin);
    }

    #[test]
    fn an_override_can_only_add_restriction() {
        // The table must not become a way to downgrade something to a read.
        let base = Effects {
            writes_data: true,
            ..Effects::default()
        };
        for (name, ov) in OVERRIDES {
            let after = apply_override(base, *ov);
            assert!(after.writes_data, "{name}: an override cleared writes_data");
            assert!(
                after.mutates() >= base.mutates(),
                "{name}: an override reduced the risk classification"
            );
        }
        // `unknown` may only be cleared by an entry that says so explicitly.
        let u = Effects::unknown();
        for (name, ov) in OVERRIDES {
            assert_eq!(
                apply_override(u, *ov).unknown,
                !ov.classified,
                "{name}: unknown was cleared without `classified`"
            );
        }
        // Every restriction already set survives every entry in the table.
        let strict = Effects {
            reads_data: true,
            writes_data: true,
            consumes: true,
            blocks: true,
            admin: true,
            destructive: true,
            unknown: false,
        };
        for (name, ov) in OVERRIDES {
            assert_eq!(apply_override(strict, *ov), strict, "{name} weakened a bit");
        }
    }

    #[test]
    fn flushall_is_destructive_even_though_the_server_only_says_write() {
        let c = raw("FLUSHALL", &["write"]);
        let s = compile_command(&c, Family::Both);
        assert!(
            s.effects.destructive,
            "FLUSHALL must not be classified as an ordinary write"
        );
        assert!(s.effects.mutates());
    }

    #[test]
    fn xreadgroup_is_marked_consuming() {
        let s = compile_command(&raw("XREADGROUP", &["write", "blocking"]), Family::Both);
        assert!(s.effects.consumes, "the group cursor advances");
        assert!(s.effects.blocks);
    }

    #[test]
    fn a_bare_container_is_not_classified_as_harmless() {
        let mut c = raw("CLIENT", &[]);
        c.arity = Some(-2);
        let s = compile_command(&c, Family::Both);
        assert!(s.is_container);
        assert!(
            s.effects.unknown,
            "a container must not report an effect set of its own"
        );
        assert!(
            s.effects.mutates(),
            "unknown is treated as maximum risk (§20.2)"
        );
    }

    #[test]
    fn a_leaf_role_comes_from_the_name_not_only_the_type() {
        let string = |n: &str| RawArg {
            name: n.into(),
            kind: "string".into(),
            ..RawArg::default()
        };
        assert_eq!(leaf_role(&string("field"), "hash"), Role::Field);
        assert_eq!(leaf_role(&string("member"), "set"), Role::Member);
        assert_eq!(leaf_role(&string("value"), "string"), Role::Value);
        assert_eq!(leaf_role(&string("group"), "stream"), Role::Group);
        assert_eq!(leaf_role(&string("channel"), "pubsub"), Role::Channel);
        // `id` is an entry id in a stream command and a plain number elsewhere.
        assert_eq!(leaf_role(&string("id"), "stream"), Role::EntryId);
        assert_ne!(leaf_role(&string("id"), "connection"), Role::EntryId);
    }

    #[test]
    fn units_survive_compilation() {
        let a = |n: &str, k: &str| RawArg {
            name: n.into(),
            kind: k.into(),
            ..RawArg::default()
        };
        assert_eq!(
            leaf_role(&a("seconds", "integer"), ""),
            Role::Integer(Unit::Seconds)
        );
        assert_eq!(
            leaf_role(&a("milliseconds", "integer"), ""),
            Role::Integer(Unit::Milliseconds)
        );
        assert_eq!(
            leaf_role(&a("unix-time-seconds", "unix-time"), ""),
            Role::Integer(Unit::UnixSeconds)
        );
        assert_eq!(
            leaf_role(&a("unix-time-milliseconds", "unix-time"), ""),
            Role::Integer(Unit::UnixMilliseconds)
        );
    }

    #[test]
    fn a_merge_conflict_keeps_the_more_restrictive_side_and_reports_it() {
        let mk = |eff: Effects| CommandSpec {
            name: "X".into(),
            summary: String::new(),
            effects: eff,
            grammar: Node::Seq(vec![]),
            exclusive: vec![],
            family: Family::Redis,
            group: String::new(),
            since: None,
            keyword_since: Vec::new(),
            is_container: false,
        };
        let m = merge(
            vec![mk(Effects::read())],
            vec![mk(Effects {
                reads_data: true,
                admin: true,
                ..Effects::default()
            })],
        );
        assert_eq!(m.effect_conflicts, vec!["X".to_string()]);
        assert!(
            m.commands[0].effects.admin,
            "the stricter classification must win"
        );
        assert_eq!(m.commands[0].family, Family::Both);
    }

    #[test]
    fn family_only_commands_are_labelled_and_listed() {
        let mk = |n: &str| CommandSpec {
            name: n.into(),
            summary: String::new(),
            effects: Effects::read(),
            grammar: Node::Seq(vec![]),
            exclusive: vec![],
            family: Family::Both,
            group: String::new(),
            since: None,
            keyword_since: Vec::new(),
            is_container: false,
        };
        let m = merge(
            vec![mk("HGETEX"), mk("GET")],
            vec![mk("GET"), mk("COMMANDLOG")],
        );
        assert_eq!(m.redis_only, vec!["HGETEX".to_string()]);
        assert_eq!(m.valkey_only, vec!["COMMANDLOG".to_string()]);
        let by = |n: &str| m.commands.iter().find(|c| c.name == n).unwrap().family;
        assert_eq!(by("HGETEX"), Family::Redis);
        assert_eq!(by("COMMANDLOG"), Family::Valkey);
        assert_eq!(by("GET"), Family::Both);
    }

    #[test]
    fn the_override_table_has_no_duplicate_entries() {
        let mut names: Vec<&str> = OVERRIDES.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "an override name is listed twice");
    }
}
