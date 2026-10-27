//! Stable identifiers. Built from the world.db internal keys only.

pub fn player(person_id: i64) -> String {
    format!("fm-p-{person_id}")
}

pub fn staff(person_id: i64) -> String {
    format!("fm-s-{person_id}")
}

pub fn club(club_id: i64) -> String {
    format!("fm-c-{club_id}")
}

pub fn competition(competition_id: i64) -> String {
    format!("fm-k-{competition_id}")
}

/// The internal key behind an id, if it has the expected prefix.
pub fn internal_key(id: &str, prefix: &str) -> Option<i64> {
    id.strip_prefix(prefix)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_built_from_internal_keys() {
        assert_eq!(player(42), "fm-p-42");
        assert_eq!(staff(7), "fm-s-7");
        assert_eq!(club(498), "fm-c-498");
        assert_eq!(competition(12), "fm-k-12");
    }

    #[test]
    fn internal_key_round_trips_and_rejects_other_prefixes() {
        assert_eq!(internal_key("fm-p-42", "fm-p-"), Some(42));
        assert_eq!(internal_key("fm-s-42", "fm-p-"), None);
        assert_eq!(internal_key("fm-p-x", "fm-p-"), None);
    }
}
