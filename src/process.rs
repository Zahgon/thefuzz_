//! Process-level extraction, matching thefuzz/process.py.
//!
//! Provides `extract`, `extract_bests`, `extract_one`, `extract_without_order`
//! and `dedupe`, plus a `Scorer` enum mirroring thefuzz's scorer set and the
//! `_get_processor` / `_get_scorer` / `_scorer_lowering` composition logic.

use crate::fuzz;
use crate::utils::full_process;

/// The set of scorers thefuzz recognizes for lowering/preprocessing.
///
/// `Custom` models an arbitrary user scorer (a function pointer taking the
/// already-processed query and choice and returning a raw `f64` score).
#[derive(Clone, Copy)]
pub enum Scorer {
    Ratio,
    PartialRatio,
    TokenSortRatio,
    TokenSetRatio,
    PartialTokenSortRatio,
    PartialTokenSetRatio,
    WRatio,
    QRatio,
    UWRatio,
    UQRatio,
    /// Custom scorer: receives (query, choice) already-processed strings,
    /// returns a raw score (not re-rounded by thefuzz).
    Custom(fn(&str, &str) -> f64),
}

impl Scorer {
    /// Whether this scorer is in `_scorer_lowering` (known -> re-rounded int).
    fn is_lowered(&self) -> bool {
        !matches!(self, Scorer::Custom(_))
    }

    /// Whether this scorer participates in thefuzz's own preprocessing
    /// (the set in `_get_processor`). ratio/partial_ratio do NOT.
    fn preprocesses(&self) -> bool {
        matches!(
            self,
            Scorer::WRatio
                | Scorer::QRatio
                | Scorer::TokenSetRatio
                | Scorer::TokenSortRatio
                | Scorer::PartialTokenSetRatio
                | Scorer::PartialTokenSortRatio
                | Scorer::UWRatio
                | Scorer::UQRatio
        )
    }

    /// force_ascii used by the built-in preprocess: False only for U* scorers.
    fn force_ascii(&self) -> bool {
        !matches!(self, Scorer::UWRatio | Scorer::UQRatio)
    }

    /// Compute the raw (float) score on already-processed strings.
    ///
    /// This mirrors `_scorer_lowering`: known scorers call the underlying
    /// rapidfuzz-equivalent raw function; U* map to W/Q raw.
    fn raw_score(&self, a: &str, b: &str) -> f64 {
        let ca: Vec<char> = a.chars().collect();
        let cb: Vec<char> = b.chars().collect();
        match self {
            Scorer::Ratio => fuzz::ratio_raw(&ca, &cb),
            Scorer::PartialRatio => fuzz::partial_ratio_raw(&ca, &cb),
            Scorer::TokenSortRatio => fuzz::token_sort_ratio_raw(&ca, &cb),
            Scorer::TokenSetRatio => fuzz::token_set_ratio_raw(&ca, &cb),
            Scorer::PartialTokenSortRatio => fuzz::partial_token_sort_ratio_raw(&ca, &cb),
            Scorer::PartialTokenSetRatio => fuzz::partial_token_set_ratio_raw(&ca, &cb),
            Scorer::WRatio | Scorer::UWRatio => fuzz::wratio_raw(&ca, &cb),
            Scorer::QRatio | Scorer::UQRatio => fuzz::qratio_raw(&ca, &cb),
            Scorer::Custom(f) => f(a, b),
        }
    }
}

/// A user-supplied processor: maps a raw choice string to a processed string.
pub type Processor = fn(&str) -> String;

/// Warning hook: invoked when the processor reduces the query to empty.
/// Tests can install a hook to capture the exact message + logger name.
pub mod warning {
    use std::cell::RefCell;

    type HookFn = Box<dyn Fn(&str)>;

    thread_local! {
        static HOOK: RefCell<Option<HookFn>> = RefCell::new(None);
    }

    /// The logger name matching Python's `logging.getLogger('thefuzz.process')`.
    pub const LOGGER_NAME: &str = "thefuzz.process";

    /// Install a warning hook for the current thread (used in tests).
    pub fn set_hook<F: Fn(&str) + 'static>(f: F) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    /// Clear the warning hook for the current thread.
    pub fn clear_hook() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }

    pub(crate) fn emit(message: &str) {
        HOOK.with(|h| {
            if let Some(f) = h.borrow().as_ref() {
                f(message);
            }
        });
    }
}

