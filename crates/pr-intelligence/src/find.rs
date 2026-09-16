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
    /// One entry per indexed phrase: which command it belongs to, and how many grams it has.
    ///
    /// Grams are stored as 32-bit hashes, not as `String`s. The corpus is 2,285 phrases and
    /// roughly 35,000 gram occurrences; keeping those as owned strings costs several megabytes
    /// against an assistance budget of twelve (§24.6, ADR-029), and nothing here needs to read
    /// a gram back — only to ask whether two phrases share one. A 32-bit collision perturbs one
    /// score slightly and is worth several megabytes.
    phrases: Vec<(u16, u16)>,
    /// Inverted index: gram -> the phrases that contain it. Scoring touches only the phrases
    /// that share something with the query rather than all of them.
    postings: BTreeMap<u32, Vec<u32>>,
    /// Command names, indexed by the `u16` in `phrases`.
    phrase_commands: Vec<&'static str>,
}

impl Finder {
    /// Build from the built-in vocabulary and purpose table.
    #[must_use]
    pub fn new() -> Self {
        Self::from_parts(crate::vocabulary::TERMS, crate::vocabulary::PURPOSES)
            .with_phrases(crate::phrases::PHRASES)
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
            phrases: Vec::new(),
            postings: BTreeMap::new(),
            phrase_commands: Vec::new(),
        }
    }

    /// Add the canonical corpus as a second, lexical way to reach a command.
    ///
    /// Two independent measurements say why this exists. Extending the synonym table from one
    /// held-out corpus was worth +19.2 points on that corpus and +2.3 points on the next one;
    /// the table had learnt one author's wording, not the language. Matching against the corpus
    /// itself generalises along an axis a word list cannot: shared content words and shared
    /// character runs. `scoring` reaches `score` and 「按分数」 reaches 「分数」 with nothing
    /// enumerating either.
    ///
    /// It is a *second* route, not a replacement. The concept layer knows things no string
    /// comparison can — that a score belongs to a sorted set, that removing a key is deleting
    /// it — and the two are combined by rank rather than by score so neither has to be
    /// calibrated against the other.
    #[must_use]
    pub fn with_phrases(mut self, phrases: &'static [(&'static str, &'static str)]) -> Self {
        let mut commands: Vec<&'static str> = Vec::new();
        let mut index_of: BTreeMap<&'static str, u16> = BTreeMap::new();
        let mut postings: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let mut rows: Vec<(u16, u16)> = Vec::new();
        for (cmd, phrase) in phrases {
            let next = u16::try_from(commands.len()).unwrap_or(u16::MAX);
            let cid = *index_of.entry(cmd).or_insert_with(|| {
                commands.push(*cmd);
                next
            });
            let g = grams(phrase);
            let pid = u32::try_from(rows.len()).unwrap_or(u32::MAX);
            for k in &g {
                postings.entry(*k).or_default().push(pid);
            }
            rows.push((cid, u16::try_from(g.len()).unwrap_or(u16::MAX)));
        }
        self.phrases = rows;
        self.postings = postings;
        self.phrase_commands = commands;
        self
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
    ///
    /// Two rankings, fused. See [`Finder::with_phrases`] for why there are two.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let by_concept = self.rank_by_concept(query);
        let by_phrase = self.rank_by_phrase(query);
        if by_phrase.is_empty() {
            let mut hits = by_concept;
            hits.truncate(limit);
            return hits;
        }
        if by_concept.is_empty() {
            let mut hits = by_phrase;
            hits.truncate(limit);
            return hits;
        }
        fuse(&by_concept, &by_phrase, limit)
    }

    /// The lexical ranking: how much of the query's vocabulary a command's canonical phrasings
    /// already contain, weighted so a gram half the corpus uses counts for little.
    fn rank_by_phrase(&self, query: &str) -> Vec<Hit> {
        if self.phrases.is_empty() {
            return Vec::new();
        }
        let q = grams(query);
        if q.is_empty() {
            return Vec::new();
        }
        let n = self.phrases.len() as f64;
        // Inverse document frequency over the corpus, so 「的」 and "the key" weigh almost
        // nothing while 「位域」 weighs a lot. Without it the longest phrases win every query.
        let mut matched: BTreeMap<u32, f64> = BTreeMap::new();
        let mut query_weight = 0.0;
        for k in &q {
            let posting = self.postings.get(k);
            let df = posting.map_or(0, Vec::len) as f64;
            let idf = (n / (1.0 + df)).ln().max(0.0);
            query_weight += idf;
            if let Some(ps) = posting {
                for pid in ps {
                    *matched.entry(*pid).or_insert(0.0) += idf;
                }
            }
        }
        if query_weight <= 0.0 {
            return Vec::new();
        }
        let mut best: BTreeMap<&'static str, f64> = BTreeMap::new();
        for (pid, num) in matched {
            let Some((cid, len)) = self.phrases.get(pid as usize).copied() else {
                continue;
            };
            let Some(cmd) = self.phrase_commands.get(cid as usize).copied() else {
                continue;
            };
            // Divided by the phrase's own size as well, so a long phrase that happens to
            // contain the query's words does not beat a short one that is about them.
            let score = num / query_weight * (num / (num + f64::from(len).sqrt()));
            let e = best.entry(cmd).or_insert(0.0);
            if score > *e {
                *e = score;
            }
        }
        let mut hits: Vec<Hit> = best
            .into_iter()
            .map(|(command, score)| Hit {
                command: command.to_owned(),
                score,
                matched: Vec::new(),
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.command.cmp(&b.command))
        });
        hits.truncate(32);
        hits
    }

    /// The concept ranking -- what `search` used to be, unchanged.
    fn rank_by_concept(&self, query: &str) -> Vec<Hit> {
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

/// Split a phrase into the units the lexical ranking compares.
///
/// Latin runs become words plus a five-character prefix, which is a stemmer poor enough to be
/// predictable: `scoring`, `scores` and `scored` all yield `score`, and nothing has to know
/// English morphology. CJK runs become character bigrams, because Chinese has no spaces and a
/// bigram is the shortest unit that carries meaning — 「分数」 and 「按分数取」 share one.
///
/// Digits are kept: `0` and `1` are the whole content of several bitmap phrasings.
fn grams(s: &str) -> BTreeSet<u32> {
    fn flush_latin(buf: &mut String, out: &mut BTreeSet<u32>) {
        if buf.len() >= 2 {
            out.insert(hash_gram(buf));
            if buf.chars().count() > 5 {
                let stem: String = buf.chars().take(5).collect();
                out.insert(hash_gram(&stem));
            }
        }
        buf.clear();
    }
    fn flush_cjk(buf: &mut Vec<char>, out: &mut BTreeSet<u32>) {
        if buf.len() == 1 {
            out.insert(hash_gram(&buf[0].to_string()));
        }
        for w in buf.windows(2) {
            let g: String = w.iter().collect();
            out.insert(hash_gram(&g));
        }
        buf.clear();
    }

    let mut out = BTreeSet::new();
    let lower = s.to_lowercase();
    let mut latin = String::new();
    let mut cjk: Vec<char> = Vec::new();
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            latin.push(ch);
        } else if is_cjk(ch) {
            flush_latin(&mut latin, &mut out);
            cjk.push(ch);
        } else {
            flush_latin(&mut latin, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_latin(&mut latin, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out
}

/// CJK ideographs, which is all this needs to tell apart from Latin.
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF)
}

/// Reciprocal rank fusion of the two rankings.
///
/// By rank, not by score. The two numbers are not on the same scale and never will be, and a
/// weight tuned to make one corpus come out well is exactly the kind of tuning §16.4's
/// held-out rule exists to catch. RRF needs only the orders.
///
/// `K` damps the top: without it first place in either list would win outright, and a command
/// that is second in both — usually the right answer — would lose to one that is first in one
/// and nowhere in the other.
fn fuse(a: &[Hit], b: &[Hit], limit: usize) -> Vec<Hit> {
    const K: f64 = 12.0;
    let mut acc: BTreeMap<&str, (f64, Vec<String>)> = BTreeMap::new();
    for (rank, h) in a.iter().enumerate() {
        let e = acc.entry(&h.command).or_insert((0.0, Vec::new()));
        e.0 += 1.0 / (K + rank as f64);
        e.1.clone_from(&h.matched);
    }
    for (rank, h) in b.iter().enumerate() {
        let e = acc.entry(&h.command).or_insert((0.0, Vec::new()));
        e.0 += 1.0 / (K + rank as f64);
    }
    let mut hits: Vec<Hit> = acc
        .into_iter()
        .map(|(command, (score, matched))| Hit {
            command: command.to_owned(),
            score,
            matched,
        })
        .collect();
    hits.sort_by(|x, y| {
        y.score
            .partial_cmp(&x.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.command.cmp(&y.command))
    });
    hits.truncate(limit);
    hits
}

/// FNV-1a, 32 bits. Chosen for being three lines and stable across runs; the index is rebuilt
/// in-process every time, so nothing depends on the value beyond this program's lifetime.
fn hash_gram(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}
