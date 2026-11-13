//! Controlled match studies: one pairing, played by either engine, read as numbers.
//!
//! A career plays two engines. Unwatched fixtures (every AI match, every delegated match, every
//! long-career study) are played by the five-zone live engine; the match a person watches is played
//! by M12. A study that wants to say how the two differ has to put the same two prepared sides into
//! both, so this module installs one fixture in the game and plays it through each entry point the
//! game itself uses (`play_unwatched_fixture`, `spatial_runner::build_fixture_report`).
//!
//! Matches are seeded from `Game::seed` and the fixture id: a study varies the seed to get a new
//! match and keeps everything else still.

use domain::league::{CompetitionType, Fixture, FixtureStatus, League};
use domain::team::{PlayStyle, TacticsPhaseSettings};
use ofm_core::game::Game;
use ofm_core::live_match_manager::play_unwatched_fixture;
use ofm_core::spatial_runner::build_fixture_report;

/// Which engine plays the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Football {
    /// The five-zone live engine every unwatched fixture is played by.
    FiveZone,
    /// M12, the spatial engine the user's watched match is played by.
    M12,
}

/// One side of one match.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SideResult {
    pub points: f64,
    pub goals_for: f64,
    pub goals_against: f64,
    pub shots: f64,
    pub on_target: f64,
    /// Share of the ball, 0 to 100.
    pub possession: f64,
}

impl SideResult {
    fn of(goals_for: u8, goals_against: u8, shots: u16, on_target: u16, possession: f64) -> Self {
        let points = match goals_for.cmp(&goals_against) {
            std::cmp::Ordering::Greater => 3.0,
            std::cmp::Ordering::Equal => 1.0,
            std::cmp::Ordering::Less => 0.0,
        };
        Self {
            points,
            goals_for: f64::from(goals_for),
            goals_against: f64::from(goals_against),
            shots: f64::from(shots),
            on_target: f64::from(on_target),
            possession,
        }
    }

    /// The mean of each field over `results`; all zero for none.
    pub fn mean(results: &[SideResult]) -> Self {
        if results.is_empty() {
            return Self::default();
        }
        let n = results.len() as f64;
        let sum = |field: fn(&SideResult) -> f64| results.iter().map(field).sum::<f64>() / n;
        Self {
            points: sum(|r| r.points),
            goals_for: sum(|r| r.goals_for),
            goals_against: sum(|r| r.goals_against),
            shots: sum(|r| r.shots),
            on_target: sum(|r| r.on_target),
            possession: sum(|r| r.possession),
        }
    }
}

/// Both sides of a match, home first, from the numbers a report carries: (goals, shots, shots on
/// target) of the home side and of the away side, and the home side's share of the ball.
fn pair(
    home: (u8, u16, u16),
    away: (u8, u16, u16),
    home_possession: f64,
) -> (SideResult, SideResult) {
    (
        SideResult::of(home.0, away.0, home.1, home.2, home_possession),
        SideResult::of(away.0, home.0, away.1, away.2, 100.0 - home_possession),
    )
}

/// Makes `home` v `away`, today, the one fixture of the game's league.
fn install_fixture(game: &mut Game, home: &str, away: &str) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.league = Some(League {
        id: "match-study".to_string(),
        name: "Match study".to_string(),
        kind: CompetitionType::League,
        fixtures: vec![Fixture {
            id: "match-study-fixture".to_string(),
            competition_id: "match-study".to_string(),
            matchday: 1,
            date: today,
            home_team_id: home.to_string(),
            away_team_id: away.to_string(),
            status: FixtureStatus::Scheduled,
            ..Fixture::default()
        }],
        ..League::default()
    });
}

/// Plays `home` v `away` once with `football` and returns (home side, away side). The game's
/// standings and club state are only touched by the five-zone path (as in a career); M12's
/// report is built without applying it.
pub fn play(
    game: &mut Game,
    football: Football,
    home: &str,
    away: &str,
) -> Result<(SideResult, SideResult), String> {
    install_fixture(game, home, away);
    match football {
        Football::FiveZone => {
            let r = play_unwatched_fixture(game, 0)?.report;
            Ok(pair(
                (
                    r.home_goals,
                    r.home_stats.shots,
                    r.home_stats.shots_on_target,
                ),
                (
                    r.away_goals,
                    r.away_stats.shots,
                    r.away_stats.shots_on_target,
                ),
                r.home_possession,
            ))
        }
        Football::M12 => {
            let r = build_fixture_report(game, 0).map_err(|error| format!("{error:?}"))?;
            Ok(pair(
                (
                    r.home_goals,
                    r.home_stats.shots,
                    r.home_stats.shots_on_target,
                ),
                (
                    r.away_goals,
                    r.away_stats.shots,
                    r.away_stats.shots_on_target,
                ),
                r.home_possession,
            ))
        }
    }
}

