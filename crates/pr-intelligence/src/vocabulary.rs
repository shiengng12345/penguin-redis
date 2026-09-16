//! The purpose-search vocabulary and purpose table (v2.1 §16.4, §13, R10, V-F08).
//!
//! **Generated is the wrong word for this file.** Every phrase in
//! `fixtures/assistance/find/canonical/` was written by hand, one command at a time, as how a
//! person actually asks for that thing; the tables below were then written against that corpus.
//! What is mechanical is only the link between them: `find_recall.rs` asserts that **every term
//! here occurs in at least one canonical phrase**, which is what makes §16.4's sentence true —
//!
//! > 人工标注的规范表述集（每命令 ≥ 3 条中/英），**同时用于构建同义词表**，因此只证明词表完整
//!
//! — and is also why the canonical number proves so little on its own. The held-out corpus is
//! where the feature is actually measured.
//!
//! ## Why concepts rather than synonyms-per-command
//!
//! A command declares what it is *for*, once. A phrasing maps to concepts. Adding a Chinese
//! colloquialism for "delete" reaches every deleting command without touching any of them,
//! and a command added later inherits the whole vocabulary by naming its concepts.
//!
//! The first concept a command lists is what it is *for*; later ones are things it also does.
//! `EXPIRE` is for expiry and incidentally about keys, and the ranking uses that.

use crate::find::{Purpose, Term};

