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
    simulate_recording(adapted, REPLAY_CONDITION_SAMPLE_MS)
}

/// How often M12 writes each player's match condition into the replay, in milliseconds. A recorder
/// only: with it off or on the match, its events and its statistics are the same (a test proves
/// it), so the condition view reads real engine data and the result is unaffected.
const REPLAY_CONDITION_SAMPLE_MS: u32 = 1_000;

fn simulate_recording(
    adapted: &AdaptedMatch,
    condition_sample_ms: u32,
) -> Result<DecodedSpatialOutput, SpatialRunError> {
    simulate_with_engine_output(adapted, condition_sample_ms).map(|(decoded, _, _)| decoded)
}

/// [`simulate_recording`] that also hands back the engine's own output, which carries what the
/// decoded contract does not keep (the fatigue each player ended with).
fn simulate_with_engine_output(
    adapted: &AdaptedMatch,
    condition_sample_ms: u32,
) -> Result<
    (
        DecodedSpatialOutput,
        apfm_match_lab::MatchOutput,
        apfm_match_lab::MatchInput,
    ),
    SpatialRunError,
> {
    let json = serde_json::to_string(&adapted.input)
        .map_err(|error| SpatialRunError::InputContract(error.to_string()))?;
    let input = MatchInput::from_json(&json)
        .map_err(|error| SpatialRunError::InputContract(error.to_string()))?;
    let config = SimulationConfig::from_input(&input);
    let mut football = match_football_config();
    football.m11.replay_interval_ms = condition_sample_ms;
    let output =
        simulate_autonomous_with(&input, config, MotionConfig::default(), football, |_| {})
            .map_err(|error| SpatialRunError::Engine(error.to_string()))?;
    let json = output
        .output
        .to_json()
        .map_err(|error| SpatialRunError::Engine(error.to_string()))?;
    let decoded = decode_json(&json).map_err(SpatialRunError::Output)?;
    Ok((decoded, output.output, input))
}

/// The football every OFM match is played with, the instant, the replayed and the interactive one
/// alike. One place, so that a match watched and a match simulated cannot drift apart. Run F4.3:
/// `full_match_f43` is `full_match_f42` plus the final balance corrections (every team defends in the
/// compact block, a beaten defender can stop an attack with a foul, a flatter keeper, walking is not
/// rest); the earlier configurations are untouched, so a checkpoint written under one replays as it
/// was recorded. Run F4.4: `full_match_f44` is `full_match_f43` plus a defensive line that follows its
/// instruction continuously instead of jumping 14 m between two values of the slider. Run F5.2:
/// `full_match_f52` is `full_match_f44` plus the corrections that bring a match's volume of play to
/// football's (dead-ball time, shot selection, in-box discipline). Run F5.3: `full_match_f53` is
/// `full_match_f52` plus what a press costs (effort the legs are charged, a team stretched between its
/// lines, a deep team that still closes the carrier down in its own box); a match saved under F5.2
/// carries its own configuration and replays as it was recorded.
pub fn match_football_config() -> FootballConfig {
    FootballConfig::full_match_f53()
}

#[cfg(test)]
mod football_configuration_tests {
    use super::*;

    /// Given the football every OFM match is played with,
    /// Then it is the F5.3 one (F5.2 plus the press costs), which keeps what F5.2 established (F4.4 plus
    /// dead-ball time and the shot and discipline corrections) and
    /// everything F4.3 and F4.4 established: every team in the compact block, a beaten defender able to stop an
    /// attack, a booked player's second caution read leniently, walking not counted as rest, and a continuous
    /// defensive line. A revert to F4.4 would put the ball back in play for 96 % of a match, so it has to fail
    /// here: `dead_ball_time` is what tells F5.2 from F4.4.
    #[test]
    fn every_ofm_match_is_played_with_the_f53_football() {
        let config = match_football_config();
        assert_eq!(config, FootballConfig::full_match_f53());
        assert!(config.m12.press_costs.enabled);
        assert_ne!(config, FootballConfig::full_match_f52());
        assert!(config.m12.dead_ball_time);
        assert!(config.m12.continuous_line);
        assert!(config.m12.organised_block);
        assert!(config.referee.tactical_fouls);
        assert!(config.referee.booked_leniency > 0.0);
        assert!(config.condition.rest_cost < FootballConfig::full_match_f42().condition.rest_cost);
        assert_ne!(config, FootballConfig::full_match_f44());
    }
}

