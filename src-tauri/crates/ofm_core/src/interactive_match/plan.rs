//! A team's tactical plan in OFM's own vocabulary, and its translation into M12 commands.
//!
//! The only conversion from OFM tactics to M12 sliders is `spatial_adapter::tactics`; the plan
//! here is what that function reads, and a change of plan becomes the M12 commands for exactly
//! the sliders that moved. Nothing here knows how M12 plays: it never recomputes a number.

use apfm_match_lab::sim::formation::FormationPreset;
use apfm_match_lab::sim::session::{MatchCommand, TacticalInstructions};
use domain::team::{PlayStyle, PressingIntensity, TacticsPhaseSettings, Team, Tempo};
use serde::{Deserialize, Serialize};

use crate::spatial_adapter::{self, Tactics};

/// Formations M12 can reshape a team into during a match.
pub fn supported_formations() -> Vec<String> {
    FormationPreset::ALL
        .iter()
        .map(|f| f.label().to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TacticalPlan {
    pub style: PlayStyle,
    pub phase: TacticsPhaseSettings,
    pub formation: String,
}

/// What a manager (or the assistant) asks to change. Only the `Some` fields change.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TacticalChange {
    #[serde(default)]
    pub style: Option<PlayStyle>,
    #[serde(default)]
    pub pressing: Option<PressingIntensity>,
    #[serde(default)]
    pub tempo: Option<Tempo>,
    #[serde(default)]
    pub formation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// M12 has no preset of that name, so it cannot reshape the team into it.
    UnsupportedFormation(String),
    /// The change alters nothing.
    NothingToChange,
}

impl TacticalPlan {
    pub fn of_team(team: &Team) -> Self {
        Self {
            style: team.play_style.clone(),
            phase: team.tactics_phase.clone(),
            formation: team.formation.clone(),
        }
    }

    /// The M12 sliders this plan means (the adapter's one conversion).
    pub fn sliders(&self) -> Tactics {
        spatial_adapter::tactics(&self.style, &self.phase)
    }

    pub fn with(&self, change: &TacticalChange) -> Self {
        let mut next = self.clone();
        if let Some(style) = &change.style {
            next.style = style.clone();
        }
        if let Some(pressing) = &change.pressing {
            next.phase.pressing_intensity = pressing.clone();
        }
        if let Some(tempo) = &change.tempo {
            next.phase.tempo = tempo.clone();
        }
        if let Some(formation) = &change.formation {
            next.formation = formation.clone();
        }
        next
    }
}

/// The M12 commands that carry a team from `from` to `to`: one per slider that moved, and a
/// formation command if the shape changed. Validated before anything is sent, so an order is
/// all or nothing.
pub fn commands_between(
    from: &TacticalPlan,
    to: &TacticalPlan,
) -> Result<Vec<MatchCommand>, PlanError> {
    if to.formation != from.formation && FormationPreset::from_label(&to.formation).is_none() {
        return Err(PlanError::UnsupportedFormation(to.formation.clone()));
    }
    let (a, b) = (from.sliders(), to.sliders());
    let mut commands = Vec::new();
    if a.mentality != b.mentality {
        commands.push(MatchCommand::SetMentality { value: b.mentality });
    }
    if a.pressing != b.pressing {
        commands.push(MatchCommand::SetPressing { value: b.pressing });
    }
    if a.tempo != b.tempo {
        commands.push(MatchCommand::SetTempo { value: b.tempo });
    }
    let instructions = TacticalInstructions {
        line_height: (a.line_height != b.line_height).then_some(b.line_height),
        width: (a.width != b.width).then_some(b.width),
        directness: (a.directness != b.directness).then_some(b.directness),
        ..TacticalInstructions::default()
    };
    if instructions != TacticalInstructions::default() {
        commands.push(MatchCommand::SetTacticalInstructions(instructions));
    }
    if to.formation != from.formation {
        commands.push(MatchCommand::SetFormation {
            formation: to.formation.clone(),
        });
    }
    if commands.is_empty() {
        return Err(PlanError::NothingToChange);
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> TacticalPlan {
        TacticalPlan {
            style: PlayStyle::Balanced,
            phase: TacticsPhaseSettings::default(),
            formation: "4-4-2".into(),
        }
    }

    /// Given a balanced plan,
    /// When the manager asks for an attacking style only,
    /// Then exactly one command carries the mentality the adapter maps that style to.
    #[test]
    fn a_style_change_becomes_one_mentality_command() {
        let from = plan();
        let to = from.with(&TacticalChange {
            style: Some(PlayStyle::Attacking),
            ..Default::default()
        });
        let expected = spatial_adapter::tactics(&PlayStyle::Attacking, &from.phase).mentality;
        assert_eq!(
            commands_between(&from, &to).unwrap(),
            vec![MatchCommand::SetMentality { value: expected }]
        );
    }

    /// Given a plan,
    /// When pressing, tempo and formation all change,
    /// Then each moved slider has its command, in a fixed order, and untouched sliders none.
    #[test]
    fn every_moved_slider_has_its_command_and_nothing_else() {
        let from = plan();
        let to = from.with(&TacticalChange {
            pressing: Some(PressingIntensity::Aggressive),
            tempo: Some(Tempo::Patient),
            formation: Some("4-3-3".into()),
            ..Default::default()
        });
        let commands = commands_between(&from, &to).unwrap();
        let names: Vec<_> = commands.iter().map(MatchCommand::name).collect();
        assert_eq!(names, ["set_pressing", "set_tempo", "set_formation"]);
        // and each carries its own slider's value, not another's
        let wanted = spatial_adapter::tactics(&to.style, &to.phase);
        assert_eq!(
            commands,
            vec![
                MatchCommand::SetPressing {
                    value: wanted.pressing
                },
                MatchCommand::SetTempo {
                    value: wanted.tempo
                },
                MatchCommand::SetFormation {
                    formation: "4-3-3".into()
                },
            ]
        );
        assert_ne!(
            wanted.pressing, wanted.tempo,
            "the values tell the sliders apart"
        );
    }

    /// Given a formation M12 cannot build,
    /// When it is ordered,
    /// Then the whole order is refused before any command exists; a no-op order is refused too.
    #[test]
    fn unsupported_formations_and_empty_orders_are_refused() {
        let from = plan();
        let bad = from.with(&TacticalChange {
            style: Some(PlayStyle::Attacking),
            formation: Some("2-3-5".into()),
            ..Default::default()
        });
        assert_eq!(
            commands_between(&from, &bad),
            Err(PlanError::UnsupportedFormation("2-3-5".into()))
        );
        assert_eq!(
            commands_between(&from, &from),
            Err(PlanError::NothingToChange)
        );
    }

    /// The formations offered in a match are exactly M12's presets.
    #[test]
    fn the_offered_formations_are_the_engines_presets() {
        let offered = supported_formations();
        assert_eq!(offered.len(), FormationPreset::ALL.len());
        assert!(
            offered
                .iter()
                .all(|f| FormationPreset::from_label(f).is_some())
        );
    }
}