/// Every term the search understands, in Chinese and English.
pub static TERMS: &[Term] = &[
    Term {
        text: "at least one subscriber",
        concepts: &["subscribe", "introspect"],
    },
    Term {
        text: "internal representation",
        concepts: &["encoding", "introspect"],
    },
    Term {
        text: "replica acknowledgement",
        concepts: &["replication", "acknowledge"],
    },
    Term {
        text: "another redis instance",
        concepts: &["migrate"],
    },
    Term {
        text: "since anything touched",
        concepts: &["idle"],
    },
    Term {
        text: "together or not at all",
        concepts: &["transaction", "atomic"],
    },
    Term {
        text: "someone else touched",
        concepts: &["watch", "optimistic-lock"],
    },
    Term {
        text: "anyone else touched",
        concepts: &["watch", "optimistic-lock"],
    },
    Term {
        text: "different database",
        concepts: &["database", "move"],
    },
    Term {
        text: "what kind of thing",
        concepts: &["type-of"],
    },
    Term {
        text: "begin transaction",
        concepts: &["transaction"],
    },
    Term {
        text: "compact bit array",
        concepts: &["bitmap"],
    },
    Term {
        text: "internal encoding",
        concepts: &["encoding", "introspect"],
    },
    Term {
        text: "numbered database",
        concepts: &["database"],
    },
    Term {
        text: "particular moment",
        concepts: &["timestamp", "expire"],
    },
    Term {
        text: "a page at a time",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "access frequency",
        concepts: &["frequency"],
    },
    Term {
        text: "different server",
        concepts: &["migrate"],
    },
    Term {
        text: "go ahead and run",
        concepts: &["commit"],
    },
    Term {
        text: "how many seconds",
        concepts: &["ttl", "seconds"],
    },
    Term {
        text: "into another key",
        concepts: &["store"],
    },
    Term {
        text: "part of a string",
        concepts: &["substring", "string"],
    },
    Term {
        text: "present in every",
        concepts: &["intersection"],
    },
    Term {
        text: "reached the file",
        concepts: &["persistence"],
    },
    Term {
        text: "subscriber count",
        concepts: &["subscribe", "count"],
    },
    Term {
        text: "too long to list",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "what is going on",
        concepts: &["introspect"],
    },
    Term {
        text: "without removing",
        concepts: &["read"],
    },
    Term {
        text: "without stalling",
        concepts: &["async"],
    },
    Term {
        text: "aof persistence",
        concepts: &["persistence"],
    },
    Term {
        text: "array of packed",
        concepts: &["bit"],
    },
    Term {
        text: "carry elsewhere",
        concepts: &["serialise", "migrate"],
    },
    Term {
        text: "going on inside",
        concepts: &["introspect"],
    },
    Term {
        text: "how much longer",
        concepts: &["ttl"],
    },
    Term {
        text: "packed integers",
        concepts: &["bit", "number"],
    },
    Term {
        text: "reference count",
        concepts: &["memory"],
    },
    Term {
        text: "replace a value",
        concepts: &["set-value", "atomic"],
    },
    Term {
        text: "start receiving",
        concepts: &["subscribe"],
    },
    Term {
        text: "too big to read",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "unique visitors",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "without loading",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "without writing",
        concepts: &["read-only"],
    },
    Term {
        text: "across several",
        concepts: &["union", "multiple"],
    },
    Term {
        text: "add with score",
        concepts: &["add-member", "score"],
    },
    Term {
        text: "as a timestamp",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "at what moment",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "coordinates of",
        concepts: &["location"],
    },
    Term {
        text: "different ones",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "expiry instant",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "how long since",
        concepts: &["idle"],
    },
    Term {
        text: "in one request",
        concepts: &["multiple"],
    },
    Term {
        text: "keeping an eye",
        concepts: &["watch"],
    },
    Term {
        text: "precise moment",
        concepts: &["timestamp", "expire"],
    },
    Term {
        text: "push a message",
        concepts: &["publish"],
    },
    Term {
        text: "return the old",
        concepts: &["read-value", "atomic"],
    },
    Term {
        text: "stop receiving",
        concepts: &["unsubscribe"],
    },
    Term {
        text: "under the hood",
        concepts: &["introspect"],
    },
    Term {
        text: "which of these",
        concepts: &["exists", "multiple"],
    },
    Term {
        text: "add a decimal",
        concepts: &["increment", "float"],
    },
    Term {
        text: "between lists",
        concepts: &["move", "list"],
    },
    Term {
        text: "check whether",
        concepts: &["exists"],
    },
    Term {
        text: "everyone from",
        concepts: &["union", "read-all"],
    },
    Term {
        text: "exact instant",
        concepts: &["timestamp", "ttl"],
    },
    Term {
        text: "has something",
        concepts: &["multiple"],
    },
    Term {
        text: "highest first",
        concepts: &["reverse"],
    },
    Term {
        text: "introspection",
        concepts: &["introspect"],
    },
    Term {
        text: "lexicographic",
        concepts: &["lexicographic"],
    },
    Term {
        text: "recently used",
        concepts: &["touch", "idle"],
    },
    Term {
        text: "references to",
        concepts: &["memory"],
    },
    Term {
        text: "small numbers",
        concepts: &["bit", "number"],
    },
    Term {
        text: "stop expiring",
        concepts: &["persist"],
    },
    Term {
        text: "stop watching",
        concepts: &["watch", "cancel"],
    },
    Term {
        text: "take five off",
        concepts: &["decrement"],
    },
    Term {
        text: "what position",
        concepts: &["rank"],
    },
    Term {
        text: "where exactly",
        concepts: &["location"],
    },
    Term {
        text: "accumulating",
        concepts: &["increment"],
    },
    Term {
        text: "across lists",
        concepts: &["multiple", "list"],
    },
    Term {
        text: "across zsets",
        concepts: &["multiple", "sortedset"],
    },
    Term {
        text: "alphabetical",
        concepts: &["lexicographic"],
    },
    Term {
        text: "another node",
        concepts: &["migrate"],
    },
    Term {
        text: "combine them",
        concepts: &["union"],
    },
    Term {
        text: "first writer",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "hyperloglogs",
        concepts: &["hyperloglog", "cardinality"],
    },
    Term {
        text: "intersection",
        concepts: &["intersection"],
    },
    Term {
        text: "life is left",
        concepts: &["ttl"],
    },
    Term {
        text: "member score",
        concepts: &["score", "member"],
    },
    Term {
        text: "milliseconds",
        concepts: &["milliseconds"],
    },
    Term {
        text: "on what date",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "onto another",
        concepts: &["move"],
    },
    Term {
        text: "onto the end",
        concepts: &["append", "tail"],
    },
    Term {
        text: "page by page",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "previous one",
        concepts: &["read-value", "atomic"],
    },
    Term {
        text: "queued batch",
        concepts: &["transaction"],
    },
    Term {
        text: "range search",
        concepts: &["radius", "search"],
    },
    Term {
        text: "seconds left",
        concepts: &["ttl", "seconds"],
    },
    Term {
        text: "take one off",
        concepts: &["decrement"],
    },
    Term {
        text: "which moment",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "access time",
        concepts: &["touch", "idle"],
    },
    Term {
        text: "acknowledge",
        concepts: &["acknowledge"],
    },
    Term {
        text: "all succeed",
        concepts: &["transaction", "atomic"],
    },
    Term {
        text: "between two",
        concepts: &["move", "multiple"],
    },
    Term {
        text: "binary blob",
        concepts: &["serialise", "backup"],
    },
    Term {
        text: "cache entry",
        concepts: &["key", "expire"],
    },
    Term {
        text: "cardinality",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "common ones",
        concepts: &["intersection"],
    },
    Term {
        text: "coordinates",
        concepts: &["location"],
    },
    Term {
        text: "cursor scan",
        concepts: &["scan", "iterate"],
    },
    Term {
        text: "deserialise",
        concepts: &["deserialise"],
    },
    Term {
        text: "field names",
        concepts: &["list-fields"],
    },
    Term {
        text: "flag arrays",
        concepts: &["bitmap", "multiple"],
    },
    Term {
        text: "hyperloglog",
        concepts: &["hyperloglog", "cardinality"],
    },
    Term {
        text: "keep a copy",
        concepts: &["store"],
    },
    Term {
        text: "keep an eye",
        concepts: &["watch", "blocking"],
    },
    Term {
        text: "leaderboard",
        concepts: &["sortedset", "leaderboard"],
    },
    Term {
        text: "legacy code",
        concepts: &["deprecated"],
    },
    Term {
        text: "member rank",
        concepts: &["rank", "member"],
    },
    Term {
        text: "millisecond",
        concepts: &["milliseconds"],
    },
    Term {
        text: "most recent",
        concepts: &["range"],
    },
    Term {
        text: "not already",
        concepts: &["set-if-absent"],
    },
    Term {
        text: "plain value",
        concepts: &["string"],
    },
    Term {
        text: "random peek",
        concepts: &["random", "read-value"],
    },
    Term {
        text: "round trips",
        concepts: &["multiple"],
    },
    Term {
        text: "slot one in",
        concepts: &["insert"],
    },
    Term {
        text: "still empty",
        concepts: &["set-if-absent"],
    },
    Term {
        text: "stock count",
        concepts: &["counter"],
    },
    Term {
        text: "top hundred",
        concepts: &["range", "rank"],
    },
    Term {
        text: "top scorers",
        concepts: &["score", "reverse"],
    },
    Term {
        text: "transaction",
        concepts: &["transaction"],
    },
    Term {
        text: "unsubscribe",
        concepts: &["unsubscribe"],
    },
    Term {
        text: "wait across",
        concepts: &["blocking", "multiple"],
    },
    Term {
        text: "waits until",
        concepts: &["blocking"],
    },
    Term {
        text: "will expire",
        concepts: &["ttl"],
    },
    Term {
        text: "attributes",
        concepts: &["field", "hash", "multiple"],
    },
    Term {
        text: "best first",
        concepts: &["reverse", "score"],
    },
    Term {
        text: "byte range",
        concepts: &["substring", "range"],
    },
    Term {
        text: "collection",
        concepts: &["list", "set", "sortedset"],
    },
    Term {
        text: "consume it",
        concepts: &["read-and-delete"],
    },
    Term {
        text: "difference",
        concepts: &["difference"],
    },
    Term {
        text: "duplicates",
        concepts: &["set", "member"],
    },
    Term {
        text: "exactly is",
        concepts: &["location"],
    },
    Term {
        text: "field name",
        concepts: &["list-fields"],
    },
    Term {
        text: "flag array",
        concepts: &["bitmap"],
    },
    Term {
        text: "get rid of",
        concepts: &["delete"],
    },
    Term {
        text: "into a key",
        concepts: &["store"],
    },
    Term {
        text: "latest few",
        concepts: &["range"],
    },
    Term {
        text: "list every",
        concepts: &["read-all"],
    },
    Term {
        text: "membership",
        concepts: &["exists", "member"],
    },
    Term {
        text: "never mind",
        concepts: &["cancel"],
    },
    Term {
        text: "optimistic",
        concepts: &["optimistic-lock"],
    },
    Term {
        text: "page views",
        concepts: &["counter", "increment"],
    },
    Term {
        text: "random pop",
        concepts: &["pop", "random"],
    },
    Term {
        text: "read write",
        concepts: &["read-value", "set-value"],
    },
    Term {
        text: "set moment",
        concepts: &["timestamp", "expire"],
    },
    Term {
        text: "short code",
        concepts: &["encoding", "location"],
    },
    Term {
        text: "sorted set",
        concepts: &["sortedset"],
    },
    Term {
        text: "the result",
        concepts: &["store"],
    },
    Term {
        text: "the values",
        concepts: &["list-values"],
    },
    Term {
        text: "walk every",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "at random",
        concepts: &["random"],
    },
    Term {
        text: "attribute",
        concepts: &["field", "hash"],
    },
    Term {
        text: "backwards",
        concepts: &["reverse"],
    },
    Term {
        text: "before an",
        concepts: &["insert"],
    },
    Term {
        text: "bit array",
        concepts: &["bitmap"],
    },
    Term {
        text: "caught up",
        concepts: &["replication"],
    },
    Term {
        text: "decrement",
        concepts: &["decrement"],
    },
    Term {
        text: "disappear",
        concepts: &["expire"],
    },
    Term {
        text: "does this",
        concepts: &["exists"],
    },
    Term {
        text: "duplicate",
        concepts: &["copy"],
    },
    Term {
        text: "estimator",
        concepts: &["hyperloglog", "cardinality"],
    },
    Term {
        text: "expire at",
        concepts: &["expire", "timestamp"],
    },
    Term {
        text: "expiry as",
        concepts: &["ttl"],
    },
    Term {
        text: "far apart",
        concepts: &["distance"],
    },
    Term {
        text: "frequency",
        concepts: &["frequency"],
    },
    Term {
        text: "geo range",
        concepts: &["radius", "search"],
    },
    Term {
        text: "how alike",
        concepts: &["compare"],
    },
    Term {
        text: "how often",
        concepts: &["frequency"],
    },
    Term {
        text: "idle time",
        concepts: &["idle"],
    },
    Term {
        text: "in common",
        concepts: &["intersection", "count"],
    },
    Term {
        text: "in one go",
        concepts: &["read-all", "multiple"],
    },
    Term {
        text: "increment",
        concepts: &["increment"],
    },
    Term {
        text: "inside it",
        concepts: &["introspect"],
    },
    Term {
        text: "keep only",
        concepts: &["trim"],
    },
    Term {
        text: "leftovers",
        concepts: &["difference"],
    },
    Term {
        text: "life left",
        concepts: &["ttl"],
    },
    Term {
        text: "listeners",
        concepts: &["subscribe", "count"],
    },
    Term {
        text: "listening",
        concepts: &["subscribe"],
    },
    Term {
        text: "make sure",
        concepts: &["acknowledge"],
    },
    Term {
        text: "more text",
        concepts: &["append", "string"],
    },
    Term {
        text: "no longer",
        concepts: &["persist", "cancel"],
    },
    Term {
        text: "read only",
        concepts: &["read-only"],
    },
    Term {
        text: "read-only",
        concepts: &["read-only"],
    },
    Term {
        text: "remaining",
        concepts: &["ttl"],
    },
    Term {
        text: "serialise",
        concepts: &["serialise"],
    },
    Term {
        text: "subscribe",
        concepts: &["subscribe"],
    },
    Term {
        text: "the names",
        concepts: &["list-fields"],
    },
    Term {
        text: "timestamp",
        concepts: &["timestamp"],
    },
    Term {
        text: "whichever",
        concepts: &["multiple"],
    },
    Term {
        text: "before a",
        concepts: &["insert"],
    },
    Term {
        text: "bitfield",
        concepts: &["bit", "bitmap"],
    },
    Term {
        text: "database",
        concepts: &["database"],
    },
    Term {
        text: "distinct",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "encoding",
        concepts: &["encoding"],
    },
    Term {
        text: "estimate",
        concepts: &["approximate"],
    },
    Term {
        text: "hit disk",
        concepts: &["persistence"],
    },
    Term {
        text: "how deep",
        concepts: &["count"],
    },
    Term {
        text: "how long",
        concepts: &["ttl"],
    },
    Term {
        text: "how many",
        concepts: &["count"],
    },
    Term {
        text: "in a set",
        concepts: &["set"],
    },
    Term {
        text: "increase",
        concepts: &["increment"],
    },
    Term {
        text: "is there",
        concepts: &["exists"],
    },
    Term {
        text: "keyspace",
        concepts: &["key", "scan"],
    },
    Term {
        text: "lined up",
        concepts: &["transaction"],
    },
    Term {
        text: "location",
        concepts: &["location"],
    },
    Term {
        text: "matching",
        concepts: &["pattern"],
    },
    Term {
        text: "one flag",
        concepts: &["bit", "index"],
    },
    Term {
        text: "one-shot",
        concepts: &["read-and-delete", "atomic"],
    },
    Term {
        text: "point at",
        concepts: &["memory"],
    },
    Term {
        text: "position",
        concepts: &["index", "rank"],
    },
    Term {
        text: "put back",
        concepts: &["deserialise"],
    },
    Term {
        text: "replicas",
        concepts: &["replication"],
    },
    Term {
        text: "shows up",
        concepts: &["blocking"],
    },
    Term {
        text: "stalling",
        concepts: &["async"],
    },
    Term {
        text: "the sets",
        concepts: &["set", "multiple"],
    },
    Term {
        text: "time out",
        concepts: &["expire"],
    },
    Term {
        text: "wait for",
        concepts: &["blocking"],
    },
    Term {
        text: "watching",
        concepts: &["watch"],
    },
    Term {
        text: "where in",
        concepts: &["index", "search"],
    },
    Term {
        text: "abandon",
        concepts: &["cancel"],
    },
    Term {
        text: "backlog",
        concepts: &["count", "list"],
    },
    Term {
        text: "balance",
        concepts: &["float", "number"],
    },
    Term {
        text: "bitwise",
        concepts: &["bit"],
    },
    Term {
        text: "but not",
        concepts: &["difference"],
    },
    Term {
        text: "channel",
        concepts: &["channel"],
    },
    Term {
        text: "cluster",
        concepts: &["cluster"],
    },
    Term {
        text: "combine",
        concepts: &["union"],
    },
    Term {
        text: "counter",
        concepts: &["counter"],
    },
    Term {
        text: "details",
        concepts: &["cardinality"],
    },
    Term {
        text: "element",
        concepts: &["member"],
    },
    Term {
        text: "entries",
        concepts: &["key"],
    },
    Term {
        text: "execute",
        concepts: &["commit"],
    },
    Term {
        text: "geohash",
        concepts: &["encoding", "location"],
    },
    Term {
        text: "give me",
        concepts: &["read-value", "read-all"],
    },
    Term {
        text: "go away",
        concepts: &["expire"],
    },
    Term {
        text: "how big",
        concepts: &["length"],
    },
    Term {
        text: "how far",
        concepts: &["distance"],
    },
    Term {
        text: "how hot",
        concepts: &["frequency"],
    },
    Term {
        text: "in both",
        concepts: &["intersection"],
    },
    Term {
        text: "inspect",
        concepts: &["introspect"],
    },
    Term {
        text: "iterate",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "kind of",
        concepts: &["type-of"],
    },
    Term {
        text: "left on",
        concepts: &["ttl"],
    },
    Term {
        text: "look at",
        concepts: &["read-value"],
    },
    Term {
        text: "migrate",
        concepts: &["migrate"],
    },
    Term {
        text: "ms left",
        concepts: &["ttl", "milliseconds"],
    },
    Term {
        text: "near me",
        concepts: &["radius", "search"],
    },
    Term {
        text: "numeric",
        concepts: &["score", "number"],
    },
    Term {
        text: "old one",
        concepts: &["read-value", "atomic"],
    },
    Term {
        text: "on this",
        concepts: &["ttl"],
    },
    Term {
        text: "only if",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "ordered",
        concepts: &["rank", "sort"],
    },
    Term {
        text: "overlap",
        concepts: &["intersection"],
    },
    Term {
        text: "package",
        concepts: &["serialise"],
    },
    Term {
        text: "pattern",
        concepts: &["pattern"],
    },
    Term {
        text: "players",
        concepts: &["sortedset", "member", "multiple"],
    },
    Term {
        text: "publish",
        concepts: &["publish"],
    },
    Term {
        text: "replica",
        concepts: &["replication"],
    },
    Term {
        text: "restore",
        concepts: &["deserialise"],
    },
    Term {
        text: "reverse",
        concepts: &["reverse"],
    },
    Term {
        text: "seconds",
        concepts: &["seconds"],
    },
    Term {
        text: "several",
        concepts: &["multiple"],
    },
    Term {
        text: "swap in",
        concepts: &["set-value", "atomic"],
    },
    Term {
        text: "timeout",
        concepts: &["expire"],
    },
    Term {
        text: "top ten",
        concepts: &["range", "rank"],
    },
    Term {
        text: "whether",
        concepts: &["exists"],
    },
    Term {
        text: "0 还是 1",
        concepts: &["bit"],
    },
    Term {
        text: "across",
        concepts: &["multiple"],
    },
    Term {
        text: "active",
        concepts: &["introspect"],
    },
    Term {
        text: "append",
        concepts: &["append"],
    },
    Term {
        text: "atomic",
        concepts: &["atomic"],
    },
    Term {
        text: "bitmap",
        concepts: &["bitmap"],
    },
    Term {
        text: "boards",
        concepts: &["sortedset", "leaderboard", "multiple"],
    },
    Term {
        text: "common",
        concepts: &["compare"],
    },
    Term {
        text: "concat",
        concepts: &["append"],
    },
    Term {
        text: "delete",
        concepts: &["delete"],
    },
    Term {
        text: "exists",
        concepts: &["exists"],
    },
    Term {
        text: "expire",
        concepts: &["expire"],
    },
    Term {
        text: "expiry",
        concepts: &["expire"],
    },
    Term {
        text: "groups",
        concepts: &["set", "member", "multiple"],
    },
    Term {
        text: "insert",
        concepts: &["insert"],
    },
    Term {
        text: "legacy",
        concepts: &["deprecated"],
    },
    Term {
        text: "length",
        concepts: &["length"],
    },
    Term {
        text: "marked",
        concepts: &["bit"],
    },
    Term {
        text: "member",
        concepts: &["member"],
    },
    Term {
        text: "nearby",
        concepts: &["radius", "search"],
    },
    Term {
        text: "not in",
        concepts: &["difference"],
    },
    Term {
        text: "number",
        concepts: &["number"],
    },
    Term {
        text: "offset",
        concepts: &["offset"],
    },
    Term {
        text: "packed",
        concepts: &["bit"],
    },
    Term {
        text: "people",
        concepts: &["member", "multiple"],
    },
    Term {
        text: "person",
        concepts: &["member"],
    },
    Term {
        text: "places",
        concepts: &["location", "multiple"],
    },
    Term {
        text: "player",
        concepts: &["sortedset", "member"],
    },
    Term {
        text: "radius",
        concepts: &["radius"],
    },
    Term {
        text: "random",
        concepts: &["random"],
    },
    Term {
        text: "record",
        concepts: &["member"],
    },
    Term {
        text: "remove",
        concepts: &["remove"],
    },
    Term {
        text: "rename",
        concepts: &["rename"],
    },
    Term {
        text: "rotate",
        concepts: &["move"],
    },
    Term {
        text: "sample",
        concepts: &["random"],
    },
    Term {
        text: "scores",
        concepts: &["score", "multiple"],
    },
    Term {
        text: "string",
        concepts: &["string"],
    },
    Term {
        text: "topics",
        concepts: &["channel", "multiple"],
    },
    Term {
        text: "unpack",
        concepts: &["deserialise"],
    },
    Term {
        text: "values",
        concepts: &["list-values"],
    },
    Term {
        text: "vanish",
        concepts: &["expire"],
    },
    Term {
        text: "window",
        concepts: &["range"],
    },
    Term {
        text: "worker",
        concepts: &["list", "queue", "blocking"],
    },
    Term {
        text: "不再自动过期",
        concepts: &["persist"],
    },
    Term {
        text: "多久没被动过",
        concepts: &["idle"],
    },
    Term {
        text: "多少地方在用",
        concepts: &["memory"],
    },
    Term {
        text: "时刻的毫秒值",
        concepts: &["timestamp", "milliseconds", "ttl"],
    },
    Term {
        text: "用得频不频繁",
        concepts: &["frequency"],
    },
    Term {
        text: "0 或 1",
        concepts: &["bit"],
    },
    Term {
        text: "a set",
        concepts: &["set"],
    },
    Term {
        text: "abort",
        concepts: &["cancel"],
    },
    Term {
        text: "block",
        concepts: &["blocking"],
    },
    Term {
        text: "board",
        concepts: &["sortedset", "leaderboard"],
    },
    Term {
        text: "cache",
        concepts: &["key", "expire"],
    },
    Term {
        text: "claim",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "clear",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "count",
        concepts: &["count"],
    },
    Term {
        text: "entry",
        concepts: &["key"],
    },
    Term {
        text: "every",
        concepts: &["read-all"],
    },
    Term {
        text: "exist",
        concepts: &["exists"],
    },
    Term {
        text: "fetch",
        concepts: &["read-value"],
    },
    Term {
        text: "field",
        concepts: &["field"],
    },
    Term {
        text: "flags",
        concepts: &["bit", "count"],
    },
    Term {
        text: "float",
        concepts: &["float"],
    },
    Term {
        text: "go up",
        concepts: &["increment"],
    },
    Term {
        text: "group",
        concepts: &["set", "member"],
    },
    Term {
        text: "index",
        concepts: &["index"],
    },
    Term {
        text: "is in",
        concepts: &["exists", "member"],
    },
    Term {
        text: "items",
        concepts: &["field", "member", "multiple"],
    },
    Term {
        text: "lapse",
        concepts: &["expire", "ttl"],
    },
    Term {
        text: "merge",
        concepts: &["union"],
    },
    Term {
        text: "minus",
        concepts: &["decrement"],
    },
    Term {
        text: "order",
        concepts: &["sort"],
    },
    Term {
        text: "place",
        concepts: &["location"],
    },
    Term {
        text: "purge",
        concepts: &["remove", "delete"],
    },
    Term {
        text: "queue",
        concepts: &["list", "queue"],
    },
    Term {
        text: "range",
        concepts: &["range"],
    },
    Term {
        text: "right",
        concepts: &["tail"],
    },
    Term {
        text: "score",
        concepts: &["score"],
    },
    Term {
        text: "shard",
        concepts: &["shard"],
    },
    Term {
        text: "share",
        concepts: &["intersection"],
    },
    Term {
        text: "shops",
        concepts: &["location", "geo", "multiple"],
    },
    Term {
        text: "slice",
        concepts: &["range", "substring"],
    },
    Term {
        text: "stall",
        concepts: &["async"],
    },
    Term {
        text: "store",
        concepts: &["set-value", "store"],
    },
    Term {
        text: "thing",
        concepts: &["key"],
    },
    Term {
        text: "throw",
        concepts: &["cancel", "delete"],
    },
    Term {
        text: "token",
        concepts: &["expire"],
    },
    Term {
        text: "topic",
        concepts: &["channel"],
    },
    Term {
        text: "union",
        concepts: &["union"],
    },
    Term {
        text: "watch",
        concepts: &["watch"],
    },
    Term {
        text: "write",
        concepts: &["set-value"],
    },
    Term {
        text: "一串二进制",
        concepts: &["serialise", "backup"],
    },
    Term {
        text: "不管叫什么",
        concepts: &["list-values"],
    },
    Term {
        text: "字段名列表",
        concepts: &["list-fields"],
    },
    Term {
        text: "很小的空间",
        concepts: &["approximate", "cardinality"],
    },
    Term {
        text: "所有订阅者",
        concepts: &["subscribe"],
    },
    Term {
        text: "新 key",
        concepts: &["store"],
    },
    Term {
        text: "相同的部分",
        concepts: &["compare"],
    },
    Term {
        text: "第 n 个",
        concepts: &["index"],
    },
    Term {
        text: "第一个请求",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "过期时间点",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "重复的不算",
        concepts: &["set", "member"],
    },
    Term {
        text: "blob",
        concepts: &["backup"],
    },
    Term {
        text: "bulk",
        concepts: &["multiple"],
    },
    Term {
        text: "bump",
        concepts: &["increment"],
    },
    Term {
        text: "copy",
        concepts: &["copy"],
    },
    Term {
        text: "drop",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "dump",
        concepts: &["read-all", "serialise"],
    },
    Term {
        text: "find",
        concepts: &["search"],
    },
    Term {
        text: "flag",
        concepts: &["bit"],
    },
    Term {
        text: "flip",
        concepts: &["set-value", "bit"],
    },
    Term {
        text: "glob",
        concepts: &["pattern"],
    },
    Term {
        text: "grab",
        concepts: &["read-value"],
    },
    Term {
        text: "hash",
        concepts: &["hash"],
    },
    Term {
        text: "head",
        concepts: &["head"],
    },
    Term {
        text: "idle",
        concepts: &["idle"],
    },
    Term {
        text: "item",
        concepts: &["member"],
    },
    Term {
        text: "left",
        concepts: &["head"],
    },
    Term {
        text: "list",
        concepts: &["list"],
    },
    Term {
        text: "live",
        concepts: &["introspect"],
    },
    Term {
        text: "lock",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "mark",
        concepts: &["bit"],
    },
    Term {
        text: "move",
        concepts: &["move"],
    },
    Term {
        text: "name",
        concepts: &["key"],
    },
    Term {
        text: "only",
        concepts: &["difference"],
    },
    Term {
        text: "peek",
        concepts: &["introspect", "read-value"],
    },
    Term {
        text: "plus",
        concepts: &["increment"],
    },
    Term {
        text: "push",
        concepts: &["push"],
    },
    Term {
        text: "rank",
        concepts: &["rank"],
    },
    Term {
        text: "read",
        concepts: &["read-value"],
    },
    Term {
        text: "scan",
        concepts: &["scan"],
    },
    Term {
        text: "shop",
        concepts: &["location", "geo"],
    },
    Term {
        text: "show",
        concepts: &["read-value", "read-all"],
    },
    Term {
        text: "size",
        concepts: &["length", "count"],
    },
    Term {
        text: "slot",
        concepts: &["insert"],
    },
    Term {
        text: "sort",
        concepts: &["sort"],
    },
    Term {
        text: "stop",
        concepts: &["cancel", "unsubscribe"],
    },
    Term {
        text: "tack",
        concepts: &["append"],
    },
    Term {
        text: "tail",
        concepts: &["tail"],
    },
    Term {
        text: "take",
        concepts: &["pop", "remove"],
    },
    Term {
        text: "trim",
        concepts: &["trim"],
    },
    Term {
        text: "type",
        concepts: &["type-of"],
    },
    Term {
        text: "walk",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "zset",
        concepts: &["sortedset"],
    },
    Term {
        text: "一共多少",
        concepts: &["count"],
    },
    Term {
        text: "一条记录",
        concepts: &["hash"],
    },
    Term {
        text: "一次全列",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "一次全读",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "一起成功",
        concepts: &["transaction", "atomic"],
    },
    Term {
        text: "一页一页",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "不一样的",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "不再接收",
        concepts: &["unsubscribe"],
    },
    Term {
        text: "东西很大",
        concepts: &["key", "length"],
    },
    Term {
        text: "两段文本",
        concepts: &["compare", "multiple"],
    },
    Term {
        text: "什么时候",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "什么结构",
        concepts: &["type-of"],
    },
    Term {
        text: "从高到低",
        concepts: &["reverse"],
    },
    Term {
        text: "他的成绩",
        concepts: &["score"],
    },
    Term {
        text: "位置在哪",
        concepts: &["bit", "index", "search"],
    },
    Term {
        text: "保证不写",
        concepts: &["read-only"],
    },
    Term {
        text: "公共子串",
        concepts: &["compare", "substring"],
    },
    Term {
        text: "其中一项",
        concepts: &["field"],
    },
    Term {
        text: "内部细节",
        concepts: &["introspect"],
    },
    Term {
        text: "内部编码",
        concepts: &["encoding", "introspect"],
    },
    Term {
        text: "分布式锁",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "刚被用过",
        concepts: &["touch"],
    },
    Term {
        text: "别人动过",
        concepts: &["watch", "optimistic-lock"],
    },
    Term {
        text: "到某个点",
        concepts: &["timestamp", "expire"],
    },
    Term {
        text: "到第几个",
        concepts: &["range"],
    },
    Term {
        text: "剩余毫秒",
        concepts: &["ttl", "milliseconds"],
    },
    Term {
        text: "副本确认",
        concepts: &["replication", "acknowledge"],
    },
    Term {
        text: "去掉过期",
        concepts: &["persist"],
    },
    Term {
        text: "去重计数",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "反序列化",
        concepts: &["deserialise"],
    },
    Term {
        text: "取出放到",
        concepts: &["move"],
    },
    Term {
        text: "取消订阅",
        concepts: &["unsubscribe"],
    },
    Term {
        text: "可靠队列",
        concepts: &["queue", "move"],
    },
    Term {
        text: "名字排序",
        concepts: &["lexicographic"],
    },
    Term {
        text: "多少毫秒",
        concepts: &["ttl", "milliseconds"],
    },
    Term {
        text: "存不存在",
        concepts: &["exists"],
    },
    Term {
        text: "存的内容",
        concepts: &["read-value"],
    },
    Term {
        text: "底层用的",
        concepts: &["encoding", "introspect"],
    },
    Term {
        text: "开始接收",
        concepts: &["subscribe"],
    },
    Term {
        text: "引用计数",
        concepts: &["memory"],
    },
    Term {
        text: "打个标记",
        concepts: &["set-value", "bit"],
    },
    Term {
        text: "换个名字",
        concepts: &["rename"],
    },
    Term {
        text: "换成新值",
        concepts: &["set-value", "atomic"],
    },
    Term {
        text: "排在第几",
        concepts: &["rank", "index"],
    },
    Term {
        text: "普通的值",
        concepts: &["string"],
    },
    Term {
        text: "有人订阅",
        concepts: &["subscribe", "introspect"],
    },
    Term {
        text: "有序集合",
        concepts: &["sortedset"],
    },
    Term {
        text: "某个范围",
        concepts: &["range"],
    },
    Term {
        text: "没有的人",
        concepts: &["difference"],
    },
    Term {
        text: "独立访客",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "用户资料",
        concepts: &["hash", "field"],
    },
    Term {
        text: "看第几位",
        concepts: &["bit", "index"],
    },
    Term {
        text: "空闲时长",
        concepts: &["idle"],
    },
    Term {
        text: "精确时刻",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "绝对时间",
        concepts: &["timestamp"],
    },
    Term {
        text: "绝对过期",
        concepts: &["expire", "timestamp"],
    },
    Term {
        text: "续一下命",
        concepts: &["expire"],
    },
    Term {
        text: "编号的库",
        concepts: &["database"],
    },
    Term {
        text: "自己消失",
        concepts: &["expire"],
    },
    Term {
        text: "落到文件",
        concepts: &["persistence"],
    },
    Term {
        text: "落盘确认",
        concepts: &["persistence", "acknowledge"],
    },
    Term {
        text: "被别人改",
        concepts: &["watch", "optimistic-lock"],
    },
    Term {
        text: "订阅人数",
        concepts: &["subscribe", "count"],
    },
    Term {
        text: "访问时间",
        concepts: &["touch"],
    },
    Term {
        text: "访问次数",
        concepts: &["counter", "increment"],
    },
    Term {
        text: "访问频率",
        concepts: &["frequency"],
    },
    Term {
        text: "过期时刻",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "过期时间",
        concepts: &["expire", "ttl"],
    },
    Term {
        text: "还有多少",
        concepts: &["ttl"],
    },
    Term {
        text: "这是什么",
        concepts: &["type-of"],
    },
    Term {
        text: "那个数字",
        concepts: &["score"],
    },
    Term {
        text: "都有哪些",
        concepts: &["read-all"],
    },
    Term {
        text: "随机弹出",
        concepts: &["pop", "random"],
    },
    Term {
        text: "频不频繁",
        concepts: &["frequency"],
    },
    Term {
        text: "add",
        concepts: &["add-member"],
    },
    Term {
        text: "aof",
        concepts: &["persistence"],
    },
    Term {
        text: "bit",
        concepts: &["bit"],
    },
    Term {
        text: "db1",
        concepts: &["database"],
    },
    Term {
        text: "geo",
        concepts: &["geo"],
    },
    Term {
        text: "job",
        concepts: &["list", "queue"],
    },
    Term {
        text: "key",
        concepts: &["key"],
    },
    Term {
        text: "pop",
        concepts: &["pop"],
    },
    Term {
        text: "put",
        concepts: &["set-value", "push"],
    },
    Term {
        text: "set",
        concepts: &["set-value"],
    },
    Term {
        text: "ttl",
        concepts: &["ttl"],
    },
    Term {
        text: "一个人",
        concepts: &["member"],
    },
    Term {
        text: "一小段",
        concepts: &["range"],
    },
    Term {
        text: "一次性",
        concepts: &["read-and-delete", "atomic"],
    },
    Term {
        text: "一起成",
        concepts: &["transaction", "atomic"],
    },
    Term {
        text: "不存在",
        concepts: &["set-if-absent"],
    },
    Term {
        text: "不想再",
        concepts: &["unsubscribe", "cancel"],
    },
    Term {
        text: "不想发",
        concepts: &["multiple"],
    },
    Term {
        text: "不要了",
        concepts: &["delete", "cancel"],
    },
    Term {
        text: "不重复",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "与或非",
        concepts: &["bit", "intersection", "union"],
    },
    Term {
        text: "乐观锁",
        concepts: &["optimistic-lock"],
    },
    Term {
        text: "从一个",
        concepts: &["move", "multiple"],
    },
    Term {
        text: "从后面",
        concepts: &["tail"],
    },
    Term {
        text: "估计器",
        concepts: &["hyperloglog", "cardinality"],
    },
    Term {
        text: "位运算",
        concepts: &["bit"],
    },
    Term {
        text: "值列表",
        concepts: &["list-values"],
    },
    Term {
        text: "写回去",
        concepts: &["deserialise"],
    },
    Term {
        text: "几个人",
        concepts: &["count"],
    },
    Term {
        text: "几秒后",
        concepts: &["seconds", "expire"],
    },
    Term {
        text: "判存在",
        concepts: &["exists"],
    },
    Term {
        text: "别人改",
        concepts: &["watch", "optimistic-lock"],
    },
    Term {
        text: "前一百",
        concepts: &["range", "rank"],
    },
    Term {
        text: "剩余秒",
        concepts: &["ttl", "seconds"],
    },
    Term {
        text: "加带分",
        concepts: &["add-member", "score"],
    },
    Term {
        text: "占多少",
        concepts: &["length"],
    },
    Term {
        text: "原来的",
        concepts: &["read-value", "atomic"],
    },
    Term {
        text: "取一个",
        concepts: &["pop"],
    },
    Term {
        text: "另一台",
        concepts: &["migrate"],
    },
    Term {
        text: "只保留",
        concepts: &["trim"],
    },
    Term {
        text: "只读地",
        concepts: &["read-only"],
    },
    Term {
        text: "合起来",
        concepts: &["union"],
    },
    Term {
        text: "后面接",
        concepts: &["append"],
    },
    Term {
        text: "哪一位",
        concepts: &["bit", "index"],
    },
    Term {
        text: "哪一刻",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "哪个有",
        concepts: &["multiple"],
    },
    Term {
        text: "在不在",
        concepts: &["exists"],
    },
    Term {
        text: "基数加",
        concepts: &["add-member", "cardinality"],
    },
    Term {
        text: "处理中",
        concepts: &["list", "queue"],
    },
    Term {
        text: "多少个",
        concepts: &["count"],
    },
    Term {
        text: "多少人",
        concepts: &["count"],
    },
    Term {
        text: "多少分",
        concepts: &["score"],
    },
    Term {
        text: "多少天",
        concepts: &["count"],
    },
    Term {
        text: "多少秒",
        concepts: &["ttl", "seconds"],
    },
    Term {
        text: "字典序",
        concepts: &["lexicographic"],
    },
    Term {
        text: "字段值",
        concepts: &["field", "list-values"],
    },
    Term {
        text: "字段名",
        concepts: &["field", "list-fields"],
    },
    Term {
        text: "字符串",
        concepts: &["string"],
    },
    Term {
        text: "存一份",
        concepts: &["store"],
    },
    Term {
        text: "存起来",
        concepts: &["store"],
    },
    Term {
        text: "存进去",
        concepts: &["add-member", "set-value"],
    },
    Term {
        text: "小数字",
        concepts: &["bit", "number"],
    },
    Term {
        text: "小整数",
        concepts: &["bit", "number"],
    },
    Term {
        text: "属性值",
        concepts: &["field", "list-values", "hash"],
    },
    Term {
        text: "带分数",
        concepts: &["score"],
    },
    Term {
        text: "序列化",
        concepts: &["serialise"],
    },
    Term {
        text: "开事务",
        concepts: &["transaction"],
    },
    Term {
        text: "往上加",
        concepts: &["increment"],
    },
    Term {
        text: "往下减",
        concepts: &["decrement"],
    },
    Term {
        text: "往后推",
        concepts: &["expire"],
    },
    Term {
        text: "持久化",
        concepts: &["persistence"],
    },
    Term {
        text: "挂着等",
        concepts: &["blocking"],
    },
    Term {
        text: "按顺序",
        concepts: &["rank", "sort"],
    },
    Term {
        text: "排一下",
        concepts: &["sort"],
    },
    Term {
        text: "排个序",
        concepts: &["sort"],
    },
    Term {
        text: "排第几",
        concepts: &["rank"],
    },
    Term {
        text: "排行榜",
        concepts: &["sortedset", "leaderboard"],
    },
    Term {
        text: "接一段",
        concepts: &["append"],
    },
    Term {
        text: "撑多久",
        concepts: &["ttl"],
    },
    Term {
        text: "改一个",
        concepts: &["set-value"],
    },
    Term {
        text: "改个名",
        concepts: &["rename"],
    },
    Term {
        text: "数一数",
        concepts: &["count"],
    },
    Term {
        text: "数据库",
        concepts: &["database"],
    },
    Term {
        text: "时间戳",
        concepts: &["timestamp"],
    },
    Term {
        text: "时间点",
        concepts: &["timestamp"],
    },
    Term {
        text: "最低的",
        concepts: &["score"],
    },
    Term {
        text: "最前面",
        concepts: &["head"],
    },
    Term {
        text: "最后面",
        concepts: &["tail"],
    },
    Term {
        text: "最大的",
        concepts: &["score"],
    },
    Term {
        text: "最小分",
        concepts: &["score"],
    },
    Term {
        text: "最高的",
        concepts: &["score"],
    },
    Term {
        text: "有哪些",
        concepts: &["read-all", "introspect"],
    },
    Term {
        text: "有多像",
        concepts: &["compare"],
    },
    Term {
        text: "有没有",
        concepts: &["exists"],
    },
    Term {
        text: "某一位",
        concepts: &["bit", "index"],
    },
    Term {
        text: "某一项",
        concepts: &["field"],
    },
    Term {
        text: "某个人",
        concepts: &["member"],
    },
    Term {
        text: "某个量",
        concepts: &["increment"],
    },
    Term {
        text: "某区间",
        concepts: &["range"],
    },
    Term {
        text: "查出来",
        concepts: &["read-value", "multiple"],
    },
    Term {
        text: "没人碰",
        concepts: &["idle"],
    },
    Term {
        text: "游标扫",
        concepts: &["scan", "iterate"],
    },
    Term {
        text: "灰名单",
        concepts: &["set", "member"],
    },
    Term {
        text: "留一份",
        concepts: &["store"],
    },
    Term {
        text: "白名单",
        concepts: &["set", "member"],
    },
    Term {
        text: "看一眼",
        concepts: &["read-value"],
    },
    Term {
        text: "真的跑",
        concepts: &["commit"],
    },
    Term {
        text: "短代码",
        concepts: &["encoding", "location"],
    },
    Term {
        text: "碰一下",
        concepts: &["touch"],
    },
    Term {
        text: "符合的",
        concepts: &["pattern"],
    },
    Term {
        text: "第一名",
        concepts: &["rank", "score"],
    },
    Term {
        text: "第几个",
        concepts: &["index"],
    },
    Term {
        text: "第几位",
        concepts: &["index"],
    },
    Term {
        text: "第几名",
        concepts: &["rank"],
    },
    Term {
        text: "经纬度",
        concepts: &["location"],
    },
    Term {
        text: "翻一遍",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "老代码",
        concepts: &["deprecated"],
    },
    Term {
        text: "自动没",
        concepts: &["expire"],
    },
    Term {
        text: "范围内",
        concepts: &["range"],
    },
    Term {
        text: "装回去",
        concepts: &["deserialise"],
    },
    Term {
        text: "计数器",
        concepts: &["counter"],
    },
    Term {
        text: "订阅数",
        concepts: &["subscribe", "count"],
    },
    Term {
        text: "订阅者",
        concepts: &["subscribe", "count"],
    },
    Term {
        text: "记录里",
        concepts: &["hash", "field"],
    },
    Term {
        text: "跨实例",
        concepts: &["migrate"],
    },
    Term {
        text: "还空着",
        concepts: &["set-if-absent"],
    },
    Term {
        text: "还能撑",
        concepts: &["ttl"],
    },
    Term {
        text: "还能活",
        concepts: &["ttl"],
    },
    Term {
        text: "通配符",
        concepts: &["pattern"],
    },
    Term {
        text: "那一刻",
        concepts: &["timestamp", "ttl"],
    },
    Term {
        text: "那一项",
        concepts: &["field"],
    },
    Term {
        text: "都有的",
        concepts: &["intersection"],
    },
    Term {
        text: "都有谁",
        concepts: &["read-all"],
    },
    Term {
        text: "随机看",
        concepts: &["random", "read-value"],
    },
    Term {
        text: "隔多远",
        concepts: &["distance"],
    },
    Term {
        text: "验证码",
        concepts: &["expire"],
    },
    Term {
        text: "db",
        concepts: &["database"],
    },
    Term {
        text: "ms",
        concepts: &["milliseconds"],
    },
    Term {
        text: "一共",
        concepts: &["count"],
    },
    Term {
        text: "一有",
        concepts: &["blocking"],
    },
    Term {
        text: "一次",
        concepts: &["multiple"],
    },
    Term {
        text: "一步",
        concepts: &["atomic"],
    },
    Term {
        text: "一段",
        concepts: &["range", "substring"],
    },
    Term {
        text: "一页",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "一项",
        concepts: &["field"],
    },
    Term {
        text: "下标",
        concepts: &["index"],
    },
    Term {
        text: "不再",
        concepts: &["persist"],
    },
    Term {
        text: "不写",
        concepts: &["read-only"],
    },
    Term {
        text: "不删",
        concepts: &["read"],
    },
    Term {
        text: "不在",
        concepts: &["difference"],
    },
    Term {
        text: "不盯",
        concepts: &["watch", "cancel"],
    },
    Term {
        text: "东西",
        concepts: &["key"],
    },
    Term {
        text: "主题",
        concepts: &["channel"],
    },
    Term {
        text: "事务",
        concepts: &["transaction"],
    },
    Term {
        text: "交集",
        concepts: &["intersection"],
    },
    Term {
        text: "从库",
        concepts: &["replication"],
    },
    Term {
        text: "令牌",
        concepts: &["expire"],
    },
    Term {
        text: "任务",
        concepts: &["list", "queue"],
    },
    Term {
        text: "会没",
        concepts: &["expire", "ttl"],
    },
    Term {
        text: "估算",
        concepts: &["approximate"],
    },
    Term {
        text: "位图",
        concepts: &["bitmap"],
    },
    Term {
        text: "位域",
        concepts: &["bit", "bitmap"],
    },
    Term {
        text: "位置",
        concepts: &["index", "bit"],
    },
    Term {
        text: "余额",
        concepts: &["float", "number"],
    },
    Term {
        text: "作废",
        concepts: &["expire", "delete"],
    },
    Term {
        text: "保存",
        concepts: &["store"],
    },
    Term {
        text: "倒序",
        concepts: &["reverse"],
    },
    Term {
        text: "偏移",
        concepts: &["offset"],
    },
    Term {
        text: "元素",
        concepts: &["member"],
    },
    Term {
        text: "全列",
        concepts: &["read-all"],
    },
    Term {
        text: "全部",
        concepts: &["read-all"],
    },
    Term {
        text: "公里",
        concepts: &["distance", "radius"],
    },
    Term {
        text: "共同",
        concepts: &["intersection"],
    },
    Term {
        text: "共有",
        concepts: &["intersection"],
    },
    Term {
        text: "内存",
        concepts: &["memory"],
    },
    Term {
        text: "内省",
        concepts: &["introspect"],
    },
    Term {
        text: "内部",
        concepts: &["introspect"],
    },
    Term {
        text: "写入",
        concepts: &["set-value"],
    },
    Term {
        text: "写好",
        concepts: &["set-value", "multiple"],
    },
    Term {
        text: "写成",
        concepts: &["set-value"],
    },
    Term {
        text: "冠军",
        concepts: &["score", "rank"],
    },
    Term {
        text: "减一",
        concepts: &["decrement"],
    },
    Term {
        text: "减去",
        concepts: &["decrement"],
    },
    Term {
        text: "减掉",
        concepts: &["decrement"],
    },
    Term {
        text: "几个",
        concepts: &["count", "multiple"],
    },
    Term {
        text: "几天",
        concepts: &["count"],
    },
    Term {
        text: "分数",
        concepts: &["score"],
    },
    Term {
        text: "分片",
        concepts: &["shard"],
    },
    Term {
        text: "分页",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "列出",
        concepts: &["read-all", "introspect"],
    },
    Term {
        text: "列表",
        concepts: &["list"],
    },
    Term {
        text: "删掉",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "删除",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "判断",
        concepts: &["exists"],
    },
    Term {
        text: "别让",
        concepts: &["persist", "cancel"],
    },
    Term {
        text: "到新",
        concepts: &["store"],
    },
    Term {
        text: "到期",
        concepts: &["expire"],
    },
    Term {
        text: "刷新",
        concepts: &["touch"],
    },
    Term {
        text: "前十",
        concepts: &["range", "rank"],
    },
    Term {
        text: "前面",
        concepts: &["head"],
    },
    Term {
        text: "剩余",
        concepts: &["ttl"],
    },
    Term {
        text: "剩几",
        concepts: &["ttl"],
    },
    Term {
        text: "副本",
        concepts: &["replication"],
    },
    Term {
        text: "加 ",
        concepts: &["increment"],
    },
    Term {
        text: "加一",
        concepts: &["increment"],
    },
    Term {
        text: "加上",
        concepts: &["increment"],
    },
    Term {
        text: "加分",
        concepts: &["increment", "score"],
    },
    Term {
        text: "加进",
        concepts: &["add-member"],
    },
    Term {
        text: "匹配",
        concepts: &["pattern", "search"],
    },
    Term {
        text: "区间",
        concepts: &["range"],
    },
    Term {
        text: "半径",
        concepts: &["radius"],
    },
    Term {
        text: "占坑",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "卡住",
        concepts: &["async"],
    },
    Term {
        text: "原子",
        concepts: &["atomic"],
    },
    Term {
        text: "去掉",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "去重",
        concepts: &["cardinality"],
    },
    Term {
        text: "及格",
        concepts: &["score", "range"],
    },
    Term {
        text: "反向",
        concepts: &["reverse"],
    },
    Term {
        text: "取值",
        concepts: &["read-value"],
    },
    Term {
        text: "取出",
        concepts: &["read-value"],
    },
    Term {
        text: "取消",
        concepts: &["cancel"],
    },
    Term {
        text: "取谁",
        concepts: &["pop"],
    },
    Term {
        text: "取走",
        concepts: &["pop"],
    },
    Term {
        text: "另存",
        concepts: &["store", "copy"],
    },
    Term {
        text: "只留",
        concepts: &["trim"],
    },
    Term {
        text: "只读",
        concepts: &["read-only"],
    },
    Term {
        text: "右出",
        concepts: &["pop", "tail"],
    },
    Term {
        text: "右边",
        concepts: &["tail"],
    },
    Term {
        text: "右进",
        concepts: &["push", "tail"],
    },
    Term {
        text: "合并",
        concepts: &["union"],
    },
    Term {
        text: "同时",
        concepts: &["atomic", "multiple"],
    },
    Term {
        text: "同步",
        concepts: &["replication"],
    },
    Term {
        text: "名单",
        concepts: &["set", "member"],
    },
    Term {
        text: "名字",
        concepts: &["key"],
    },
    Term {
        text: "名次",
        concepts: &["rank"],
    },
    Term {
        text: "后台",
        concepts: &["async"],
    },
    Term {
        text: "后面",
        concepts: &["tail"],
    },
    Term {
        text: "周边",
        concepts: &["radius"],
    },
    Term {
        text: "哈希",
        concepts: &["hash"],
    },
    Term {
        text: "哪些",
        concepts: &["introspect", "read-all"],
    },
    Term {
        text: "哪天",
        concepts: &["ttl", "timestamp"],
    },
    Term {
        text: "回收",
        concepts: &["async", "delete"],
    },
    Term {
        text: "在听",
        concepts: &["subscribe"],
    },
    Term {
        text: "在哪",
        concepts: &["location"],
    },
    Term {
        text: "在线",
        concepts: &["publish", "channel"],
    },
    Term {
        text: "地方",
        concepts: &["location"],
    },
    Term {
        text: "地点",
        concepts: &["location"],
    },
    Term {
        text: "坐标",
        concepts: &["location"],
    },
    Term {
        text: "垫底",
        concepts: &["score"],
    },
    Term {
        text: "基数",
        concepts: &["cardinality"],
    },
    Term {
        text: "增量",
        concepts: &["increment"],
    },
    Term {
        text: "复制",
        concepts: &["copy"],
    },
    Term {
        text: "复刻",
        concepts: &["copy"],
    },
    Term {
        text: "多个",
        concepts: &["multiple"],
    },
    Term {
        text: "多大",
        concepts: &["length"],
    },
    Term {
        text: "多远",
        concepts: &["distance"],
    },
    Term {
        text: "多长",
        concepts: &["length"],
    },
    Term {
        text: "失效",
        concepts: &["expire"],
    },
    Term {
        text: "失败",
        concepts: &["optimistic-lock"],
    },
    Term {
        text: "头像",
        concepts: &["field", "hash"],
    },
    Term {
        text: "头部",
        concepts: &["head"],
    },
    Term {
        text: "字段",
        concepts: &["field"],
    },
    Term {
        text: "字节",
        concepts: &["length"],
    },
    Term {
        text: "存到",
        concepts: &["set-value"],
    },
    Term {
        text: "存在",
        concepts: &["exists", "set-if-absent"],
    },
    Term {
        text: "存好",
        concepts: &["store"],
    },
    Term {
        text: "存活",
        concepts: &["ttl"],
    },
    Term {
        text: "存起",
        concepts: &["store"],
    },
    Term {
        text: "守着",
        concepts: &["blocking"],
    },
    Term {
        text: "导入",
        concepts: &["deserialise"],
    },
    Term {
        text: "导出",
        concepts: &["serialise", "backup"],
    },
    Term {
        text: "寿命",
        concepts: &["ttl", "expire"],
    },
    Term {
        text: "小数",
        concepts: &["float"],
    },
    Term {
        text: "就等",
        concepts: &["blocking"],
    },
    Term {
        text: "尾部",
        concepts: &["tail"],
    },
    Term {
        text: "属于",
        concepts: &["member", "exists"],
    },
    Term {
        text: "属性",
        concepts: &["field", "hash"],
    },
    Term {
        text: "左出",
        concepts: &["pop", "head"],
    },
    Term {
        text: "左边",
        concepts: &["head"],
    },
    Term {
        text: "左进",
        concepts: &["push", "head"],
    },
    Term {
        text: "差集",
        concepts: &["difference"],
    },
    Term {
        text: "并集",
        concepts: &["union"],
    },
    Term {
        text: "广播",
        concepts: &["publish"],
    },
    Term {
        text: "库存",
        concepts: &["counter"],
    },
    Term {
        text: "底层",
        concepts: &["introspect", "encoding"],
    },
    Term {
        text: "引用",
        concepts: &["memory"],
    },
    Term {
        text: "弹出",
        concepts: &["pop"],
    },
    Term {
        text: "待办",
        concepts: &["list", "queue"],
    },
    Term {
        text: "恢复",
        concepts: &["deserialise"],
    },
    Term {
        text: "情况",
        concepts: &["introspect"],
    },
    Term {
        text: "成员",
        concepts: &["member"],
    },
    Term {
        text: "成绩",
        concepts: &["score"],
    },
    Term {
        text: "截取",
        concepts: &["substring"],
    },
    Term {
        text: "所有",
        concepts: &["read-all", "list-fields"],
    },
    Term {
        text: "打包",
        concepts: &["serialise", "batch"],
    },
    Term {
        text: "批量",
        concepts: &["multiple"],
    },
    Term {
        text: "找出",
        concepts: &["search"],
    },
    Term {
        text: "拼接",
        concepts: &["append"],
    },
    Term {
        text: "拿掉",
        concepts: &["remove", "delete"],
    },
    Term {
        text: "指着",
        concepts: &["memory"],
    },
    Term {
        text: "按位",
        concepts: &["bit"],
    },
    Term {
        text: "换库",
        concepts: &["database", "move"],
    },
    Term {
        text: "换掉",
        concepts: &["index", "set-value"],
    },
    Term {
        text: "排名",
        concepts: &["rank"],
    },
    Term {
        text: "排在",
        concepts: &["rank", "index"],
    },
    Term {
        text: "排好",
        concepts: &["transaction", "sort"],
    },
    Term {
        text: "排序",
        concepts: &["sort"],
    },
    Term {
        text: "排的",
        concepts: &["transaction"],
    },
    Term {
        text: "排队",
        concepts: &["transaction"],
    },
    Term {
        text: "接收",
        concepts: &["subscribe"],
    },
    Term {
        text: "推送",
        concepts: &["publish", "channel"],
    },
    Term {
        text: "提交",
        concepts: &["commit"],
    },
    Term {
        text: "插入",
        concepts: &["push", "insert"],
    },
    Term {
        text: "插到",
        concepts: &["insert"],
    },
    Term {
        text: "搜索",
        concepts: &["search"],
    },
    Term {
        text: "搬到",
        concepts: &["migrate", "move"],
    },
    Term {
        text: "撤掉",
        concepts: &["remove", "delete"],
    },
    Term {
        text: "改名",
        concepts: &["rename"],
    },
    Term {
        text: "改掉",
        concepts: &["set-value", "index"],
    },
    Term {
        text: "放到",
        concepts: &["move"],
    },
    Term {
        text: "放进",
        concepts: &["add-member"],
    },
    Term {
        text: "数值",
        concepts: &["number"],
    },
    Term {
        text: "数字",
        concepts: &["number"],
    },
    Term {
        text: "数量",
        concepts: &["count"],
    },
    Term {
        text: "整个",
        concepts: &["read-all"],
    },
    Term {
        text: "整数",
        concepts: &["number"],
    },
    Term {
        text: "文件",
        concepts: &["persistence"],
    },
    Term {
        text: "文本",
        concepts: &["string", "compare"],
    },
    Term {
        text: "日志",
        concepts: &["append", "string"],
    },
    Term {
        text: "旧值",
        concepts: &["read-value", "atomic"],
    },
    Term {
        text: "旧的",
        concepts: &["deprecated"],
    },
    Term {
        text: "时刻",
        concepts: &["timestamp", "ttl"],
    },
    Term {
        text: "明细",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "昵称",
        concepts: &["field", "hash"],
    },
    Term {
        text: "最近",
        concepts: &["range"],
    },
    Term {
        text: "有而",
        concepts: &["difference"],
    },
    Term {
        text: "某段",
        concepts: &["range"],
    },
    Term {
        text: "查分",
        concepts: &["score"],
    },
    Term {
        text: "标记",
        concepts: &["bit"],
    },
    Term {
        text: "模式",
        concepts: &["pattern"],
    },
    Term {
        text: "比较",
        concepts: &["compare"],
    },
    Term {
        text: "毫秒",
        concepts: &["milliseconds"],
    },
    Term {
        text: "没掉",
        concepts: &["expire"],
    },
    Term {
        text: "活的",
        concepts: &["introspect"],
    },
    Term {
        text: "活跃",
        concepts: &["introspect"],
    },
    Term {
        text: "浮点",
        concepts: &["float"],
    },
    Term {
        text: "消失",
        concepts: &["expire"],
    },
    Term {
        text: "消息",
        concepts: &["publish", "channel", "list", "queue"],
    },
    Term {
        text: "淘汰",
        concepts: &["eviction"],
    },
    Term {
        text: "清掉",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "游标",
        concepts: &["scan"],
    },
    Term {
        text: "片段",
        concepts: &["substring"],
    },
    Term {
        text: "版本",
        concepts: &["compare"],
    },
    Term {
        text: "状态",
        concepts: &["introspect"],
    },
    Term {
        text: "玩家",
        concepts: &["sortedset", "member"],
    },
    Term {
        text: "监视",
        concepts: &["watch"],
    },
    Term {
        text: "盯着",
        concepts: &["watch", "blocking"],
    },
    Term {
        text: "相隔",
        concepts: &["distance"],
    },
    Term {
        text: "看看",
        concepts: &["read-value", "introspect"],
    },
    Term {
        text: "确保",
        concepts: &["acknowledge"],
    },
    Term {
        text: "确认",
        concepts: &["acknowledge"],
    },
    Term {
        text: "积分",
        concepts: &["score"],
    },
    Term {
        text: "积压",
        concepts: &["count", "list"],
    },
    Term {
        text: "移到",
        concepts: &["move"],
    },
    Term {
        text: "移除",
        concepts: &["remove"],
    },
    Term {
        text: "空闲",
        concepts: &["idle"],
    },
    Term {
        text: "等到",
        concepts: &["blocking"],
    },
    Term {
        text: "等待",
        concepts: &["blocking"],
    },
    Term {
        text: "等着",
        concepts: &["blocking"],
    },
    Term {
        text: "签到",
        concepts: &["bit", "bitmap"],
    },
    Term {
        text: "类型",
        concepts: &["type-of"],
    },
    Term {
        text: "精确",
        concepts: &["timestamp"],
    },
    Term {
        text: "累加",
        concepts: &["increment", "float"],
    },
    Term {
        text: "统计",
        concepts: &["count"],
    },
    Term {
        text: "续期",
        concepts: &["expire"],
    },
    Term {
        text: "缓存",
        concepts: &["key", "expire"],
    },
    Term {
        text: "编码",
        concepts: &["encoding"],
    },
    Term {
        text: "置位",
        concepts: &["set-value", "bit"],
    },
    Term {
        text: "自减",
        concepts: &["decrement"],
    },
    Term {
        text: "自增",
        concepts: &["increment"],
    },
    Term {
        text: "范围",
        concepts: &["range"],
    },
    Term {
        text: "落到",
        concepts: &["persistence"],
    },
    Term {
        text: "落盘",
        concepts: &["persistence"],
    },
    Term {
        text: "补上",
        concepts: &["set-if-absent", "set-value"],
    },
    Term {
        text: "裁剪",
        concepts: &["trim"],
    },
    Term {
        text: "要么",
        concepts: &["transaction", "atomic"],
    },
    Term {
        text: "覆盖",
        concepts: &["set-value"],
    },
    Term {
        text: "见过",
        concepts: &["cardinality"],
    },
    Term {
        text: "订阅",
        concepts: &["subscribe"],
    },
    Term {
        text: "记下",
        concepts: &["add-member"],
    },
    Term {
        text: "记录",
        concepts: &["hash", "field", "add-member"],
    },
    Term {
        text: "设置",
        concepts: &["set-value"],
    },
    Term {
        text: "访问",
        concepts: &["touch", "idle"],
    },
    Term {
        text: "读位",
        concepts: &["read-value", "bit"],
    },
    Term {
        text: "读写",
        concepts: &["read-value", "set-value"],
    },
    Term {
        text: "读取",
        concepts: &["read-value"],
    },
    Term {
        text: "谁有",
        concepts: &["multiple"],
    },
    Term {
        text: "距离",
        concepts: &["distance"],
    },
    Term {
        text: "转到",
        concepts: &["move"],
    },
    Term {
        text: "转移",
        concepts: &["move"],
    },
    Term {
        text: "迁移",
        concepts: &["migrate"],
    },
    Term {
        text: "过期",
        concepts: &["expire"],
    },
    Term {
        text: "近似",
        concepts: &["approximate"],
    },
    Term {
        text: "还剩",
        concepts: &["ttl"],
    },
    Term {
        text: "还有",
        concepts: &["ttl"],
    },
    Term {
        text: "这堆",
        concepts: &["key", "multiple"],
    },
    Term {
        text: "这段",
        concepts: &["range"],
    },
    Term {
        text: "追加",
        concepts: &["append", "push", "tail"],
    },
    Term {
        text: "退订",
        concepts: &["unsubscribe"],
    },
    Term {
        text: "遍历",
        concepts: &["iterate", "scan"],
    },
    Term {
        text: "都在",
        concepts: &["intersection"],
    },
    Term {
        text: "重合",
        concepts: &["intersection"],
    },
    Term {
        text: "长度",
        concepts: &["length", "count"],
    },
    Term {
        text: "门店",
        concepts: &["location", "geo"],
    },
    Term {
        text: "闲置",
        concepts: &["idle"],
    },
    Term {
        text: "队列",
        concepts: &["list", "queue"],
    },
    Term {
        text: "阻塞",
        concepts: &["blocking"],
    },
    Term {
        text: "附近",
        concepts: &["radius", "search"],
    },
    Term {
        text: "随便",
        concepts: &["random"],
    },
    Term {
        text: "随手",
        concepts: &["random"],
    },
    Term {
        text: "随机",
        concepts: &["random"],
    },
    Term {
        text: "集合",
        concepts: &["set"],
    },
    Term {
        text: "集群",
        concepts: &["cluster"],
    },
    Term {
        text: "零点",
        concepts: &["timestamp"],
    },
    Term {
        text: "顺序",
        concepts: &["rank", "sort"],
    },
    Term {
        text: "频率",
        concepts: &["frequency"],
    },
    Term {
        text: "频道",
        concepts: &["channel"],
    },
    Term {
        text: "高分",
        concepts: &["score"],
    },
    Term {
        text: "人",
        concepts: &["member"],
    },
    Term {
        text: "位",
        concepts: &["bit"],
    },
    Term {
        text: "写",
        concepts: &["set-value"],
    },
    Term {
        text: "减",
        concepts: &["decrement"],
    },
    Term {
        text: "出",
        concepts: &["pop"],
    },
    Term {
        text: "删",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "加",
        concepts: &["add-member", "increment"],
    },
    Term {
        text: "发",
        concepts: &["publish"],
    },
    Term {
        text: "取",
        concepts: &["read-value"],
    },
    Term {
        text: "右",
        concepts: &["tail"],
    },
    Term {
        text: "塞",
        concepts: &["set-value", "push"],
    },
    Term {
        text: "存",
        concepts: &["set-value", "store"],
    },
    Term {
        text: "左",
        concepts: &["head"],
    },
    Term {
        text: "店",
        concepts: &["location", "geo"],
    },
    Term {
        text: "打",
        concepts: &["set-value", "bit"],
    },
    Term {
        text: "扣",
        concepts: &["decrement"],
    },
    Term {
        text: "扫",
        concepts: &["scan"],
    },
    Term {
        text: "找",
        concepts: &["search"],
    },
    Term {
        text: "抽",
        concepts: &["random"],
    },
    Term {
        text: "拷",
        concepts: &["copy"],
    },
    Term {
        text: "拿",
        concepts: &["read-value"],
    },
    Term {
        text: "挑",
        concepts: &["random"],
    },
    Term {
        text: "挪",
        concepts: &["move"],
    },
    Term {
        text: "排",
        concepts: &["sort", "rank"],
    },
    Term {
        text: "推",
        concepts: &["publish"],
    },
    Term {
        text: "插",
        concepts: &["push", "insert"],
    },
    Term {
        text: "搜",
        concepts: &["search"],
    },
    Term {
        text: "搬",
        concepts: &["move"],
    },
    Term {
        text: "收",
        concepts: &["subscribe"],
    },
    Term {
        text: "改",
        concepts: &["set-value"],
    },
    Term {
        text: "放",
        concepts: &["add-member"],
    },
    Term {
        text: "榜",
        concepts: &["sortedset", "leaderboard"],
    },
    Term {
        text: "清",
        concepts: &["delete", "remove"],
    },
    Term {
        text: "看",
        concepts: &["read-value", "introspect"],
    },
    Term {
        text: "碰",
        concepts: &["touch"],
    },
    Term {
        text: "秒",
        concepts: &["seconds"],
    },
    Term {
        text: "移",
        concepts: &["move"],
    },
    Term {
        text: "等",
        concepts: &["blocking"],
    },
    Term {
        text: "组",
        concepts: &["set", "member"],
    },
    Term {
        text: "续",
        concepts: &["expire"],
    },
    Term {
        text: "群",
        concepts: &["set", "member"],
    },
    Term {
        text: "翻",
        concepts: &["iterate", "range"],
    },
    Term {
        text: "记",
        concepts: &["set-value", "add-member"],
    },
    Term {
        text: "读",
        concepts: &["read-value"],
    },
    Term {
        text: "进",
        concepts: &["push"],
    },
    Term {
        text: "退",
        concepts: &["unsubscribe"],
    },
    Term {
        text: "锁",
        concepts: &["set-if-absent", "conditional"],
    },
    Term {
        text: "键",
        concepts: &["key"],
    },
    Term {
        text: "项",
        concepts: &["field"],
    },
    Term {
        text: "assign",
        concepts: &["set-value"],
    },
    Term {
        text: "overwriting",
        concepts: &["set-value"],
    },
    Term {
        text: "overwrit",
        concepts: &["set-value"],
    },
    Term {
        text: "赋一个值",
        concepts: &["set-value", "string"],
    },
    Term {
        text: "赋值",
        concepts: &["set-value", "string"],
    },
    Term {
        text: "look up",
        concepts: &["read-value"],
    },
    Term {
        text: "stored under",
        concepts: &["read-value"],
    },
    Term {
        text: "读出",
        concepts: &["read-value"],
    },
    Term {
        text: "存着的",
        concepts: &["read-value"],
    },
    Term {
        text: "atomically",
        concepts: &["atomic"],
    },
    Term {
        text: "take the key away",
        concepts: &["delete"],
    },
    Term {
        text: "hand back",
        concepts: &["read-value"],
    },
    Term {
        text: "replace the value",
        concepts: &["set-value"],
    },
    Term {
        text: "subtract",
        concepts: &["decrement"],
    },
    Term {
        text: "a given amount",
        concepts: &["number"],
    },
    Term {
        text: "floating point",
        concepts: &["float"],
    },
    Term {
        text: "delta",
        concepts: &["number"],
    },
    Term {
        text: "is absent",
        concepts: &["set-if-absent"],
    },
    Term {
        text: "multi-key",
        concepts: &["multiple"],
    },
    Term {
        text: "only if none",
        concepts: &["set-if-absent"],
    },
    Term {
        text: "keep adding",
        concepts: &["append"],
    },
    Term {
        text: "superseded",
        concepts: &["deprecated"],
    },
    Term {
        text: "starting at an offset",
        concepts: &["offset"],
    },
    Term {
        text: "pairs",
        concepts: &["multiple"],
    },
    Term {
        text: "middle part",
        concepts: &["substring"],
    },
    Term {
        text: "add one",
        concepts: &["increment"],
    },
    Term {
        text: "numeric value",
        concepts: &["number"],
    },
    Term {
        text: "knock",
        concepts: &["decrement"],
    },
    Term {
        text: "down by one",
        concepts: &["decrement"],
    },
    Term {
        text: "wipe",
        concepts: &["delete"],
    },
    Term {
        text: "out of the database",
        concepts: &["database"],
    },
    Term {
        text: "干掉",
        concepts: &["delete"],
    },
    Term {
        text: "background thread",
        concepts: &["async"],
    },
    Term {
        text: "free the memory",
        concepts: &["memory", "async"],
    },
    Term {
        text: "serialize",
        concepts: &["serialise"],
    },
    Term {
        text: "rebuilt later",
        concepts: &["serialise"],
    },
    Term {
        text: "recreate",
        concepts: &["deserialise"],
    },
    Term {
        text: "serialized payload",
        concepts: &["deserialise"],
    },
    Term {
        text: "unix time",
        concepts: &["timestamp"],
    },
    Term {
        text: "die at",
        concepts: &["expire"],
    },
    Term {
        text: "deadline",
        concepts: &["timestamp"],
    },
    Term {
        text: "按毫秒时间戳",
        concepts: &["timestamp", "milliseconds"],
    },
    Term {
        text: "定时删除",
        concepts: &["expire"],
    },
    Term {
        text: "absolute second",
        concepts: &["timestamp"],
    },
    Term {
        text: "permanent",
        concepts: &["persist"],
    },
    Term {
        text: "transfer",
        concepts: &["migrate"],
    },
    Term {
        text: "lfu",
        concepts: &["frequency"],
    },
    Term {
        text: "被访问得",
        concepts: &["frequency"],
    },
    Term {
        text: "没人读过",
        concepts: &["idle"],
    },
    Term {
        text: "多久没人",
        concepts: &["idle"],
    },
    Term {
        text: "arbitrary",
        concepts: &["random"],
    },
    Term {
        text: "pick an",
        concepts: &["random"],
    },
    Term {
        text: "change the name",
        concepts: &["rename"],
    },
    Term {
        text: "ranked output",
        concepts: &["sort"],
    },
    Term {
        text: "排好的",
        concepts: &["sort"],
    },
    Term {
        text: "不写回",
        concepts: &["read-only"],
    },
    Term {
        text: "refresh",
        concepts: &["touch"],
    },
    Term {
        text: "how recently",
        concepts: &["idle", "touch"],
    },
    Term {
        text: "lifetime",
        concepts: &["ttl"],
    },
    Term {
        text: "walk the",
        concepts: &["iterate"],
    },
    Term {
        text: "scored set",
        concepts: &["sortedset"],
    },
    Term {
        text: "scored",
        concepts: &["sortedset"],
    },
    Term {
        text: "排序集合",
        concepts: &["sortedset"],
    },
    Term {
        text: "打个分",
        concepts: &["score"],
    },
    Term {
        text: "combined",
        concepts: &["union", "intersection"],
    },
    Term {
        text: "between a min and a max",
        concepts: &["range"],
    },
    Term {
        text: "lies between",
        concepts: &["range"],
    },
    Term {
        text: "destination key",
        concepts: &["store"],
    },
    Term {
        text: "destination",
        concepts: &["store"],
    },
    Term {
        text: "写进新的 key",
        concepts: &["store"],
    },
    Term {
        text: "index positions",
        concepts: &["rank"],
    },
    Term {
        text: "裁掉名次",
        concepts: &["rank"],
    },
    Term {
        text: "counting down from",
        concepts: &["reverse", "rank"],
    },
    Term {
        text: "随机抽几个",
        concepts: &["random"],
    },
    Term {
        text: "lowest scoring",
        concepts: &["score", "rank"],
    },
    Term {
        text: "highest scoring",
        concepts: &["score", "rank"],
    },
    Term {
        text: "evict",
        concepts: &["remove"],
    },
    Term {
        text: "threshold",
        concepts: &["range"],
    },
    Term {
        text: "bounds",
        concepts: &["range"],
    },
    Term {
        text: "同分",
        concepts: &["sortedset", "lexicographic"],
    },
    Term {
        text: "字典区间",
        concepts: &["lexicographic", "range"],
    },
    Term {
        text: "shared members",
        concepts: &["intersection", "union"],
    },
    Term {
        text: "prepend",
        concepts: &["push", "head"],
    },
    Term {
        text: "newest",
        concepts: &["head"],
    },
    Term {
        text: "back end",
        concepts: &["tail"],
    },
    Term {
        text: "队尾",
        concepts: &["tail"],
    },
    Term {
        text: "just before",
        concepts: &["insert"],
    },
    Term {
        text: "前面插一条",
        concepts: &["insert"],
    },
    Term {
        text: "drop the rest",
        concepts: &["trim"],
    },
    Term {
        text: "handoff",
        concepts: &["move"],
    },
    Term {
        text: "the front one",
        concepts: &["head"],
    },
    Term {
        text: "broadcast",
        concepts: &["publish"],
    },
    Term {
        text: "listener",
        concepts: &["subscribe"],
    },
    Term {
        text: "wildcard",
        concepts: &["pattern"],
    },
    Term {
        text: "通配",
        concepts: &["pattern"],
    },
    Term {
        text: "leave the",
        concepts: &["unsubscribe"],
    },
    Term {
        text: "运行情况",
        concepts: &["introspect"],
    },
    Term {
        text: "ignore any duplicates",
        concepts: &["set"],
    },
    Term {
        text: "标签集合",
        concepts: &["set"],
    },
    Term {
        text: "丢进",
        concepts: &["add-member"],
    },
    Term {
        text: "draw a random",
        concepts: &["random"],
    },
    Term {
        text: "随机抽一个",
        concepts: &["random"],
    },
    Term {
        text: "queued up",
        concepts: &["transaction"],
    },
    Term {
        text: "run everything",
        concepts: &["commit"],
    },
    Term {
        text: "drop all",
        concepts: &["cancel"],
    },
    Term {
        text: "fail if",
        concepts: &["optimistic-lock"],
    },
    Term {
        text: "有多少个 1",
        concepts: &["bit", "count"],
    },
    Term {
        text: "custom width",
        concepts: &["bit", "offset"],
    },
    Term {
        text: "counters",
        concepts: &["counter"],
    },
    Term {
        text: "inside a box",
        concepts: &["search", "radius"],
    },
    Term {
        text: "a circle",
        concepts: &["radius"],
    },
    Term {
        text: "points",
        concepts: &["location"],
    },
    Term {
        text: "proximity",
        concepts: &["search", "distance"],
    },
    Term {
        text: "save the result",
        concepts: &["store"],
    },
    Term {
        text: "latitude",
        concepts: &["location"],
    },
    Term {
        text: "longitude",
        concepts: &["location"],
    },
    Term {
        text: "how far apart",
        concepts: &["distance"],
    },
    Term {
        text: "neighbour",
        concepts: &["search", "radius"],
    },
    Term {
        text: "anchored on",
        concepts: &["radius"],
    },
    Term {
        text: "remaining before",
        concepts: &["ttl"],
    },
    Term {
        text: "skip the names",
        concepts: &["list-values"],
    },
    Term {
        text: "cheap",
        concepts: &["approximate"],
    },
    Term {
        text: "unique counter",
        concepts: &["cardinality"],
    },
    Term {
        text: "UV",
        concepts: &["cardinality", "approximate"],
    },
    Term {
        text: "location point",
        concepts: &["location"],
    },
    Term {
        text: "point",
        concepts: &["location"],
    },
    Term {
        text: "proximity lookups",
        concepts: &["search", "distance"],
    },
    Term {
        text: "compact location string",
        concepts: &["encoding", "location"],
    },
    Term {
        text: "within",
        concepts: &["radius"],
    },
    Term {
        text: "km",
        concepts: &["distance", "radius"],
    },
    Term {
        text: "replica safe",
        concepts: &["read-only"],
    },
    Term {
        text: "safe",
        concepts: &["read-only"],
    },
    Term {
        text: "不带写操作",
        concepts: &["read-only"],
    },
    Term {
        text: "area lookup",
        concepts: &["search", "radius"],
    },
    Term {
        text: "area",
        concepts: &["radius", "search"],
    },
    Term {
        text: "area query",
        concepts: &["search", "radius"],
    },
    Term {
        text: "落成新 key",
        concepts: &["store"],
    },
    Term {
        text: "wall clock",
        concepts: &["timestamp"],
    },
    Term {
        text: "stays forever",
        concepts: &["persist"],
    },
    Term {
        text: "stays",
        concepts: &["persist"],
    },
    Term {
        text: "at position",
        concepts: &["index"],
    },
    Term {
        text: "head item",
        concepts: &["head"],
    },
    Term {
        text: "grab the head",
        concepts: &["head", "pop"],
    },
    Term {
        text: "最前面那条",
        concepts: &["head"],
    },
    Term {
        text: "最早进来",
        concepts: &["tail"],
    },
    Term {
        text: "highest-scoring",
        concepts: &["score", "rank"],
    },
    Term {
        text: "smallest-score",
        concepts: &["score", "rank"],
    },
    Term {
        text: "blocking while",
        concepts: &["blocking"],
    },
    Term {
        text: "highest score first",
        concepts: &["reverse", "rank"],
    },
    Term {
        text: "top ranked",
        concepts: &["rank", "reverse"],
    },
    Term {
        text: "从高往低",
        concepts: &["reverse"],
    },
    Term {
        text: "第一个非空",
        concepts: &["multiple"],
    },
    Term {
        text: "分数合并",
        concepts: &["intersection", "score"],
    },
    Term {
        text: "分数相加",
        concepts: &["union", "score"],
    },
    Term {
        text: "存到另一个 key",
        concepts: &["store"],
    },
    Term {
        text: "string-ordered",
        concepts: &["lexicographic"],
    },
    Term {
        text: "interval",
        concepts: &["range"],
    },
    Term {
        text: "不重复的访客",
        concepts: &["cardinality", "member"],
    },
    Term {
        text: "几点钟",
        concepts: &["timestamp"],
    },
    Term {
        text: "还有多久",
        concepts: &["ttl"],
    },
    Term {
        text: "optimistic lock",
        concepts: &["optimistic-lock", "watch", "transaction"],
    },
    Term {
        text: "乐观地锁住",
        concepts: &["optimistic-lock", "watch", "transaction"],
    },
    Term {
        text: "a few members",
        concepts: &["member", "random"],
    },
    Term {
        text: "without taking",
        concepts: &["read-value"],
    },
];

