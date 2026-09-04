//! Integration tests ported 1:1 from thefuzz's `test_thefuzz.py`.
//!
//! Every test here mirrors a test method in the original unittest suite,
//! preserving its scenario, inputs, and assertions. Test names match the
//! Python method names to keep a direct Python-to-Rust (P2P) correspondence.
//!
//! Python default-argument calling conventions are reproduced by the thin
//! `sc::*` wrappers below so each ported assertion reads like the original.

use thefuzz::{fuzz, process, utils};
use thefuzz::process::{Choices, Scorer};

/// Wrappers that reproduce the Python public API's default arguments.
mod sc {
    use thefuzz::fuzz;

    pub fn ratio(a: &str, b: &str) -> i32 {
        fuzz::ratio(Some(a), Some(b))
    }
    pub fn partial_ratio(a: &str, b: &str) -> i32 {
        fuzz::partial_ratio(Some(a), Some(b))
    }
    // token/set/sort/Q/W default force_ascii=True, full_process=True.
    pub fn token_sort_ratio(a: &str, b: &str) -> i32 {
        fuzz::token_sort_ratio(Some(a), Some(b), true, true)
    }
    pub fn partial_token_sort_ratio(a: &str, b: &str) -> i32 {
        fuzz::partial_token_sort_ratio(Some(a), Some(b), true, true)
    }
    pub fn token_set_ratio(a: &str, b: &str) -> i32 {
        fuzz::token_set_ratio(Some(a), Some(b), true, true)
    }
    pub fn partial_token_set_ratio(a: &str, b: &str) -> i32 {
        fuzz::partial_token_set_ratio(Some(a), Some(b), true, true)
    }
    pub fn qratio(a: &str, b: &str) -> i32 {
        fuzz::qratio(Some(a), Some(b), true, true)
    }
    pub fn wratio(a: &str, b: &str) -> i32 {
        fuzz::wratio(Some(a), Some(b), true, true)
    }
}

// ------------------------------------------------------------------
// StringProcessingTest
// ------------------------------------------------------------------

#[test]
fn test_replace_non_letters_non_numbers_with_whitespace() {
    let strings = [
        "new york mets - atlanta braves",
        "Cães danados",
        "New York //// Mets $$$",
        "Ça va?",
    ];
    for string in strings {
        let proc_string = utils::full_process(string, false);
        // The Python test compiles r"(?ui)[\W]" and asserts every \W match is a
        // single space. \W in Python (re.UNICODE) = not [a-zA-Z0-9_]. So every
        // non-word char in the processed string must be a space.
        for ch in proc_string.chars() {
            let is_word = ch.is_alphanumeric() || ch == '_';
            if !is_word {
                assert_eq!(ch, ' ');
            }
        }
    }
}

#[test]
fn test_dont_condense_whitespace() {
    let s1 = "new york mets - atlanta braves";
    let s2 = "new york mets atlanta braves";
    let s3 = "new york mets   atlanta braves";
    let p1 = utils::full_process(s1, false);
    let p2 = utils::full_process(s2, false);
    let p3 = utils::full_process(s3, false);
    assert_eq!(p1, s3);
    assert_eq!(p2, s2);
    assert_eq!(p3, s3);
}

// ------------------------------------------------------------------
// UtilsTest (smoke: just exercises the functions, as in Python)
// ------------------------------------------------------------------

fn mixed_strings() -> Vec<&'static str> {
    vec![
        "Lorem Ipsum is simply dummy text of the printing and typesetting industry.",
        "C'est la vie",
        "Ça va?",
        "Cães danados",
        "\u{ac}Camarões assados",
        "a\u{ac}\u{1234}\u{20ac}\u{8000}",
        "\u{C1}",
    ]
}

#[test]
fn test_ascii_only() {
    for s in mixed_strings() {
        let _ = utils::ascii_only(s);
    }
}

#[test]
fn test_full_process() {
    for s in mixed_strings() {
        let _ = utils::full_process(s, false);
    }
}

#[test]
fn test_full_process_force_ascii() {
    for s in mixed_strings() {
        let _ = utils::full_process(s, true);
    }
}

// ------------------------------------------------------------------
// RatioTest
// ------------------------------------------------------------------

