//! V-F08 — purpose search, measured against two corpora (v2.1 §16.4, R10, ASSIST-050/051).
//!
//! | corpus | threshold | what it proves |
//! |---|---|---|
//! | canonical | top-3 recall ≥ 95% | the vocabulary is **complete** — and no more, because the vocabulary was built from this corpus |
//! | held-out | top-3 ≥ 80%, top-5 ≥ 90% | it **generalises** to phrasings that did not build it |
//!
//! Measuring only the first is measuring whether a lookup table contains its own contents, so
//! §16.4 asks for both and this file reports them apart.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    missing_docs,
    // Recall percentages: ratios of small counts, printed for a person to read.
    clippy::cast_precision_loss
)]

use pr_intelligence::find::Finder;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("fixtures/assistance/find")
}

/// One expected (phrase -> command) pair.
struct Case {
    command: String,
    phrase: String,
    file: String,
}

fn load(dir: &str) -> Vec<Case> {
    let root = fixtures().join(dir);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        panic!("no corpus at {}", root.display());
    };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    for f in files {
        if f.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap();
        let t: toml::Table = text
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        for case in t["case"].as_array().expect("[[case]] entries") {
            let command = case["command"].as_str().unwrap().to_owned();
            for p in case["phrases"].as_array().unwrap() {
                out.push(Case {
                    command: command.clone(),
                    phrase: p.as_str().unwrap().to_owned(),
                    file: name.clone(),
                });
            }
        }
    }
    assert!(!out.is_empty(), "the {dir} corpus is empty");
    out
}

struct Recall {
    total: usize,
    top3: usize,
    top5: usize,
    misses: Vec<String>,
}

impl Recall {
    fn top3_pct(&self) -> f64 {
        self.top3 as f64 * 100.0 / self.total as f64
    }
    fn top5_pct(&self) -> f64 {
        self.top5 as f64 * 100.0 / self.total as f64
    }
}

fn measure(cases: &[Case]) -> Recall {
    let f = Finder::new();
    let mut r = Recall {
        total: cases.len(),
        top3: 0,
        top5: 0,
        misses: Vec::new(),
    };
    for c in cases {
        let hits = f.search(&c.phrase, 5);
        let names: Vec<&str> = hits.iter().map(|h| h.command.as_str()).collect();
        let at = names.iter().position(|n| *n == c.command);
        match at {
            Some(i) if i < 3 => {
                r.top3 += 1;
                r.top5 += 1;
            }
            Some(_) => r.top5 += 1,
            None => r.misses.push(format!(
                "{} [{}] {:?} -> {:?}",
                c.command, c.file, c.phrase, names
            )),
        }
        if let Some(i) = at
            && i >= 3
        {
            r.misses.push(format!(
                "{} [{}] rank {} {:?} -> {:?}",
                c.command,
                c.file,
                i + 1,
                c.phrase,
                names
            ));
        }
    }
    r
}

fn report(name: &str, r: &Recall) {
    eprintln!(
        "V-F08 {name}: {} phrases, top-3 {:.1}% ({}/{}), top-5 {:.1}% ({}/{})",
        r.total,
        r.top3_pct(),
        r.top3,
        r.total,
        r.top5_pct(),
        r.top5,
        r.total
    );
    for m in r.misses.iter().take(400) {
        eprintln!("  miss: {m}");
    }
    if r.misses.len() > 400 {
        eprintln!("  ... and {} more", r.misses.len() - 400);
    }
}

#[test]
fn canonical_recall_meets_the_gate() {
    let cases = load("canonical");
    let r = measure(&cases);
    report("canonical", &r);
    assert!(
        r.top3_pct() >= 95.0,
        "canonical top-3 recall is {:.1}%, under §16.4's 95%",
        r.top3_pct()
    );
}

/// The held-out corpora that were measured, then used to extend the vocabulary, then retired.
///
/// §16.4: 「未达标 → 扩词表后 held-out 集换新（旧集作废，防止泄漏）」. Kept rather than deleted so
/// the extensions can be audited.
const VOID_CORPORA: &[&str] = &[
    "heldout-v1-void",
    "heldout-v2-void",
    "heldout-v3-void",
    // Written independently, measured once at 67.3% / 75.5%, then used to extend the
    // vocabulary. Spent, and kept so the extension can be audited against it.
    "heldout-independent-v1-void",
];

/// The floor for the void corpora, as a ratchet.
///
/// Not §16.4's gate. Every corpus behind this number has been used to extend the vocabulary,
/// so it measures retention, not generalisation. The gate itself is
/// `heldout_recall_meets_the_independent_gate`, and it can only ever be read from a corpus
/// that has not yet been through this door.
const HELDOUT_FLOOR_TOP3: f64 = 70.0;
/// The same, for top-5.
const HELDOUT_FLOOR_TOP5: f64 = 75.0;

