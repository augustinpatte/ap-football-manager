//! Sporting rules a competition carries with it: how a table is ordered, who moves between two
//! divisions at the end of a season (direct places, play-offs, barrages), which clubs are
//! reserve sides, and how a national cup is drawn and dated.
//!
//! Everything here is plain data on [`crate::league::CompetitionRules`], so it is saved with the
//! competition, and every field defaults to "nothing authored": a competition that carries none of
//! it plays by the generic rules OpenFootManager shipped with.

use serde::{Deserialize, Serialize};

/// How far a rule can be trusted, shown next to the rule in the competition rules documents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RuleStatus {
    /// Written in the source the importer read.
    #[default]
    Confirmed,
    /// A plausible value the source did not state.
    Unverified,
    /// Filled in by the game's authors where the source is silent or contradicts itself.
    Assumed,
}

/// One criterion of a league table, applied in order until the tie is broken. The `HeadToHead…`
/// criteria are read from the matches between the clubs still tied.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Tiebreaker {
    Points,
    GoalDifference,
    GoalsFor,
    Wins,
    AwayWins,
    AwayGoalsFor,
    HeadToHeadPoints,
    HeadToHeadGoalDifference,
    HeadToHeadGoalsFor,
    HeadToHeadAwayGoals,
    /// A draw of lots, fixed by the season and the clubs so a reload gives the same order.
    Lots,
}

/// Month and day of a calendar date that repeats every season.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonthDay {
    pub month: u8,
    pub day: u8,
}

/// A play-off among the clubs of the lower division finishing `from_rank..=to_rank`; the winner is
/// promoted on top of the directly promoted clubs. Played as a seeded bracket: the best-placed
/// clubs wait for the later rounds when the field is not a power of two.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromotionPlayoff {
    pub from_rank: u32,
    pub to_rank: u32,
    /// Legs of every round but the last (1 or 2).
    pub round_legs: u8,
    /// Legs of the final (1 or 2).
    pub final_legs: u8,
}

/// A barrage between a club of the upper division and the winner of a bracket among the lower
/// division's `lower_from_rank..=lower_to_rank`: the winner takes the upper-division place. A
/// single lower club (`from == to`) plays the upper club directly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Barrage {
    pub upper_rank: u32,
    pub lower_from_rank: u32,
    pub lower_to_rank: u32,
    /// Legs of the bracket among the lower clubs.
    pub bracket_legs: u8,
    /// Legs of the tie against the upper club.
    pub tie_legs: u8,
}

/// What moves between a division and the one directly below it, for one season onward.
///
/// Sizes must hold: `promoted_direct + 1` (when there is a promotion play-off) equals
/// `relegated_direct`, and with a barrage `promoted_direct == relegated_direct` (the barrage then
/// decides one extra exchange). [`BoundaryRule::is_consistent`] states it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundaryRule {
    /// Lower division, top-N: promoted without a play-off.
    pub promoted_direct: u32,
    pub promotion_playoff: Option<PromotionPlayoff>,
    /// Upper division, bottom-N: relegated without a play-off.
    pub relegated_direct: u32,
    pub barrage: Option<Barrage>,
    #[serde(default)]
    pub status: RuleStatus,
    /// Why the rule is what it is: the source, or what the authors changed and why.
    #[serde(default)]
    pub note: String,
}

impl BoundaryRule {
    /// Clubs that always change division: the direct places, plus the play-off winner.
    pub fn guaranteed_exchanges(&self) -> u32 {
        self.promoted_direct + u32::from(self.promotion_playoff.is_some())
    }

    /// Whether the two sides of the boundary hold the same number of places.
    pub fn is_consistent(&self) -> bool {
        match self.barrage {
            Some(_) => self.promoted_direct == self.relegated_direct,
            None => self.guaranteed_exchanges() == self.relegated_direct,
        }
    }
}

/// A [`BoundaryRule`] with the first season (the year the season starts) it applies to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeasonBoundary {
    pub from_season: u32,
    pub rule: BoundaryRule,
}

/// The movement rules of the boundary between this division and the one below it. Empty when
/// nothing is authored; the generic OpenFootManager swap then applies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LadderRules {
    pub boundaries: Vec<SeasonBoundary>,
}

impl LadderRules {
    /// The rule in force for `season`: the last entry that starts no later than it.
    pub fn rule_for(&self, season: u32) -> Option<&BoundaryRule> {
        self.boundaries
            .iter()
            .filter(|entry| entry.from_season <= season)
            .max_by_key(|entry| entry.from_season)
            .map(|entry| &entry.rule)
    }

    pub fn is_authored(&self) -> bool {
        !self.boundaries.is_empty()
    }
}

/// A reserve side (B team) and the club it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReserveSide {
    pub team_id: String,
    pub parent_id: String,
}