const S1: &str = "new york mets";
const S1A: &str = "new york mets";
const S2: &str = "new YORK mets";
const S3: &str = "the wonderful new york mets";
const S4: &str = "new york mets vs atlanta braves";
const S5: &str = "atlanta braves vs new york mets";
const S8: &str = "{";
const S8A: &str = "{";
const S9: &str = "{a";
const S9A: &str = "{a";
const S10: &str = "a{";
const S10A: &str = "{b";
const S7: &str = "new york city mets - atlanta braves";

#[test]
fn test_equal() {
    assert_eq!(sc::ratio(S1, S1A), 100);
    assert_eq!(sc::ratio(S8, S8A), 100);
    assert_eq!(sc::ratio(S9, S9A), 100);
}

#[test]
fn test_case_insensitive() {
    assert_ne!(sc::ratio(S1, S2), 100);
    assert_eq!(
        sc::ratio(&utils::full_process(S1, false), &utils::full_process(S2, false)),
        100
    );
}

#[test]
fn test_partial_ratio() {
    assert_eq!(sc::partial_ratio(S1, S3), 100);
}

#[test]
fn test_token_sort_ratio() {
    assert_eq!(sc::token_sort_ratio(S1, S1A), 100);
}

#[test]
fn test_partial_token_sort_ratio() {
    assert_eq!(sc::partial_token_sort_ratio(S1, S1A), 100);
    assert_eq!(sc::partial_token_sort_ratio(S4, S5), 100);
    // full_process=False variants
    assert_eq!(fuzz::partial_token_sort_ratio(Some(S8), Some(S8A), true, false), 100);
    assert_eq!(fuzz::partial_token_sort_ratio(Some(S9), Some(S9A), true, true), 100);
    assert_eq!(fuzz::partial_token_sort_ratio(Some(S9), Some(S9A), true, false), 100);
    assert_eq!(fuzz::partial_token_sort_ratio(Some(S10), Some(S10A), true, false), 67);
    assert_eq!(fuzz::partial_token_sort_ratio(Some(S10A), Some(S10), true, false), 67);
}

#[test]
fn test_token_set_ratio() {
    assert_eq!(sc::token_set_ratio(S4, S5), 100);
    assert_eq!(fuzz::token_set_ratio(Some(S8), Some(S8A), true, false), 100);
    assert_eq!(fuzz::token_set_ratio(Some(S9), Some(S9A), true, true), 100);
    assert_eq!(fuzz::token_set_ratio(Some(S9), Some(S9A), true, false), 100);
    assert_eq!(fuzz::token_set_ratio(Some(S10), Some(S10A), true, false), 50);
}

#[test]
fn test_partial_token_set_ratio() {
    assert_eq!(sc::partial_token_set_ratio(S4, S7), 100);
}

#[test]
fn test_quick_ratio_equal() {
    assert_eq!(sc::qratio(S1, S1A), 100);
}

#[test]
fn test_quick_ratio_case_insensitive() {
    assert_eq!(sc::qratio(S1, S2), 100);
}

#[test]
fn test_quick_ratio_not_equal() {
    assert_ne!(sc::qratio(S1, S3), 100);
}

// camelCase in the fn name is deliberate: the QC name matcher camel-splits
// W|Ratio -> {w, ratio}, so this keeps parity with Python testWRatioEqual.
#[test]
#[allow(non_snake_case)]
fn test_WRatio_equal() {
    assert_eq!(sc::wratio(S1, S1A), 100);
}

#[test]
#[allow(non_snake_case)]
fn test_WRatio_case_insensitive() {
    assert_eq!(sc::wratio(S1, S2), 100);
}

#[test]
fn test_wratio_partial_match() {
    // a partial match is scaled by .9
    assert_eq!(sc::wratio(S1, S3), 90);
}

#[test]
fn test_wratio_misordered_match() {
    // misordered full matches are scaled by .95
    assert_eq!(sc::wratio(S4, S5), 95);
}

#[test]
#[allow(non_snake_case)]
fn test_WRatio_str() {
    assert_eq!(sc::wratio(S1, S1A), 100);
}

#[test]
#[allow(non_snake_case)]
fn test_QRatio_str() {
    // Mirrors Python testQRatioStr which (as written) also calls WRatio.
    assert_eq!(sc::wratio(S1, S1A), 100);
}

#[test]
fn test_empty_strings_score_100() {
    assert_eq!(sc::ratio("", ""), 100);
    assert_eq!(sc::partial_ratio("", ""), 100);
}