/// Records on each player of the report how much of his aerobic reserve the match engine spent on him
/// (`end fatigue - fatigue he arrived with`). The report's `physical_load` is what the career's one
/// post-match wear step reads, so a high press or a hectic game costs condition and a low block saves it.
pub fn attach_physical_load(
    report: &mut MatchReport,
    input: &apfm_match_lab::MatchInput,
    output: &apfm_match_lab::MatchOutput,
) {
    for end in &output.end_state {
        let arrived = input
            .home
            .players
            .iter()
            .chain(&input.away.players)
            .find(|p| p.id == end.id)
            .map_or(0.0, |p| p.condition.fatigue);
        if let Some(stats) = report.player_stats.get_mut(&end.id.0) {
            stats.physical_load = Some((end.fatigue_end - arrived).max(0.0));
        }
    }
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
    let (decoded, engine_output, engine_input) =
        simulate_with_engine_output(adapted, REPLAY_CONDITION_SAMPLE_MS)?;
    let mut report = project_decoded(&projection_context(adapted), &decoded)?;
    attach_physical_load(&mut report, &engine_input, &engine_output);
    Ok(PreparedSpatialReplay {
        report,
        replay_json: decoded.replay_json,
    })
}

fn projection_context(adapted: &AdaptedMatch) -> ProjectionContext {
    ProjectionContext {
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
    }
}

/// The projection context of the M12 input an interactive session runs (the same identities as
/// [`projection_context`], read from the engine's own input type).
pub fn session_projection_context(input: &MatchInput) -> ProjectionContext {
    let ids =
        |team: &apfm_match_lab::model::Team| team.players.iter().map(|p| p.id.0.clone()).collect();
    ProjectionContext {
        match_id: input.match_id.0.clone(),
        home_team_id: input.home.id.0.clone(),
        away_team_id: input.away.id.0.clone(),
        home_player_ids: ids(&input.home),
        away_player_ids: ids(&input.away),
    }
}

fn project_decoded(
    context: &ProjectionContext,
    decoded: &DecodedSpatialOutput,
) -> Result<MatchReport, SpatialRunError> {
    Ok(project(context, &decoded.result)
        .map_err(|error| SpatialRunError::Engine(error.to_string()))?
        .report)
}

/// The report of a match that an interactive session played to full time, through the same
/// validation and projection as a whole-match run. The session's output document is checked
/// like any other M12 output before it can reach the career.
pub fn report_from_session_output(
    context: &ProjectionContext,
    output: &apfm_match_lab::MatchOutput,
) -> Result<MatchReport, SpatialRunError> {
    let json = output
        .to_json()
        .map_err(|error| SpatialRunError::Engine(error.to_string()))?;
    let decoded = decode_json(&json).map_err(SpatialRunError::Output)?;
    project_decoded(context, &decoded)
}

/// Adapts the fixture a live session identifies. A cup fixture selected while the legacy league
/// mirror points at the domestic league is resolved by competition and fixture id, never by a
/// raw fixture index alone.
pub fn adapt_selected_fixture(
    game: &Game,
    fixture_index: usize,
    competition_id: &str,
    fixture_id: &str,
) -> Result<AdaptedMatch, SpatialRunError> {
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
    adapt_spatial_fixture(&selected_game, fixture_index).map_err(SpatialRunError::InputContract)
}

