//! Purpose search: "what do I use to …" (v2.1 §16.4, §13, R10, ASSIST-050/051, V-F08).
//!
//! A user knows what they want to *do*; they do not know that the command is called `PEXPIRE`.
//! This maps a phrase, in Chinese or English, onto commands.
//!
//! ## What it must not be
//!
//! **No network** (ADR-020: 「Find / Guide / Learn / 单位助手 / 错误帮助默认零联网」). Nothing
//! here can reach a server, and `pr-intelligence` has no transport dependency at all, so that
//! is a property of the crate graph rather than a rule someone remembers.
//!
//! **No upstream prose.** V-I03 concluded the catalog carries interface structure and not
//! somebody else's English, so `CommandSpec::summary` is empty. The vocabulary here is written
//! for this repository, which is the whole reason V-F08 has to measure whether it is any good.
//!
//! **Never an executor.** ASSIST-051: a query like 「清 cache」 must not produce a `FLUSHDB`
//! that then runs. Search returns candidates for a person to read; §10 keeps acceptance to the
//! input buffer.
//!
//! ## Two corpora, and why the second one is the honest one
//!
//! §16.4 gives this feature two metrics, not one:
//!
//! | corpus | threshold | what it proves |
//! |---|---|---|
//! | canonical | top-3 recall ≥ 95% | the vocabulary is **complete** — and no more, because the vocabulary is built from this corpus |
//! | held-out | top-3 ≥ 80%, top-5 ≥ 90% | it **generalises** to phrasings that did not build it |
//!
//! Measuring only the first is measuring whether a lookup table contains its own contents.

// Scores are ratios of small counts. `f64` cannot lose precision at these magnitudes, and
// integer arithmetic on scaled fixed point would make the ranking formula unreadable for no
// gain.
#![allow(clippy::cast_precision_loss)]

use std::collections::{BTreeMap, BTreeSet};

/// One concept a phrase can be about, e.g. "expire" or "read".
///
/// Concepts rather than raw synonyms, so that a command declares what it is *for* once and
/// every phrasing that means that reaches it. Adding a Chinese colloquialism for "delete"
/// should not require touching every deleting command.
pub type Concept = &'static str;

/// A term that maps onto concepts.
///
/// Terms are matched case-insensitively for ASCII and by substring for CJK, because there is
/// no word segmenter here and a CJK phrase has no spaces. That is not a compromise: for a
/// vocabulary of known terms, substring matching over CJK is what a segmenter would produce
/// anyway, without a dictionary dependency that would then need its own provenance.
#[derive(Clone, Copy, Debug)]
pub struct Term {
    /// The text a user might type.
    pub text: &'static str,
    /// What it means.
    pub concepts: &'static [Concept],
}

/// What a command is for.
#[derive(Clone, Copy, Debug)]
pub struct Purpose {
    /// Command name, uppercase, as the catalog spells it.
    pub command: &'static str,
    /// Concepts this command serves, most defining first.
    pub concepts: &'static [Concept],
}

/// The concept that marks a command as superseded.
///
/// Not matchable — no phrasing means "deprecated" — but it changes the ranking. Redis keeps
/// `GEORADIUS`, `GEORADIUS_RO`, `GEORADIUSBYMEMBER` and `GEORADIUSBYMEMBER_RO` alongside
/// `GEOSEARCH`, and all five serve the same concepts. Without this, the four superseded ones
/// crowd `GEOSEARCH` out of the top three for every phrasing that means "find things nearby",
/// which is the opposite of helpful.
pub const DEPRECATED: Concept = "deprecated";

/// The concepts that name a *kind of thing* rather than an action.
///
/// When a query names one — 「哈希」, 「队列」, `sorted set` — it is not asking for a hash-flavoured
/// answer, it is saying which family the answer lives in. Treating that as one more concept to
/// score made 「数一数哈希有几个字段」 compete against every command that counts anything.
///
/// Not a hard filter: a query may name no container at all, and a command may serve several.
/// `key` is deliberately absent: it is not a family but "any key at all", claimed by every
/// generic command, so treating it as one penalised every data-type command for the word 「键」.
pub const CONTAINERS: &[Concept] = &[
    "string",
    "hash",
    "list",
    "set",
    "sortedset",
    "bitmap",
    "geo",
    "hyperloglog",
    "pubsub",
    "transaction",
];

/// When a query means more than the sum of its words.
///
/// 「去掉 key 的过期时间」 is `remove` plus `expire`, and what it *means* is `persist` — a
/// concept neither word produces on its own. A flat bag of concepts cannot express that, and
/// the alternative (a term for every compound phrasing) is a vocabulary that grows with the
/// corpus instead of with the language.
///
/// Read as: when every concept on the left is present, the one on the right is too.
///
/// The `bool` says whether the left-hand concepts are then *removed*. "Pop from one list and
/// push onto another" is one operation, not a pop and a push, and leaving `pop` and `push` in
/// the query made `LPUSH` a better answer than `LMOVE` to a question about moving.
pub type Implication = (&'static [Concept], Concept, bool);

/// A scored result.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Command name.
    pub command: String,
    /// Score; higher is better. Only the order is meaningful.
    pub score: f64,
    /// Which concepts matched, for the UI to explain itself with.
    pub matched: Vec<String>,
}