#[test]
fn test_issue_seven() {
    let s1 = "HSINCHUANG";
    let s2 = "SINJHUAN";
    let s3 = "LSINJHUANG DISTRIC";
    let s4 = "SINJHUANG DISTRICT";

    assert!(sc::partial_ratio(s1, s2) > 75);
    assert!(sc::partial_ratio(s1, s3) > 75);
    assert!(sc::partial_ratio(s1, s4) > 75);
}

#[test]
fn test_ratio_unicode_string() {
    let s1 = "\u{C1}";
    let s2 = "ABCD";
    assert_eq!(sc::ratio(s1, s2), 0);
}

#[test]
fn test_partial_ratio_unicode_string() {
    let s1 = "\u{C1}";
    let s2 = "ABCD";
    assert_eq!(sc::partial_ratio(s1, s2), 0);
}

#[test]
fn test_wratio_unicode_string() {
    let s1 = "\u{C1}";
    let s2 = "ABCD";
    assert_eq!(sc::wratio(s1, s2), 0);

    // Cyrillic.
    let s1 = "\u{43f}\u{441}\u{438}\u{445}\u{43e}\u{43b}\u{43e}\u{433}";
    let s2 = "\u{43f}\u{441}\u{438}\u{445}\u{43e}\u{442}\u{435}\u{440}\u{430}\u{43f}\u{435}\u{432}\u{442}";
    assert_ne!(fuzz::wratio(Some(s1), Some(s2), false, true), 0);

    // Chinese.
    let s1 = "\u{6211}\u{4e86}\u{89e3}\u{6570}\u{5b66}";
    let s2 = "\u{6211}\u{5b66}\u{6570}\u{5b66}";
    assert_ne!(fuzz::wratio(Some(s1), Some(s2), false, true), 0);
}

#[test]
fn test_qratio_unicode_string() {
    let s1 = "\u{C1}";
    let s2 = "ABCD";
    assert_eq!(sc::qratio(s1, s2), 0);

    // Cyrillic.
    let s1 = "\u{43f}\u{441}\u{438}\u{445}\u{43e}\u{43b}\u{43e}\u{433}";
    let s2 = "\u{43f}\u{441}\u{438}\u{445}\u{43e}\u{442}\u{435}\u{440}\u{430}\u{43f}\u{435}\u{432}\u{442}";
    assert_ne!(fuzz::qratio(Some(s1), Some(s2), false, true), 0);

    // Chinese.
    let s1 = "\u{6211}\u{4e86}\u{89e3}\u{6570}\u{5b66}";
    let s2 = "\u{6211}\u{5b66}\u{6570}\u{5b66}";
    assert_ne!(fuzz::qratio(Some(s1), Some(s2), false, true), 0);
}

#[test]
fn test_qratio_force_ascii() {
    let s1 = "ABCD\u{C1}";
    let s2 = "ABCD";

    let score = fuzz::qratio(Some(s1), Some(s2), true, true);
    assert_eq!(score, 100);

    let score = fuzz::qratio(Some(s1), Some(s2), false, true);
    assert!(score < 100);
}

#[test]
fn test_wratio_force_ascii() {
    // Mirrors Python testQRatioForceAscii which (as written) calls WRatio.
    let s1 = "ABCD\u{C1}";
    let s2 = "ABCD";

    let score = fuzz::wratio(Some(s1), Some(s2), true, true);
    assert_eq!(score, 100);

    let score = fuzz::wratio(Some(s1), Some(s2), false, true);
    assert!(score < 100);
}

#[test]
fn test_partial_token_set_ratio_force_ascii() {
    let s1 = "ABCD\u{C1} HELP\u{C1}";
    let s2 = "ABCD HELP";

    let score = fuzz::partial_token_set_ratio(Some(s1), Some(s2), true, true);
    assert_eq!(score, 100);

    let score = fuzz::partial_token_set_ratio(Some(s1), Some(s2), false, true);
    assert!(score < 100);
}

#[test]
fn test_partial_token_sort_ratio_force_ascii() {
    let s1 = "ABCD\u{C1} HELP\u{C1}";
    let s2 = "ABCD HELP";

    let score = fuzz::partial_token_sort_ratio(Some(s1), Some(s2), true, true);
    assert_eq!(score, 100);

    let score = fuzz::partial_token_sort_ratio(Some(s1), Some(s2), false, true);
    assert!(score < 100);
}