/// Builds one replay for the live session's immutable fixture identity.
pub fn build_selected_fixture_replay(
    game: &Game,
    fixture_index: usize,
    competition_id: &str,
    fixture_id: &str,
) -> Result<PreparedSpatialReplay, SpatialRunError> {
    build_adapted_replay(&adapt_selected_fixture(
        game,
        fixture_index,
        competition_id,
        fixture_id,
    )?)
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
pub(crate) mod tests {
    use super::{
        AdaptedMatch, REPLAY_CONDITION_SAMPLE_MS, SpatialRunError, adapt_selected_fixture,
        build_selected_fixture_replay, simulate,
    };
    use crate::spatial_match_output::OutputDecodeError;
    use crate::{clock::GameClock, game::Game};
    use apfm_match_lab::{
        MatchInput,
        sim::{
            SimulationConfig, export::simulate_autonomous_with, football::FootballConfig,
            motion::MotionConfig,
        },
    };
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

    pub(crate) fn game_with_complete_m12_fixture() -> Game {
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
    pub(crate) fn game_with_substitutes(available_subs: usize) -> Game {
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

    /// What the five-zone engine would field for the fixture: (starters, bench, formation) of each
    /// side, and the substitutions it allows.
    type SideSheet = (Vec<String>, Vec<String>, String);

    fn five_zone_sheet(game: &Game) -> (SideSheet, SideSheet, u8) {
        let session = crate::live_match_manager::create_live_match(
            game,
            0,
            crate::live_match_manager::MatchMode::Instant,
            false,
        )
        .expect("the five-zone engine can play the fixture");
        let snapshot = session.snapshot();
        let ids = |players: &[engine::PlayerData]| -> Vec<String> {
            players.iter().map(|player| player.id.clone()).collect()
        };
        (
            (
                ids(&snapshot.home_team.players),
                ids(&snapshot.home_bench),
                snapshot.home_team.formation.clone(),
            ),
            (
                ids(&snapshot.away_team.players),
                ids(&snapshot.away_bench),
                snapshot.away_team.formation.clone(),
            ),
            snapshot.max_subs,
        )
    }

    fn m12_sheet(adapted: &AdaptedMatch) -> (SideSheet, SideSheet, u8) {
        let side = |side: &crate::spatial_adapter::SpatialTeam| -> SideSheet {
            (
                side.lineup.iter().map(|slot| slot.player.clone()).collect(),
                side.bench.clone(),
                side.formation.clone(),
            )
        };
        (
            side(&adapted.input.home),
            side(&adapted.input.away),
            adapted.input.config.max_substitutions,
        )
    }

    /// Given a prepared match (the user's hand-picked eleven against an AI club's, each with
    ///   substitutes), in different formations
    /// When it is read as the five-zone engine would play it and as M12 would
    /// Then both name the same eleven in the same slots, the same bench, the same formation for
    ///   each side and the same number of substitutions: the engines differ in how they play,
    ///   never in who plays.
    #[test]
    fn both_engines_are_handed_the_same_match() {
        let mut game = game_with_substitutes(7);
        game.teams[0].formation = "4-3-3".to_string();
        game.teams[1].formation = "3-5-2".to_string();

        let five = five_zone_sheet(&game);
        let m12 = m12_sheet(
            &crate::live_match_manager::adapt_spatial_fixture(&game, 0).expect("M12 can adapt it"),
        );

        assert_eq!(five.0, m12.0, "the home side");
        assert_eq!(five.1, m12.1, "the away side");
        assert_eq!(five.2, m12.2, "the substitutions allowed");
        assert_eq!(five.0.2, "4-3-3");
        assert_eq!(five.1.2, "3-5-2");
    }

    /// Given a starter injured in the user's saved eleven and a starter suspended at the AI club
    /// When the match is read as each engine would play it
    /// Then neither fields either player, in the eleven or on the bench.
    #[test]
    fn neither_engine_fields_an_injured_or_a_suspended_player() {
        let mut game = game_with_substitutes(7);
        game.teams[0].formation = "4-4-2".to_string();
        game.teams[1].formation = "4-4-2".to_string();
        let injured = "home-p3";
        let suspended = "away-p4";
        // The control: both start the match when they are fit, in both engines.
        let fit_five = five_zone_sheet(&game);
        let fit_m12 = m12_sheet(
            &crate::live_match_manager::adapt_spatial_fixture(&game, 0).expect("M12 can adapt it"),
        );
        for sheet in [&fit_five, &fit_m12] {
            assert!(sheet.0.0.iter().any(|id| id == injured), "fit, he starts");
            assert!(sheet.1.0.iter().any(|id| id == suspended), "fit, he starts");
        }
        game.players
            .iter_mut()
            .find(|p| p.id == injured)
            .expect("he is in the squad")
            .injury = Some(domain::player::Injury {
            name: "Knock".to_string(),
            days_remaining: 12,
        });
        game.players
            .iter_mut()
            .find(|p| p.id == suspended)
            .expect("he is in the squad")
            .suspended_matches = 2;

        let five = five_zone_sheet(&game);
        let m12 = m12_sheet(
            &crate::live_match_manager::adapt_spatial_fixture(&game, 0).expect("M12 can adapt it"),
        );

        for (engine, sheet) in [("five-zone", &five), ("M12", &m12)] {
            for (named, side) in [(injured, &sheet.0), (suspended, &sheet.1)] {
                assert!(
                    !side.0.iter().chain(&side.1).any(|id| id == named),
                    "{engine} fielded {named}"
                );
            }
        }
        assert_eq!(five.0, m12.0);
        assert_eq!(five.1, m12.1);
    }

    /// The replay document the engine writes for `adapted` under an explicit football.
    fn replay_under(adapted: &AdaptedMatch, mut football: FootballConfig) -> String {
        let json = serde_json::to_string(&adapted.input).expect("serialise the input");
        let input = MatchInput::from_json(&json).expect("the input parses");
        football.m11.replay_interval_ms = REPLAY_CONDITION_SAMPLE_MS;
        let output = simulate_autonomous_with(
            &input,
            SimulationConfig::from_input(&input),
            MotionConfig::default(),
            football,
            |_| {},
        )
        .expect("the engine runs");
        output.output.to_json().expect("the output serialises")
    }

    /// Given a match played by F5.2 and by F5.3,
    /// When the instant path plays it,
    /// Then it is the F5.3 match and not the F5.2 one (either call site could silently revert to an
    /// earlier football, because the configuration test only looked at the function).
    #[test]
    fn the_instant_path_plays_the_f53_football() {
        let adapted = adapt_selected_fixture(
            &game_with_complete_m12_fixture(),
            0,
            "competition-m12-1",
            "fixture-m12-1",
        )
        .expect("adapts");
        let played = simulate(&adapted).expect("plays").replay_json;
        let f53 = replay_under(&adapted, FootballConfig::full_match_f53());
        let f52 = replay_under(&adapted, FootballConfig::full_match_f52());
        assert_ne!(f53, f52, "the fixture does not tell F5.2 from F5.3 apart");
        assert_eq!(
            played, f53,
            "the instant path is not playing the F5.3 football"
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
    /// Given the same complete fixture simulated with the condition recorder off and on
    /// When both replay documents are compared apart from the recorded condition
    /// Then they are identical (frames, events, statistics, end state), and only the second carries
    ///      a condition for every player on the pitch: recording does not change the match.
    #[test]
    fn recording_player_condition_does_not_change_the_match() {
        let game = game_with_complete_m12_fixture();
        let adapted =
            crate::live_match_manager::adapt_spatial_fixture(&game, 0).expect("the fixture adapts");

        let plain = super::simulate_recording(&adapted, 0).expect("M12 runs without the recorder");
        let recorded =
            super::simulate_recording(&adapted, 1_000).expect("M12 runs with the recorder");

        let strip = |json: &str| -> serde_json::Value {
            let mut value: serde_json::Value = serde_json::from_str(json).expect("json");
            if let Some(frames) = value["frames"].as_array_mut() {
                for frame in frames {
                    if let Some(object) = frame.as_object_mut() {
                        object.remove("extras");
                    }
                }
            }
            value
        };
        let plain_value = strip(&plain.replay_json);
        let recorded_value = strip(&recorded.replay_json);
        assert_eq!(
            plain_value, recorded_value,
            "same match, with or without it"
        );
        let plain_doc: serde_json::Value = serde_json::from_str(&plain.replay_json).unwrap();
        let recorded_doc: serde_json::Value = serde_json::from_str(&recorded.replay_json).unwrap();
        let has_condition = |doc: &serde_json::Value| {
            doc["frames"]
                .as_array()
                .is_some_and(|frames| frames.iter().any(|f| f["extras"]["agents"].is_array()))
        };
        assert!(!has_condition(&plain_doc));
        assert!(has_condition(&recorded_doc));
    }

    /// Writes the real M12 replay document of the complete fixture to `APFM_REPLAY_OUT`, for the
    /// viewer's real-data validation (`APFM_REAL_REPLAY` in `src/components/match`). Not a test of
    /// anything: it is the way to obtain a genuine document, and is ignored unless asked for.
    #[test]
    #[ignore = "writes a file: set APFM_REPLAY_OUT to the destination"]
    fn export_a_real_replay_for_the_viewer_validation() {
        let Some(out) = std::env::var_os("APFM_REPLAY_OUT") else {
            return;
        };
        let game = game_with_complete_m12_fixture();
        let replay = build_selected_fixture_replay(&game, 0, "competition-m12-1", "fixture-m12-1")
            .expect("M12 runs");
        std::fs::write(out, replay.replay_json).expect("the replay is written");
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
