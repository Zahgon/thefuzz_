//! Indel (insertion/deletion) distance, LCS-based, matching rapidfuzz.
//!
//! Indel.distance(a, b) = len(a) + len(b) - 2 * LCS(a, b)
//! normalized_similarity = 1 - distance / (len(a) + len(b)); both-empty => 1.0

/// Length of the longest common subsequence of two char slices.
fn lcs_length(a: &[char], b: &[char]) -> usize {
    let n = a.len();
    let m = b.len();
    if n == 0 || m == 0 {
        return 0;
    }
    // Rolling 1-D DP over the shorter dimension to bound memory.
    let (a, b) = if m < n { (b, a) } else { (a, b) };
    let n = a.len();
    let m = b.len();
    let mut prev = vec![0usize; m + 1];
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        let ai = a[i - 1];
        for j in 1..=m {
            if ai == b[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j - 1]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        // curr now holds old prev row; clear it for reuse.
        for v in curr.iter_mut() {
            *v = 0;
        }
    }
    prev[m]
}

/// Indel distance between two char slices.
pub fn indel_distance(a: &[char], b: &[char]) -> usize {
    let lcs = lcs_length(a, b);
    a.len() + b.len() - 2 * lcs
}

/// Normalized Indel similarity in `[0.0, 1.0]`.
///
/// Both-empty yields 1.0 (matching rapidfuzz: identical empty sequences).
pub fn normalized_similarity(a: &[char], b: &[char]) -> f64 {
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let dist = indel_distance(a, b) as f64;
    1.0 - dist / (total as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn indel_known_distances() {
        assert_eq!(indel_distance(&cv("new york mets"), &cv("new YORK mets")), 8);
        assert_eq!(indel_distance(&cv("Some"), &cv("")), 4);
        assert_eq!(indel_distance(&cv("kitten"), &cv("sitting")), 5);
        assert_eq!(indel_distance(&cv("a{"), &cv("{b")), 2);
    }

    #[test]
    fn normalized_known() {
        let eps = 1e-12;
        assert!((normalized_similarity(&cv("new york mets"), &cv("new YORK mets")) - 0.6923076923076923).abs() < eps);
        assert_eq!(normalized_similarity(&cv("Some"), &cv("")), 0.0);
        assert!((normalized_similarity(&cv("kitten"), &cv("sitting")) - 0.6153846153846154).abs() < eps);
        assert_eq!(normalized_similarity(&cv("a{"), &cv("{b")), 0.5);
        assert_eq!(normalized_similarity(&cv(""), &cv("")), 1.0);
    }
}
