//! Fuzzy scorers, matching thefuzz/fuzz.py (which delegates to rapidfuzz).
//!
//! All public scorers return an `i32` in `[0, 100]`, computed by rounding the
//! raw rapidfuzz float via Python's `int(round(x))` (round-half-to-even).

use crate::distance::{indel_distance, normalized_similarity};
use crate::utils::full_process;

// rapidfuzz `_norm_distance`. Keep the `100 - 100*dist/lensum` form exactly:
// it rounds differently from `(1 - dist/lensum)*100` and that float difference
// is load-bearing for parity (WRatio boundary cases).
fn norm_distance(dist: usize, lensum: usize) -> f64 {
    if lensum != 0 {
        100.0 - 100.0 * (dist as f64) / (lensum as f64)
    } else {
        100.0
    }
}

/// Python `round()` semantics: round-half-to-even, then truncate to int.
///
/// rapidfuzz returns integer-valued or fractional floats in `[0, 100]`;
/// `int(round(x))` uses banker's rounding (0.5 -> 0, 1.5 -> 2, 2.5 -> 2).
pub fn py_round_to_int(x: f64) -> i32 {
    let floor = x.floor();
    let diff = x - floor;
    let rounded = if diff < 0.5 {
        floor
    } else if diff > 0.5 {
        floor + 1.0
    } else {
        // Exactly halfway: round to even.
        let f = floor as i64;
        if f % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    };
    rounded as i32
}

// ---------------------------------------------------------------------------
// Raw (float) scorers operating on already-resolved char slices.
// ---------------------------------------------------------------------------

/// Raw ratio in `[0.0, 100.0]` = Indel normalized similarity * 100.
pub fn ratio_raw(a: &[char], b: &[char]) -> f64 {
    normalized_similarity(a, b) * 100.0
}

/// Raw partial_ratio, reproducing rapidfuzz `partial_ratio` exactly.
///
/// rapidfuzz scans three window groups of the longer string against the needle
/// (growing left-edge prefixes, full-length middle windows, shrinking
/// right-edge suffixes) and takes the best Indel similarity. When both inputs
/// have equal length it additionally re-runs the scan with the roles swapped
/// and keeps the higher score (e.g. partial_ratio("a{","{b") == 66.667, not the
/// plain ratio 50).
pub fn partial_ratio_raw(a: &[char], b: &[char]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 100.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (shorter, longer) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut score = partial_ratio_short_needle(shorter, longer);
    // Equal-length inputs: rapidfuzz retries with the needle/haystack swapped.
    if score != 1.0 && a.len() == b.len() {
        let score2 = partial_ratio_short_needle(longer, shorter);
        if score2 > score {
            score = score2;
        }
    }
    score * 100.0
}

/// The rapidfuzz `_partial_ratio_short_needle` scan, returning a `[0.0, 1.0]`
/// similarity. `needle` is the (not-longer) string, `haystack` the other.
fn partial_ratio_short_needle(needle: &[char], haystack: &[char]) -> f64 {
    let len1 = needle.len();
    let len2 = haystack.len();
    let needle_set: std::collections::HashSet<char> = needle.iter().copied().collect();
    let mut best = 0.0f64;
    // Group 1: growing left-edge prefixes haystack[..i], i in 1..len1.
    for i in 1..len1 {
        if !needle_set.contains(&haystack[i - 1]) {
            continue;
        }
        let sim = normalized_similarity(needle, &haystack[..i]);
        if sim > best {
            best = sim;
            if best == 1.0 {
                return 1.0;
            }
        }
    }
    // Group 2: full-length middle windows haystack[i..i+len1], i in 0..len2-len1.
    if len2 >= len1 {
        for i in 0..(len2 - len1) {
            if !needle_set.contains(&haystack[i + len1 - 1]) {
                continue;
            }
            let sim = normalized_similarity(needle, &haystack[i..i + len1]);
            if sim > best {
                best = sim;
                if best == 1.0 {
                    return 1.0;
                }
            }
        }
    }
    // Group 3: shrinking right-edge suffixes haystack[i..], i in len2-len1..len2.
    for i in (len2 - len1)..len2 {
        if !needle_set.contains(&haystack[i]) {
            continue;
        }
        let sim = normalized_similarity(needle, &haystack[i..]);
        if sim > best {
            best = sim;
            if best == 1.0 {
                return 1.0;
            }
        }
    }
    best
}

/// Tokenize on ASCII/Unicode whitespace (Python `str.split()` semantics:
/// split on runs of whitespace, dropping empty tokens).
fn tokenize(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_string()).collect()
}

fn join_chars(tokens: &[String]) -> Vec<char> {
    tokens.join(" ").chars().collect()
}

/// Raw token_sort_ratio: sort tokens, join, then `ratio_raw`.
pub fn token_sort_ratio_raw(a: &[char], b: &[char]) -> f64 {
    token_sort_generic(a, b, ratio_raw)
}

