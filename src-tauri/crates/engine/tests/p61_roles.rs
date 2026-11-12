//! P6.1: which tactical roles the five-zone engine actually consumes, and what each is worth.
//!
//! A role that never fires leaves a match bit-for-bit what it was, so the audit plays the same seeds
//! with and without the role and counts the matches that change: zero is *inert*, not "small".
//! Sides are eleven identical players, so whatever a role is worth here is the role, not the squad.
//!
//! `P61_MATCHES=4000 cargo test --release -p engine --test p61_roles -- --ignored --nocapture`
//! `P61_HOLDERS=2` gives the role to that many players of the group (default: all of them),
//! `P61_SUPPORT=1` sets the role support weight, `P61_GAP=-5` rates the subject that many points
//! below the opponent, `P61_ROLES=AnchorMan,Poacher` limits the audit to those roles.

use engine::live_match::LiveMatchState;
use engine::{
    EventType, MatchConfig, PlayStyle, PlayerData, PlayerRole, Position, Side, TacticsConfig,
    TeamData,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

/// A player whose every attribute is `rating`.
fn player(id: String, position: Position, rating: u8) -> PlayerData {
    PlayerData {
        id: id.clone(),
        name: id,
        position,
        ovr: rating,
        condition: 95,
        fitness: 80,
        pace: rating,
        stamina: rating,
        strength: rating,
        agility: rating,
        passing: rating,
        shooting: rating,
        tackling: rating,
        dribbling: rating,
        defending: rating,
        positioning: rating,
        vision: rating,
        decisions: rating,
        composure: rating,
        aggression: rating,
        teamwork: rating,
        leadership: rating,
        handling: rating,
        reflexes: rating,
        aerial: rating,
        traits: vec![],
        role: PlayerRole::Standard,
    }
}

/// A goalkeeper and a 4-4-2 of players rated `rating`, every one on the Standard role.
fn four_four_two(id: &str, rating: u8) -> TeamData {
    let mut players = vec![player(format!("{id}_gk"), Position::Goalkeeper, rating)];
    for (position, count, tag) in [
        (Position::Defender, 4, "d"),
        (Position::Midfielder, 4, "m"),
        (Position::Forward, 2, "f"),
    ] {
        for n in 0..count {
            players.push(player(format!("{id}_{tag}{n}"), position, rating));
        }
    }
    TeamData {
        id: id.to_string(),
        name: id.to_string(),
        formation: "4-4-2".to_string(),
        play_style: PlayStyle::Balanced,
        tactics: TacticsConfig::default(),
        players,
    }
}

/// What one side took from one match.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct Outcome {
    points: f64,
    goals_for: f64,
    goals_against: f64,
    shots_for: f64,
    shots_against: f64,
    possession: f64,
}

/// Plays `home` against `away` with `seed`; returns (home, away).
fn play(home: TeamData, away: TeamData, config: &MatchConfig, seed: u64) -> (Outcome, Outcome) {
    let mut state = LiveMatchState::new(home, away, config.clone(), vec![], vec![], false);
    let mut rng = StdRng::seed_from_u64(seed);
    let (mut home_shots, mut away_shots) = (0.0, 0.0);
    loop {
        let minute = state.step_minute(&mut rng);
        for event in &minute.events {
            if matches!(
                event.event_type,
                EventType::Goal
                    | EventType::ShotSaved
                    | EventType::ShotOffTarget
                    | EventType::ShotBlocked
            ) {
                match event.side {
                    Side::Home => home_shots += 1.0,
                    Side::Away => away_shots += 1.0,
                }
            }
        }
        if minute.is_finished {
            break;
        }
    }
    let snapshot = state.snapshot();
    let (hg, ag) = (
        f64::from(snapshot.home_score),
        f64::from(snapshot.away_score),
    );
    let points = |mine: f64, theirs: f64| {
        if mine > theirs {
            3.0
        } else if mine == theirs {
            1.0
        } else {
            0.0
        }
    };
    (
        Outcome {
            points: points(hg, ag),
            goals_for: hg,
            goals_against: ag,
            shots_for: home_shots,
            shots_against: away_shots,
            possession: snapshot.home_possession_pct,
        },
        Outcome {
            points: points(ag, hg),
            goals_for: ag,
            goals_against: hg,
            shots_for: away_shots,
            shots_against: home_shots,
            possession: 100.0 - snapshot.home_possession_pct,
        },
    )
}

