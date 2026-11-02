//! Direct, side-effect-free M12 execution using the versioned JSON contracts.

use crate::{
    game::Game,
    live_match_manager::adapt_spatial_fixture,
    spatial_adapter::AdaptedMatch,
    spatial_match_output::{DecodedSpatialOutput, OutputDecodeError, decode_json},
    spatial_match_projection::{ProjectionContext, project},
};
use apfm_match_lab::{
    MatchInput,
    sim::{
        SimulationConfig, export::simulate_autonomous_with, football::FootballConfig,
        motion::MotionConfig,
    },
};
use engine::MatchReport;

/// A completed M12 execution before its sporting result is applied to OFM.
/// Keeping the source document here means the 2D viewer and finalisation use
/// one deterministic run, never two independently generated matches.
#[derive(Clone)]
pub struct PreparedSpatialReplay {
    pub report: MatchReport,
    pub replay_json: String,
}

#[derive(Debug)]
pub enum SpatialRunError {
    InputContract(String),
    Engine(String),
    Output(OutputDecodeError),
}

impl std::fmt::Display for SpatialRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SpatialRunError {}

/// First identifier of a technical message (`"spatial adapter: WrongStarterCount { .. }"` gives
/// `WrongStarterCount`), safe to carry in a translation key's query string.
fn technical_code(message: &str) -> String {
    let message = message.strip_prefix("spatial adapter: ").unwrap_or(message);
    let code: String = message
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if code.is_empty() {
        "unknown".to_string()
    } else {
        code
    }
}

