//! The group-depth rule (`MatchConfig::group_depth_weight`) reaches the live resolvers.
//!
//! `group_depth` is unit-tested as a function, and `shared::defensive_quality` as a reading; what
//! those cannot show is that the live match's contests actually multiply the factor in. These
//! tests play seeded matches at weight 0 (the plain mean: the engine before the rule) and at the
//! shipped weight, and ask what a thin group of the club's best players is worth.

use engine::live_match::LiveMatchState;
use engine::{
    MatchConfig, PlayStyle, PlayerData, PlayerRole, Position, TacticsConfig, TeamData,
    group_depth::SHIPPED_GROUP_DEPTH_WEIGHT,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

const MATCHES: u64 = 400;

fn player(id: &str, position: Position, rating: u8) -> PlayerData {
    PlayerData {
        id: id.to_string(),
        name: id.to_string(),
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

/// A side of `rating` players in a `defenders`-`midfielders`-`forwards` shape, with the players of
/// one group replaced by `elite` rated ones.
fn side(
    id: &str,
    shape: (usize, usize, usize),
    elite: Option<(Position, u8)>,
    rating: u8,
) -> TeamData {
    let mut players = vec![player(&format!("{id}_gk"), Position::Goalkeeper, rating)];
    for (position, count, tag) in [
        (Position::Defender, shape.0, "d"),
        (Position::Midfielder, shape.1, "m"),
        (Position::Forward, shape.2, "f"),
    ] {
        let level = match elite {
            Some((group, elite_rating)) if group == position => elite_rating,
            _ => rating,
        };
        for n in 0..count {
            players.push(player(&format!("{id}_{tag}{n}"), position, level));
        }
    }
    TeamData {
        id: id.to_string(),
        name: id.to_string(),
        formation: format!("{}-{}-{}", shape.0, shape.1, shape.2),
        play_style: PlayStyle::Balanced,
        tactics: TacticsConfig::default(),
        players,
    }
}

/// (the subject's goals for, goals against) a match, over `MATCHES` seeds with the subject at home
/// half the time.
fn goals_a_match(subject: &TeamData, opponent: &TeamData, weight: f64) -> (f64, f64) {
    // No fouls (so no cards, a second yellow included) and no injuries: either leaves a group thin
    // mid-match, which is exactly what the rule prices, and would make the full-group control
    // differ for a reason of its own.
    let config = MatchConfig {
        group_depth_weight: weight,
        foul_probability: 0.0,
        injury_probability: 0.0,
        ..MatchConfig::default()
    };
    let (mut scored, mut conceded) = (0.0, 0.0);
    for seed in 0..MATCHES {
        let (home, away) = if seed % 2 == 0 {
            (subject.clone(), opponent.clone())
        } else {
            (opponent.clone(), subject.clone())
        };
        let mut state = LiveMatchState::new(home, away, config.clone(), vec![], vec![], false);
        let mut rng = StdRng::seed_from_u64(seed);
        while !state.step_minute(&mut rng).is_finished {}
        let snapshot = state.snapshot();
        let (home_goals, away_goals) = (
            f64::from(snapshot.home_score),
            f64::from(snapshot.away_score),
        );
        if seed % 2 == 0 {
            scored += home_goals;
            conceded += away_goals;
        } else {
            scored += away_goals;
            conceded += home_goals;
        }
    }
    (scored / MATCHES as f64, conceded / MATCHES as f64)
}

/// Given two sides whose every group is a full one (4-4-2 against 4-4-2)
/// When the same seeded matches are played at weight 0 and at the shipped weight
/// Then every match is identical: a full group is read as its plain mean, so the rule is the
///      identity wherever nobody is thin.
#[test]
fn a_full_group_is_unaffected_by_the_depth_weight() {
    let subject = side("s", (4, 4, 2), Some((Position::Forward, 80)), 65);
    let opponent = side("o", (4, 4, 2), None, 65);

    let plain = goals_a_match(&subject, &opponent, 0.0);
    let shipped = goals_a_match(&subject, &opponent, SHIPPED_GROUP_DEPTH_WEIGHT);

    assert_eq!(plain, shipped);
}

/// Given a lone forward whose finishing attributes (shooting, composure, decisions) are 95 and
///   everything else is the side's standard of 65, in a 4-5-1 against a plain side, with the same
///   seeded matches
/// When they are played with the plain mean (weight 0) and with the depth-aware reading
/// Then the lone forward scores clearly less under the depth-aware reading: he is read as if his
///      missing partner were an average outfield player, so concentrating the best finisher into a
///      thin group is no longer free at the shot. Only the finishing read differs from the side's
///      standard, so this is the finishing contest's factor and no other.
#[test]
fn a_lone_elite_forward_is_worth_less_than_he_was() {
    let mut subject = side("s", (4, 5, 1), None, 65);
    for striker in subject
        .players
        .iter_mut()
        .filter(|player| player.position == Position::Forward)
    {
        striker.shooting = 95;
        striker.composure = 95;
        striker.decisions = 95;
    }
    let opponent = side("o", (4, 4, 2), None, 65);

    let (plain, _) = goals_a_match(&subject, &opponent, 0.0);
    let (depth_aware, _) = goals_a_match(&subject, &opponent, SHIPPED_GROUP_DEPTH_WEIGHT);

    assert!(
        plain - depth_aware > 0.15,
        "plain mean {plain:.3} goals a match against depth-aware {depth_aware:.3}"
    );
}

/// Given a back three of 95-rated defenders against a plain 4-4-2, and the same seeded matches
/// When they are played with the plain mean and with the depth-aware reading
/// Then the back three concedes clearly more under the depth-aware reading: three internationals
///      behind a back three are no longer read as if they were a full four.
#[test]
fn a_thin_elite_back_line_is_worth_less_than_it_was() {
    let subject = side("s", (3, 5, 2), Some((Position::Defender, 95)), 65);
    let opponent = side("o", (4, 4, 2), None, 65);

    let (_, plain) = goals_a_match(&subject, &opponent, 0.0);
    let (_, depth_aware) = goals_a_match(&subject, &opponent, SHIPPED_GROUP_DEPTH_WEIGHT);

    assert!(
        depth_aware - plain > 0.08,
        "plain mean concedes {plain:.3} a match against depth-aware {depth_aware:.3}"
    );
}