/// Mean and standard error of the mean of `values`.
pub fn mean_se(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n.max(1.0);
    if values.len() < 2 {
        return (mean, f64::NAN);
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, (variance / n).sqrt())
}

/// The five-zone engine's own ratings of the side a club would field against `opponent` today, as
/// the contests read them: group averages of the attributes each contest uses.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EngineRatings {
    pub attack: f64,
    pub midfield: f64,
    pub defence: f64,
    pub goalkeeper: f64,
}

/// Prepares `club` v `opponent` (club at home) exactly as a kick-off does and reads the club's
/// ratings from the engine's own team, without playing the match.
pub fn engine_ratings(
    game: &mut Game,
    club: &str,
    opponent: &str,
) -> Result<EngineRatings, String> {
    install_fixture(game, club, opponent);
    let session = ofm_core::live_match_manager::kick_off_live_match(
        game,
        0,
        ofm_core::live_match_manager::MatchMode::Instant,
        false,
    )?;
    let team = session.match_state.snapshot().home_team;
    Ok(EngineRatings {
        attack: team.attack_rating(),
        midfield: team.midfield_rating(),
        defence: team.defense_rating(),
        goalkeeper: team.goalkeeper_rating(),
    })
}

/// The eleven a club fields, summarised by what the live engine's contests read of each group.
///
/// These are not [`EngineRatings`]: that type reads `TeamData::attack_rating` and friends, which use
/// strength, the keeper's pace and `decisions` where the resolvers use dribbling, teamwork and
/// aerial. Every mean here is of exactly the attributes the named contest in
/// `engine/src/live_match/zone_resolution.rs` averages, so a number that moves here moves a contest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LineupFeatures {
    pub defenders: f64,
    pub midfielders: f64,
    pub forwards: f64,
    /// Midfielders in the midfield contest, on the ball: dribbling, passing, vision, teamwork.
    pub mid_carry: f64,
    /// Midfielders in the midfield contest, winning it back: tackling, positioning, decisions, teamwork.
    pub mid_tackle: f64,
    /// Forwards in the attacking third: dribbling, pace, agility, composure.
    pub forward_run: f64,
    /// Defenders in the attacking third: defending, tackling, positioning, aerial.
    pub back_hold: f64,
    /// Forwards shooting: shooting, composure, decisions.
    pub finishing: f64,
    /// Defenders' share of how well a shot is defended: defending, positioning.
    pub back_screen: f64,
    /// The goalkeeper's saves: handling, reflexes, positioning.
    pub keeping: f64,
    /// Defenders starting a move: passing, vision, composure, teamwork.
    pub build_up: f64,
    /// Midfielders' press: stamina, tackling, pace.
    pub press: f64,
    /// Mean stored overall of the eleven.
    pub ovr: f64,
}

/// The mean of `read` over `rows`, or the engine's own fallback of 50 for a group with nobody in it.
fn mean_or_fallback<T>(rows: &[&T], read: impl Fn(&T) -> f64) -> f64 {
    if rows.is_empty() {
        return 50.0;
    }
    rows.iter().map(|row| read(row)).sum::<f64>() / rows.len() as f64
}

