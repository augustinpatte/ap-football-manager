//! Reading a study's settings from the environment, once, for every study.
//!
//! Each study file used to carry its own copy of the same few lines (a comma list of seeds, a
//! season count, a world name). They are here so a new world name or a changed default is one edit.

use crate::worlds::WorldSpec;

/// The numbers in a comma-separated `raw` setting, or those in `default` when it is unset. Entries
/// that are not numbers are skipped.
fn parse(raw: Option<String>, default: &str) -> Vec<u64> {
    raw.unwrap_or_else(|| default.to_string())
        .split(',')
        .filter_map(|entry| entry.trim().parse().ok())
        .collect()
}

/// The comma-separated numbers in the environment variable `name`, or those in `default`.
pub fn list(name: &str, default: &str) -> Vec<u64> {
    parse(std::env::var(name).ok(), default)
}

/// The first number in `name`, or the first in `default`.
pub fn first(name: &str, default: &str) -> u64 {
    let numbers = list(name, default);
    numbers
        .first()
        .or(parse(None, default).first())
        .copied()
        .unwrap_or(0)
}

/// The world a study plays: `shipped`, `lower` or (by default) the 80-club `gate` world, named by
/// the environment variable `name`.
pub fn world(name: &str, seed: u64) -> WorldSpec {
    match std::env::var(name).as_deref() {
        Ok("shipped") => WorldSpec::shipped(seed),
        Ok("lower") => WorldSpec::lower_nations(seed),
        _ => WorldSpec::gate(seed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given a setting that is unset, one with a bad entry, and a clean one
    /// When each is parsed
    /// Then the default stands in for the first, the bad entry is skipped in the second, and the
    /// third is read as written.
    #[test]
    fn a_list_falls_back_to_its_default_and_skips_what_is_not_a_number() {
        assert_eq!(parse(None, "1,2, 3"), vec![1, 2, 3]);
        assert_eq!(parse(Some("4,x,5".to_string()), "9"), vec![4, 5]);
        assert_eq!(parse(Some("7".to_string()), "9"), vec![7]);
    }

    /// Given a setting that is not set in this process
    /// When its first number is read
    /// Then it is the default's.
    #[test]
    fn the_first_number_is_the_defaults_when_nothing_is_set() {
        assert_eq!(first("P45_A_SETTING_NOBODY_SETS", "9,10"), 9);
    }
}
