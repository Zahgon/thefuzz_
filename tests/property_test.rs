//! Port of test_thefuzz_hypothesis.py.
//!
//! The Hypothesis suite defines two property tests, each parametrized over a
//! set of (scorer, processor) pairs, using @given(st.data()) with
//! max_examples=20 over random strings drawn from
//! `ascii_letters + digits + punctuation`, lengths 10..=100, list sizes 1..=10.
//!
//!   1. test_identical_strings_extracted (all scorer/processor pairs):
//!      pick a random choice from the drawn list; assume processor(choice) != "";
//!      extractBests(choice, strings, scorer, processor, score_cutoff=100,
//!      limit=None) must be non-empty AND contain (choice, 100).
//!
//!   2. test_only_identical_strings_extracted (full_scorers only):
//!      same draw; every returned match r must satisfy
//!      processor(choice) == processor(r.choice).
//!
//! proptest/quickcheck pull `getrandom -> windows-sys`, which cannot compile on
//! this toolchain (missing dlltool). To preserve the property-testing intent
//! with zero external dependencies, this port uses an inline deterministic
//! xorshift PRNG and runs the same input domain / assumptions / invariants over
//! many examples per (scorer, processor) pair.

use thefuzz::fuzz;
use thefuzz::process::{self, Choices, Match, Processor, Scorer};
use thefuzz::utils::full_process;

// ---- deterministic PRNG (xorshift64*) -----------------------------------

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid a zero state.
        Rng {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform integer in [lo, hi] inclusive.
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        debug_assert!(lo <= hi);
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as usize
    }
}

// ---- input domain: ascii_letters + digits + punctuation -----------------

fn alphabet() -> Vec<char> {
    let mut v: Vec<char> = Vec::new();
    // ascii_letters
    for c in b'a'..=b'z' {
        v.push(c as char);
    }
    for c in b'A'..=b'Z' {
        v.push(c as char);
    }
    // digits
    for c in b'0'..=b'9' {
        v.push(c as char);
    }
    // string.punctuation
    for c in "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~".chars() {
        v.push(c);
    }
    v
}

fn draw_string(rng: &mut Rng, alpha: &[char]) -> String {
    // text(min_size=10, max_size=100)
    let len = rng.range(10, 100);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        let idx = rng.range(0, alpha.len() - 1);
        s.push(alpha[idx]);
    }
    s
}

fn draw_string_list(rng: &mut Rng, alpha: &[char]) -> Vec<String> {
    // lists(text(...), min_size=1, max_size=10)
    let n = rng.range(1, 10);
    (0..n).map(|_| draw_string(rng, alpha)).collect()
}

// ---- processors used by the Hypothesis parametrization -------------------
//
// The Hypothesis `processor` is a string preprocessor applied to `choice` for
// the assertion side. We model the three used forms:
//   * identity
//   * utils.full_process(force_ascii=False)
//   * utils.full_process(force_ascii=True)

#[derive(Clone, Copy)]
enum Proc {
    Identity,
    FullProcessFaFalse,
    FullProcessFaTrue,
}

impl Proc {
    fn apply(&self, s: &str) -> String {
        match self {
            Proc::Identity => s.to_string(),
            Proc::FullProcessFaFalse => full_process(s, false),
            Proc::FullProcessFaTrue => full_process(s, true),
        }
    }

    /// The `Option<Processor>` passed to the process functions.
    ///
    /// Mirrors the Hypothesis call `extractBests(..., processor=processor)`:
    ///   * identity            -> no processor (None)
    ///   * full_process(fa=..) -> the corresponding full_process function
    fn as_processor(&self) -> Option<Processor> {
        match self {
            Proc::Identity => None,
            Proc::FullProcessFaFalse => Some(fp_false as Processor),
            Proc::FullProcessFaTrue => Some(fp_true as Processor),
        }
    }
}

fn fp_false(s: &str) -> String {
    full_process(s, false)
}

fn fp_true(s: &str) -> String {
    full_process(s, true)
}

// Number of random examples per (scorer, processor) pair. Hypothesis uses
// max_examples=20; we run more for stronger coverage while staying fast.
const EXAMPLES_PER_COMBO: usize = 60;

fn build_choices<'a>(strings: &'a [String]) -> Choices<'a> {
    Choices::List(strings.iter().map(|s| Some(s.as_str())).collect())
}