/// Percent-encodes `text` for a translation key's query string (the front end reads it with
/// `URLSearchParams`), cut to a length a message can show.
fn query_value(text: &str) -> String {
    const MAX_CHARS: usize = 160;
    let mut encoded = String::new();
    for byte in text.chars().take(MAX_CHARS).collect::<String>().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

impl SpatialRunError {
    /// The error as the player is told about it: a translation key with the technical cause, so a
    /// 2D match that cannot start says *why* instead of being reported as a corrupted replay.
    /// The engine never ran on a rejected input, and no replay is invented in its place.
    pub fn translation_key(&self) -> String {
        match self {
            // The match-day layer already speaks in keys (`be.error.liveMatch.*`): keep them.
            Self::InputContract(message) if message.starts_with("be.error.") => message.clone(),
            Self::InputContract(message) => format!(
                "be.error.spatialReplay.inputRejected?code={}",
                technical_code(message)
            ),
            Self::Engine(message) => format!(
                "be.error.spatialReplay.engineFailed?detail={}",
                query_value(message)
            ),
            Self::Output(error) => format!(
                "be.error.spatialReplay.outputInvalid?code={}",
                technical_code(&format!("{error:?}"))
            ),
        }
    }
}

/// Runs M12 from an already validated OFM adaptation. This function has no
/// access to `Game`, persistence, or post-match finalisation.
pub fn simulate(adapted: &AdaptedMatch) -> Result<DecodedSpatialOutput, SpatialRunError> {
    let json = serde_json::to_string(&adapted.input)
        .map_err(|error| SpatialRunError::InputContract(error.to_string()))?;
    let input = MatchInput::from_json(&json)
        .map_err(|error| SpatialRunError::InputContract(error.to_string()))?;
    let config = SimulationConfig::from_input(&input);
    let output = simulate_autonomous_with(
        &input,
        config,
        MotionConfig::default(),
        FootballConfig::full_match_m12(),
        |_| {},
    )
    .map_err(|error| SpatialRunError::Engine(error.to_string()))?;
    let json = output
        .output
        .to_json()
        .map_err(|error| SpatialRunError::Engine(error.to_string()))?;
    decode_json(&json).map_err(SpatialRunError::Output)
}

/// Simulates and validates a fixture without changing the career.
///
/// Keeping this operation pure is essential: callers can discard a rejected
/// M12 result without having to reconstruct a partially applied career.
pub fn build_fixture_report(
    game: &Game,
    fixture_index: usize,
) -> Result<MatchReport, SpatialRunError> {
    let adapted =
        adapt_spatial_fixture(game, fixture_index).map_err(SpatialRunError::InputContract)?;
    Ok(build_adapted_replay(&adapted)?.report)
}

fn build_adapted_replay(adapted: &AdaptedMatch) -> Result<PreparedSpatialReplay, SpatialRunError> {
    let decoded = simulate(adapted)?;
    let context = ProjectionContext {
        match_id: adapted.input.match_id.clone(),
        home_team_id: adapted.input.home.id.clone(),
        away_team_id: adapted.input.away.id.clone(),
        home_player_ids: adapted
            .input
            .home
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect(),
        away_player_ids: adapted
            .input
            .away
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect(),
    };
    let report = project(&context, &decoded.result)
        .map_err(|error| SpatialRunError::Engine(error.to_string()))?
        .report;
    Ok(PreparedSpatialReplay {
        report,
        replay_json: decoded.replay_json,
    })
}

/// Builds one replay for the live session's immutable fixture identity.
pub fn build_selected_fixture_replay(
    game: &Game,
    fixture_index: usize,
    competition_id: &str,
    fixture_id: &str,
) -> Result<PreparedSpatialReplay, SpatialRunError> {
    let mut selected_game = game.clone();
    if !selected_game.competitions.is_empty() {
        let competition = selected_game
            .competitions
            .iter()
            .find(|competition| competition.id == competition_id)
            .cloned()
            .ok_or_else(|| {
                SpatialRunError::InputContract("SelectedCompetitionNotFound".to_string())
            })?;
        let fixture = competition
            .fixtures
            .get(fixture_index)
            .ok_or_else(|| SpatialRunError::InputContract("SelectedFixtureNotFound".to_string()))?;
        if fixture.id != fixture_id {
            return Err(SpatialRunError::InputContract(
                "SelectedFixtureIdentityMismatch".to_string(),
            ));
        }
        selected_game.league = Some(competition);
    }
    let adapted = adapt_spatial_fixture(&selected_game, fixture_index)
        .map_err(SpatialRunError::InputContract)?;
    build_adapted_replay(&adapted)
}

/// Builds a report for the fixture identity retained by the application live
/// session. The legacy league mirror can point back to the domestic league
/// while a cup fixture is selected, so never trust a raw fixture index alone.
pub fn build_selected_fixture_report(
    game: &Game,
    fixture_index: usize,
    competition_id: &str,
    fixture_id: &str,
) -> Result<MatchReport, SpatialRunError> {
    Ok(build_selected_fixture_replay(game, fixture_index, competition_id, fixture_id)?.report)
}

/// Compatibility helper for non-interactive callers. Application code should
/// use [`build_fixture_report`] and its own authoritative match-day
/// finalisation, so the fixture, standings and captures advance together.
pub fn play_fixture(game: &mut Game, fixture_index: usize) -> Result<MatchReport, SpatialRunError> {
    let report = build_fixture_report(game, fixture_index)?;
    let fixture = game
        .league
        .as_ref()
        .and_then(|league| league.fixtures.get(fixture_index))
        .ok_or_else(|| SpatialRunError::InputContract("fixture not found".to_string()))?;
    let home_team_id = fixture.home_team_id.clone();
    let away_team_id = fixture.away_team_id.clone();
    crate::turn::apply_match_report(game, fixture_index, &home_team_id, &away_team_id, &report);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::{SpatialRunError, build_selected_fixture_replay};
    use crate::spatial_match_output::OutputDecodeError;
    use crate::{clock::GameClock, game::Game};
    use chrono::{TimeZone, Utc};
    use domain::{
        league::{Fixture, FixtureCompetition, FixtureStatus, League, StandingEntry},
        manager::Manager,
        player::{Player, PlayerAttributes, Position},
        team::Team,
    };

    fn attributes(position: Position) -> PlayerAttributes {
        let goalkeeper = matches!(position, Position::Goalkeeper);
        PlayerAttributes {
            pace: 68,
            stamina: 72,
            strength: 67,
            agility: 69,
            passing: 70,
            shooting: if goalkeeper { 25 } else { 71 },
            tackling: if goalkeeper { 25 } else { 66 },
            dribbling: if goalkeeper { 25 } else { 69 },
            defending: if goalkeeper { 25 } else { 66 },
            positioning: 68,
            vision: 69,
            decisions: 70,
            composure: 71,
            aggression: 52,
            teamwork: 73,
            leadership: 60,
            handling: if goalkeeper { 78 } else { 20 },
            reflexes: if goalkeeper { 78 } else { 20 },
            aerial: 68,
        }
    }

    fn player(id: String, team_id: &str, position: Position) -> Player {
        let mut player = Player::new(
            id.clone(),
            id.clone(),
            id,
            "1995-01-01".into(),
            "England".into(),
            position.clone(),
            attributes(position),
        );
        player.team_id = Some(team_id.into());
        player.condition = 94;
        player.morale = 76;
        player.fitness = 88;
        player
    }

    fn squad(team_id: &str) -> Vec<Player> {
        let positions = [
            Position::Goalkeeper,
            Position::Defender,
            Position::Defender,
            Position::Defender,
            Position::Defender,
            Position::Midfielder,
            Position::Midfielder,
            Position::Midfielder,
            Position::Midfielder,
            Position::Forward,
            Position::Forward,
            Position::Goalkeeper,
            Position::Defender,
            Position::Midfielder,
            Position::Forward,
        ];
        positions
            .into_iter()
            .enumerate()
            .map(|(index, position)| player(format!("{team_id}-p{index}"), team_id, position))
            .collect()
    }

    fn team(id: &str, name: &str) -> Team {
        Team::new(
            id.into(),
            name.into(),
            id[..3].into(),
            "England".into(),
            "London".into(),
            "Test Ground".into(),
            25_000,
        )
    }

    fn game_with_complete_m12_fixture() -> Game {
        let mut manager = Manager::new(
            "manager".into(),
            "Test".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        manager.hire("home".into());
        let home = team("home", "Home FC");
        let away = team("away", "Away FC");
        let fixture = Fixture {
            id: "fixture-m12-1".into(),
            competition_id: "competition-m12-1".into(),
            matchday: 1,
            date: "2025-06-15".into(),
            home_team_id: home.id.clone(),
            away_team_id: away.id.clone(),
            competition: FixtureCompetition::League,
            status: FixtureStatus::Scheduled,
            result: None,
            postponement: None,
        };
        let competition = League {
            id: "competition-m12-1".into(),
            name: "M12 Test League".into(),
            season: 1,
            fixtures: vec![fixture],
            standings: vec![
                StandingEntry::new(home.id.clone()),
                StandingEntry::new(away.id.clone()),
            ],
            ..League::default()
        };
        let mut players = squad(&home.id);
        players.extend(squad(&away.id));
        let mut game = Game::new(
            GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()),
            manager,
            vec![home, away],
            players,
            vec![],
            vec![],
        );
        game.seed = 73_926;
        game.league = Some(competition.clone());
        game.competitions = vec![competition];
        game
    }

    /// The fixture's game with each club given `extra` more players than an eleven, the user's
    /// club (`home`) fielding the eleven it chose by hand.
    fn game_with_substitutes(available_subs: usize) -> Game {
        let mut game = game_with_complete_m12_fixture();
        for team_id in ["home", "away"] {
            game.players
                .retain(|p| p.team_id.as_deref() != Some(team_id));
            let mut squad = squad(team_id);
            squad.truncate(11);
            let shape = [
                Position::Goalkeeper,
                Position::Defender,
                Position::Midfielder,
                Position::Forward,
            ];
            for index in 0..available_subs {
                squad.push(player(
                    format!("{team_id}-sub{index:02}"),
                    team_id,
                    shape[index % shape.len()].clone(),
                ));
            }
            game.players.extend(squad);
        }
        let chosen_by_hand: Vec<String> = (0..11).map(|i| format!("home-p{}", 10 - i)).collect();
        game.teams[0].starting_xi_ids = chosen_by_hand;
        game
    }

    /// Given each club with 0, 7, 12, 13, 15 or 20 substitutes available behind a hand-picked
    /// eleven (the user's) and an AI-picked one
    /// When the match is adapted for M12
    /// Then it is accepted for both teams with at most twelve on each bench, the user's eleven is
    /// exactly the one he chose, and nobody leaves the permanent squad.
    #[test]
    fn a_squad_bigger_than_the_bench_never_makes_the_match_invalid() {
        for available in [0, 7, 12, 13, 15, 20] {
            let game = game_with_substitutes(available);
            let squad_size_before = game.players.len();

            let adapted = crate::live_match_manager::adapt_spatial_fixture(&game, 0)
                .unwrap_or_else(|error| panic!("{available} substitutes: {error}"));

            for side in [&adapted.input.home, &adapted.input.away] {
                assert_eq!(side.lineup.len(), 11);
                assert_eq!(side.bench.len(), available.min(12), "{available} available");
            }
            let user_xi: Vec<&str> = adapted
                .input
                .home
                .lineup
                .iter()
                .map(|slot| slot.player.as_str())
                .collect();
            let chosen: Vec<String> = (0..11).map(|i| format!("home-p{}", 10 - i)).collect();
            assert_eq!(user_xi, chosen, "the user's eleven is not changed");
            assert_eq!(game.players.len(), squad_size_before, "no one is removed");
        }
    }

    /// Given the same game adapted twice
    /// When the bench is trimmed
    /// Then both calls name the same twelve in the same order.
    #[test]
    fn the_trimmed_bench_is_the_same_every_time() {
        let game = game_with_substitutes(20);
        let first = crate::live_match_manager::adapt_spatial_fixture(&game, 0).unwrap();
        let second = crate::live_match_manager::adapt_spatial_fixture(&game, 0).unwrap();
        assert_eq!(first.input.home.bench, second.input.home.bench);
        assert_eq!(first.input.away.bench, second.input.away.bench);
        assert!(
            first.input.home.bench.iter().any(|id| id.contains("sub00")),
            "the best-covering twelve keep a goalkeeper"
        );
    }

    /// Given complete OFM squads, tactics, fixture identifiers and a fixed seed,
    /// when M12 runs through the production adapter and projection, then it yields a validated,
    /// coherent and deterministic native match report.
    #[test]
    fn m12_execution_projects_a_deterministic_coherent_match_report() {
        let game = game_with_complete_m12_fixture();

        let first = build_selected_fixture_replay(&game, 0, "competition-m12-1", "fixture-m12-1")
            .expect("M12 must execute and pass output validation");
        let second = build_selected_fixture_replay(&game, 0, "competition-m12-1", "fixture-m12-1")
            .expect("the same M12 input must execute again");

        assert!(
            !first.replay_json.is_empty(),
            "M12 emitted its replay document"
        );
        assert_eq!(
            first.replay_json, second.replay_json,
            "the same fixture and seed must produce the same M12 replay"
        );
        assert_eq!(
            serde_json::to_value(&first.report).expect("serializable report"),
            serde_json::to_value(&second.report).expect("serializable report"),
            "the projected report is deterministic too"
        );
        assert_eq!(
            first.report.home_goals, first.report.home_stats.goals,
            "home score agrees with projected statistics"
        );
        assert_eq!(
            first.report.away_goals, first.report.away_stats.goals,
            "away score agrees with projected statistics"
        );
        assert_eq!(
            first.report.goals.len(),
            usize::from(first.report.home_goals + first.report.away_goals),
            "every score has one projected goal event"
        );
        assert!(
            first
                .report
                .events
                .iter()
                .all(|event| event.minute <= first.report.total_minutes),
            "projected events stay inside the simulated match duration"
        );
        assert!(
            first.report.player_stats.keys().all(|player_id| {
                player_id.starts_with("home-") || player_id.starts_with("away-")
            }),
            "projection only attributes statistics to selected fixture players"
        );
    }
    /// Given the three ways preparing a 2D match can fail
    /// When they are reported to the player
    /// Then each has its own translation key and technical code, never the generic
    /// "replay unavailable or corrupted" and never the Debug text of the error.
    #[test]
    fn a_failed_preparation_is_reported_with_its_own_key_and_code() {
        assert_eq!(
            SpatialRunError::InputContract(
                "spatial adapter: WrongStarterCount { found: 10 }".into()
            )
            .translation_key(),
            "be.error.spatialReplay.inputRejected?code=WrongStarterCount"
        );
        assert_eq!(
            SpatialRunError::InputContract("SelectedFixtureNotFound".into()).translation_key(),
            "be.error.spatialReplay.inputRejected?code=SelectedFixtureNotFound"
        );
        assert_eq!(
            SpatialRunError::InputContract("be.error.liveMatch.fixtureNotScheduled".into())
                .translation_key(),
            "be.error.liveMatch.fixtureNotScheduled",
            "a key from the match-day layer is passed through"
        );
        assert_eq!(
            SpatialRunError::Engine("M12 stub: real M12 engine unavailable".into())
                .translation_key(),
            "be.error.spatialReplay.engineFailed?detail=M12%20stub%3A%20real%20M12%20engine%20unavailable"
        );
        assert_eq!(
            SpatialRunError::Output(OutputDecodeError::InvalidJson).translation_key(),
            "be.error.spatialReplay.outputInvalid?code=InvalidJson"
        );
    }
}