#[test]
fn no_corpus_that_built_the_vocabulary_regresses() {
    // A regression guard, **not** a measurement of generalisation. Every corpus named here has
    // been used to extend the vocabulary, so its recall says only that the vocabulary still
    // contains what it was given. That is worth guarding — a refactor that silently drops
    // half the terms would show up here — and it is worth being clear it is not the number
    // §16.4 asks for.
    let mut cases = Vec::new();
    for void in VOID_CORPORA {
        cases.extend(load(void));
    }
    let r = measure(&cases);
    report("void corpora (regression guard only)", &r);
    assert!(
        r.top3_pct() >= HELDOUT_FLOOR_TOP3,
        "recall over the corpora that built the vocabulary fell to {:.1}%, below the \
         {HELDOUT_FLOOR_TOP3}% already reached. This is not §16.4's gate, but it must not rot",
        r.top3_pct()
    );
    assert!(
        r.top5_pct() >= HELDOUT_FLOOR_TOP5,
        "top-5 fell to {:.1}%",
        r.top5_pct()
    );
}

#[test]
fn heldout_recall_meets_the_independent_gate() {
    // §16.4's actual gate, waiting for its input.
    //
    // It turns itself on: drop an independently written corpus into
    // `fixtures/assistance/find/heldout-independent/` and this stops skipping and starts
    // enforcing 80%/90%. No flag to remember, no test to re-enable -- the one thing that has
    // to happen is the one thing an agent cannot do for itself.
    let dir = fixtures().join("heldout-independent");
    if !dir.exists() {
        eprintln!(
            "V-F08: §16.4's held-out gate is WAITING. It needs a corpus written by someone who \
             did not build the vocabulary (ADR-034). Put it in {} and this test enforces \
             80% top-3 / 90% top-5 automatically.",
            dir.display()
        );
        return;
    }
    let cases = load("heldout-independent");
    let r = measure(&cases);
    report("held-out (independent)", &r);
    assert!(
        r.top3_pct() >= 80.0,
        "independent held-out top-3 recall is {:.1}%, under §16.4's 80%",
        r.top3_pct()
    );
    assert!(
        r.top5_pct() >= 90.0,
        "independent held-out top-5 recall is {:.1}%, under §16.4's 90%",
        r.top5_pct()
    );
}

#[test]
fn the_independent_corpus_would_not_be_one_of_the_corpora_that_built_the_vocabulary() {
    // The failure mode this guards against: someone satisfies the gate by copying an existing
    // corpus into the independent directory. Every one of those has already been used to
    // extend the vocabulary.
    let dir = fixtures().join("heldout-independent");
    if !dir.exists() {
        return;
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for corpus in [
        "canonical",
        "heldout-v1-void",
        "heldout-v2-void",
        "heldout-v3-void",
    ] {
        for c in load(corpus) {
            seen.insert(c.phrase.to_lowercase());
        }
    }
    let reused: Vec<String> = load("heldout-independent")
        .into_iter()
        .filter(|c| seen.contains(&c.phrase.to_lowercase()))
        .map(|c| c.phrase)
        .collect();
    assert!(
        reused.is_empty(),
        "the independent corpus reuses phrases from a corpus that built the vocabulary: \
         {reused:?}"
    );
}

#[test]
fn no_void_corpus_repeats_a_canonical_phrase() {
    // §16.4: 「任何一条进入词表即从该集移除」. A phrase in two corpora has already been used to
    // build the thing it was meant to test.
    let canonical: BTreeSet<String> = load("canonical")
        .into_iter()
        .map(|c| c.phrase.to_lowercase())
        .collect();
    for void in VOID_CORPORA {
        let shared: Vec<String> = load(void)
            .into_iter()
            .filter(|c| canonical.contains(&c.phrase.to_lowercase()))
            .map(|c| c.phrase)
            .collect();
        assert!(
            shared.is_empty(),
            "{void} repeats canonical phrases: {shared:?}"
        );
    }
}

#[test]
fn every_void_corpus_covers_every_command_the_canonical_one_does() {
    // A held-out set that quietly omits the commands that fail measures nothing, so coverage
    // is checked rather than trusted — including retrospectively, for the retired ones.
    let canonical: BTreeSet<String> = load("canonical").into_iter().map(|c| c.command).collect();
    for void in VOID_CORPORA {
        let held: BTreeSet<String> = load(void).into_iter().map(|c| c.command).collect();
        let missing: Vec<&String> = canonical.difference(&held).collect();
        assert!(missing.is_empty(), "{void} omits {missing:?}");
    }
}

#[test]
fn every_command_in_scope_has_at_least_three_phrasings_in_both_languages() {
    // §16.4: 「每命令 ≥ 3 条中/英」. Read as: at least three phrasings, and not all in one
    // language — a corpus that is entirely English measures an English feature.
    let cases = load("canonical");
    let mut by_command: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::default();
    for c in &cases {
        by_command
            .entry(c.command.as_str())
            .or_default()
            .push(c.phrase.as_str());
    }
    for (cmd, phrases) in &by_command {
        assert!(
            phrases.len() >= 3,
            "{cmd} has only {} canonical phrasings",
            phrases.len()
        );
        let has_cjk = phrases.iter().any(|p| p.chars().any(is_cjk));
        let has_latin = phrases
            .iter()
            .any(|p| p.chars().filter(char::is_ascii_alphabetic).count() > 8);
        assert!(has_cjk, "{cmd} has no Chinese phrasing");
        assert!(has_latin, "{cmd} has no English phrasing");
    }
    eprintln!("V-F08 scope: {} commands covered", by_command.len());
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x4e00..=0x9fff | 0x3400..=0x4dbf)
}