/// Raw partial_token_sort_ratio: sort tokens, join, then `partial_ratio_raw`.
pub fn partial_token_sort_ratio_raw(a: &[char], b: &[char]) -> f64 {
    token_sort_generic(a, b, partial_ratio_raw)
}

fn token_sort_generic(a: &[char], b: &[char], cmp: fn(&[char], &[char]) -> f64) -> f64 {
    let sa: String = a.iter().collect();
    let sb: String = b.iter().collect();
    let mut ta = tokenize(&sa);
    let mut tb = tokenize(&sb);
    ta.sort();
    tb.sort();
    let ja = join_chars(&ta);
    let jb = join_chars(&tb);
    cmp(&ja, &jb)
}

fn token_set_parts(a: &[char], b: &[char]) -> (bool, String, String, String) {
    use std::collections::BTreeSet;
    let sa: String = a.iter().collect();
    let sb: String = b.iter().collect();
    let ta: BTreeSet<String> = tokenize(&sa).into_iter().collect();
    let tb: BTreeSet<String> = tokenize(&sb).into_iter().collect();
    let intersection: Vec<String> = ta.intersection(&tb).cloned().collect();
    let diff_ab: Vec<String> = ta.difference(&tb).cloned().collect();
    let diff_ba: Vec<String> = tb.difference(&ta).cloned().collect();
    (
        !intersection.is_empty(),
        diff_ab.join(" "),
        diff_ba.join(" "),
        intersection.join(" "),
    )
}

// rapidfuzz token_set_ratio uses a length-distance form, NOT a max of three
// plain ratios; the intuitive form is wrong (yields 100 where rapidfuzz gives
// 67/80/86). Keep this exact.
pub fn token_set_ratio_raw(a: &[char], b: &[char]) -> f64 {
    use std::collections::BTreeSet;
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: String = a.iter().collect();
    let sb: String = b.iter().collect();
    let ta: BTreeSet<String> = tokenize(&sa).into_iter().collect();
    let tb: BTreeSet<String> = tokenize(&sb).into_iter().collect();
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let intersect = !ta.is_disjoint(&tb);
    let diff_ab: Vec<String> = ta.difference(&tb).cloned().collect();
    let diff_ba: Vec<String> = tb.difference(&ta).cloned().collect();
    if !intersect && (diff_ab.is_empty() || diff_ba.is_empty()) {
        return 100.0;
    }
    if intersect && (diff_ab.is_empty() || diff_ba.is_empty()) {
        return 100.0;
    }
    let diff_ab_joined = diff_ab.join(" ");
    let diff_ba_joined = diff_ba.join(" ");
    let ab_len = diff_ab_joined.chars().count();
    let ba_len = diff_ba_joined.chars().count();
    let intersection: Vec<String> = ta.intersection(&tb).cloned().collect();
    let sect_len = intersection.join(" ").chars().count();
    let sect_flag = if sect_len != 0 { 1 } else { 0 };
    let sect_ab_len = sect_len + sect_flag + ab_len;
    let sect_ba_len = sect_len + sect_flag + ba_len;

    let dv: Vec<char> = diff_ab_joined.chars().collect();
    let dw: Vec<char> = diff_ba_joined.chars().collect();
    let result = norm_distance(indel_distance(&dv, &dw), sect_ab_len + sect_ba_len);
    if sect_len == 0 {
        return result;
    }
    let sect_ab_ratio = norm_distance(sect_flag + ab_len, sect_len + sect_ab_len);
    let sect_ba_ratio = norm_distance(sect_flag + ba_len, sect_len + sect_ba_len);
    result.max(sect_ab_ratio).max(sect_ba_ratio)
}

pub fn partial_token_set_ratio_raw(a: &[char], b: &[char]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (intersect, diff_ab_joined, diff_ba_joined, _sect) = token_set_parts(a, b);
    if diff_ab_joined.is_empty() && diff_ba_joined.is_empty() && !intersect {
        return 0.0;
    }
    if intersect {
        return 100.0;
    }
    let dv: Vec<char> = diff_ab_joined.chars().collect();
    let dw: Vec<char> = diff_ba_joined.chars().collect();
    partial_ratio_raw(&dv, &dw)
}

fn token_ratio_raw(a: &[char], b: &[char]) -> f64 {
    token_set_ratio_raw(a, b).max(token_sort_ratio_raw(a, b))
}

