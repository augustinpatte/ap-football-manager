use serde::{Deserialize, Serialize};

use crate::team::TacticsPhaseSettings;

/// A tactic the player saved: a name, a formation and a play style, kept with the career rather
/// than the club, so it survives a change of club and travels with the save.
///
/// `formation` and `play_style` are the strings a team carries (`"4-4-2"`, `"HighPress"`); the
/// rules for which are valid live in `ofm_core::custom_tactics`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomTactic {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub formation: String,
    pub play_style: String,
    /// The defensive shape to use when this saved tactic is selected. Empty
    /// keeps the in-possession formation for legacy tactics.
    #[serde(default)]
    pub out_of_possession_formation: String,
    /// The manager's complete editable instruction blueprint. `None` means a
    /// tactic saved before editable instructions existed.
    #[serde(default)]
    pub tactics_phase: Option<TacticsPhaseSettings>,
    /// The preset this one was started from, if any.
    #[serde(default)]
    pub source_preset_name: Option<String>,
}