/// A reserve side taken out of its division because its parent club arrived there and there is no
/// division below to send it to. It comes back when the parent has left again.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WithdrawnSide {
    pub team_id: String,
    pub since_season: u32,
}

/// How the pairings of each round of a knockout cup are made.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum DrawMode {
    /// The order the clubs are listed in: the first plays the second, and so on. The way
    /// OpenFootManager always drew its cups.
    #[default]
    Sequential,
    /// A fixed-seed draw of the clubs that play, redone every round. The best-seeded clubs still
    /// take the byes of an uneven round.
    Random,
}

/// How a national cup is fed, drawn and dated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CupRules {
    pub draw: DrawMode,
    /// Divisions whose clubs play the cup. Read again at every rollover, so a club that changed
    /// division is still entered and a club that left the world is not.
    pub feeder_ids: Vec<String>,
    /// Reserve sides may not play the cup (Spain, Germany).
    pub exclude_reserve_sides: bool,
    /// Where the cup falls, counted back from the final: `[final, semi-finals, quarter-finals…]`.
    /// A round with no entry follows the previous round by the cup's round gap.
    pub round_dates: Vec<MonthDay>,
    /// What the authors simplified compared with the real cup, shown in the rules document.
    #[serde(default)]
    pub note: String,
}

impl CupRules {
    pub fn is_authored(&self) -> bool {
        self.draw != DrawMode::Sequential
            || !self.feeder_ids.is_empty()
            || !self.round_dates.is_empty()
    }
}

/// The result of one tie of a play-off or barrage, kept for the season history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayoffTie {
    pub stage: String,
    pub home_team_id: String,
    pub away_team_id: String,
    /// Score of each leg as `(home club's goals, away club's goals)`, the home club being the one
    /// named first above in every leg.
    pub legs: Vec<(u8, u8)>,
    pub penalties: Option<(u8, u8)>,
    pub winner_id: String,
}

/// What a division boundary moved at the end of one season. Kept in the edition archive of both
/// divisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BoundaryMovements {
    pub upper_competition_id: String,
    pub lower_competition_id: String,
    pub season: u32,
    pub promoted_direct: Vec<String>,
    pub promoted_playoff: Vec<String>,
    pub relegated_direct: Vec<String>,
    pub relegated_barrage: Vec<String>,
    pub ties: Vec<PlayoffTie>,
    /// Reserve sides left out of promotion because their parent club is in or above the division.
    pub reserves_blocked: Vec<String>,
    /// Reserve sides forced out of their division (relegated or withdrawn) by their parent.
    pub reserves_forced_down: Vec<String>,
    pub reserves_readmitted: Vec<String>,
}

impl BoundaryMovements {
    pub fn promoted(&self) -> impl Iterator<Item = &String> {
        self.promoted_direct.iter().chain(&self.promoted_playoff)
    }

    pub fn relegated(&self) -> impl Iterator<Item = &String> {
        self.relegated_direct.iter().chain(&self.relegated_barrage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(direct_up: u32, playoff: bool, direct_down: u32, barrage: bool) -> BoundaryRule {
        BoundaryRule {
            promoted_direct: direct_up,
            promotion_playoff: playoff.then_some(PromotionPlayoff {
                from_rank: 3,
                to_rank: 6,
                round_legs: 2,
                final_legs: 1,
            }),
            relegated_direct: direct_down,
            barrage: barrage.then_some(Barrage {
                upper_rank: 16,
                lower_from_rank: 3,
                lower_to_rank: 5,
                bracket_legs: 2,
                tie_legs: 2,
            }),
            status: RuleStatus::Confirmed,
            note: String::new(),
        }
    }

    #[test]
    fn a_play_off_winner_is_one_more_place_than_the_direct_ones() {
        assert!(rule(2, true, 3, false).is_consistent());
        assert!(!rule(2, false, 3, false).is_consistent());
    }

    #[test]
    fn a_barrage_exchanges_one_extra_club_so_the_direct_places_match() {
        assert!(rule(2, false, 2, true).is_consistent());
        assert!(!rule(2, false, 3, true).is_consistent());
    }

    #[test]
    fn the_rule_in_force_is_the_latest_one_that_has_started() {
        let ladder = LadderRules {
            boundaries: vec![
                SeasonBoundary {
                    from_season: 2026,
                    rule: rule(2, true, 3, false),
                },
                SeasonBoundary {
                    from_season: 2030,
                    rule: rule(3, false, 3, false),
                },
            ],
        };
        assert!(ladder.rule_for(2025).is_none());
        assert_eq!(ladder.rule_for(2026).unwrap().promoted_direct, 2);
        assert_eq!(ladder.rule_for(2029).unwrap().promoted_direct, 2);
        assert_eq!(ladder.rule_for(2031).unwrap().promoted_direct, 3);
    }
}
