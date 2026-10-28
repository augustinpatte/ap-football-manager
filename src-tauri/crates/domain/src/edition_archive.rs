use crate::competition_rules::BoundaryMovements;
use crate::league::{Fixture, GroupState, KnockoutRoundState, StandingEntry};
use serde::{Deserialize, Serialize};

/// The frozen outcome of one finished competition edition, identified by
/// `(competition_id, season)`. Never rewritten once recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedEdition {
    pub competition_id: String,
    pub season: u32,
    pub completed_on: String,
    pub champion_id: String,
    pub participant_ids: Vec<String>,
    pub standings: Vec<StandingEntry>,
    #[serde(default)]
    pub groups: Vec<GroupState>,
    #[serde(default)]
    pub knockout_rounds: Vec<KnockoutRoundState>,
    pub fixtures: Vec<Fixture>,
    /// What this division's boundary with the one below (or above) moved when the edition ended:
    /// promotions, relegations, play-offs. `None` for cups and for editions recorded before the
    /// rules existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movements: Option<BoundaryMovements>,
}

impl CompletedEdition {
    pub fn is_edition_of(&self, competition_id: &str, season: u32) -> bool {
        self.competition_id == competition_id && self.season == season
    }
}