// rapidfuzz `partial_token_ratio`: intersection => 100, else partial_ratio of
// the sorted-all-token joins, additionally max'd with partial_ratio of the
// sorted diffs unless the diffs already cover every token.
fn partial_token_ratio_raw(a: &[char], b: &[char]) -> f64 {
    use std::collections::BTreeSet;
    let sa: String = a.iter().collect();
    let sb: String = b.iter().collect();
    let split_a: Vec<String> = tokenize(&sa);
    let split_b: Vec<String> = tokenize(&sb);
    let ta: BTreeSet<String> = split_a.iter().cloned().collect();
    let tb: BTreeSet<String> = split_b.iter().cloned().collect();
    if !ta.is_disjoint(&tb) {
        return 100.0;
    }
    let diff_ab: Vec<String> = ta.difference(&tb).cloned().collect();
    let diff_ba: Vec<String> = tb.difference(&ta).cloned().collect();

    // rapidfuzz uses the FULL split lists (with duplicates), sorted, for the
    // all-token join; the diff-cover check compares full-split counts to the
    // set-diff counts. Deduplicating here would change the score.
    let mut sorted_a = split_a.clone();
    let mut sorted_b = split_b.clone();
    sorted_a.sort();
    sorted_b.sort();
    let all_a: Vec<char> = sorted_a.join(" ").chars().collect();
    let all_b: Vec<char> = sorted_b.join(" ").chars().collect();
    let result = partial_ratio_raw(&all_a, &all_b);

    if split_a.len() == diff_ab.len() && split_b.len() == diff_ba.len() {
        return result;
    }
    let mut dab = diff_ab;
    let mut dba = diff_ba;
    dab.sort();
    dba.sort();
    let dv: Vec<char> = dab.join(" ").chars().collect();
    let dw: Vec<char> = dba.join(" ").chars().collect();
    result.max(partial_ratio_raw(&dv, &dw))
}

// Mirrors rapidfuzz `WRatio` including its token_ratio/partial_token_ratio
// max-combining. score_cutoff is fixed at 0 (thefuzz re-rounds; sub-scorers
// return raw at cutoff 0), so the cutoff threading collapses to nested `max`.
pub fn wratio_raw(a: &[char], b: &[char]) -> f64 {
    const UNBASE_SCALE: f64 = 0.95;
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let base = ratio_raw(a, b);
    let len_a = a.len() as f64;
    let len_b = b.len() as f64;
    let len_ratio = len_a.max(len_b) / len_a.min(len_b);

    if len_ratio < 1.5 {
        base.max(token_ratio_raw(a, b) * UNBASE_SCALE)
    } else {
        let partial_scale = if len_ratio < 8.0 { 0.9 } else { 0.6 };
        let end_ratio = base.max(partial_ratio_raw(a, b) * partial_scale);
        end_ratio.max(partial_token_ratio_raw(a, b) * UNBASE_SCALE * partial_scale)
    }
}

/// Raw QRatio = ratio, but 0 if either side is empty.
pub fn qratio_raw(a: &[char], b: &[char]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    ratio_raw(a, b)
}

// ---------------------------------------------------------------------------
// Public scorers (Option inputs model Python None; return rounded i32).
// ---------------------------------------------------------------------------

