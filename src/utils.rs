//! String preprocessing, matching thefuzz/utils.py + rapidfuzz.utils.default_process.

/// `ascii_only`: delete code points in the range 128..=255 (0x80..=0xFF).
///
/// Mirrors Python `translation_table = {i: None for i in range(128, 256)}`
/// applied via `str.translate`. Code points >= 256 are left intact.
pub fn ascii_only(s: &str) -> String {
    s.chars().filter(|&c| !((0x80..=0xFF).contains(&(c as u32)))).collect()
}

/// `default_process` (rapidfuzz.utils.default_process):
/// replace every char that is not alphanumeric and not `'_'` with a space,
/// then trim leading/trailing whitespace, then lowercase.
///
/// Internal whitespace runs are NOT condensed.
pub fn default_process(s: &str) -> String {
    // Replace non-(alphanumeric | '_') with a space.
    let replaced: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { ' ' })
        .collect();
    // Trim ASCII/Unicode whitespace on the ends (matches Python str.strip()),
    // then lowercase (Python str.lower / rapidfuzz lowercasing).
    let trimmed = replaced.trim_matches(|c: char| c.is_whitespace());
    trimmed.to_lowercase()
}

/// `full_process(s, force_ascii)`: optionally strip 128..255, then default_process.
pub fn full_process(s: &str, force_ascii: bool) -> String {
    if force_ascii {
        let a = ascii_only(s);
        default_process(&a)
    } else {
        default_process(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_process_cases() {
        assert_eq!(default_process("new york mets - atlanta braves"), "new york mets   atlanta braves");
        assert_eq!(default_process("  Hello World!  "), "hello world");
        assert_eq!(default_process("MiXeD CaSe 123"), "mixed case 123");
        assert_eq!(default_process(":::::::"), "");
        assert_eq!(default_process(""), "");
        assert_eq!(default_process("a-b_c.d"), "a b_c d");
    }

    #[test]
    fn ascii_only_cases() {
        assert_eq!(ascii_only("ABCD\u{00C1}"), "ABCD");
        assert_eq!(ascii_only("\u{1234}\u{20ac}"), "\u{1234}\u{20ac}");
        assert_eq!(ascii_only("a\u{00ac}b"), "ab");
    }

    #[test]
    fn full_process_force_ascii() {
        assert_eq!(full_process("ABCD\u{00C1}", true), "abcd");
        assert_eq!(full_process("ABCD\u{00C1}", false), "abcd\u{00e1}");
    }

    #[test]
    fn dont_condense_whitespace() {
        assert_eq!(default_process("new york mets atlanta braves"), "new york mets atlanta braves");
        assert_eq!(default_process("new york mets   atlanta braves"), "new york mets   atlanta braves");
    }
}
