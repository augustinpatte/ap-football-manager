//! An interactive match inside a saved career.
//!
//! A career can be saved while a match is being played. What is stored is compact and authoritative
//! (never a replay, never a frame): the engine's checkpoint (input, configuration and the journal of
//! accepted commands with the tick each was applied at), the controller's own memory (authority,
//! assistant state, order ledger, computer managers' cursors) and the round baselines the career's
//! finalisation needs and cannot recompute after a reload.
//!
//! # Why the baselines travel
//! Starting a live session plays the rest of the day's fixtures and captures the league table as it
//! stood *before* the round, for the round digest. After a reload those fixtures are already played,
//! so re-deriving the baseline would read every delta as zero. They are saved, not recomputed.
//!
//! # Compatibility
//! * `format_version` guards this envelope; the engine's own `version` and the world digest guard the
//!   replay. A build whose engine plays the recorded commands differently refuses the checkpoint
//!   ([`RestoreProblem::Unusable`]) instead of resuming a different match.
//! * Nothing here changes a career without a stored match: old saves have none.
//! * A match that cannot be restored is discarded, the career loads, and its fixture is still
//!   scheduled, so the match can be played from kick-off.

use domain::league::StandingEntry;
use serde::{Deserialize, Serialize};

use super::{InteractiveCheckpoint, InteractiveError, InteractiveMatch};
use crate::live_match_manager::LiveMatchSession;
use crate::matchday::LeagueRoundContext;

/// Bumped when this envelope changes in a way an older build cannot read.
pub const FORMAT_VERSION: u32 = 1;

/// The parts of the application's live session that finalisation reads and a reload cannot rebuild.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundBaselines {
    pub round_matchday: u32,
    pub round_previous_standings: Vec<StandingEntry>,
    pub league_round_context: Option<LeagueRoundContext>,
}

impl RoundBaselines {
    pub fn of(session: &LiveMatchSession) -> Self {
        Self {
            round_matchday: session.round_matchday,
            round_previous_standings: session.round_previous_standings.clone(),
            league_round_context: session.league_round_context.clone(),
        }
    }

    /// Puts the saved baselines back on a live session rebuilt after a reload.
    pub fn apply(self, session: &mut LiveMatchSession) {
        session.round_matchday = self.round_matchday;
        session.round_previous_standings = self.round_previous_standings;
        session.league_round_context = self.league_round_context;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedMatch {
    format_version: u32,
    baselines: RoundBaselines,
    checkpoint: InteractiveCheckpoint,
}

/// Why a stored match was not resumed. Every one leaves the career loaded and the fixture
/// scheduled; the player is told once ([`RestoreProblem::translation_key`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreProblem {
    /// The stored text is not an interactive match.
    Unreadable,
    /// Written by a build with another envelope version.
    Incompatible(u32),
    /// The engine rejected the checkpoint, or replaying it does not reproduce the saved world.
    Unusable,
}

impl RestoreProblem {
    /// One message for all of them: the match starts again from kick-off.
    pub fn translation_key(&self) -> &'static str {
        "be.error.interactive.savedMatchDiscarded"
    }
}

/// The text to store for a match in progress.
pub fn encode(
    interactive: &InteractiveMatch,
    baselines: RoundBaselines,
) -> Result<String, InteractiveError> {
    let saved = SavedMatch {
        format_version: FORMAT_VERSION,
        baselines,
        checkpoint: interactive.checkpoint()?,
    };
    serde_json::to_string(&saved).map_err(|e| InteractiveError::Engine(e.to_string()))
}

/// Rebuilds the match and its baselines from stored text, or says why not.
pub fn decode(text: &str) -> Result<(RoundBaselines, InteractiveMatch), RestoreProblem> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| RestoreProblem::Unreadable)?;
    let version = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(RestoreProblem::Unreadable)?;
    if version != u64::from(FORMAT_VERSION) {
        return Err(RestoreProblem::Incompatible(version as u32));
    }
    let saved: SavedMatch =
        serde_json::from_value(value).map_err(|_| RestoreProblem::Unreadable)?;
    let interactive =
        InteractiveMatch::restore(saved.checkpoint).map_err(|_| RestoreProblem::Unusable)?;
    Ok((saved.baselines, interactive))
}