fn cv(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// `fuzz.ratio(s1, s2)` — no preprocessing; None => 0.
pub fn ratio(s1: Option<&str>, s2: Option<&str>) -> i32 {
    match (s1, s2) {
        (Some(a), Some(b)) => py_round_to_int(ratio_raw(&cv(a), &cv(b))),
        _ => 0,
    }
}

/// `fuzz.partial_ratio(s1, s2)` — no preprocessing; None => 0.
pub fn partial_ratio(s1: Option<&str>, s2: Option<&str>) -> i32 {
    match (s1, s2) {
        (Some(a), Some(b)) => py_round_to_int(partial_ratio_raw(&cv(a), &cv(b))),
        _ => 0,
    }
}

/// Helper: preprocessing scorers None-guard to 0, else full_process both sides.
fn preprocessed(
    s1: Option<&str>,
    s2: Option<&str>,
    force_ascii: bool,
    full: bool,
    raw: fn(&[char], &[char]) -> f64,
) -> i32 {
    if full {
        let (a, b) = match (s1, s2) {
            (Some(a), Some(b)) => (a, b),
            _ => return 0,
        };
        let pa = full_process(a, force_ascii);
        let pb = full_process(b, force_ascii);
        py_round_to_int(raw(&cv(&pa), &cv(&pb)))
    } else {
        match (s1, s2) {
            (Some(a), Some(b)) => py_round_to_int(raw(&cv(a), &cv(b))),
            _ => 0,
        }
    }
}

/// `fuzz.token_sort_ratio(s1, s2, force_ascii=True, full_process=True)`.
pub fn token_sort_ratio(s1: Option<&str>, s2: Option<&str>, force_ascii: bool, full: bool) -> i32 {
    preprocessed(s1, s2, force_ascii, full, token_sort_ratio_raw)
}

/// `fuzz.partial_token_sort_ratio(...)`.
pub fn partial_token_sort_ratio(s1: Option<&str>, s2: Option<&str>, force_ascii: bool, full: bool) -> i32 {
    preprocessed(s1, s2, force_ascii, full, partial_token_sort_ratio_raw)
}

/// `fuzz.token_set_ratio(...)`.
pub fn token_set_ratio(s1: Option<&str>, s2: Option<&str>, force_ascii: bool, full: bool) -> i32 {
    preprocessed(s1, s2, force_ascii, full, token_set_ratio_raw)
}

/// `fuzz.partial_token_set_ratio(...)`.
pub fn partial_token_set_ratio(s1: Option<&str>, s2: Option<&str>, force_ascii: bool, full: bool) -> i32 {
    preprocessed(s1, s2, force_ascii, full, partial_token_set_ratio_raw)
}

/// `fuzz.QRatio(s1, s2, force_ascii=True, full_process=True)`.
pub fn qratio(s1: Option<&str>, s2: Option<&str>, force_ascii: bool, full: bool) -> i32 {
    preprocessed(s1, s2, force_ascii, full, qratio_raw)
}

/// `fuzz.UQRatio(s1, s2, full_process=True)` = QRatio with force_ascii=False.
pub fn uqratio(s1: Option<&str>, s2: Option<&str>, full: bool) -> i32 {
    qratio(s1, s2, false, full)
}

/// `fuzz.WRatio(s1, s2, force_ascii=True, full_process=True)`.
pub fn wratio(s1: Option<&str>, s2: Option<&str>, force_ascii: bool, full: bool) -> i32 {
    preprocessed(s1, s2, force_ascii, full, wratio_raw)
}

/// `fuzz.UWRatio(s1, s2, full_process=True)` = WRatio with force_ascii=False.
pub fn uwratio(s1: Option<&str>, s2: Option<&str>, full: bool) -> i32 {
    wratio(s1, s2, false, full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_half_even() {
        assert_eq!(py_round_to_int(0.5), 0);
        assert_eq!(py_round_to_int(1.5), 2);
        assert_eq!(py_round_to_int(2.5), 2);
        assert_eq!(py_round_to_int(69.23076923076923), 69);
        assert_eq!(py_round_to_int(75.78947368421052), 76);
    }

    #[test]
    fn ratio_golden() {
        assert_eq!(ratio(Some("new york mets"), Some("new york mets")), 100);
        assert_eq!(ratio(Some("new york mets"), Some("new YORK mets")), 69);
        assert_eq!(ratio(Some(""), Some("")), 100);
        assert_eq!(ratio(Some("Some"), Some("")), 0);
        assert_eq!(ratio(None, None), 0);
        assert_eq!(ratio(Some("Some"), None), 0);
    }

    #[test]
    fn partial_ratio_golden() {
        assert_eq!(partial_ratio(Some("new york mets"), Some("the wonderful new york mets")), 100);
        assert_eq!(partial_ratio(Some(""), Some("")), 100);
        assert!(partial_ratio(Some("HSINCHUANG"), Some("SINJHUAN")) > 75);
        assert!(partial_ratio(Some("HSINCHUANG"), Some("SINJHUANG DISTRICT")) > 75);
    }

    #[test]
    fn wratio_golden() {
        assert_eq!(wratio(Some("new york mets"), Some("new york mets"), true, true), 100);
        assert_eq!(wratio(Some("new york mets"), Some("new YORK mets"), true, true), 100);
        assert_eq!(wratio(Some("new york mets"), Some("the wonderful new york mets"), true, true), 90);
        assert_eq!(
            wratio(Some("new york mets vs atlanta braves"), Some("atlanta braves vs new york mets"), true, true),
            95
        );
    }

    #[test]
    fn qratio_golden() {
        assert_eq!(qratio(Some("new york mets"), Some("new york mets"), true, true), 100);
        assert_eq!(qratio(Some("new york mets"), Some("new YORK mets"), true, true), 100);
        assert_eq!(qratio(Some(""), Some(""), true, true), 0);
    }

    #[test]
    fn empty_string_matrix() {
        // ratio, partial_ratio, token_sort, partial_token_sort => 100 on ('','')
        assert_eq!(ratio(Some(""), Some("")), 100);
        assert_eq!(partial_ratio(Some(""), Some("")), 100);
        assert_eq!(token_sort_ratio(Some(""), Some(""), true, true), 100);
        assert_eq!(partial_token_sort_ratio(Some(""), Some(""), true, true), 100);
        // token_set, partial_token_set, W, Q => 0 on ('','')
        assert_eq!(token_set_ratio(Some(""), Some(""), true, true), 0);
        assert_eq!(partial_token_set_ratio(Some(""), Some(""), true, true), 0);
        assert_eq!(wratio(Some(""), Some(""), true, true), 0);
        assert_eq!(qratio(Some(""), Some(""), true, true), 0);
    }
}
