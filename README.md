# thefuzz (Rust)

A Rust port of [seatgeek/thefuzz](https://github.com/seatgeek/thefuzz) v0.22.1 — fuzzy
string matching that scores how similar two strings are (0–100). This crate reproduces
thefuzz's behavior **exactly**: the same scorers, the same preprocessing, the same integer
scores (including thefuzz's own rounding), and the same `process`-module selection/ordering
semantics.

The scoring algorithms are the ones thefuzz delegates to
[RapidFuzz](https://github.com/rapidfuzz/RapidFuzz) (3.4.0). They are reimplemented directly
in pure Rust — this crate has **zero runtime dependencies** — and are verified score-for-score
against the Python reference across thousands of differential cases (see *Parity & testing*).

## Requirements

- Rust (edition 2021). Build/test with Cargo. No system libraries, no network, no external
  crates are required.

## Build, test, lint

```sh
cargo build
cargo test
cargo clippy --all-targets
cargo fmt --check
```

`cargo test` runs the full port of thefuzz's suite (unit tests, the process-warning test, and
the property tests). `cargo clippy`/`cargo fmt --check` are the Rust equivalents of thefuzz's
`pycodestyle` (PEP 8) style gate.

## Modules & API

The crate exposes four modules, mirroring thefuzz's `fuzz`, `process`, `utils` plus an
internal `distance` helper.

```rust
use thefuzz::{fuzz, process, utils};
```

### `fuzz` — scorers

Every scorer returns an `i32` in `0..=100`. `None` models a Python `None` argument (returns 0).
Names use Rust snake_case; the trailing `*_ratio` scorers that thefuzz spells in CamelCase
(`QRatio`, `WRatio`, …) map to `qratio`, `uqratio`, `wratio`, `uwratio`.

| Python (thefuzz)             | Rust                          | Signature |
| ---------------------------- | ----------------------------- | --------- |
| `ratio`                      | `ratio`                       | `(Option<&str>, Option<&str>) -> i32` |
| `partial_ratio`              | `partial_ratio`               | `(Option<&str>, Option<&str>) -> i32` |
| `token_sort_ratio`           | `token_sort_ratio`            | `(Option<&str>, Option<&str>, force_ascii: bool, full: bool) -> i32` |
| `token_set_ratio`            | `token_set_ratio`             | `(Option<&str>, Option<&str>, force_ascii: bool, full: bool) -> i32` |
| `partial_token_sort_ratio`   | `partial_token_sort_ratio`    | `(Option<&str>, Option<&str>, force_ascii: bool, full: bool) -> i32` |
| `partial_token_set_ratio`    | `partial_token_set_ratio`     | `(Option<&str>, Option<&str>, force_ascii: bool, full: bool) -> i32` |
| `QRatio`                     | `qratio`                      | `(Option<&str>, Option<&str>, force_ascii: bool, full: bool) -> i32` |
| `WRatio`                     | `wratio`                      | `(Option<&str>, Option<&str>, force_ascii: bool, full: bool) -> i32` |
| `UQRatio`                    | `uqratio`                     | `(Option<&str>, Option<&str>, full: bool) -> i32` |
| `UWRatio`                    | `uwratio`                     | `(Option<&str>, Option<&str>, full: bool) -> i32` |

Defaults in thefuzz: the token/`Q`/`W` scorers use `force_ascii=true, full=true`; `ratio` and
`partial_ratio` never preprocess; the `U*` scorers use `full=true` and force_ascii off. Pass
those values explicitly here.

```rust
use thefuzz::fuzz;

assert_eq!(fuzz::ratio(Some("new york mets"), Some("new york mets")), 100);
assert_eq!(fuzz::partial_ratio(Some("new york mets"),
                               Some("the wonderful new york mets")), 100);
assert_eq!(fuzz::token_sort_ratio(Some("new york mets"), Some("new YORK mets"),
                                  true, true), 100);
assert_eq!(fuzz::wratio(Some("new york mets vs atlanta braves"),
                        Some("atlanta braves vs new york mets"), true, true), 95);
```

### `utils` — preprocessing

- `utils::full_process(s: &str, force_ascii: bool) -> String` — thefuzz/RapidFuzz
  `default_process`: replace every non-alphanumeric character (Unicode) except `_` with a
  space, trim the ends, and lowercase; internal whitespace runs are **not** condensed. When
  `force_ascii` is set, code points `0x80..=0xFF` are deleted first (code points `>= 0x100`
  are kept).
- `utils::ascii_only(s: &str) -> String` — deletes code points `0x80..=0xFF`.
- `utils::default_process(s: &str) -> String` — `full_process(s, false)`.

### `process` — searching collections

```rust
pub enum Scorer {
    Ratio, PartialRatio, TokenSortRatio, TokenSetRatio,
    PartialTokenSortRatio, PartialTokenSetRatio,
    WRatio, QRatio, UWRatio, UQRatio,
    Custom(fn(&str, &str) -> f64),
}

pub type Processor = fn(&str) -> String;

pub enum Choices<'a> {
    List(Vec<Option<&'a str>>),                 // `None` models a Python None entry
    Mapping(Vec<(&'a str, Option<&'a str>)>),   // (key, choice)
}

pub struct Match {
    pub choice: Option<String>,
    pub score: i32,
    pub key: Option<String>,                    // Some(..) only for Mapping choices
}
```

| Python (thefuzz)        | Rust                       | Signature |
| ----------------------- | -------------------------- | --------- |
| `extractWithoutOrder`   | `extract_without_order`    | `(query, &Choices, Option<Processor>, Scorer, score_cutoff: i32) -> Vec<Match>` |
| `extractBests`          | `extract_bests`            | `(query, &Choices, Option<Processor>, Scorer, score_cutoff: i32, limit: Option<usize>) -> Vec<Match>` |
| `extract`               | `extract`                  | `(query, &Choices, Option<Processor>, Scorer, limit: Option<usize>) -> Vec<Match>` |
| `extractOne`            | `extract_one`              | `(query, &Choices, Option<Processor>, Scorer, score_cutoff: i32) -> Option<Match>` |
| `dedupe`                | `dedupe`                   | `(&[&str], threshold: i32, Scorer) -> Vec<String>` |

Semantics preserved from thefuzz: the default scorer is `WRatio` and the default processor is
`full_process`; `extract`'s default limit is 5; `score_cutoff` is inclusive (`>=`); `None`
choices are skipped; results are sorted by descending score with stable tie ordering; `Mapping`
results carry the key. `dedupe` keeps, for each cluster above `threshold`, the **longest** item
(ties broken alphabetically, taking the greatest), and returns the original list unchanged when
nothing was deduplicated. Pass `Some(process::default_processor)` for the default processor, or
`None`.

```rust
use thefuzz::process::{self, Choices, Scorer};

let baseball = Choices::List(vec![
    Some("new york mets vs chicago cubs"),
    Some("chicago cubs vs chicago white sox"),
    Some("philladelphia phillies vs atlanta braves"),
    Some("braves vs mets"),
]);

let best = process::extract_one(
    "new york mets at atlanta braves",
    &baseball,
    Some(process::default_processor),
    Scorer::WRatio,
    0,
);
assert_eq!(best.unwrap().choice.as_deref(), Some("braves vs mets"));
```

To capture the "processed query is empty" warning that thefuzz logs to the
`thefuzz.process` logger, install a thread-local hook:

```rust
use thefuzz::process::warning;
warning::set_hook(|msg| eprintln!("{msg}"));
// ... run a query whose processed form is empty ...
warning::clear_hook();
assert_eq!(warning::LOGGER_NAME, "thefuzz.process");
```

## Parity & testing

This port was built for exact behavioral parity with thefuzz 0.22.1 (RapidFuzz 3.4.0). All 71
tests from the original suite are ported and preserved (no test was removed, skipped, disabled,
or weakened):

- **`tests/thefuzz_test.rs`** — the 49 tests of `test_thefuzz.py`, ported 1:1 by name
  (snake_case), covering string processing, the utils smoke tests, all 30 `RatioTest`
  assertions (including the Unicode / `force_ascii` / `None` / empty-string matrices), and the
  13 `ProcessTest` cases.
- **`tests/warning_test.rs`** — `test_process_warning` from `test_thefuzz_pytest.py`: asserts
  exactly one warning with the exact message and the `thefuzz.process` logger name.
- **`tests/property_test.rs`** — the two Hypothesis property tests
  (`test_identical_strings_extracted`, `test_only_identical_strings_extracted`). Each Rust
  property function loops over **all** scorer/processor combinations (the 14 + 7 Hypothesis
  parametrizations) with 60 randomized examples each, using a self-contained seeded PRNG (no
  external crate), preserving the input domain (the `ascii_letters + digits + punctuation`
  alphabet, 1–10 strings of length 10–100), the `assume(processed != "")` guard, the
  `score_cutoff = 100` / unlimited-limit call, and both invariants.
- The Python `TestCodeFormat::test_pep8_conformance` (PEP 8 gate) maps to
  `test_code_format_style_gate_documented`, which pins the crate version and documents the
  Rust style gate (`cargo fmt --check` + `cargo clippy`) that replaces `pycodestyle`.

Beyond the ported suite, every scorer was validated against the Python reference over 4000+
randomized string pairs (ASCII, punctuation, and non-ASCII including code points above U+00FF)
plus fixed edge cases, with **0 mismatches** on all ten scorers.

## License

MIT — see [LICENSE](LICENSE). Copyright (c) 2014 SeatGeek.