/// What each command is for, most defining concept first.
///
/// Scope is published in `fixtures/assistance/find/README.md`: the data-type and generic
/// groups a person searches for *by purpose*. Module, cluster and server-administration
/// commands are out of scope and say so there, because a coverage boundary nobody wrote down
/// is indistinguishable from a gap.
pub static PURPOSES: &[Purpose] = &[
    Purpose {
        command: "APPEND",
        concepts: &["append", "string"],
    },
    Purpose {
        command: "BITCOUNT",
        concepts: &["bit", "count", "bitmap", "string"],
    },
    Purpose {
        command: "BITFIELD",
        concepts: &["bit", "number", "bitmap", "offset", "string"],
    },
    Purpose {
        command: "BITFIELD_RO",
        concepts: &[
            "read-only",
            "bit",
            "number",
            "bitmap",
            "offset",
            "read-value",
            "string",
        ],
    },
    Purpose {
        command: "BITOP",
        concepts: &[
            "bit",
            "bitmap",
            "intersection",
            "union",
            "difference",
            "multiple",
            "string",
        ],
    },
    Purpose {
        command: "BITPOS",
        concepts: &["bit", "index", "search", "bitmap", "string"],
    },
    Purpose {
        command: "BLMOVE",
        concepts: &["move", "blocking", "list", "queue", "multiple"],
    },
    Purpose {
        command: "BLMPOP",
        concepts: &["pop", "multiple", "blocking", "queue", "list"],
    },
    Purpose {
        command: "BLPOP",
        concepts: &["pop", "head", "blocking", "queue", "list"],
    },
    Purpose {
        command: "BRPOP",
        concepts: &["pop", "tail", "blocking", "queue", "list"],
    },
    Purpose {
        command: "BRPOPLPUSH",
        concepts: &[
            "move",
            "queue",
            "blocking",
            "list",
            "multiple",
            "deprecated",
        ],
    },
    Purpose {
        command: "BZMPOP",
        concepts: &["pop", "blocking", "multiple", "sortedset", "member"],
    },
    Purpose {
        command: "BZPOPMAX",
        concepts: &["pop", "blocking", "score", "sortedset", "member"],
    },
    Purpose {
        command: "BZPOPMIN",
        concepts: &["pop", "blocking", "score", "sortedset", "member"],
    },
    Purpose {
        command: "COPY",
        concepts: &["copy", "key"],
    },
    Purpose {
        command: "DECR",
        concepts: &["decrement", "counter", "number", "string"],
    },
    Purpose {
        command: "DECRBY",
        concepts: &["decrement", "counter", "number", "string"],
    },
    Purpose {
        command: "DEL",
        concepts: &["delete", "key", "blocking"],
    },
    Purpose {
        command: "DISCARD",
        concepts: &["transaction", "cancel", "rollback"],
    },
    Purpose {
        command: "DUMP",
        concepts: &["serialise", "backup", "key"],
    },
    Purpose {
        command: "EXEC",
        concepts: &["transaction", "atomic", "commit"],
    },
    Purpose {
        command: "EXISTS",
        concepts: &["exists", "key"],
    },
    Purpose {
        command: "EXPIRE",
        concepts: &["expire", "ttl", "seconds", "key"],
    },
    Purpose {
        command: "EXPIREAT",
        concepts: &["expire", "ttl", "timestamp", "key"],
    },
    Purpose {
        command: "EXPIRETIME",
        concepts: &["ttl", "timestamp", "key", "read-value"],
    },
    Purpose {
        command: "GEOADD",
        concepts: &["add-member", "location", "geo", "member", "set-value"],
    },
    Purpose {
        command: "GEODIST",
        concepts: &["distance", "location", "geo", "member"],
    },
    Purpose {
        command: "GEOHASH",
        concepts: &["location", "encoding", "geo", "member", "read-value"],
    },
    Purpose {
        command: "GEOPOS",
        concepts: &["location", "geo", "member", "read-value"],
    },
    Purpose {
        command: "GEORADIUS",
        concepts: &["search", "location", "radius", "geo", "deprecated"],
    },
    Purpose {
        command: "GEORADIUSBYMEMBER",
        concepts: &[
            "search",
            "location",
            "radius",
            "member",
            "geo",
            "deprecated",
        ],
    },
    Purpose {
        command: "GEORADIUSBYMEMBER_RO",
        concepts: &[
            "read-only",
            "search",
            "location",
            "radius",
            "member",
            "geo",
            "deprecated",
        ],
    },
    Purpose {
        command: "GEORADIUS_RO",
        concepts: &[
            "read-only",
            "search",
            "location",
            "radius",
            "geo",
            "deprecated",
        ],
    },
    Purpose {
        command: "GEOSEARCH",
        concepts: &["search", "location", "radius", "geo"],
    },
    Purpose {
        command: "GEOSEARCHSTORE",
        concepts: &["search", "radius", "location", "store", "geo"],
    },
    Purpose {
        command: "GET",
        concepts: &["read-value", "string"],
    },
    Purpose {
        command: "GETBIT",
        concepts: &["bit", "read-value", "index", "offset", "bitmap", "string"],
    },
    Purpose {
        command: "GETDEL",
        concepts: &[
            "read-and-delete",
            "read-value",
            "delete",
            "atomic",
            "string",
        ],
    },
    Purpose {
        command: "GETEX",
        concepts: &["read-and-expire", "read-value", "expire", "string"],
    },
    Purpose {
        command: "GETRANGE",
        concepts: &[
            "substring",
            "range",
            "offset",
            "index",
            "string",
            "read-value",
        ],
    },
    Purpose {
        command: "GETSET",
        concepts: &["atomic", "read-value", "set-value", "string", "deprecated"],
    },
    Purpose {
        command: "HDEL",
        concepts: &["delete", "remove", "hash", "field"],
    },
    Purpose {
        command: "HEXISTS",
        concepts: &["exists", "hash", "field"],
    },
    Purpose {
        command: "HEXPIRE",
        concepts: &["expire", "field-ttl", "hash", "seconds", "field"],
    },
    Purpose {
        command: "HEXPIREAT",
        concepts: &["expire", "field-ttl", "timestamp", "hash", "field"],
    },
    Purpose {
        command: "HEXPIRETIME",
        concepts: &[
            "ttl",
            "field-ttl",
            "timestamp",
            "hash",
            "field",
            "read-value",
        ],
    },
    Purpose {
        command: "HGET",
        concepts: &["read-value", "hash", "field"],
    },
    Purpose {
        command: "HGETALL",
        concepts: &["read-all", "hash", "read-value"],
    },
    Purpose {
        command: "HGETDEL",
        concepts: &[
            "read-and-delete",
            "read-value",
            "delete",
            "atomic",
            "hash",
            "field",
        ],
    },
    Purpose {
        command: "HGETEX",
        concepts: &["read-and-expire", "read-value", "expire", "hash", "field"],
    },
    Purpose {
        command: "HINCRBY",
        concepts: &["increment", "counter", "number", "hash", "field"],
    },
    Purpose {
        command: "HINCRBYFLOAT",
        concepts: &["increment", "float", "number", "hash", "field"],
    },
    Purpose {
        command: "HKEYS",
        concepts: &["list-fields", "read-all", "hash", "field", "read-value"],
    },
    Purpose {
        command: "HLEN",
        concepts: &["count", "hash", "field"],
    },
    Purpose {
        command: "HMGET",
        concepts: &["read-value", "multiple", "hash", "field"],
    },
    Purpose {
        command: "HMSET",
        concepts: &["set-value", "multiple", "hash", "deprecated", "field"],
    },
    Purpose {
        command: "HPERSIST",
        concepts: &["persist", "field-ttl", "hash", "field"],
    },
    Purpose {
        command: "HPEXPIRE",
        concepts: &["expire", "field-ttl", "hash", "milliseconds", "field"],
    },
    Purpose {
        command: "HPEXPIREAT",
        concepts: &[
            "expire",
            "field-ttl",
            "timestamp",
            "milliseconds",
            "hash",
            "field",
        ],
    },
    Purpose {
        command: "HPEXPIRETIME",
        concepts: &[
            "ttl",
            "field-ttl",
            "timestamp",
            "milliseconds",
            "hash",
            "field",
            "read-value",
        ],
    },
    Purpose {
        command: "HPTTL",
        concepts: &[
            "ttl",
            "field-ttl",
            "milliseconds",
            "hash",
            "field",
            "read-value",
        ],
    },
    Purpose {
        command: "HRANDFIELD",
        concepts: &["random", "hash", "field", "read-value"],
    },
    Purpose {
        command: "HSCAN",
        concepts: &["scan", "iterate", "hash", "field"],
    },
    Purpose {
        command: "HSET",
        concepts: &["set-value", "hash", "field"],
    },
    Purpose {
        command: "HSETEX",
        concepts: &[
            "set-and-expire",
            "set-value",
            "expire",
            "field-ttl",
            "hash",
            "field",
        ],
    },
    Purpose {
        command: "HSETNX",
        concepts: &["set-value", "set-if-absent", "hash", "conditional", "field"],
    },
    Purpose {
        command: "HSTRLEN",
        concepts: &["length", "hash", "field", "read-value"],
    },
    Purpose {
        command: "HTTL",
        concepts: &["ttl", "field-ttl", "hash", "field", "read-value"],
    },
    Purpose {
        command: "HVALS",
        concepts: &["list-values", "read-all", "hash", "field", "read-value"],
    },
    Purpose {
        command: "INCR",
        concepts: &["increment", "counter", "number", "string"],
    },
    Purpose {
        command: "INCRBY",
        concepts: &["increment", "counter", "number", "string"],
    },
    Purpose {
        command: "INCRBYFLOAT",
        concepts: &["increment", "float", "number", "string"],
    },
    Purpose {
        command: "KEYS",
        concepts: &["search", "pattern", "key", "dangerous", "read-all"],
    },
    Purpose {
        command: "LCS",
        concepts: &["compare", "substring", "multiple", "string"],
    },
    Purpose {
        command: "LINDEX",
        concepts: &["index", "list", "member", "read-value", "queue"],
    },
    Purpose {
        command: "LINSERT",
        concepts: &["insert", "list", "queue", "member"],
    },
    Purpose {
        command: "LLEN",
        concepts: &["count", "list", "queue"],
    },
    Purpose {
        command: "LMOVE",
        concepts: &["move", "list", "atomic", "queue", "multiple"],
    },
    Purpose {
        command: "LMPOP",
        concepts: &["pop", "multiple", "list", "queue", "member"],
    },
    Purpose {
        command: "LPOP",
        concepts: &["pop", "head", "list", "queue", "member"],
    },
    Purpose {
        command: "LPOS",
        concepts: &["search", "index", "list", "queue", "member"],
    },
    Purpose {
        command: "LPUSH",
        concepts: &["push", "head", "list", "queue", "member"],
    },
    Purpose {
        command: "LPUSHX",
        concepts: &["push", "head", "list", "conditional", "queue"],
    },
    Purpose {
        command: "LRANGE",
        concepts: &["range", "index", "list", "read-value", "queue", "member"],
    },
    Purpose {
        command: "LREM",
        concepts: &["remove", "member", "list", "queue"],
    },
    Purpose {
        command: "LSET",
        concepts: &["index", "set-value", "list", "member", "queue"],
    },
    Purpose {
        command: "LTRIM",
        concepts: &["trim", "range", "list", "queue"],
    },
    Purpose {
        command: "MGET",
        concepts: &["read-value", "multiple", "string"],
    },
    Purpose {
        command: "MIGRATE",
        concepts: &["migrate", "move", "cluster", "key"],
    },
    Purpose {
        command: "MOVE",
        concepts: &["move", "database", "key"],
    },
    Purpose {
        command: "MSET",
        concepts: &["set-value", "multiple", "string"],
    },
    Purpose {
        command: "MSETNX",
        concepts: &["set-if-absent", "multiple", "string", "conditional"],
    },
    Purpose {
        command: "MULTI",
        concepts: &["transaction", "atomic", "batch"],
    },
    Purpose {
        command: "OBJECT",
        concepts: &["introspect", "key"],
    },
    Purpose {
        command: "OBJECT|ENCODING",
        concepts: &["introspect", "encoding", "memory", "key"],
    },
    Purpose {
        command: "OBJECT|FREQ",
        concepts: &["introspect", "frequency", "eviction", "key"],
    },
    Purpose {
        command: "OBJECT|IDLETIME",
        concepts: &["idle", "introspect", "eviction", "key"],
    },
    Purpose {
        command: "OBJECT|REFCOUNT",
        concepts: &["memory", "introspect", "key"],
    },
    Purpose {
        command: "PERSIST",
        concepts: &["persist", "ttl", "key"],
    },
    Purpose {
        command: "PEXPIRE",
        concepts: &["expire", "ttl", "milliseconds", "key"],
    },
    Purpose {
        command: "PEXPIREAT",
        concepts: &["expire", "ttl", "timestamp", "milliseconds", "key"],
    },
    Purpose {
        command: "PEXPIRETIME",
        concepts: &["ttl", "timestamp", "milliseconds", "key", "read-value"],
    },
    Purpose {
        command: "PFADD",
        concepts: &[
            "add-member",
            "cardinality",
            "approximate",
            "hyperloglog",
            "set-value",
        ],
    },
    Purpose {
        command: "PFCOUNT",
        concepts: &["count", "cardinality", "approximate", "hyperloglog"],
    },
    Purpose {
        command: "PFMERGE",
        concepts: &[
            "union",
            "cardinality",
            "approximate",
            "hyperloglog",
            "multiple",
        ],
    },
    Purpose {
        command: "PSETEX",
        concepts: &[
            "set-and-expire",
            "expire",
            "set-value",
            "milliseconds",
            "string",
        ],
    },
    Purpose {
        command: "PSUBSCRIBE",
        concepts: &["subscribe", "pattern", "channel", "pubsub"],
    },
    Purpose {
        command: "PTTL",
        concepts: &["ttl", "milliseconds", "key", "read-value"],
    },
    Purpose {
        command: "PUBLISH",
        concepts: &["publish", "channel", "pubsub"],
    },
    Purpose {
        command: "PUBSUB",
        concepts: &["introspect", "pubsub", "subscribe"],
    },
    Purpose {
        command: "PUBSUB|CHANNELS",
        concepts: &["introspect", "channel", "subscribe", "pubsub"],
    },
    Purpose {
        command: "PUBSUB|NUMPAT",
        concepts: &["count", "introspect", "pattern", "subscribe", "pubsub"],
    },
    Purpose {
        command: "PUBSUB|NUMSUB",
        concepts: &["count", "introspect", "channel", "subscribe", "pubsub"],
    },
    Purpose {
        command: "PUBSUB|SHARDCHANNELS",
        concepts: &[
            "introspect",
            "shard",
            "channel",
            "subscribe",
            "cluster",
            "pubsub",
        ],
    },
    Purpose {
        command: "PUBSUB|SHARDNUMSUB",
        concepts: &[
            "count",
            "introspect",
            "shard",
            "channel",
            "subscribe",
            "cluster",
            "pubsub",
        ],
    },
    Purpose {
        command: "PUNSUBSCRIBE",
        concepts: &["unsubscribe", "pattern", "channel", "pubsub"],
    },
    Purpose {
        command: "RANDOMKEY",
        concepts: &["random", "key"],
    },
    Purpose {
        command: "RENAME",
        concepts: &["rename", "key"],
    },
    Purpose {
        command: "RENAMENX",
        concepts: &["rename", "set-if-absent", "key", "conditional"],
    },
    Purpose {
        command: "RESTORE",
        concepts: &["deserialise", "backup", "key"],
    },
    Purpose {
        command: "RPOP",
        concepts: &["pop", "tail", "list", "queue", "member"],
    },
    Purpose {
        command: "RPOPLPUSH",
        concepts: &["move", "queue", "list", "atomic", "multiple", "deprecated"],
    },
    Purpose {
        command: "RPUSH",
        concepts: &["push", "tail", "append", "list", "queue", "member"],
    },
    Purpose {
        command: "RPUSHX",
        concepts: &["push", "tail", "append", "list", "conditional", "queue"],
    },
    Purpose {
        command: "SADD",
        concepts: &["add-member", "set", "member", "set-value"],
    },
    Purpose {
        command: "SCAN",
        concepts: &["scan", "key", "iterate", "pattern"],
    },
    Purpose {
        command: "SCARD",
        concepts: &["count", "set", "member"],
    },
    Purpose {
        command: "SDIFF",
        concepts: &["difference", "set", "member"],
    },
    Purpose {
        command: "SDIFFSTORE",
        concepts: &["difference", "store", "set"],
    },
    Purpose {
        command: "SET",
        concepts: &["set-value", "string", "conditional", "expire"],
    },
    Purpose {
        command: "SETBIT",
        concepts: &["bit", "set-value", "index", "offset", "bitmap", "string"],
    },
    Purpose {
        command: "SETEX",
        concepts: &["set-and-expire", "expire", "set-value", "seconds", "string"],
    },
    Purpose {
        command: "SETNX",
        concepts: &["set-if-absent", "conditional", "set-value", "string"],
    },
    Purpose {
        command: "SETRANGE",
        concepts: &["substring", "offset", "string", "set-value"],
    },
    Purpose {
        command: "SINTER",
        concepts: &["intersection", "set", "member"],
    },
    Purpose {
        command: "SINTERCARD",
        concepts: &["intersection", "count", "set", "member"],
    },
    Purpose {
        command: "SINTERSTORE",
        concepts: &["intersection", "store", "set"],
    },
    Purpose {
        command: "SISMEMBER",
        concepts: &["exists", "member", "set"],
    },
    Purpose {
        command: "SMEMBERS",
        concepts: &["read-all", "set", "member", "read-value"],
    },
    Purpose {
        command: "SMISMEMBER",
        concepts: &["exists", "member", "multiple", "set"],
    },
    Purpose {
        command: "SMOVE",
        concepts: &["move", "set", "atomic", "member", "multiple"],
    },
    Purpose {
        command: "SORT",
        concepts: &["sort", "list", "set", "sortedset"],
    },
    Purpose {
        command: "SORT_RO",
        concepts: &["read-only", "sort", "list", "set", "sortedset"],
    },
    Purpose {
        command: "SPOP",
        concepts: &["pop", "random", "set", "member"],
    },
    Purpose {
        command: "SPUBLISH",
        concepts: &["publish", "shard", "channel", "cluster", "pubsub"],
    },
    Purpose {
        command: "SRANDMEMBER",
        concepts: &["random", "set", "member"],
    },
    Purpose {
        command: "SREM",
        concepts: &["remove", "member", "set"],
    },
    Purpose {
        command: "SSCAN",
        concepts: &["scan", "iterate", "set"],
    },
    Purpose {
        command: "SSUBSCRIBE",
        concepts: &["subscribe", "shard", "channel", "cluster", "pubsub"],
    },
    Purpose {
        command: "STRLEN",
        concepts: &["length", "string"],
    },
    Purpose {
        command: "SUBSCRIBE",
        concepts: &["subscribe", "channel", "pubsub"],
    },
    Purpose {
        command: "SUBSTR",
        concepts: &[
            "substring",
            "range",
            "offset",
            "string",
            "deprecated",
            "read-value",
        ],
    },
    Purpose {
        command: "SUNION",
        concepts: &["union", "set", "member"],
    },
    Purpose {
        command: "SUNIONSTORE",
        concepts: &["union", "store", "set"],
    },
    Purpose {
        command: "SUNSUBSCRIBE",
        concepts: &["unsubscribe", "shard", "channel", "cluster", "pubsub"],
    },
    Purpose {
        command: "TOUCH",
        concepts: &["touch", "idle", "key"],
    },
    Purpose {
        command: "TTL",
        concepts: &["ttl", "seconds", "key", "read-value"],
    },
    Purpose {
        command: "TYPE",
        concepts: &["type-of", "key"],
    },
    Purpose {
        command: "UNLINK",
        concepts: &["delete", "key", "async"],
    },
    Purpose {
        command: "UNSUBSCRIBE",
        concepts: &["unsubscribe", "channel", "pubsub"],
    },
    Purpose {
        command: "UNWATCH",
        concepts: &["watch", "cancel", "optimistic-lock", "transaction"],
    },
    Purpose {
        command: "WAIT",
        concepts: &["replication", "durability", "acknowledge"],
    },
    Purpose {
        command: "WAITAOF",
        concepts: &["persistence", "durability", "replication", "acknowledge"],
    },
    Purpose {
        command: "WATCH",
        concepts: &["watch", "optimistic-lock", "transaction"],
    },
    Purpose {
        command: "ZADD",
        concepts: &[
            "add-member",
            "score",
            "sortedset",
            "conditional",
            "member",
            "set-value",
            "insert",
        ],
    },
    Purpose {
        command: "ZCARD",
        concepts: &["count", "sortedset", "member"],
    },
    Purpose {
        command: "ZCOUNT",
        concepts: &["count", "score", "range", "sortedset", "member"],
    },
    Purpose {
        command: "ZDIFF",
        concepts: &["difference", "sortedset", "member"],
    },
    Purpose {
        command: "ZDIFFSTORE",
        concepts: &["difference", "store", "sortedset"],
    },
    Purpose {
        command: "ZINCRBY",
        concepts: &["increment", "score", "sortedset", "member"],
    },
    Purpose {
        command: "ZINTER",
        concepts: &["intersection", "sortedset", "member"],
    },
    Purpose {
        command: "ZINTERCARD",
        concepts: &["intersection", "count", "sortedset", "member"],
    },
    Purpose {
        command: "ZINTERSTORE",
        concepts: &["intersection", "store", "sortedset"],
    },
    Purpose {
        command: "ZLEXCOUNT",
        concepts: &["count", "lexicographic", "range", "sortedset"],
    },
    Purpose {
        command: "ZMPOP",
        concepts: &["pop", "multiple", "sortedset", "member"],
    },
    Purpose {
        command: "ZMSCORE",
        concepts: &["score", "multiple", "sortedset", "member", "read-value"],
    },
    Purpose {
        command: "ZPOPMAX",
        concepts: &["pop", "score", "sortedset", "member"],
    },
    Purpose {
        command: "ZPOPMIN",
        concepts: &["pop", "score", "sortedset", "member"],
    },
    Purpose {
        command: "ZRANDMEMBER",
        concepts: &["random", "sortedset", "member"],
    },
    Purpose {
        command: "ZRANGE",
        concepts: &[
            "range",
            "rank",
            "leaderboard",
            "sortedset",
            "member",
            "read-value",
        ],
    },
    Purpose {
        command: "ZRANGEBYLEX",
        concepts: &[
            "range",
            "lexicographic",
            "sortedset",
            "deprecated",
            "member",
        ],
    },
    Purpose {
        command: "ZRANGEBYSCORE",
        concepts: &["range", "score", "sortedset", "deprecated", "member"],
    },
    Purpose {
        command: "ZRANGESTORE",
        concepts: &["range", "store", "sortedset"],
    },
    Purpose {
        command: "ZRANK",
        concepts: &["rank", "leaderboard", "sortedset", "member"],
    },
    Purpose {
        command: "ZREM",
        concepts: &["remove", "member", "sortedset"],
    },
    Purpose {
        command: "ZREMRANGEBYLEX",
        concepts: &["remove", "lexicographic", "range", "sortedset", "member"],
    },
    Purpose {
        command: "ZREMRANGEBYRANK",
        concepts: &["remove", "rank", "range", "sortedset", "member"],
    },
    Purpose {
        command: "ZREMRANGEBYSCORE",
        concepts: &["remove", "score", "range", "sortedset", "member"],
    },
    Purpose {
        command: "ZREVRANGE",
        concepts: &[
            "range",
            "rank",
            "reverse",
            "leaderboard",
            "sortedset",
            "member",
            "read-value",
            "deprecated",
        ],
    },
    Purpose {
        command: "ZREVRANGEBYLEX",
        concepts: &[
            "range",
            "lexicographic",
            "reverse",
            "sortedset",
            "deprecated",
            "member",
        ],
    },
    Purpose {
        command: "ZREVRANGEBYSCORE",
        concepts: &[
            "range",
            "score",
            "reverse",
            "sortedset",
            "deprecated",
            "member",
        ],
    },
    Purpose {
        command: "ZREVRANK",
        concepts: &["rank", "reverse", "leaderboard", "sortedset", "member"],
    },
    Purpose {
        command: "ZSCAN",
        concepts: &["scan", "iterate", "sortedset"],
    },
    Purpose {
        command: "ZSCORE",
        concepts: &["score", "sortedset", "member", "read-value"],
    },
    Purpose {
        command: "ZUNION",
        concepts: &["union", "sortedset", "member"],
    },
    Purpose {
        command: "ZUNIONSTORE",
        concepts: &["union", "store", "sortedset"],
    },
];