/// The search index.
///
/// Built once from the vocabulary and the purpose table, both of which are `const` data in
/// this crate. There is no learned state and no file to load: the same query gives the same
/// answer in every process, which is what makes the recall numbers reproducible.
#[derive(Debug)]
pub struct Finder {
    /// concept -> commands that serve it, with the rank the command gave it.
    by_concept: BTreeMap<Concept, Vec<(&'static str, usize)>>,
    /// Commands that are superseded, and rank below their replacement.
    superseded: BTreeSet<&'static str>,
    /// Which families each command belongs to.
    containers_of: BTreeMap<&'static str, Vec<Concept>>,
    /// Rank-weighted total each command could score, for the coverage term.
    ///
    /// Rank-weighted rather than a count: a command that names a fifth, incidental concept
    /// should not be punished for the honesty. With a plain count, adding `queue` to `LPOP`
    /// made `LPOP` a worse answer to every question about popping.
    weight_of: BTreeMap<&'static str, f64>,
    /// Every term, longest first, so that a longer phrase wins over a substring of itself.
    terms: Vec<Term>,
    /// How many commands are indexed.
    commands: usize,
}

impl Finder {
    /// Build from the built-in vocabulary and purpose table.
    #[must_use]
    pub fn new() -> Self {
        Self::from_parts(crate::vocabulary::TERMS, crate::vocabulary::PURPOSES)
    }

    /// The implications this index applies.
    #[must_use]
    fn implications() -> &'static [Implication] {
        crate::vocabulary::IMPLIES
    }

    /// Build from explicit tables, for tests that need a small index.
    #[must_use]
    pub fn from_parts(terms: &'static [Term], purposes: &'static [Purpose]) -> Self {
        let mut by_concept: BTreeMap<Concept, Vec<(&'static str, usize)>> = BTreeMap::new();
        let mut superseded = BTreeSet::new();
        let mut weight_of: BTreeMap<&'static str, f64> = BTreeMap::new();
        let mut containers_of: BTreeMap<&'static str, Vec<Concept>> = BTreeMap::new();
        for p in purposes {
            let mut rank = 0usize;
            for c in p.concepts {
                if *c == DEPRECATED {
                    superseded.insert(p.command);
                    continue;
                }
                by_concept.entry(c).or_default().push((p.command, rank));
                *weight_of.entry(p.command).or_insert(0.0) += 1.0 / (1.0 + rank as f64);
                if CONTAINERS.contains(c) {
                    containers_of.entry(p.command).or_default().push(c);
                }
                rank += 1;
            }
        }
        let mut terms: Vec<Term> = terms.to_vec();
        // Longest first: `过期时间` must win over `过期`, and `sorted set` over `set`.
        terms.sort_by_key(|t| std::cmp::Reverse(t.text.chars().count()));
        Self {
            by_concept,
            superseded,
            weight_of,
            containers_of,
            terms,
            commands: purposes.len(),
        }
    }

    /// How many commands the index covers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands
    }

