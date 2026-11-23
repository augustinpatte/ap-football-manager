//! Calendar specification and edition provenance; renewal policy lives in `ofm_core`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SeasonPhase {
    #[default]
    Annual,
    Opening,
    Closing,
}

/// Explicit division identity, independent of current participants or display names.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DivisionIdentity {
    #[serde(default)]
    pub family_id: String,
    #[serde(default)]
    pub tier: u32,
    #[serde(default)]
    pub phase: SeasonPhase,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CalendarDate {
    #[serde(default)]
    pub month: u8,
    #[serde(default)]
    pub day: u8,
}

/// How a league table's rounds fall across the week, the way a real league's do: a few fixtures on
/// Friday, most on Saturday and Sunday, a midweek round now and then when the season would not
/// otherwise fit. Absent (or with no weight at all): the legacy cadence, every fixture of a round on
/// one day, seven days after the last. The dates are generated, never official.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MatchdayPattern {
    /// Relative weight of each weekday (Monday first) for the fixtures of an ordinary round.
    #[serde(default)]
    pub weekend: [u8; 7],
    /// The same for a midweek round.
    #[serde(default)]
    pub midweek: [u8; 7],
    /// Rounds played midweek at least; the scheduler adds more when the season window needs them.
    #[serde(default)]
    pub midweek_rounds: u32,
}

impl MatchdayPattern {
    /// Whether the pattern says anything: an all-zero one is the legacy cadence.
    pub fn is_authored(&self) -> bool {
        self.weekend.iter().any(|weight| *weight > 0)
    }
}

/// Optional authored bounds. The opener remains League's existing month/day fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeasonCalendar {
    #[serde(default)]
    pub division: Option<DivisionIdentity>,
    /// An end before the opener's month/day belongs to the following year.
    #[serde(default)]
    pub window_end: Option<CalendarDate>,
    /// How the table's rounds fall across the week; absent keeps the legacy cadence. Not written when
    /// absent, so a league without one serializes exactly as it did before the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matchday_pattern: Option<MatchdayPattern>,
}

/// Interpretation of the existing `League::season`, without changing old counters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EditionBasis {
    CalendarYear,
    LegacyOrdinal {
        /// The saved season counter at `opener_year`, refreshed when this edition is generated.
        #[serde(default, alias = "first_season")]
        season_at_opener: u32,
        #[serde(default)]
        opener_year: i32,
    },
    #[default]
    Unresolved,
}

/// Durable definition identity, supported edition provenance and authored ordinary-table shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CalendarMetadata {
    #[serde(default)]
    pub definition_id: String,
    #[serde(default)]
    pub edition_basis: EditionBasis,
    /// Authored ordinary table legs; unknown on old saves that never stored the authoring format.
    #[serde(default)]
    pub league_legs: Option<u8>,
    /// Ordinary table cadence; cups retain their existing format-specific rules.
    #[serde(default)]
    pub matchday_gap_days: Option<u32>,
    #[serde(default)]
    pub season: SeasonCalendar,
}

#[cfg(test)]
mod tests {
    use super::{EditionBasis, MatchdayPattern, SeasonCalendar};
    use serde_json::json;

    /// Given tip JSON, when loaded and saved, then the old counter gets its precise name.
    #[test]
    fn legacy_ordinal_field_loads_and_writes_its_meaning() {
        let basis: EditionBasis = serde_json::from_value(json!({
            "kind": "legacyOrdinal", "first_season": 6, "opener_year": 2031
        }))
        .unwrap();
        assert_eq!(
            serde_json::to_value(basis).unwrap(),
            json!({
                "kind": "legacyOrdinal", "season_at_opener": 6, "opener_year": 2031
            })
        );
    }

    /// Given the renamed field, when loaded and saved, then its counter and defaults survive.
    #[test]
    fn season_at_opener_round_trips_without_losing_the_counter() {
        let raw = json!({"kind": "legacyOrdinal", "season_at_opener": 6, "opener_year": 2031});
        let basis: EditionBasis = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(basis).unwrap(), raw);
        let missing: EditionBasis =
            serde_json::from_value(json!({"kind": "legacyOrdinal"})).unwrap();
        assert_eq!(
            serde_json::to_value(missing).unwrap(),
            json!({
                "kind": "legacyOrdinal", "season_at_opener": 0, "opener_year": 0
            })
        );
    }
    /// Given ambiguous or malformed counters, when loaded, then deserialization rejects them.
    #[test]
    fn conflicting_or_invalid_ordinal_counters_are_rejected() {
        for raw in [
            json!({"kind": "legacyOrdinal", "first_season": 6, "season_at_opener": 7, "opener_year": 2031}),
            json!({"kind": "legacyOrdinal", "season_at_opener": "six", "opener_year": 2031}),
        ] {
            assert!(serde_json::from_value::<EditionBasis>(raw).is_err());
        }
    }
    /// Given a season calendar saved before weekday patterns existed,
    /// When it is loaded and saved again,
    /// Then it has no pattern and writes none: the legacy cadence, and the same JSON as before.
    #[test]
    fn a_calendar_without_a_matchday_pattern_loads_and_saves_unchanged() {
        let old = json!({"windowEnd": {"month": 5, "day": 31}});

        let calendar: SeasonCalendar = serde_json::from_value(old.clone()).unwrap();

        assert!(calendar.matchday_pattern.is_none());
        assert_eq!(
            serde_json::to_value(&calendar).unwrap()["windowEnd"],
            old["windowEnd"]
        );
        assert!(
            serde_json::to_value(&calendar)
                .unwrap()
                .get("matchdayPattern")
                .is_none()
        );
    }

    /// Given a pattern,
    /// When it is saved and loaded,
    /// Then every weight and the midweek minimum survive, and an all-zero pattern says nothing.
    #[test]
    fn a_matchday_pattern_round_trips_and_an_empty_one_is_not_authored() {
        let pattern = MatchdayPattern {
            weekend: [0, 0, 0, 0, 2, 3, 4],
            midweek: [0, 0, 1, 0, 0, 0, 0],
            midweek_rounds: 4,
        };

        let back: MatchdayPattern =
            serde_json::from_value(serde_json::to_value(&pattern).unwrap()).unwrap();

        assert_eq!(back, pattern);
        assert!(pattern.is_authored());
        assert!(!MatchdayPattern::default().is_authored());
    }
}