#[test]
fn every_vocabulary_term_occurs_in_a_corpus_that_built_it() {
    // The mechanical link §16.4 describes: the vocabulary is built *from* a corpus. A term
    // nobody ever wrote is a term added to make a test pass, and it would inflate the
    // measured recall without helping any real query.
    //
    // Four corpora count as having built it: the canonical set and the three **void** held-out
    // sets. Each was measured on first exposure, then used to extend the vocabulary, then
    // retired — exactly the fallback §16.4 prescribes. All are kept in the repository so the
    // extensions can be audited; deleting them would leave the vocabulary looking as though it
    // had always known those words.
    let mut cases = load("canonical");
    for void in VOID_CORPORA {
        cases.extend(load(void));
    }
    let corpus: String = cases
        .iter()
        .map(|c| c.phrase.to_lowercase())
        .collect::<Vec<_>>()
        .join(" | ");
    let mut ungrounded = Vec::new();
    for t in pr_intelligence::vocabulary::TERMS {
        if !corpus.contains(&t.text.to_lowercase()) {
            ungrounded.push(t.text);
        }
    }
    assert!(
        ungrounded.is_empty(),
        "these vocabulary terms appear in no canonical phrase: {ungrounded:?}"
    );
    eprintln!(
        "V-F08 vocabulary: {} terms, all grounded in the corpus",
        pr_intelligence::vocabulary::TERMS.len()
    );
}

/// Concepts that are deliberately not typeable.
///
/// Each one narrows the ranking without ever being a word someone writes. The list is short
/// and published on purpose: without it the reachability test below would either be disabled
/// or would force a fake term for every structural distinction, and both of those are worse
/// than naming the exceptions.
const STRUCTURAL: &[(&str, &str)] = &[
    (
        "deprecated",
        "a ranking penalty, never a query — see find::DEPRECATED",
    ),
    (
        "pubsub",
        "groups the pub/sub family so a channel query does not reach a list command",
    ),
    (
        "dangerous",
        "marks KEYS so it can be ranked and warned about, not searched for",
    ),
    (
        "async",
        "distinguishes UNLINK from DEL in the result, not in the query",
    ),
    (
        "durability",
        "distinguishes WAIT/WAITAOF from each other, reached by their own terms",
    ),
    (
        "rollback",
        "a synonym for what DISCARD does, kept for the explanation text",
    ),
    (
        "store",
        "reached by 「存起来」 and friends; also marks the *STORE variants",
    ),
];

#[test]
fn every_concept_a_command_claims_is_reachable() {
    // A command that claims a concept nothing can ever produce is a command that concept can
    // never find — and worse, the dead concept widens the command's breadth and lowers its own
    // score. Reachable means: some term produces it, some implication produces it, or it is on
    // the published structural list above.
    let mut reachable: BTreeSet<&str> = BTreeSet::new();
    for t in pr_intelligence::vocabulary::TERMS {
        for c in t.concepts {
            reachable.insert(c);
        }
    }
    for (_, then, _) in pr_intelligence::vocabulary::IMPLIES {
        reachable.insert(then);
    }
    for (c, _) in STRUCTURAL {
        reachable.insert(c);
    }

    let mut unreachable: BTreeSet<(&str, &str)> = BTreeSet::new();
    for p in pr_intelligence::vocabulary::PURPOSES {
        for c in p.concepts {
            if !reachable.contains(c) {
                unreachable.insert((p.command, c));
            }
        }
    }
    assert!(
        unreachable.is_empty(),
        "these concepts are claimed by a command but nothing can produce them: {unreachable:?}"
    );
}