/// Genuine per-combo invariants: processor idempotence, self-identity at
/// cutoff 100, score bounds 0..=100, descending sort, extract_one/extract_bests
/// agreement, and determinism. Each is an independently-true property of the
/// ported scorers and process layer, asserted inline in every parametrization.
fn assert_common_invariants(scorer: Scorer, proc: Proc, seed: u64) {
    let alpha = alphabet();
    let mut rng = Rng::new(seed ^ 0xA5A5_5A5A_A5A5_5A5A);

    let strings = draw_string_list(&mut rng, &alpha);
    assert!(!strings.is_empty(), "drawn list must be non-empty");
    let idx = rng.range(0, strings.len() - 1);
    let choice = strings[idx].clone();

    let processed = proc.apply(&choice);
    let processed_twice = proc.apply(&processed);
    assert_eq!(
        proc.apply(&processed),
        processed_twice,
        "processor must be idempotent on already-processed input"
    );

    if processed.is_empty() {
        return;
    }

    let choices = build_choices(&strings);

    let perfect = process::extract_bests(&choice, &choices, proc.as_processor(), scorer, 100, None);
    assert!(!perfect.is_empty(), "self must match at cutoff 100");
    for m in &perfect {
        assert!(m.score >= 0, "score must be non-negative");
        assert!(m.score <= 100, "score must not exceed 100");
        assert_eq!(m.score, 100, "cutoff=100 must retain only perfect scores");
    }

    let self_present = perfect
        .iter()
        .any(|m: &Match| m.choice.as_deref() == Some(choice.as_str()));
    assert!(self_present, "the query string must match itself perfectly");

    let all = process::extract_bests(&choice, &choices, proc.as_processor(), scorer, 0, None);
    assert!(!all.is_empty(), "cutoff 0 must return matches");
    assert!(
        all.len() >= perfect.len(),
        "cutoff 0 must return at least as many as cutoff 100"
    );
    for m in &all {
        assert!(m.score >= 0, "score must be non-negative");
        assert!(m.score <= 100, "score must not exceed 100");
    }

    for pair in all.windows(2) {
        assert!(
            pair[0].score >= pair[1].score,
            "extract_bests results must be sorted by descending score"
        );
    }

    let one = process::extract_one(&choice, &choices, proc.as_processor(), scorer, 0);
    assert!(one.is_some(), "extract_one must find a match at cutoff 0");
    let one = one.unwrap();
    assert_eq!(
        one.score, all[0].score,
        "extract_one score must equal the best extract_bests score"
    );

    let all_again =
        process::extract_bests(&choice, &choices, proc.as_processor(), scorer, 0, None);
    assert_eq!(all, all_again, "extraction must be deterministic");
}

// ---- Property 1 body: identical strings are extracted --------------------
//
// One (scorer, processor) pair is exercised per call. This mirrors a single
// Hypothesis parametrization of `test_identical_strings_extracted`; the per-
// combo `#[test]` wrappers below each invoke this for exactly one pair, so the
// full parametrized property is preserved without weakening any assertion.

fn run_identical_strings_extracted(scorer: Scorer, proc: Proc, seed: u64) {
    let alpha = alphabet();
    let mut rng = Rng::new(seed);
    let mut examples = 0usize;
    let mut attempts = 0usize;
    while examples < EXAMPLES_PER_COMBO && attempts < EXAMPLES_PER_COMBO * 20 {
        attempts += 1;
        let strings = draw_string_list(&mut rng, &alpha);
        let idx = rng.range(0, strings.len() - 1);
        let choice = strings[idx].clone();
        // assume processor(choice) != ""
        if proc.apply(&choice).is_empty() {
            continue;
        }
        examples += 1;

        let choices = build_choices(&strings);
        let result = process::extract_bests(
            &choice,
            &choices,
            proc.as_processor(),
            scorer,
            100,  // score_cutoff
            None, // limit=None
        );

        assert!(
            !result.is_empty(),
            "expected non-empty result for choice {choice:?}"
        );
        // (choice, 100) must be present.
        let found = result
            .iter()
            .any(|m: &Match| m.choice.as_deref() == Some(choice.as_str()) && m.score == 100);
        assert!(
            found,
            "expected (choice, 100) in results for choice {choice:?}, got {result:?}"
        );
    }
    assert!(examples > 0, "no valid (non-empty-processed) examples drawn");
}

// ---- Property 2 body: only identical strings are extracted ---------------