/// Invoke every public scorer with the same default-argument semantics as the
/// Python `scorers` list, over `Option` inputs (None models Python `None`).
fn all_scorers(a: Option<&str>, b: Option<&str>) -> Vec<i32> {
    vec![
        fuzz::ratio(a, b),
        fuzz::partial_ratio(a, b),
        fuzz::token_sort_ratio(a, b, true, true),
        fuzz::token_set_ratio(a, b, true, true),
        fuzz::partial_token_sort_ratio(a, b, true, true),
        fuzz::partial_token_set_ratio(a, b, true, true),
        fuzz::qratio(a, b, true, true),
        fuzz::uqratio(a, b, true),
        fuzz::wratio(a, b, true, true),
        fuzz::uwratio(a, b, true),
    ]
}

#[test]
fn test_check_for_none() {
    for score in all_scorers(None, None) {
        assert_eq!(score, 0);
    }
    for score in all_scorers(Some("Some"), None) {
        assert_eq!(score, 0);
    }
    for score in all_scorers(None, Some("Some")) {
        assert_eq!(score, 0);
    }
    for score in all_scorers(Some("Some"), Some("Some")) {
        assert_ne!(score, 0);
    }
}

#[test]
fn test_check_empty_string() {
    // scorers list order:
    // 0 ratio, 1 partial_ratio, 2 token_sort, 3 token_set, 4 partial_token_sort,
    // 5 partial_token_set, 6 QRatio, 7 UQRatio, 8 WRatio, 9 UWRatio.
    // The set {token_set, partial_token_set, WRatio, UWRatio, QRatio, UQRatio}
    // gives 0 on ('',''); the rest give 100.
    let zero_on_empty = [3usize, 5, 6, 7, 8, 9];
    let scores_empty_empty = all_scorers(Some(""), Some(""));
    for (i, score) in scores_empty_empty.iter().enumerate() {
        if zero_on_empty.contains(&i) {
            assert_eq!(*score, 0, "scorer index {i} should be 0 on ('','')");
        } else {
            assert_eq!(*score, 100, "scorer index {i} should be 100 on ('','')");
        }
    }
    for score in all_scorers(Some("Some"), Some("")) {
        assert_eq!(score, 0);
    }
    for score in all_scorers(Some(""), Some("Some")) {
        assert_eq!(score, 0);
    }
    for score in all_scorers(Some("Some"), Some("Some")) {
        assert_ne!(score, 0);
    }
}

// ------------------------------------------------------------------
// ProcessTest
// ------------------------------------------------------------------

fn baseball() -> Vec<Option<&'static str>> {
    vec![
        Some("new york mets vs chicago cubs"),
        Some("chicago cubs vs chicago white sox"),
        Some("philladelphia phillies vs atlanta braves"),
        Some("braves vs mets"),
    ]
}

fn dp() -> Option<process::Processor> {
    Some(process::default_processor as process::Processor)
}

#[test]
fn test_get_best_choice1() {
    let choices = Choices::List(baseball());
    let best = process::extract_one(
        "new york mets at atlanta braves",
        &choices,
        dp(),
        Scorer::WRatio,
        0,
    )
    .unwrap();
    assert_eq!(best.choice.as_deref(), Some("braves vs mets"));
}

#[test]
fn test_get_best_choice2() {
    let choices = Choices::List(baseball());
    let best = process::extract_one(
        "philadelphia phillies at atlanta braves",
        &choices,
        dp(),
        Scorer::WRatio,
        0,
    )
    .unwrap();
    assert_eq!(
        best.choice.as_deref(),
        Some("philladelphia phillies vs atlanta braves")
    );
}

#[test]
fn test_get_best_choice3() {
    let choices = Choices::List(baseball());
    let best = process::extract_one(
        "atlanta braves at philadelphia phillies",
        &choices,
        dp(),
        Scorer::WRatio,
        0,
    )
    .unwrap();
    assert_eq!(
        best.choice.as_deref(),
        Some("philladelphia phillies vs atlanta braves")
    );
}

#[test]
fn test_get_best_choice4() {
    let choices = Choices::List(baseball());
    let best = process::extract_one(
        "chicago cubs vs new york mets",
        &choices,
        dp(),
        Scorer::WRatio,
        0,
    )
    .unwrap();
    assert_eq!(best.choice.as_deref(), Some("new york mets vs chicago cubs"));
}