    /// Whether the index covers nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands == 0
    }

    /// The concepts a query mentions, with the terms that produced them.
    ///
    /// Exposed because V-F08's held-out rule — 「任何一条进入词表即从该集移除」 — needs to ask
    /// whether a phrase is already entirely covered by the vocabulary, and that question is
    /// about the terms, not the results.
    #[must_use]
    pub fn concepts_of(&self, query: &str) -> BTreeSet<Concept> {
        let mut found = BTreeSet::new();
        let mut consumed = vec![false; query.chars().count()];
        let lower = query.to_lowercase();
        let chars: Vec<char> = lower.chars().collect();

        for t in &self.terms {
            let term: Vec<char> = t.text.to_lowercase().chars().collect();
            if term.is_empty() || term.len() > chars.len() {
                continue;
            }
            let mut at = 0;
            while at + term.len() <= chars.len() {
                if chars[at..at + term.len()] != term[..]
                    || consumed[at..at + term.len()].iter().any(|c| *c)
                {
                    at += 1;
                    continue;
                }
                // An English plural is the same word. Without this, "keys", "several sets"
                // and "sorted sets" all miss terms that match their singular, and the corpus
                // is full of them because that is how people write. One trailing `s` or `es`,
                // and only when what follows is a boundary — so `set` still does not match
                // inside `offset`.
                let mut len = term.len();
                for extra in [0usize, 1, 2] {
                    let end = at + term.len() + extra;
                    if end > chars.len() {
                        continue;
                    }
                    let suffix: String = chars[at + term.len()..end].iter().collect();
                    let plural = matches!(suffix.as_str(), "" | "s" | "es");
                    if plural
                        && !consumed[at..end].iter().any(|c| *c)
                        && is_word_boundary(&chars, at, end - at)
                    {
                        len = end - at;
                    }
                }
                if !is_word_boundary(&chars, at, len) {
                    at += 1;
                    continue;
                }
                for c in &mut consumed[at..at + len] {
                    *c = true;
                }
                for c in t.concepts {
                    found.insert(*c);
                }
                at += 1;
            }
        }
        found
    }

    /// Search, returning at most `limit` commands, best first.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let mut concepts = self.concepts_of(query);
        if concepts.is_empty() {
            return Vec::new();
        }
        // Apply implications to a fixed point, so a rule may depend on one another rule
        // produced. Bounded by the number of rules, which is small and static.
        for _ in 0..4 {
            let mut added = false;
            for (needs, then, consumes) in Self::implications() {
                if !concepts.contains(then) && needs.iter().all(|n| concepts.contains(n)) {
                    concepts.insert(then);
                    if *consumes {
                        for n in *needs {
                            concepts.remove(n);
                        }
                    }
                    added = true;
                }
            }
            if !added {
                break;
            }
        }

        let query_size = concepts.len();
        let wants_superseded = concepts.contains(&DEPRECATED);
        let asked_containers: BTreeSet<Concept> = CONTAINERS
            .iter()
            .copied()
            .filter(|c| concepts.contains(c))
            .collect();
        // Per command: (weighted score, matched concepts, weight of what matched).
        let mut scores: BTreeMap<&'static str, (f64, Vec<String>, f64)> = BTreeMap::new();
        for c in &concepts {
            let Some(commands) = self.by_concept.get(c) else {
                continue;
            };
            // A concept that half the catalog serves says much less than one only two commands
            // serve. Inverse frequency, so 「哈希」 narrows and 「redis」 would not.
            let rarity = 1.0 / (commands.len() as f64).sqrt();
            for (cmd, rank) in commands {
                // A command's first-listed concept is what it is *for*; later ones are things
                // it also does. `EXPIRE` is for expiry and incidentally about keys.
                let position = 1.0 / (1.0 + *rank as f64);
                let e = scores.entry(cmd).or_insert((0.0, Vec::new(), 0.0));
                e.0 += rarity * position;
                e.1.push((*c).to_owned());
                e.2 += position;
            }
        }

        let mut hits: Vec<Hit> = scores
            .into_iter()
            .map(|(command, (score, matched, matched_weight))| {
                // Two coverage terms, and they answer different questions.
                //
                // *How much of what was asked did this command cover?* A query meaning
                // {bitmap, count} is better served by something that is about both than by
                // something that is only about counting.
                let asked = matched.len() as f64 / query_size as f64;
                // *How much of what this command is for did the query name?* `BITCOUNT` is
                // about bits and counting and bitmaps; `HLEN` is about counting and hashes.
                // For 「数一数位图里有多少个 1」 the first is mostly what was asked for and the
                // second is mostly not, and without this term the common concept wins purely
                // by being common.
                //
                // Square-rooted so a narrowly-defined command does not beat a well-matched
                // broader one outright; it is a tiebreaker with weight, not an override.
                let mine =
                    (matched_weight / self.weight_of.get(command).copied().unwrap_or(1.0)).sqrt();
                // And a superseded command ranks below its replacement, which serves the same
                // concepts. 0.7 rather than something decisive: `GEORADIUSBYMEMBER` really is
                // the better answer for 「以成员为中心搜附近」 even though it is deprecated, so
                // this must break ties without overruling specificity.
                // ...unless the query asked for the old one. 「旧的按分取」 and "legacy range by
                // score" are people who know `ZRANGE` exists and want the other thing.
                let currency = if self.superseded.contains(command) && !wants_superseded {
                    0.7
                } else {
                    1.0
                };
                // If the query named a family, answers from a *different* family are almost
                // certainly wrong — 「哈希」 does not want a list command — while answers from no
                // particular family (`DEL`, `EXPIRE`) are still plausible.
                let family = if asked_containers.is_empty() {
                    1.0
                } else {
                    let empty: Vec<Concept> = Vec::new();
                    let mine_containers = self.containers_of.get(command).unwrap_or(&empty);
                    if mine_containers.is_empty() {
                        0.6
                    } else if mine_containers.iter().any(|c| asked_containers.contains(c)) {
                        1.0
                    } else {
                        0.2
                    }
                };
                Hit {
                    command: command.to_owned(),
                    score: score * asked * mine * currency * family,
                    matched,
                }
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Ties break by name, so the order is total and the recall numbers do not
                // depend on hash iteration order.
                .then_with(|| a.command.cmp(&b.command))
        });
        hits.truncate(limit);
        hits
    }
}

impl Default for Finder {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a match at `at` is at a word boundary.
///
/// ASCII needs one (so `set` does not match inside `offset`); CJK does not have spaces, so a
/// CJK term matches anywhere. Mixed-script queries are common — 「用 SET 设置」 — so the rule is
/// per-character rather than per-query.
fn is_word_boundary(chars: &[char], at: usize, len: usize) -> bool {
    let first = chars[at];
    let last = chars[at + len - 1];
    let start_ok =
        !first.is_ascii_alphanumeric() || at == 0 || !chars[at - 1].is_ascii_alphanumeric();
    let end_ok = !last.is_ascii_alphanumeric()
        || at + len == chars.len()
        || !chars[at + len].is_ascii_alphanumeric();
    start_ok && end_ok
}