/// The subject's outcome against `opponent` with `seed`, the subject at home on even seeds.
fn subject_outcome(
    subject: TeamData,
    opponent: TeamData,
    config: &MatchConfig,
    seed: u64,
) -> Outcome {
    if seed.is_multiple_of(2) {
        play(subject, opponent, config, seed).0
    } else {
        play(opponent, subject, config, seed).1
    }
}

/// Mean and standard error of the mean.
fn mean_se(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    if values.len() < 2 {
        return (values.first().copied().unwrap_or(0.0), 0.0);
    }
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, (variance / n).sqrt())
}

/// The football under study: the game's own, with the role support weight `P61_SUPPORT` where set.
fn config_from_env() -> MatchConfig {
    let mut config = MatchConfig::default();
    if let Some(weight) = std::env::var("P61_SUPPORT")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        config.role_support_weight = weight;
    }
    config
}

/// `team` with `role` on the players of `group`: all of them, or only the first.
fn with_role(mut team: TeamData, group: Position, role: PlayerRole, holders: usize) -> TeamData {
    for holder in team
        .players
        .iter_mut()
        .filter(|p| p.position == group)
        .take(holders)
    {
        holder.role = role;
    }
    team
}

fn matches_per_role() -> u64 {
    std::env::var("P61_MATCHES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4_000)
}

/// Every role, for every group that may hold it, against the same role-free side on the same
/// seeds. "changed" is the share of matches whose result, shots or possession differ at all: 0 means
/// the engine never consulted the role.
#[test]
#[ignore = "measurement study"]
fn p61_role_audit() {
    let matches = matches_per_role();
    let holders: usize = std::env::var("P61_HOLDERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX);
    let gap: i16 = std::env::var("P61_GAP")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let only: Option<Vec<String>> = std::env::var("P61_ROLES")
        .ok()
        .map(|list| list.split(',').map(str::to_string).collect());
    let subject_rating = (70 + gap) as u8;
    let config = config_from_env();
    let opponent = || four_four_two("o", 70);
    let baseline: Vec<Outcome> = (0..matches)
        .map(|seed| {
            subject_outcome(
                four_four_two("s", subject_rating),
                opponent(),
                &config,
                seed,
            )
        })
        .collect();

    eprintln!(
        "\n== role audit: {} holder(s), subject {gap:+} rating points, {matches} matches a row, paired seeds ==",
        if holders == usize::MAX {
            "every".to_string()
        } else {
            holders.to_string()
        }
    );
    eprintln!(
        "{:<20}{:<11}{:>9}{:>9}{:>8}{:>8}{:>8}{:>8}{:>8}",
        "role", "as", "changed%", "dPts", "se", "dGF", "dGA", "dShotF", "dShotA"
    );
    for role in PlayerRole::ALL {
        if role == PlayerRole::Standard
            || only
                .as_ref()
                .is_some_and(|list| !list.contains(&format!("{role:?}")))
        {
            continue;
        }
        for &group in role.position_groups() {
            let outcomes: Vec<Outcome> = (0..matches)
                .map(|seed| {
                    let subject =
                        with_role(four_four_two("s", subject_rating), group, role, holders);
                    subject_outcome(subject, opponent(), &config, seed)
                })
                .collect();
            let changed = outcomes
                .iter()
                .zip(&baseline)
                .filter(|(with, without)| with != without)
                .count() as f64
                / matches as f64;
            let delta = |field: fn(&Outcome) -> f64| -> (f64, f64) {
                let diffs: Vec<f64> = outcomes
                    .iter()
                    .zip(&baseline)
                    .map(|(with, without)| field(with) - field(without))
                    .collect();
                mean_se(&diffs)
            };
            let (points, points_se) = delta(|o| o.points);
            eprintln!(
                "{:<20}{:<11}{:>9.1}{:>+9.3}{:>8.3}{:>+8.3}{:>+8.3}{:>+8.2}{:>+8.2}",
                format!("{role:?}"),
                format!("{group:?}"),
                changed * 100.0,
                points,
                points_se,
                delta(|o| o.goals_for).0,
                delta(|o| o.goals_against).0,
                delta(|o| o.shots_for).0,
                delta(|o| o.shots_against).0,
            );
        }
    }
}

/// The football of a given role support weight, everything else the game's own.
fn football(role_support_weight: f64) -> MatchConfig {
    MatchConfig {
        role_support_weight,
        ..MatchConfig::default()
    }
}

/// The share of `seeds` matches (the subject at home on even seeds, 70-rated 4-4-2s on both sides)
/// whose outcome differs between a subject holding `role` in `group` under `without_support` and
/// under `with_support`.
fn share_changed_by_support(
    role: PlayerRole,
    group: Position,
    seeds: u64,
    without_support: &MatchConfig,
    with_support: &MatchConfig,
) -> f64 {
    let changed = (0..seeds)
        .filter(|&seed| {
            let subject = || with_role(four_four_two("s", 70), group, role, usize::MAX);
            subject_outcome(subject(), four_four_two("o", 70), without_support, seed)
                != subject_outcome(subject(), four_four_two("o", 70), with_support, seed)
        })
        .count();
    changed as f64 / seeds as f64
}

/// The (role, group) pairs the P6 engine never consulted: the table has an entry for them and the
/// resolvers never ask the position about its phase.
const ROLES_THE_ENGINE_NEVER_CONSULTED: [(PlayerRole, Position); 14] = [
    (PlayerRole::BallPlayingKeeper, Position::Goalkeeper),
    (PlayerRole::SweeperKeeper, Position::Goalkeeper),
    (PlayerRole::BallPlayingCB, Position::Defender),
    (PlayerRole::AttackingFB, Position::Defender),
    (PlayerRole::InvertedFB, Position::Defender),
    (PlayerRole::WingBack, Position::Defender),
    (PlayerRole::Mezzala, Position::Midfielder),
    (PlayerRole::AdvancedPlaymaker, Position::Midfielder),
    (PlayerRole::ShadowStriker, Position::Midfielder),
    (PlayerRole::WideForward, Position::Midfielder),
    (PlayerRole::InsideForward, Position::Midfielder),
    (PlayerRole::InvertedWinger, Position::Forward),
    (PlayerRole::DeepLyingForward, Position::Forward),
    (PlayerRole::PressingForward, Position::Forward),
];

mod role_support {
    use super::*;

    /// Given every role the P6 engine never consulted, held by the whole group that may hold it
    /// When the same 300 seeds are played with a role support weight of 0 and of 1
    /// Then at 0 a match is bit-for-bit what it was without the role, and at 1 it is not: the role
    ///      is consulted. (Zero changed matches is the definition of inert, so this is the test that
    ///      fails if a role's wiring is removed.)
    #[test]
    fn a_role_nobody_consulted_acts_at_weight_one_and_is_inert_at_weight_zero() {
        for (role, group) in ROLES_THE_ENGINE_NEVER_CONSULTED {
            let seeds = 300;
            let against_standard = |config: &MatchConfig| {
                let changed = (0..seeds)
                    .filter(|&seed| {
                        let with = with_role(four_four_two("s", 70), group, role, usize::MAX);
                        subject_outcome(with, four_four_two("o", 70), config, seed)
                            != subject_outcome(
                                four_four_two("s", 70),
                                four_four_two("o", 70),
                                config,
                                seed,
                            )
                    })
                    .count();
                changed as f64 / seeds as f64
            };
            assert_eq!(
                against_standard(&football(0.0)),
                0.0,
                "{role:?} as {group:?} changed a match with no role support"
            );
            assert!(
                against_standard(&football(1.0)) > 0.02,
                "{role:?} as {group:?} still changes (almost) no match at weight 1"
            );
        }
    }

    /// Given roles whose bonus the engine already consulted (a stopper, a ball winner, an anchor
    ///   man, a poacher) and whose penalty sits on a phase their holder is not drawn on
    /// When the same seeds are played at weight 0 and at weight 1
    /// Then not one match differs: support credits an unconsulted bonus and nothing else, so a role
    ///      that works today is not moved, and a penalty is not charged to a whole side's attack.
    #[test]
    fn a_role_that_already_worked_is_bit_for_bit_the_same_with_support() {
        for (role, group) in [
            (PlayerRole::Stopper, Position::Defender),
            (PlayerRole::BallWinner, Position::Midfielder),
            (PlayerRole::AnchorMan, Position::Midfielder),
            (PlayerRole::Poacher, Position::Forward),
            (PlayerRole::DefensiveFB, Position::Defender),
        ] {
            assert_eq!(
                share_changed_by_support(role, group, 300, &football(0.0), &football(1.0)),
                0.0,
                "{role:?} as {group:?} moved when support was priced"
            );
        }
    }

    /// Given a side whose four defenders are attacking full backs, against a Standard side
    /// When 3,000 matches are played with support priced and with it not
    /// Then the side shoots more (the overlap is paid) and concedes more shots (what it gives up is
    ///      charged): a role with an upside and a cost, where before it had neither.
    #[test]
    fn an_attacking_full_back_overlaps_and_leaves_space() {
        let matches = 3_000u64;
        let mean_shots = |config: &MatchConfig, role: PlayerRole| -> (f64, f64) {
            let outcomes: Vec<Outcome> = (0..matches)
                .map(|seed| {
                    let subject = with_role(four_four_two("s", 70), Position::Defender, role, 4);
                    subject_outcome(subject, four_four_two("o", 70), config, seed)
                })
                .collect();
            (
                mean_se(&outcomes.iter().map(|o| o.shots_for).collect::<Vec<_>>()).0,
                mean_se(&outcomes.iter().map(|o| o.shots_against).collect::<Vec<_>>()).0,
            )
        };
        let (standard_for, standard_against) = mean_shots(&football(1.0), PlayerRole::Standard);
        let (full_back_for, full_back_against) =
            mean_shots(&football(1.0), PlayerRole::AttackingFB);
        assert!(
            full_back_for > standard_for + 0.15,
            "shots for {full_back_for:.2} against {standard_for:.2}"
        );
        assert!(
            full_back_against > standard_against + 0.1,
            "shots against {full_back_against:.2} against {standard_against:.2}"
        );
    }

    /// Given a side holding every support role at once (wing backs, advanced playmakers, mezzalas
    ///   and a sweeper keeper, the whole group each), against a Standard side
    /// When the same seeds are played with support priced and without (where those roles are inert,
    ///   so the side is a Standard one)
    /// Then pricing support gains the stacked side well under a fifth of a point a match: the
    ///      multiplier is capped and each holder's share diluted, so a side that gives every
    ///      outfielder a role is not paid several times over. P6's best dial stack was +0.08 to +0.11.
    #[test]
    fn stacking_every_support_role_does_not_make_a_side_dominant() {
        let stacked = |config: &MatchConfig, seed: u64| {
            let mut team = four_four_two("s", 70);
            for (group, role) in [
                (Position::Defender, PlayerRole::WingBack),
                (Position::Midfielder, PlayerRole::AdvancedPlaymaker),
                (Position::Midfielder, PlayerRole::Mezzala),
                (Position::Goalkeeper, PlayerRole::SweeperKeeper),
            ] {
                team = with_role(team, group, role, usize::MAX);
            }
            subject_outcome(team, four_four_two("o", 70), config, seed).points
        };
        let gains: Vec<f64> = (0..4_000u64)
            .map(|seed| stacked(&football(1.0), seed) - stacked(&football(0.0), seed))
            .collect();
        let (gain, se) = mean_se(&gains);
        eprintln!("stacked side gains {gain:+.3} points a match (se {se:.3})");
        assert!(
            gain > 0.0,
            "stacking gains {gain:.3} points a match (se {se:.3})"
        );
        assert!(
            gain < 0.2 - 3.0 * se,
            "stacking gains {gain:.3} points a match (se {se:.3})"
        );
    }

    /// Given a match configuration written before role support existed
    /// When it is read
    /// Then the weight is 0, so an old configuration plays the football it was written for.
    #[test]
    fn an_older_configuration_reads_a_weight_of_zero() {
        let mut value = serde_json::to_value(MatchConfig::default()).expect("serializes");
        value
            .as_object_mut()
            .expect("an object")
            .remove("role_support_weight");
        let config: MatchConfig =
            serde_json::from_value(value).expect("an old configuration reads");
        assert_eq!(config.role_support_weight, 0.0);
    }
}

/// What a rating point of squad quality is worth in the engine, to put a formation's price in the
/// same units as the squad model's rating points: a side whose every attribute is `r + gap` against
/// one whose every attribute is `r`, 4-4-2 each.
#[test]
#[ignore = "measurement study"]
fn p61_points_per_rating_point() {
    let matches = matches_per_role().max(20_000);
    let config = MatchConfig::default();
    eprintln!(
        "\n== a uniform side {{gap}} rating points above a 70-rated 4-4-2 ({matches} matches) =="
    );
    for gap in [1u8, 2, 3, 5] {
        let outcomes: Vec<Outcome> = (0..matches)
            .map(|seed| {
                subject_outcome(
                    four_four_two("s", 70 + gap),
                    four_four_two("o", 70),
                    &config,
                    seed,
                )
            })
            .collect();
        let points = mean_se(&outcomes.iter().map(|o| o.points).collect::<Vec<_>>());
        let difference = mean_se(
            &outcomes
                .iter()
                .map(|o| o.goals_for - o.goals_against)
                .collect::<Vec<_>>(),
        );
        eprintln!(
            "+{gap}: points {:.3} (se {:.3}), goal difference {:+.3} (se {:.3}); per point: {:+.3} points, {:+.3} goals",
            points.0,
            points.1,
            difference.0,
            difference.1,
            (points.0 - 1.36) / f64::from(gap),
            difference.0 / f64::from(gap),
        );
    }
}