/// Prepares `club` v `opponent` (club at home) as a kick-off does and summarises the eleven the club
/// fields, without playing the match.
pub fn lineup_features(
    game: &mut Game,
    club: &str,
    opponent: &str,
) -> Result<LineupFeatures, String> {
    /// One fielded player as far as the contests are concerned.
    struct Fielded {
        group: String,
        attributes: std::collections::HashMap<&'static str, f64>,
        ovr: f64,
    }
    install_fixture(game, club, opponent);
    let session = ofm_core::live_match_manager::kick_off_live_match(
        game,
        0,
        ofm_core::live_match_manager::MatchMode::Instant,
        false,
    )?;
    let team = session.match_state.snapshot().home_team;
    // `engine::Position` is not a dependency of this crate; its `Debug` name is the group.
    let fielded: Vec<Fielded> = team
        .players
        .iter()
        .map(|p| Fielded {
            group: format!("{:?}", p.position),
            attributes: std::collections::HashMap::from([
                ("pace", f64::from(p.pace)),
                ("stamina", f64::from(p.stamina)),
                ("agility", f64::from(p.agility)),
                ("passing", f64::from(p.passing)),
                ("shooting", f64::from(p.shooting)),
                ("tackling", f64::from(p.tackling)),
                ("dribbling", f64::from(p.dribbling)),
                ("defending", f64::from(p.defending)),
                ("positioning", f64::from(p.positioning)),
                ("vision", f64::from(p.vision)),
                ("decisions", f64::from(p.decisions)),
                ("composure", f64::from(p.composure)),
                ("teamwork", f64::from(p.teamwork)),
                ("handling", f64::from(p.handling)),
                ("reflexes", f64::from(p.reflexes)),
                ("aerial", f64::from(p.aerial)),
            ]),
            ovr: f64::from(p.ovr),
        })
        .collect();
    let in_group =
        |name: &str| -> Vec<&Fielded> { fielded.iter().filter(|p| p.group == name).collect() };
    let average = |names: &[&'static str]| {
        let names = names.to_vec();
        move |p: &Fielded| names.iter().map(|n| p.attributes[n]).sum::<f64>() / names.len() as f64
    };
    let (defenders, midfielders, forwards, keepers) = (
        in_group("Defender"),
        in_group("Midfielder"),
        in_group("Forward"),
        in_group("Goalkeeper"),
    );
    Ok(LineupFeatures {
        defenders: defenders.len() as f64,
        midfielders: midfielders.len() as f64,
        forwards: forwards.len() as f64,
        mid_carry: mean_or_fallback(
            &midfielders,
            average(&["dribbling", "passing", "vision", "teamwork"]),
        ),
        mid_tackle: mean_or_fallback(
            &midfielders,
            average(&["tackling", "positioning", "decisions", "teamwork"]),
        ),
        forward_run: mean_or_fallback(
            &forwards,
            average(&["dribbling", "pace", "agility", "composure"]),
        ),
        back_hold: mean_or_fallback(
            &defenders,
            average(&["defending", "tackling", "positioning", "aerial"]),
        ),
        finishing: mean_or_fallback(&forwards, average(&["shooting", "composure", "decisions"])),
        back_screen: mean_or_fallback(&defenders, average(&["defending", "positioning"])),
        keeping: mean_or_fallback(&keepers, average(&["handling", "reflexes", "positioning"])),
        build_up: mean_or_fallback(
            &defenders,
            average(&["passing", "vision", "composure", "teamwork"]),
        ),
        press: mean_or_fallback(&midfielders, average(&["stamina", "tackling", "pace"])),
        ovr: fielded.iter().map(|p| p.ovr).sum::<f64>() / fielded.len().max(1) as f64,
    })
}

// ---------------------------------------------------------------------------
// Equal-player setups
// ---------------------------------------------------------------------------

/// Every player of `club` at one rating, fit and uncontracted from the injury list.
pub fn make_uniform(game: &mut Game, club: &str, rating: u8) {
    for player in game
        .players
        .iter_mut()
        .filter(|p| p.team_id.as_deref() == Some(club))
    {
        let a = &mut player.attributes;
        for attribute in [
            &mut a.pace,
            &mut a.stamina,
            &mut a.strength,
            &mut a.agility,
            &mut a.passing,
            &mut a.shooting,
            &mut a.tackling,
            &mut a.dribbling,
            &mut a.defending,
            &mut a.positioning,
            &mut a.vision,
            &mut a.decisions,
            &mut a.composure,
            &mut a.aggression,
            &mut a.teamwork,
            &mut a.leadership,
            &mut a.handling,
            &mut a.reflexes,
            &mut a.aerial,
        ] {
            *attribute = rating;
        }
        player.ovr = rating;
        player.condition = 100;
        player.injury = None;
    }
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == club) {
        team.play_style = PlayStyle::Balanced;
        team.tactics_phase = TacticsPhaseSettings::default();
    }
}

pub fn set_formation(game: &mut Game, club: &str, formation: &str) {
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == club) {
        team.formation = formation.to_string();
        team.starting_xi_ids.clear();
    }
}

/// Two clubs of the first league that are not the user's.
pub fn two_clubs(game: &Game) -> (String, String) {
    let user = game.manager.team_id.clone();
    let league = game
        .competitions
        .iter()
        .find(|c| c.participant_ids.len() >= 4)
        .expect("the world has a league");
    let mut ids = league
        .participant_ids
        .iter()
        .filter(|id| Some(*id) != user.as_ref());
    (
        ids.next().expect("a first club").clone(),
        ids.next().expect("a second club").clone(),
    )
}

/// The subject's mean result against the reference over `matches`, home and away alternating.
pub fn subject_results(
    game: &mut Game,
    football: Football,
    subject: &str,
    reference: &str,
    matches: u64,
    seed: u64,
) -> Vec<SideResult> {
    (0..matches)
        .map(|n| {
            game.seed = seed.wrapping_mul(100_000).wrapping_add(n);
            if n.is_multiple_of(2) {
                play(game, football, subject, reference).expect("plays").0
            } else {
                play(game, football, reference, subject).expect("plays").1
            }
        })
        .collect()
}
