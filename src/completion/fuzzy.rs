use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Substitute,
    Transpose,
    Insert,
    Delete,
}

pub type NeighborMap = HashMap<char, Vec<char>>;

pub fn is_neighbor(a: char, b: char, neighbors: &NeighborMap) -> bool {
    if a == b {
        return true;
    }
    neighbors.get(&a).is_some_and(|v| v.contains(&b))
        || neighbors.get(&b).is_some_and(|v| v.contains(&a))
}

/// Whether some prefix of `word` is one Damerau edit from `token`.
/// Exact prefixes (zero edits) return `None`; the dictionary handles those.
pub fn is_fuzzy_prefix(
    token: &str,
    word: &str,
    neighbors: &NeighborMap,
    min_fuzzy_len: usize,
    transpose_neighbors_only: bool,
) -> Option<EditKind> {
    let t: Vec<char> = token.chars().collect();
    let w: Vec<char> = word.chars().collect();
    let n = t.len();
    if n == 0 {
        return None;
    }
    if word.starts_with(token) {
        return None;
    }

    if n >= min_fuzzy_len && w.len() >= n && one_neighbor_subst(&t, &w[..n], neighbors) {
        return Some(EditKind::Substitute);
    }

    if n >= 2
        && w.len() >= n
        && let Some(i) = one_adjacent_transpose(&t, &w[..n])
    {
        let pair_ok = !transpose_neighbors_only || is_neighbor(t[i], t[i + 1], neighbors);
        if pair_ok {
            return Some(EditKind::Transpose);
        }
    }

    if n >= min_fuzzy_len && w.len() > n && one_delete(&w[..n + 1], &t) {
        return Some(EditKind::Insert);
    }

    if n >= min_fuzzy_len && w.len() + 1 >= n && one_delete(&t, &w[..n - 1]) {
        return Some(EditKind::Delete);
    }

    None
}

fn one_neighbor_subst(t: &[char], w: &[char], neighbors: &NeighborMap) -> bool {
    if t.len() != w.len() {
        return false;
    }
    let mut diffs = 0;
    for i in 0..t.len() {
        if t[i] == w[i] {
            continue;
        }
        diffs += 1;
        if diffs > 1 || !is_neighbor(t[i], w[i], neighbors) {
            return false;
        }
    }
    diffs == 1
}

fn one_adjacent_transpose(a: &[char], b: &[char]) -> Option<usize> {
    if a.len() != b.len() {
        return None;
    }
    let mut i = 0;
    while i < a.len() && a[i] == b[i] {
        i += 1;
    }
    if i + 1 >= a.len() {
        return None;
    }
    if a[i] == b[i + 1] && a[i + 1] == b[i] && a[i + 2..] == b[i + 2..] {
        return Some(i);
    }
    None
}

fn one_delete(longer: &[char], shorter: &[char]) -> bool {
    if longer.len() != shorter.len() + 1 {
        return false;
    }
    let mut i = 0;
    let mut j = 0;
    let mut skipped = false;
    while i < longer.len() && j < shorter.len() {
        if longer[i] == shorter[j] {
            i += 1;
            j += 1;
            continue;
        }
        if skipped {
            return false;
        }
        skipped = true;
        i += 1;
    }
    j == shorter.len() && (skipped || i < longer.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re_map() -> NeighborMap {
        let mut m = NeighborMap::new();
        m.insert('r', vec!['e']);
        m.insert('e', vec!['r']);
        m
    }

    #[test]
    fn subst_whrn_when() {
        assert_eq!(
            is_fuzzy_prefix("whrn", "when", &re_map(), 3, false),
            Some(EditKind::Substitute)
        );
        assert_eq!(
            is_fuzzy_prefix("whrn", "whenever", &re_map(), 3, false),
            Some(EditKind::Substitute)
        );
    }

    #[test]
    fn far_subst_rejected() {
        assert_eq!(
            is_fuzzy_prefix("whqn", "when", &NeighborMap::new(), 3, false),
            None
        );
    }

    #[test]
    fn omission_wen_when() {
        assert_eq!(
            is_fuzzy_prefix("wen", "when", &NeighborMap::new(), 3, false),
            Some(EditKind::Insert)
        );
    }

    #[test]
    fn omission_apostrophe() {
        assert_eq!(
            is_fuzzy_prefix("dont", "don't", &NeighborMap::new(), 3, false),
            Some(EditKind::Insert)
        );
    }

    #[test]
    fn transpose_wehn_when() {
        assert_eq!(
            is_fuzzy_prefix("wehn", "when", &NeighborMap::new(), 3, false),
            Some(EditKind::Transpose)
        );
    }

    #[test]
    fn transpose_no_on_at_len_2() {
        assert_eq!(
            is_fuzzy_prefix("no", "on", &NeighborMap::new(), 3, false),
            Some(EditKind::Transpose)
        );
    }

    #[test]
    fn exact_is_not_fuzzy() {
        assert_eq!(is_fuzzy_prefix("when", "when", &re_map(), 3, false), None);
        assert_eq!(
            is_fuzzy_prefix("the", "there", &NeighborMap::new(), 3, false),
            None
        );
    }

    #[test]
    fn extra_key_delete() {
        assert_eq!(
            is_fuzzy_prefix("whenn", "when", &NeighborMap::new(), 3, false),
            Some(EditKind::Delete)
        );
    }

    #[test]
    fn subst_gated_by_min_len() {
        let mut n = NeighborMap::new();
        n.insert('a', vec!['s']);
        assert_eq!(is_fuzzy_prefix("a", "s", &n, 3, false), None);
    }
}
