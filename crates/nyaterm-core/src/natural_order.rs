//! Human-friendly name ordering.
//!
//! Plain lexicographic ordering puts `192.168.142.100` before `192.168.142.13`,
//! which reads as broken in a host list — and host lists are most of what this
//! app sorts. The pre-GPUI UI avoided that by sorting through
//! `localeCompare(a, b, { numeric: true, sensitivity: "base" })`; this is the
//! same rule, so saved connections keep the order users already know.

use std::cmp::Ordering;

/// Compare two names the way a person reads them: digit runs by value, the rest
/// case-insensitively.
///
/// Ties under case folding fall back to the raw comparison, so the result is a
/// total order and sorts stay deterministic.
pub fn natural_compare(left: &str, right: &str) -> Ordering {
    let mut left_rest = left;
    let mut right_rest = right;

    loop {
        match (left_rest.is_empty(), right_rest.is_empty()) {
            (true, true) => break,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => {}
        }

        if starts_with_digit(left_rest) && starts_with_digit(right_rest) {
            let (left_run, left_tail) = split_digit_run(left_rest);
            let (right_run, right_tail) = split_digit_run(right_rest);
            let ordering = compare_digit_runs(left_run, right_run);
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_rest = left_tail;
            right_rest = right_tail;
            continue;
        }

        let (left_char, left_tail) = split_char(left_rest);
        let (right_char, right_tail) = split_char(right_rest);
        let ordering = left_char.to_lowercase().cmp(right_char.to_lowercase());
        if ordering != Ordering::Equal {
            return ordering;
        }
        left_rest = left_tail;
        right_rest = right_tail;
    }

    // Equal once folded (`Host` vs `host`): keep a total order.
    left.cmp(right)
}

fn starts_with_digit(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_digit())
}

fn split_digit_run(text: &str) -> (&str, &str) {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    text.split_at(end)
}

fn split_char(text: &str) -> (char, &str) {
    let mut chars = text.chars();
    let first = chars.next().expect("caller checked the slice is non-empty");
    (first, chars.as_str())
}

/// Compare two runs of digits by value without parsing, so arbitrarily long runs
/// (a serial number, a timestamp) cannot overflow.
fn compare_digit_runs(left: &str, right: &str) -> Ordering {
    let left_value = left.trim_start_matches('0');
    let right_value = right.trim_start_matches('0');
    left_value
        .len()
        .cmp(&right_value.len())
        .then_with(|| left_value.cmp(right_value))
        // Same value, different padding: `007` after `7`.
        .then_with(|| left.len().cmp(&right.len()))
}

#[cfg(test)]
mod tests {
    use super::natural_compare;
    use std::cmp::Ordering;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        names.sort_by(|left, right| natural_compare(left, right));
        names
    }

    #[test]
    fn digit_runs_order_by_value_not_by_text() {
        assert_eq!(
            sorted(&[
                "192.168.142.100",
                "192.168.142.13",
                "192.168.142.9",
                "192.168.142.14",
            ]),
            vec![
                "192.168.142.9",
                "192.168.142.13",
                "192.168.142.14",
                "192.168.142.100",
            ]
        );
    }

    #[test]
    fn long_digit_runs_do_not_overflow() {
        // Longer than u128; a parsing implementation would fail here.
        let small = "node-99999999999999999999999999999999999998";
        let large = "node-99999999999999999999999999999999999999";
        assert_eq!(natural_compare(small, large), Ordering::Less);
    }

    #[test]
    fn padding_only_breaks_a_tie() {
        assert_eq!(natural_compare("host-7", "host-007"), Ordering::Less);
        assert_eq!(natural_compare("host-07", "host-8"), Ordering::Less);
    }

    #[test]
    fn letters_compare_case_insensitively_but_stay_totally_ordered() {
        assert_eq!(
            sorted(&["beta", "Alpha", "gamma"]),
            vec!["Alpha", "beta", "gamma"]
        );
        // Folding cannot report Equal for distinct names, or sorts would wobble.
        assert_ne!(natural_compare("Host", "host"), Ordering::Equal);
        assert_eq!(natural_compare("host", "host"), Ordering::Equal);
    }

    #[test]
    fn shorter_prefix_sorts_first_and_non_ascii_is_ordered() {
        assert_eq!(natural_compare("web", "web-1"), Ordering::Less);
        assert_eq!(natural_compare("", "a"), Ordering::Less);
        assert_eq!(natural_compare("南京", "南京讯思雅"), Ordering::Less);
        assert_ne!(natural_compare("北京", "南京"), Ordering::Equal);
    }
}