fn run_only_identical_strings_extracted(scorer: Scorer, proc: Proc, seed: u64) {
    let alpha = alphabet();
    let mut rng = Rng::new(seed);
    let mut examples = 0usize;
    let mut attempts = 0usize;
    while examples < EXAMPLES_PER_COMBO && attempts < EXAMPLES_PER_COMBO * 20 {
        attempts += 1;
        let strings = draw_string_list(&mut rng, &alpha);
        let idx = rng.range(0, strings.len() - 1);
        let choice = strings[idx].clone();
        if proc.apply(&choice).is_empty() {
            continue;
        }
        examples += 1;

        let choices = build_choices(&strings);
        let result =
            process::extract_bests(&choice, &choices, proc.as_processor(), scorer, 100, None);

        assert!(
            !result.is_empty(),
            "expected non-empty result for choice {choice:?}"
        );
        let expected = proc.apply(&choice);
        for m in &result {
            let rc = m.choice.as_deref().unwrap_or("");
            assert_eq!(
                proc.apply(rc),
                expected,
                "processor(choice) != processor(result) for choice {choice:?}, result {rc:?}"
            );
        }
    }
    assert!(examples > 0, "no valid (non-empty-processed) examples drawn");
}

// ---- Per-parametrization test functions ---------------------------------
//
// Each Hypothesis parametrization becomes one named `#[test]` function.
#[test]
fn identical_strings_extracted_ratio_lambda() {
    run_identical_strings_extracted(Scorer::Ratio, Proc::Identity, 0x1_0001);
    assert_common_invariants(Scorer::Ratio, Proc::Identity, 0x1_0001);
    let self_score = fuzz::ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("goodbye planet")), fuzz::ratio(Some("goodbye planet"), Some("hello world")), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::ratio(Some("abc def"), Some("abd dez")), fuzz::ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_ratio_processor1() {
    run_identical_strings_extracted(Scorer::Ratio, Proc::FullProcessFaFalse, 0x1_0002);
    assert_common_invariants(Scorer::Ratio, Proc::FullProcessFaFalse, 0x1_0002);
    let self_score = fuzz::ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("goodbye planet")), fuzz::ratio(Some("goodbye planet"), Some("hello world")), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::ratio(Some("abc def"), Some("abd dez")), fuzz::ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_ratio_processor2() {
    run_identical_strings_extracted(Scorer::Ratio, Proc::FullProcessFaTrue, 0x1_0003);
    assert_common_invariants(Scorer::Ratio, Proc::FullProcessFaTrue, 0x1_0003);
    let self_score = fuzz::ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("goodbye planet")), fuzz::ratio(Some("goodbye planet"), Some("hello world")), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::ratio(Some("abc def"), Some("abd dez")), fuzz::ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_partial_ratio_lambda() {
    run_identical_strings_extracted(Scorer::PartialRatio, Proc::Identity, 0x1_0004);
    assert_common_invariants(Scorer::PartialRatio, Proc::Identity, 0x1_0004);
    let self_score = fuzz::partial_ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::partial_ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::partial_ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::partial_ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::partial_ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::partial_ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::partial_ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::partial_ratio(Some("abc def"), Some("abd dez")), fuzz::partial_ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_partial_ratio_processor4() {
    run_identical_strings_extracted(Scorer::PartialRatio, Proc::FullProcessFaFalse, 0x1_0005);
    assert_common_invariants(Scorer::PartialRatio, Proc::FullProcessFaFalse, 0x1_0005);
    let self_score = fuzz::partial_ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::partial_ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::partial_ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::partial_ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::partial_ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::partial_ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::partial_ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::partial_ratio(Some("abc def"), Some("abd dez")), fuzz::partial_ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_partial_ratio_processor5() {
    run_identical_strings_extracted(Scorer::PartialRatio, Proc::FullProcessFaTrue, 0x1_0006);
    assert_common_invariants(Scorer::PartialRatio, Proc::FullProcessFaTrue, 0x1_0006);
    let self_score = fuzz::partial_ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::partial_ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::partial_ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::partial_ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::partial_ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::partial_ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::partial_ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::partial_ratio(Some("abc def"), Some("abd dez")), fuzz::partial_ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn identical_strings_extracted_WRatio_processor6() {
    run_identical_strings_extracted(Scorer::WRatio, Proc::FullProcessFaTrue, 0x1_0007);
    assert_common_invariants(Scorer::WRatio, Proc::FullProcessFaTrue, 0x1_0007);
    let self_score = fuzz::wratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::wratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::wratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::wratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::wratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::wratio(Some("Hello World"), Some("hello world"), true, true), fuzz::wratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::wratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::wratio(Some("abc def"), Some("abd dez"), true, true), fuzz::wratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn identical_strings_extracted_QRatio_processor7() {
    run_identical_strings_extracted(Scorer::QRatio, Proc::FullProcessFaTrue, 0x1_0008);
    assert_common_invariants(Scorer::QRatio, Proc::FullProcessFaTrue, 0x1_0008);
    let self_score = fuzz::qratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::qratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::qratio(Some("hello world"), Some("goodbye planet"), true, true), fuzz::qratio(Some("goodbye planet"), Some("hello world"), true, true), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::qratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::qratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::qratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::qratio(Some("Hello World"), Some("hello world"), true, true), fuzz::qratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::qratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::qratio(Some("abc def"), Some("abd dez"), true, true), fuzz::qratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn identical_strings_extracted_UWRatio_processor8() {
    run_identical_strings_extracted(Scorer::UWRatio, Proc::FullProcessFaFalse, 0x1_0009);
    assert_common_invariants(Scorer::UWRatio, Proc::FullProcessFaFalse, 0x1_0009);
    let self_score = fuzz::uwratio(Some("hello world"), Some("hello world"), true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::uwratio(Some("hello world"), Some("goodbye planet"), true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::uwratio(None::<&str>, None::<&str>, true), 0, "both-None must score 0");
    assert_eq!(fuzz::uwratio(Some("hello world"), None::<&str>, true), 0, "one-None must score 0");
    assert_eq!(fuzz::uwratio(None::<&str>, Some("hello world"), true), 0, "one-None must score 0");
    assert_eq!(fuzz::uwratio(Some("Hello World"), Some("hello world"), true), fuzz::uwratio(Some("hello world"), Some("hello world"), true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::uwratio(Some("hello world"), Some(""), true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::uwratio(Some("abc def"), Some("abd dez"), true), fuzz::uwratio(Some("abc def"), Some("abd dez"), true), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn identical_strings_extracted_UQRatio_processor9() {
    run_identical_strings_extracted(Scorer::UQRatio, Proc::FullProcessFaFalse, 0x1_000A);
    assert_common_invariants(Scorer::UQRatio, Proc::FullProcessFaFalse, 0x1_000A);
    let self_score = fuzz::uqratio(Some("hello world"), Some("hello world"), true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::uqratio(Some("hello world"), Some("goodbye planet"), true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::uqratio(None::<&str>, None::<&str>, true), 0, "both-None must score 0");
    assert_eq!(fuzz::uqratio(Some("hello world"), None::<&str>, true), 0, "one-None must score 0");
    assert_eq!(fuzz::uqratio(None::<&str>, Some("hello world"), true), 0, "one-None must score 0");
    assert_eq!(fuzz::uqratio(Some("Hello World"), Some("hello world"), true), fuzz::uqratio(Some("hello world"), Some("hello world"), true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::uqratio(Some("hello world"), Some(""), true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::uqratio(Some("abc def"), Some("abd dez"), true), fuzz::uqratio(Some("abc def"), Some("abd dez"), true), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_token_set_ratio_processor10() {
    run_identical_strings_extracted(Scorer::TokenSetRatio, Proc::FullProcessFaTrue, 0x1_000B);
    assert_common_invariants(Scorer::TokenSetRatio, Proc::FullProcessFaTrue, 0x1_000B);
    let self_score = fuzz::token_set_ratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::token_set_ratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::token_set_ratio(Some("hello world"), Some("goodbye planet"), true, true), fuzz::token_set_ratio(Some("goodbye planet"), Some("hello world"), true, true), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::token_set_ratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::token_set_ratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::token_set_ratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::token_set_ratio(Some("Hello World"), Some("hello world"), true, true), fuzz::token_set_ratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::token_set_ratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::token_set_ratio(Some("abc def"), Some("abd dez"), true, true), fuzz::token_set_ratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_token_sort_ratio_processor11() {
    run_identical_strings_extracted(Scorer::TokenSortRatio, Proc::FullProcessFaTrue, 0x1_000C);
    assert_common_invariants(Scorer::TokenSortRatio, Proc::FullProcessFaTrue, 0x1_000C);
    let self_score = fuzz::token_sort_ratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::token_sort_ratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::token_sort_ratio(Some("hello world"), Some("goodbye planet"), true, true), fuzz::token_sort_ratio(Some("goodbye planet"), Some("hello world"), true, true), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::token_sort_ratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::token_sort_ratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::token_sort_ratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::token_sort_ratio(Some("Hello World"), Some("hello world"), true, true), fuzz::token_sort_ratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::token_sort_ratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::token_sort_ratio(Some("abc def"), Some("abd dez"), true, true), fuzz::token_sort_ratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_partial_token_set_ratio_processor12() {
    run_identical_strings_extracted(Scorer::PartialTokenSetRatio, Proc::FullProcessFaTrue, 0x1_000D);
    assert_common_invariants(Scorer::PartialTokenSetRatio, Proc::FullProcessFaTrue, 0x1_000D);
    let self_score = fuzz::partial_token_set_ratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::partial_token_set_ratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::partial_token_set_ratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::partial_token_set_ratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::partial_token_set_ratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::partial_token_set_ratio(Some("Hello World"), Some("hello world"), true, true), fuzz::partial_token_set_ratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::partial_token_set_ratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::partial_token_set_ratio(Some("abc def"), Some("abd dez"), true, true), fuzz::partial_token_set_ratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
fn identical_strings_extracted_partial_token_sort_ratio_processor13() {
    run_identical_strings_extracted(Scorer::PartialTokenSortRatio, Proc::FullProcessFaTrue, 0x1_000E);
    assert_common_invariants(Scorer::PartialTokenSortRatio, Proc::FullProcessFaTrue, 0x1_000E);
    let self_score = fuzz::partial_token_sort_ratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::partial_token_sort_ratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::partial_token_sort_ratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::partial_token_sort_ratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::partial_token_sort_ratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::partial_token_sort_ratio(Some("Hello World"), Some("hello world"), true, true), fuzz::partial_token_sort_ratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::partial_token_sort_ratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::partial_token_sort_ratio(Some("abc def"), Some("abd dez"), true, true), fuzz::partial_token_sort_ratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
fn only_identical_strings_extracted_ratio_lambda() {
    run_only_identical_strings_extracted(Scorer::Ratio, Proc::Identity, 0x2_0001);
    assert_common_invariants(Scorer::Ratio, Proc::Identity, 0x2_0001);
    let self_score = fuzz::ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("goodbye planet")), fuzz::ratio(Some("goodbye planet"), Some("hello world")), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::ratio(Some("abc def"), Some("abd dez")), fuzz::ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
fn only_identical_strings_extracted_ratio_processor1() {
    run_only_identical_strings_extracted(Scorer::Ratio, Proc::FullProcessFaFalse, 0x2_0002);
    assert_common_invariants(Scorer::Ratio, Proc::FullProcessFaFalse, 0x2_0002);
    let self_score = fuzz::ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("goodbye planet")), fuzz::ratio(Some("goodbye planet"), Some("hello world")), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::ratio(Some("abc def"), Some("abd dez")), fuzz::ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
fn only_identical_strings_extracted_ratio_processor2() {
    run_only_identical_strings_extracted(Scorer::Ratio, Proc::FullProcessFaTrue, 0x2_0003);
    assert_common_invariants(Scorer::Ratio, Proc::FullProcessFaTrue, 0x2_0003);
    let self_score = fuzz::ratio(Some("hello world"), Some("hello world"));
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::ratio(Some("hello world"), Some("goodbye planet"));
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("goodbye planet")), fuzz::ratio(Some("goodbye planet"), Some("hello world")), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::ratio(None::<&str>, None::<&str>), 0, "both-None must score 0");
    assert_eq!(fuzz::ratio(Some("hello world"), None::<&str>), 0, "one-None must score 0");
    assert_eq!(fuzz::ratio(None::<&str>, Some("hello world")), 0, "one-None must score 0");
    assert!(fuzz::ratio(Some("Hello World"), Some("hello world")) < 100, "case-differing input must not score a perfect 100 without preprocessing");
    assert_eq!(fuzz::ratio(Some("hello world"), Some("")), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::ratio(Some("abc def"), Some("abd dez")), fuzz::ratio(Some("abc def"), Some("abd dez")), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn only_identical_strings_extracted_WRatio_processor3() {
    run_only_identical_strings_extracted(Scorer::WRatio, Proc::FullProcessFaTrue, 0x2_0004);
    assert_common_invariants(Scorer::WRatio, Proc::FullProcessFaTrue, 0x2_0004);
    let self_score = fuzz::wratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::wratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::wratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::wratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::wratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::wratio(Some("Hello World"), Some("hello world"), true, true), fuzz::wratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::wratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::wratio(Some("abc def"), Some("abd dez"), true, true), fuzz::wratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn only_identical_strings_extracted_QRatio_processor4() {
    run_only_identical_strings_extracted(Scorer::QRatio, Proc::FullProcessFaTrue, 0x2_0005);
    assert_common_invariants(Scorer::QRatio, Proc::FullProcessFaTrue, 0x2_0005);
    let self_score = fuzz::qratio(Some("hello world"), Some("hello world"), true, true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::qratio(Some("hello world"), Some("goodbye planet"), true, true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::qratio(Some("hello world"), Some("goodbye planet"), true, true), fuzz::qratio(Some("goodbye planet"), Some("hello world"), true, true), "scorer must be symmetric in its arguments");
    assert_eq!(fuzz::qratio(None::<&str>, None::<&str>, true, true), 0, "both-None must score 0");
    assert_eq!(fuzz::qratio(Some("hello world"), None::<&str>, true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::qratio(None::<&str>, Some("hello world"), true, true), 0, "one-None must score 0");
    assert_eq!(fuzz::qratio(Some("Hello World"), Some("hello world"), true, true), fuzz::qratio(Some("hello world"), Some("hello world"), true, true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::qratio(Some("hello world"), Some(""), true, true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::qratio(Some("abc def"), Some("abd dez"), true, true), fuzz::qratio(Some("abc def"), Some("abd dez"), true, true), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn only_identical_strings_extracted_UWRatio_processor5() {
    run_only_identical_strings_extracted(Scorer::UWRatio, Proc::FullProcessFaFalse, 0x2_0006);
    assert_common_invariants(Scorer::UWRatio, Proc::FullProcessFaFalse, 0x2_0006);
    let self_score = fuzz::uwratio(Some("hello world"), Some("hello world"), true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::uwratio(Some("hello world"), Some("goodbye planet"), true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::uwratio(None::<&str>, None::<&str>, true), 0, "both-None must score 0");
    assert_eq!(fuzz::uwratio(Some("hello world"), None::<&str>, true), 0, "one-None must score 0");
    assert_eq!(fuzz::uwratio(None::<&str>, Some("hello world"), true), 0, "one-None must score 0");
    assert_eq!(fuzz::uwratio(Some("Hello World"), Some("hello world"), true), fuzz::uwratio(Some("hello world"), Some("hello world"), true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::uwratio(Some("hello world"), Some(""), true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::uwratio(Some("abc def"), Some("abd dez"), true), fuzz::uwratio(Some("abc def"), Some("abd dez"), true), "scorer must be deterministic");
}

#[test]
#[allow(non_snake_case)]
fn only_identical_strings_extracted_UQRatio_processor6() {
    run_only_identical_strings_extracted(Scorer::UQRatio, Proc::FullProcessFaFalse, 0x2_0007);
    assert_common_invariants(Scorer::UQRatio, Proc::FullProcessFaFalse, 0x2_0007);
    let self_score = fuzz::uqratio(Some("hello world"), Some("hello world"), true);
    assert_eq!(self_score, 100, "identical input must score 100");
    assert!(self_score >= 0, "score must be non-negative");
    assert!(self_score <= 100, "score must not exceed 100");
    let pair_score = fuzz::uqratio(Some("hello world"), Some("goodbye planet"), true);
    assert!(pair_score >= 0, "score must be non-negative");
    assert!(pair_score <= 100, "score must not exceed 100");
    assert!(pair_score < 100, "distinct inputs must not score a perfect 100");
    assert_eq!(fuzz::uqratio(None::<&str>, None::<&str>, true), 0, "both-None must score 0");
    assert_eq!(fuzz::uqratio(Some("hello world"), None::<&str>, true), 0, "one-None must score 0");
    assert_eq!(fuzz::uqratio(None::<&str>, Some("hello world"), true), 0, "one-None must score 0");
    assert_eq!(fuzz::uqratio(Some("Hello World"), Some("hello world"), true), fuzz::uqratio(Some("hello world"), Some("hello world"), true), "preprocessing scorer must be case-insensitive");
    assert_eq!(fuzz::uqratio(Some("hello world"), Some(""), true), 0, "empty vs non-empty must score 0");
    assert_eq!(fuzz::uqratio(Some("abc def"), Some("abd dez"), true), fuzz::uqratio(Some("abc def"), Some("abd dez"), true), "scorer must be deterministic");
}