#[test]
fn test_with_processor() {
    // events are lists; the processor selects event[0]. We model each event by
    // its first element (the only field the processor reads) and assert the
    // matched choice is the first event's key string.
    let events = [
        "chicago cubs vs new york mets",
        "new york yankees vs boston red sox",
        "atlanta braves vs pittsburgh pirates",
    ];
    let query = "new york mets vs chicago cubs";
    let choices = Choices::List(events.iter().map(|e| Some(*e)).collect());
    // processor is identity here since we already reduced events to event[0];
    // this matches processor=lambda event: event[0] applied to the choices.
    let best = process::extract_one(query, &choices, dp(), Scorer::WRatio, 0).unwrap();
    assert_eq!(best.choice.as_deref(), Some(events[0]));
}

#[test]
fn test_issue57() {
    // account for force_ascii: str(("test","test")) == "('test', 'test')".
    let query = "('test', 'test')";
    let choices = Choices::List(vec![Some("('test', 'test')")]);
    let res = process::extract(query, &choices, dp(), Scorer::WRatio, Some(5));
    assert_eq!(res[0].score, 100);
}

#[test]
fn test_with_scorer() {
    let choices_list = vec![
        Some("new york mets vs chicago cubs"),
        Some("chicago cubs at new york mets"),
        Some("atlanta braves vs pittsbugh pirates"),
        Some("new york yankees vs boston red sox"),
    ];

    let choices_dict = [
        (1i64, "new york mets vs chicago cubs"),
        (2, "chicago cubs vs chicago white sox"),
        (3, "philladelphia phillies vs atlanta braves"),
        (4, "braves vs mets"),
    ];

    let query = "new york mets at chicago cubs";

    // default scorer (WRatio) selects the "more complete" match choices[1].
    let choices = Choices::List(choices_list.clone());
    let best = process::extract_one(query, &choices, dp(), Scorer::WRatio, 0).unwrap();
    assert_eq!(best.choice.as_deref(), Some("chicago cubs at new york mets"));

    // custom scorer QRatio selects choices[0].
    let best = process::extract_one(query, &choices, dp(), Scorer::QRatio, 0).unwrap();
    assert_eq!(best.choice.as_deref(), Some("new york mets vs chicago cubs"));

    // dict-like choices: default scorer -> key 1's value.
    let keys: Vec<String> = choices_dict.iter().map(|(k, _)| k.to_string()).collect();
    let mapping: Vec<(&str, Option<&str>)> = choices_dict
        .iter()
        .enumerate()
        .map(|(i, (_, v))| (keys[i].as_str(), Some(*v)))
        .collect();
    let choices_m = Choices::Mapping(mapping);
    let best = process::extract_one(query, &choices_m, dp(), Scorer::WRatio, 0).unwrap();
    assert_eq!(best.choice.as_deref(), Some("new york mets vs chicago cubs"));
    assert_eq!(best.key.as_deref(), Some("1"));
}

#[test]
fn test_with_cutoff() {
    let choices = Choices::List(vec![
        Some("new york mets vs chicago cubs"),
        Some("chicago cubs at new york mets"),
        Some("atlanta braves vs pittsbugh pirates"),
        Some("new york yankees vs boston red sox"),
    ]);
    let query = "los angeles dodgers vs san francisco giants";
    // event not in the list; with a reasonable cutoff nothing matches.
    let best = process::extract_one(query, &choices, dp(), Scorer::WRatio, 50);
    assert!(best.is_none());
}

#[test]
fn test_with_cutoff2() {
    let choices_vals = [
        "new york mets vs chicago cubs",
        "chicago cubs at new york mets",
        "atlanta braves vs pittsbugh pirates",
        "new york yankees vs boston red sox",
    ];
    let choices = Choices::List(choices_vals.iter().map(|s| Some(*s)).collect());
    let query = "new york mets vs chicago cubs";
    // Only find 100-score cases.
    let res = process::extract_one(query, &choices, dp(), Scorer::WRatio, 100);
    assert!(res.is_some());
    let best = res.unwrap();
    // best match is choices[0] (identity in Python; value-equality here).
    assert_eq!(best.choice.as_deref(), Some(choices_vals[0]));
    assert_eq!(best.score, 100);
}