#[test]
fn every_implication_depends_on_concepts_that_exist() {
    // An implication whose left-hand side names a concept no term produces can never fire, and
    // would look like a working rule.
    let mut from_terms: BTreeSet<&str> = BTreeSet::new();
    for t in pr_intelligence::vocabulary::TERMS {
        for c in t.concepts {
            from_terms.insert(c);
        }
    }
    let mut produced: BTreeSet<&str> = BTreeSet::new();
    for (_, then, _) in pr_intelligence::vocabulary::IMPLIES {
        produced.insert(then);
    }
    for (needs, then, _) in pr_intelligence::vocabulary::IMPLIES {
        for n in *needs {
            assert!(
                from_terms.contains(n) || produced.contains(n),
                "the implication -> {then} needs {n}, which nothing produces"
            );
        }
    }
}

#[test]
fn every_structural_concept_is_actually_claimed_by_something() {
    // The other direction: an exception that protects nothing is an exception someone will
    // copy the next time a test is inconvenient.
    let claimed: BTreeSet<&str> = pr_intelligence::vocabulary::PURPOSES
        .iter()
        .flat_map(|p| p.concepts.iter().copied())
        .collect();
    for (c, why) in STRUCTURAL {
        assert!(
            claimed.contains(c),
            "{c} is listed as structural ({why}) but no command claims it"
        );
        assert!(why.len() > 30, "{c}'s justification is too short to be one");
    }
}

#[test]
fn search_never_returns_a_command_outside_the_published_scope() {
    // §16.4's corpus is scoped, and the scope is published. A hit outside it would be a
    // command with no phrasings behind it, which no measurement covers.
    let f = Finder::new();
    let in_scope: BTreeSet<&str> = pr_intelligence::vocabulary::PURPOSES
        .iter()
        .map(|p| p.command)
        .collect();
    for c in load("canonical") {
        for h in f.search(&c.phrase, 5) {
            assert!(
                in_scope.contains(h.command.as_str()),
                "{:?} returned {}, which is outside the published scope",
                c.phrase,
                h.command
            );
        }
    }
}

// -----------------------------------------------------------------------------------------
// The seal
// -----------------------------------------------------------------------------------------

/// The vocabulary as it stood when the last void corpus was retired.
///
/// §16.4 asks for a held-out set 「由不参与词表构建的人独立撰写」 — written by someone who did not
/// build the vocabulary. One agent wrote both corpora here, which is stated plainly in
/// `fixtures/assistance/find/README.md` and is a Phase 2 gate item, exactly as V-I03 records
/// that a lawyer has not yet read the licence conclusion.
///
/// What *can* be enforced mechanically is the thing independence protects against: leakage.
/// This hash is the vocabulary at the moment the held-out phrasings were written. Any later
/// edit changes it, and a diff that changes both this constant and the held-out numbers in the
/// same commit is visible for what it is — tuning against the held-out set, which is the one
/// thing the second corpus exists to prevent.
const VOCABULARY_SEAL: &str = "cc708ddf7166cc3481e5e2286d9ac46c";

#[test]
fn the_vocabulary_is_sealed_against_the_corpora_that_built_it() {
    // Hashes the tables, not the file. `cargo fmt` moving a line break is not a change to the
    // vocabulary, and a seal that breaks on formatting is a seal people learn to re-stamp
    // without reading — which is exactly the habit it exists to prevent.
    let mut h = blake3::Hasher::new();
    h.update(b"penguin.find.vocabulary.v1\0");
    for t in pr_intelligence::vocabulary::TERMS {
        h.update(t.text.as_bytes());
        h.update(b"\x1f");
        for c in t.concepts {
            h.update(c.as_bytes());
            h.update(b",");
        }
        h.update(b"\x1e");
    }
    for p in pr_intelligence::vocabulary::PURPOSES {
        h.update(p.command.as_bytes());
        h.update(b"\x1f");
        for c in p.concepts {
            h.update(c.as_bytes());
            h.update(b",");
        }
        h.update(b"\x1e");
    }
    for (needs, then, consumes) in pr_intelligence::vocabulary::IMPLIES {
        for n in *needs {
            h.update(n.as_bytes());
            h.update(b",");
        }
        h.update(then.as_bytes());
        h.update(if *consumes { b"!" } else { b"." });
        h.update(b"\x1e");
    }
    let got = h.finalize().to_hex().to_string();
    assert_eq!(
        &got[..32],
        VOCABULARY_SEAL,
        "the vocabulary changed after the last corpus was retired. That is allowed -- but \
         re-seal it in a commit of its own, with the numbers measured before and after, so \
         the change is not tuning hiding inside a refactor. If an independent corpus exists, \
         measure it BEFORE extending the vocabulary: that measurement is the only one §16.4's \
         gate can be read from"
    );
}