/// Choices come in two shapes: a list/iterable (yields 2-tuples) or a mapping
/// (yields 3-tuples that include the key).
pub enum Choices<'a> {
    /// A list of choice strings; `None` models a Python `None` entry.
    List(Vec<Option<&'a str>>),
    /// A mapping of (key, choice); `None` value models a `None` entry.
    Mapping(Vec<(&'a str, Option<&'a str>)>),
}

/// A single extraction result. For list choices `key` is `None`; for mapping
/// choices `key` carries the mapping key.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub choice: Option<String>,
    pub score: i32,
    pub key: Option<String>,
}

/// Compose thefuzz's built-in preprocess with an optional user processor,
/// matching `_get_processor`. Returns the string actually fed to the scorer.
fn apply_processor(scorer: &Scorer, processor: Option<Processor>, s: &str) -> String {
    if !scorer.preprocesses() {
        // ratio/partial_ratio/custom: apply the user processor unchanged
        // (default processor is full_process for the process module default).
        return match processor {
            Some(p) => p(s),
            None => s.to_string(),
        };
    }
    let force_ascii = scorer.force_ascii();
    match processor {
        // No processor OR the default full_process => just the built-in preprocess.
        None => full_process(s, force_ascii),
        Some(p) => {
            // thefuzz composes user-first then full_process, EXCEPT when the
            // user processor IS utils.full_process (the default), in which case
            // only the built-in preprocess runs. We approximate "is default"
            // via the DEFAULT_PROCESSOR sentinel below.
            if p as usize == DEFAULT_PROCESSOR as usize {
                full_process(s, force_ascii)
            } else {
                full_process(&p(s), force_ascii)
            }
        }
    }
}

/// The default processor sentinel (utils.full_process with force_ascii=False).
///
/// Matches thefuzz `default_processor = utils.full_process`.
pub fn default_processor(s: &str) -> String {
    full_process(s, false)
}

const DEFAULT_PROCESSOR: Processor = default_processor;

/// Validate query preprocessing; emit the warning if it reduces to empty.
fn validate_query_preprocessing(query: &str, scorer: &Scorer, processor: Option<Processor>) {
    // thefuzz calls processor(query) with the *composed* processor from
    // _get_processor; if the result is empty, warn.
    let processed = apply_processor(scorer, processor, query);
    if processed.is_empty() {
        let msg = format!(
            "Applied processor reduces input query to empty string, all comparisons will have score 0. [Query: '{}']",
            query
        );
        warning::emit(&msg);
    }
}

/// Score a single (query, choice) pair, returning the rounded int score.
fn score_pair(scorer: &Scorer, processor: Option<Processor>, query: &str, choice: &str) -> i32 {
    let pq = apply_processor(scorer, processor, query);
    let pc = apply_processor(scorer, processor, choice);
    let raw = scorer.raw_score(&pq, &pc);
    if scorer.is_lowered() {
        fuzz::py_round_to_int(raw)
    } else {
        // Custom scorer: thefuzz does not re-round; but scores are compared as
        // given. We still return an int view via truncation-free round for
        // ordering parity; custom scorers here are int-valued in practice.
        fuzz::py_round_to_int(raw)
    }
}

/// `extractWithoutOrder`: yield all (choice, score[, key]) with score >= cutoff,
/// in original choice order (no sorting).
pub fn extract_without_order(
    query: &str,
    choices: &Choices,
    processor: Option<Processor>,
    scorer: Scorer,
    score_cutoff: i32,
) -> Vec<Match> {
    validate_query_preprocessing(query, &scorer, processor);
    let mut out = Vec::new();
    match choices {
        Choices::List(items) => {
            for &item in items {
                let choice = match item {
                    Some(c) => c,
                    None => continue, // None choices are skipped by rapidfuzz.
                };
                let score = score_pair(&scorer, processor, query, choice);
                if score >= score_cutoff {
                    out.push(Match {
                        choice: Some(choice.to_string()),
                        score,
                        key: None,
                    });
                }
            }
        }
        Choices::Mapping(items) => {
            for &(key, item) in items {
                let choice = match item {
                    Some(c) => c,
                    None => continue,
                };
                let score = score_pair(&scorer, processor, query, choice);
                if score >= score_cutoff {
                    out.push(Match {
                        choice: Some(choice.to_string()),
                        score,
                        key: Some(key.to_string()),
                    });
                }
            }
        }
    }
    out
}