/// Compound meanings: when a query means more than the sum of its words.
///
/// 「去掉 key 的过期时间」 is `remove` plus `expire`, and it means `persist` — a concept neither
/// word produces alone. The alternative is a term for every compound phrasing, which is a
/// vocabulary that grows with the corpus rather than with the language.
pub static IMPLIES: &[crate::find::Implication] = &[
    (&["remove", "expire"], "persist", false),
    (&["delete", "expire"], "persist", false),
    (&["read-value", "delete"], "delete", false),
    (&["count", "member"], "count", false),
    (&["search", "location"], "radius", false),
    (&["read-all", "hash"], "read-all", false),
    (&["set-value", "expire"], "expire", false),
    (&["set-value", "expire"], "field-ttl", false),
    (&["ttl", "hash"], "field-ttl", false),
    (&["expire", "hash"], "field-ttl", false),
    (&["persist", "hash"], "field-ttl", false),
    (&["sort", "read"], "sort", false),
    (&["read-value", "expire"], "read-and-expire", false),
    (&["read-value", "delete"], "read-and-delete", false),
    (&["set-value", "expire"], "set-and-expire", false),
    (&["cancel", "subscribe"], "unsubscribe", false),
    (&["pop", "push"], "move", false),
    (&["pop", "move"], "move", false),
    (&["pop", "push"], "move", true),
    (&["set-value", "ttl"], "set-and-expire", false),
    (&["set-value", "ttl"], "expire", false),
    (&["read-value", "ttl"], "read-and-expire", false),
    // Family implications: these say what a word *is about*, not what it does.
    //
    // A score belongs to a sorted set; there is nowhere else in Redis to put one. A field
    // belongs to a hash, a channel to pub/sub, a queue is a list. Without these, 「按分数区间
    // 取一段」 produced {range, score, substring} and no family at all, so `CONTAINERS` never
    // engaged and `GETRANGE` — a string command that is genuinely about ranges — outranked every
    // sorted-set answer. The family filter can only help when the query says which family it is
    // in, and a person who says "score" has said so.
    (&["score"], "sortedset", false),
    (&["field"], "hash", false),
    (&["channel"], "pubsub", false),
    (&["queue"], "list", false),
    // Removing a *key* is deleting it. `remove` and `delete` are deliberately different
    // concepts — `SREM` removes a member, `DEL` deletes a key — but "remove keys from the
    // database" is the second one said with the first one's verb, and it used to answer
    // `MOVE`.
    (&["remove", "key"], "delete", false),
    // And the mirror image: "delete these members" is a removal, not a `DEL`.
    (&["delete", "member"], "remove", false),
    // "Remove and return the highest scoring member" is a pop said the long way round.
    (&["remove", "read-value"], "pop", false),
];