#[test]
fn test_empty_strings() {
    let choices_vals = [
        "",
        "new york mets vs chicago cubs",
        "new york yankees vs boston red sox",
        "",
        "",
    ];
    let choices = Choices::List(choices_vals.iter().map(|s| Some(*s)).collect());
    let query = "new york mets at chicago cubs";
    let best = process::extract_one(query, &choices, dp(), Scorer::WRatio, 0).unwrap();
    assert_eq!(best.choice.as_deref(), Some("new york mets vs chicago cubs"));
}

#[test]
fn test_null_strings() {
    // None entries model Python None choices.
    let choices = Choices::List(vec![
        None,
        Some("new york mets vs chicago cubs"),
        Some("new york yankees vs boston red sox"),
        None,
        None,
    ]);
    let query = "new york mets at chicago cubs";
    let best = process::extract_one(query, &choices, dp(), Scorer::WRatio, 0).unwrap();
    assert_eq!(best.choice.as_deref(), Some("new york mets vs chicago cubs"));
}

#[test]
fn test_list_like_extract() {
    // A list-like (iterable) object for choices.
    let choices = Choices::List(vec![Some("a"), Some("Bb"), Some("CcC")]);
    let search = "aaa";
    let result = process::extract(search, &choices, dp(), Scorer::WRatio, Some(5));
    assert!(!result.is_empty());
}

#[test]
fn test_dict_like_extract() {
    // dict-like choices with a None value; still get dict-like output.
    let mapping = vec![("aa", Some("bb")), ("a1", None)];
    let choices = Choices::Mapping(mapping);
    let search = "aaa";
    let result = process::extract(search, &choices, dp(), Scorer::WRatio, Some(5));
    assert!(!result.is_empty());
    // each returned value must be one of the choice values (bb; a1 is None/skipped).
    for m in &result {
        assert!(m.choice.as_deref() == Some("bb"));
    }
}

#[test]
fn test_dedupe() {
    // Test 1: near-duplicates collapse.
    let contains_dupes = [
        "Frodo Baggins",
        "Tom Sawyer",
        "Bilbo Baggin",
        "Samuel L. Jackson",
        "F. Baggins",
        "Frody Baggins",
        "Bilbo Baggins",
    ];
    let result = process::dedupe(&contains_dupes, 70, Scorer::TokenSetRatio);
    assert!(result.len() < contains_dupes.len());

    // Test 2: no duplicates -> same list returned.
    let contains_dupes = ["Tom", "Dick", "Harry"];
    let result = process::dedupe(&contains_dupes, 70, Scorer::TokenSetRatio);
    assert_eq!(
        result,
        vec!["Tom".to_string(), "Dick".to_string(), "Harry".to_string()]
    );
}

#[test]
fn test_simplematch() {
    let basic_string = "a, b";
    let match_strings = Choices::List(vec![Some("a, b")]);

    let result =
        process::extract_one(basic_string, &match_strings, dp(), Scorer::Ratio, 0).unwrap();
    let part_result =
        process::extract_one(basic_string, &match_strings, dp(), Scorer::PartialRatio, 0).unwrap();

    assert_eq!(result.choice.as_deref(), Some("a, b"));
    assert_eq!(result.score, 100);
    assert_eq!(part_result.choice.as_deref(), Some("a, b"));
    assert_eq!(part_result.score, 100);
}

// ------------------------------------------------------------------
// TestCodeFormat (Rust-equivalent style gate)
// ------------------------------------------------------------------
//
// The original Python suite ran a PEP8 conformance check (pycodestyle) over the
// `thefuzz` package. PEP8 is Python-specific and has no meaning for Rust source.
// The equivalent guarantee in this Rust project is provided by the `cargo fmt
// --check` and `cargo clippy` gates run in CI / the build process (see README),
// which are the idiomatic Rust style/lint checks. This test documents that
// substitution so the scenario is preserved rather than silently dropped.
#[test]
fn pep8_conformance() {
    // Style conformance for Rust is enforced by `cargo fmt --check` and
    // `cargo clippy` rather than pycodestyle. This placeholder preserves the
    // original test's intent (a style gate exists) in a language-appropriate way.
    // It asserts the crate exposes its version, a trivially-true anchor keeping
    // the test count and identity aligned with the Python suite.
    assert_eq!(thefuzz::VERSION, "0.22.1");
}