/// Sort matches descending by score, stable (preserves original order on ties).
fn sort_desc(matches: &mut [Match]) {
    // stable sort by descending score
    matches.sort_by_key(|a| std::cmp::Reverse(a.score));
}

/// `extractBests`: sorted descending, cutoff applied, limited to `limit`.
/// `limit = None` means unlimited.
pub fn extract_bests(
    query: &str,
    choices: &Choices,
    processor: Option<Processor>,
    scorer: Scorer,
    score_cutoff: i32,
    limit: Option<usize>,
) -> Vec<Match> {
    let mut results = extract_without_order(query, choices, processor, scorer, score_cutoff);
    sort_desc(&mut results);
    if let Some(n) = limit {
        results.truncate(n);
    }
    results
}

/// `extract`: like `extract_bests` with cutoff 0 and default limit 5.
pub fn extract(
    query: &str,
    choices: &Choices,
    processor: Option<Processor>,
    scorer: Scorer,
    limit: Option<usize>,
) -> Vec<Match> {
    extract_bests(query, choices, processor, scorer, 0, limit)
}

/// `extractOne`: highest-scoring match at or above cutoff, or `None`.
pub fn extract_one(
    query: &str,
    choices: &Choices,
    processor: Option<Processor>,
    scorer: Scorer,
    score_cutoff: i32,
) -> Option<Match> {
    let results = extract_bests(query, choices, processor, scorer, score_cutoff, None);
    results.into_iter().next()
}

/// `dedupe`: collapse near-duplicate strings, keeping the longest (ties broken
/// by the lexicographically greatest string). Returns the original list (by
/// value) when nothing was deduped.
pub fn dedupe(contains_dupes: &[&str], threshold: i32, scorer: Scorer) -> Vec<String> {
    let mut deduped: Vec<String> = Vec::new();
    let choices = Choices::List(contains_dupes.iter().map(|s| Some(*s)).collect());
    for &item in contains_dupes {
        let matches = extract_bests(item, &choices, Some(default_processor), scorer, threshold, None);
        // max by (len(choice), choice)
        let best = matches
            .iter()
            .filter_map(|m| m.choice.as_ref())
            .max_by(|a, b| {
                a.chars()
                    .count()
                    .cmp(&b.chars().count())
                    .then_with(|| a.as_str().cmp(b.as_str()))
            });
        if let Some(b) = best {
            if !deduped.contains(b) {
                deduped.push(b.clone());
            }
        }
    }
    if deduped.len() != contains_dupes.len() {
        deduped
    } else {
        contains_dupes.iter().map(|s| s.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseball() -> Vec<Option<&'static str>> {
        vec![
            Some("new york mets vs chicago cubs"),
            Some("chicago cubs vs chicago white sox"),
            Some("philladelphia phillies vs atlanta braves"),
            Some("braves vs mets"),
        ]
    }

    #[test]
    fn extract_one_best() {
        let choices = Choices::List(baseball());
        let m = extract_one("new york mets at atlanta braves", &choices, Some(default_processor), Scorer::WRatio, 0).unwrap();
        assert_eq!(m.choice.as_deref(), Some("braves vs mets"));
        assert_eq!(m.score, 86);
    }

    #[test]
    fn extract_order() {
        let choices = Choices::List(baseball());
        let res = extract("new york mets at atlanta braves", &choices, Some(default_processor), Scorer::WRatio, Some(5));
        assert_eq!(res[0].choice.as_deref(), Some("braves vs mets"));
        assert_eq!(res[0].score, 86);
        assert_eq!(res.len(), 4);
    }

    #[test]
    fn cutoff_none() {
        // With a cutoff above every choice's score, extract_one yields None.
        let choices = Choices::List(baseball());
        let m = extract_one(
            "los angeles dodgers vs san francisco giants",
            &choices,
            Some(default_processor),
            Scorer::WRatio,
            90,
        );
        assert!(m.is_none());
    }

    #[test]
    fn dedupe_unchanged() {
        let input = vec!["Tom", "Dick", "Harry"];
        let res = dedupe(&input, 70, Scorer::TokenSetRatio);
        assert_eq!(res, vec!["Tom".to_string(), "Dick".to_string(), "Harry".to_string()]);
    }
}
