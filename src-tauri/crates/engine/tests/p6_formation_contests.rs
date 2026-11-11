//! P6: what the five-zone engine makes of a formation when the players are the same.
//!
//! Every side here is eleven identical players (one rating, every attribute), so the only thing
//! that differs between two sides is how many of them stand in each group. Anything a formation
//! is worth in this table is therefore what the engine prices a *shape* at, not a squad.
//!
//! `P6_MATCHES=4000 cargo test --release -p engine --test p6_formation_contests -- --ignored --nocapture`

use engine::live_match::LiveMatchState;
use engine::{
    EventType, MatchConfig, PlayStyle, PlayerData, PlayerRole, Position, Side, TacticsConfig,
    TeamData,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

/// Defenders, midfielders and forwards of the eight formations the game offers, as the engine
/// reads them (it sees four groups, not slots).
const SHAPES: [(&str, usize, usize, usize); 8] = [
    ("4-4-2", 4, 4, 2),
    ("4-3-3", 4, 3, 3),
    ("3-5-2", 3, 5, 2),
    ("5-3-2", 5, 3, 2),
    ("4-5-1", 4, 5, 1),
    ("3-4-3", 3, 4, 3),
    ("4-2-3-1", 4, 5, 1),
    ("4-1-4-1", 4, 5, 1),
];

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

fn side(
    id: &str,
    formation: &str,
    shape: (usize, usize, usize),
    rating: u8,
    play_style: PlayStyle,
    tactics: TacticsConfig,
) -> TeamData {
    let mut players = vec![player(format!("{id}_gk"), Position::Goalkeeper, rating)];
    let groups = [
        (Position::Defender, shape.0, "d"),
        (Position::Midfielder, shape.1, "m"),
        (Position::Forward, shape.2, "f"),
    ];
    for (position, count, tag) in groups {
        for n in 0..count {
            players.push(player(format!("{id}_{tag}{n}"), position, rating));
        }
    }
    TeamData {
        id: id.to_string(),
        name: id.to_string(),
        formation: formation.to_string(),
        play_style,
        tactics,
        players,
    }
}

#[derive(Default, Clone, Copy)]
struct Result {
    points: f64,
    goals_for: f64,
    goals_against: f64,
    shots_for: f64,
    shots_against: f64,
    possession: f64,
}

fn play(
    home: TeamData,
    away: TeamData,
    config: &MatchConfig,
    seed: u64,
    before_kick_off: &dyn Fn(&mut LiveMatchState),
) -> (Result, Result) {
    let mut state = LiveMatchState::new(home, away, config.clone(), vec![], vec![], false);
    before_kick_off(&mut state);
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
        Result {
            points: points(hg, ag),
            goals_for: hg,
            goals_against: ag,
            shots_for: home_shots,
            shots_against: away_shots,
            possession: snapshot.home_possession_pct,
        },
        Result {
            points: points(ag, hg),
            goals_for: ag,
            goals_against: hg,
            shots_for: away_shots,
            shots_against: home_shots,
            possession: 100.0 - snapshot.home_possession_pct,
        },
    )
}

/// One shape against another over `matches` seeds, half of them with the subject at home. The
/// mean of the subject's results.
fn meet(
    subject: (&str, usize, usize, usize),
    opponent: (&str, usize, usize, usize),
    matches: u64,
    config: &MatchConfig,
    configure: &dyn Fn(&mut TeamData),
) -> Result {
    let mut total = Result::default();
    for seed in 0..matches {
        let mut mine = side(
            "s",
            subject.0,
            (subject.1, subject.2, subject.3),
            70,
            PlayStyle::Balanced,
            TacticsConfig::default(),
        );
        configure(&mut mine);
        let theirs = side(
            "o",
            opponent.0,
            (opponent.1, opponent.2, opponent.3),
            70,
            PlayStyle::Balanced,
            TacticsConfig::default(),
        );
        let ours = if seed % 2 == 0 {
            play(mine, theirs, config, seed, &|_| {}).0
        } else {
            play(theirs, mine, config, seed, &|_| {}).1
        };
        total.points += ours.points;
        total.goals_for += ours.goals_for;
        total.goals_against += ours.goals_against;
        total.shots_for += ours.shots_for;
        total.shots_against += ours.shots_against;
        total.possession += ours.possession;
    }
    let n = matches as f64;
    Result {
        points: total.points / n,
        goals_for: total.goals_for / n,
        goals_against: total.goals_against / n,
        shots_for: total.shots_for / n,
        shots_against: total.shots_against / n,
        possession: total.possession / n,
    }
}

/// The football under study: the game's own, with the numbers weights `P6_MID` and `P6_ATT` where set.
fn config_from_env() -> MatchConfig {
    let weight = |name: &str, default: f64| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    let mut config = MatchConfig::default();
    config.midfield_numbers_weight = weight("P6_MID", config.midfield_numbers_weight);
    config.attack_numbers_weight = weight("P6_ATT", config.attack_numbers_weight);
    config
}

fn matches_per_cell() -> u64 {
    std::env::var("P6_MATCHES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4_000)
}

/// Every shape against every other with identical players. Standard error of a cell's points a
/// game is about 1.3 / sqrt(matches): 0.02 at 4,000.
#[test]
#[ignore = "measurement study"]
fn p6_equal_players_shape_matrix() {
    let matches = matches_per_cell();
    eprintln!(
        "\n== Equal players, shape against shape: points a game for the row ({matches} matches a cell) =="
    );
    eprint!("{:>9}", "");
    for column in SHAPES {
        eprint!("{:>8}", column.0);
    }
    eprintln!("{:>8}{:>8}{:>8}{:>8}", "mean", "poss%", "shots", "shotsA");
    for row in SHAPES {
        eprint!("{:>9}", row.0);
        let mut sum = 0.0;
        let mut possession = 0.0;
        let mut shots = 0.0;
        let mut shots_against = 0.0;
        for column in SHAPES {
            let cell = meet(row, column, matches, &config_from_env(), &|_| {});
            eprint!("{:>8.3}", cell.points);
            sum += cell.points;
            possession += cell.possession;
            shots += cell.shots_for;
            shots_against += cell.shots_against;
        }
        let n = SHAPES.len() as f64;
        eprintln!(
            "{:>8.3}{:>8.1}{:>8.2}{:>8.2}",
            sum / n,
            possession / n,
            shots / n,
            shots_against / n
        );
    }
}

fn priced(midfield: f64, attack: f64) -> MatchConfig {
    MatchConfig {
        midfield_numbers_weight: midfield,
        attack_numbers_weight: attack,
        ..MatchConfig::default()
    }
}

fn shape(name: &str) -> (&'static str, usize, usize, usize) {
    SHAPES
        .into_iter()
        .find(|candidate| candidate.0 == name)
        .expect("a formation the game offers")
}

/// `subject` against a 4-4-2 of the same players, `matches` matches.
fn against_442(subject: &str, matches: u64, config: &MatchConfig) -> Result {
    meet(shape(subject), shape("4-4-2"), matches, config, &|_| {})
}

#[cfg(test)]
mod styles_pay_for_what_they_buy {
    use super::*;

    /// A 4-4-2 of the same players that plays `style`, against a balanced one: the shots it faces.
    fn shots_faced(style: PlayStyle) -> f64 {
        meet(
            shape("4-4-2"),
            shape("4-4-2"),
            4_000,
            &MatchConfig::default(),
            &|team| team.play_style = style,
        )
        .shots_against
    }

    /// Given sides of identical players, one of them playing a Defensive style, one Attacking and
    ///   one Balanced
    /// When the shots each faces are counted
    /// Then the Defensive side faces fewer than the Balanced and the Attacking side more: the
    ///      defensive half of a style is a real thing. It used to be asked of the defending side
    ///      as "is this your own phase?", answered no, and so was never applied: a Defensive side
    ///      was no better at defending and an Attacking one never paid for its attack.
    #[test]
    fn a_defensive_style_defends_and_an_attacking_one_pays() {
        let balanced = shots_faced(PlayStyle::Balanced);
        let defensive = shots_faced(PlayStyle::Defensive);
        let attacking = shots_faced(PlayStyle::Attacking);
        assert!(
            defensive < balanced - 0.2,
            "Defensive faced {defensive:.2} shots, Balanced {balanced:.2}"
        );
        assert!(
            attacking > balanced + 0.15,
            "Attacking faced {attacking:.2} shots, Balanced {balanced:.2}"
        );
    }
}

#[cfg(test)]
mod numbers_in_the_contests {
    use super::*;

    /// Given a football that prices no numbers, and sides of identical players
    /// When shapes with different counts in each group play the same 4-4-2
    /// Then every one of them gets exactly the results a 4-4-2 gets: with nothing but its numbers
    ///      to tell them apart a formation was worth nothing in this engine.
    #[test]
    fn a_football_that_prices_no_numbers_reads_every_shape_alike() {
        let football = priced(0.0, 0.0);
        let level = against_442("4-4-2", 300, &football);
        for name in ["4-3-3", "3-5-2", "5-3-2", "4-5-1", "3-4-3"] {
            let other = against_442(name, 300, &football);
            assert_eq!(
                (other.points, other.goals_for, other.shots_for),
                (level.points, level.goals_for, level.shots_for),
                "{name}"
            );
        }
    }

    /// Given sides of identical players, one with five midfielders and one with four
    /// When numbers in midfield are priced
    /// Then the five keep the ball more than the four do (about 54-46 in M12's own football with
    ///      this squad; here it only has to be on the right side of even by a clear margin).
    #[test]
    fn a_side_with_more_midfielders_keeps_more_of_the_ball() {
        let football = priced(0.4, 0.0);
        let five = against_442("4-5-1", 600, &football);
        assert!(
            five.possession > 51.0,
            "five midfielders against four kept {:.1} % of the ball",
            five.possession
        );
        let three = against_442("4-3-3", 600, &football);
        assert!(
            three.possession < 49.0,
            "three midfielders against four kept {:.1} % of the ball",
            three.possession
        );
    }

    /// Given sides of identical players, one with three forwards and one with one, against a back four
    /// When numbers in the attacking third are priced
    /// Then the three shoot more than they are shot at and the lone striker shoots less, and
    ///      neither is true of a football that prices no numbers.
    #[test]
    fn forwards_per_defender_decide_who_gets_the_shots() {
        let football = priced(0.0, 0.5);
        let three = against_442("4-3-3", 600, &football);
        let one = against_442("4-5-1", 600, &football);
        assert!(
            three.shots_for - three.shots_against > 0.5,
            "three forwards: {:.2} shots for, {:.2} against",
            three.shots_for,
            three.shots_against
        );
        assert!(
            one.shots_for - one.shots_against < -0.5,
            "one forward: {:.2} shots for, {:.2} against",
            one.shots_for,
            one.shots_against
        );
    }

    /// The points a 4-4-2 takes a match against the same 4-4-2 when it has one midfielder sent off
    /// before the kick-off, or has not.
    fn points_with_ten(football: &MatchConfig, sent_off: bool) -> f64 {
        let mut total = 0.0;
        let matches = 800;
        for seed in 0..matches {
            let ours = |id: &str| {
                side(
                    id,
                    "4-4-2",
                    (4, 4, 2),
                    70,
                    PlayStyle::Balanced,
                    TacticsConfig::default(),
                )
            };
            let hook = |state: &mut LiveMatchState| {
                if sent_off {
                    state.test_send_off("s_m0");
                }
            };
            total += if seed % 2 == 0 {
                play(ours("s"), ours("o"), football, seed, &hook).0.points
            } else {
                play(ours("o"), ours("s"), football, seed, &hook).1.points
            };
        }
        total / matches as f64
    }

    /// Given two equal sides, one of which loses a midfielder to a red card
    /// When numbers are priced (the football the game ships)
    /// Then the side with ten takes clearly fewer points than the same side with eleven; priced at
    ///      nothing the red card costs it next to nothing, because the engine counted nobody.
    #[test]
    fn a_sending_off_costs_the_side_that_suffers_it_only_where_numbers_are_priced() {
        let shipped = MatchConfig::default();
        let cost = points_with_ten(&shipped, false) - points_with_ten(&shipped, true);
        assert!(cost > 0.08, "a man down cost {cost:.2} points a match");

        let free = priced(0.0, 0.0);
        let free_cost = points_with_ten(&free, false) - points_with_ten(&free, true);
        assert!(
            free_cost.abs() < 0.05,
            "unpriced, a man down cost {free_cost:.2}"
        );
    }

    /// Given the football the game ships
    /// When its configuration is read
    /// Then it prices numbers in both contests, and a configuration written before the weights
    ///      existed reads as pricing none.
    #[test]
    fn the_shipped_football_prices_numbers_and_an_older_configuration_does_not() {
        let shipped = MatchConfig::default();
        assert!(shipped.midfield_numbers_weight > 0.0 && shipped.attack_numbers_weight > 0.0);

        let mut older = serde_json::to_value(&shipped).expect("serialises");
        let fields = older.as_object_mut().expect("an object");
        fields.remove("midfield_numbers_weight");
        fields.remove("attack_numbers_weight");
        let read: MatchConfig = serde_json::from_value(older).expect("an older save still loads");
        assert_eq!(
            (read.midfield_numbers_weight, read.attack_numbers_weight),
            (0.0, 0.0)
        );
    }
}

/// One setting moved on an otherwise neutral side, against a neutral 4-4-2 of the same players.
/// Each row is the points a match it adds, on the same seeds as the neutral side, so the standard
/// error is that of a paired difference (about 0.012 at 6,000 matches).
#[test]
#[ignore = "measurement study"]
fn p6_equal_players_dials() {
    use engine::{
        BreakSpeed, CounterPressDuration, DefensiveLine, DefensiveShape, MarkingStyle,
        PressingIntensity, TacticsBuildUpStyle, TacticsPitchWidth, Tempo,
    };
    type Dial = (&'static str, fn(&mut TeamData));
    let dials: [Dial; 22] = [
        ("style Attacking", |t| t.play_style = PlayStyle::Attacking),
        ("style Defensive", |t| t.play_style = PlayStyle::Defensive),
        ("style Possession", |t| t.play_style = PlayStyle::Possession),
        ("style Counter", |t| t.play_style = PlayStyle::Counter),
        ("style HighPress", |t| t.play_style = PlayStyle::HighPress),
        ("line VeryLow", |t| {
            t.tactics.defensive_line = DefensiveLine::VeryLow
        }),
        ("line Low", |t| {
            t.tactics.defensive_line = DefensiveLine::Low
        }),
        ("line High", |t| {
            t.tactics.defensive_line = DefensiveLine::High
        }),
        ("press Passive", |t| {
            t.tactics.pressing_intensity = PressingIntensity::Passive
        }),
        ("press Aggressive", |t| {
            t.tactics.pressing_intensity = PressingIntensity::Aggressive
        }),
        ("width Narrow", |t| {
            t.tactics.width = TacticsPitchWidth::Narrow
        }),
        ("width Wide", |t| t.tactics.width = TacticsPitchWidth::Wide),
        ("tempo Patient", |t| t.tactics.tempo = Tempo::Patient),
        ("shape Compact", |t| {
            t.tactics.defensive_shape = DefensiveShape::Compact
        }),
        ("shape Stretched", |t| {
            t.tactics.defensive_shape = DefensiveShape::Stretched
        }),
        ("buildup Short", |t| {
            t.tactics.build_up_style = TacticsBuildUpStyle::Short
        }),
        ("buildup Long", |t| {
            t.tactics.build_up_style = TacticsBuildUpStyle::Long
        }),
        ("marking ManToMan", |t| {
            t.tactics.marking_style = MarkingStyle::ManToMan
        }),
        ("counterpress Long", |t| {
            t.tactics.counter_press_duration = CounterPressDuration::Long
        }),
        ("break Fast", |t| t.tactics.break_speed = BreakSpeed::Fast),
        ("STACK poss+deep+narrow", |t| {
            t.play_style = PlayStyle::Possession;
            t.tactics.defensive_line = DefensiveLine::VeryLow;
            t.tactics.width = TacticsPitchWidth::Narrow;
        }),
        ("STACK att+deep+narrow+cp", |t| {
            t.play_style = PlayStyle::Attacking;
            t.tactics.defensive_line = DefensiveLine::VeryLow;
            t.tactics.width = TacticsPitchWidth::Narrow;
            t.tactics.counter_press_duration = CounterPressDuration::Long;
        }),
    ];
    let matches = matches_per_cell().max(6_000);
    let config = config_from_env();
    let neutral = meet(shape("4-4-2"), shape("4-4-2"), matches, &config, &|_| {});
    eprintln!(
        "\n== Equal players, one setting moved, {matches} matches each (neutral {:.3} points) ==",
        neutral.points
    );
    for (name, apply) in dials {
        let moved = meet(shape("4-4-2"), shape("4-4-2"), matches, &config, &apply);
        eprintln!(
            "{name:>18}: {:+.3} points | goals {:+.2} / {:+.2} | shots {:+.2} / {:+.2} | possession {:+.1}",
            moved.points - neutral.points,
            moved.goals_for - neutral.goals_for,
            moved.goals_against - neutral.goals_against,
            moved.shots_for - neutral.shots_for,
            moved.shots_against - neutral.shots_against,
            moved.possession - neutral.possession,
        );
    }
}

/// What M12 gave each shape against a 4-4-2 of the same players (500 matches each, 12 June of the
/// P6 cross-engine study; the standard error of a points figure is about 0.06): points, shots and
/// possession relative to M12's own 4-4-2. Shapes that differ only by slot (4-5-1, 4-2-3-1, 4-1-4-1)
/// are one shape to an engine that reads four groups, so they are entered as their mean.
const M12_AGAINST_442: [(&str, f64, f64, f64); 5] = [
    ("4-3-3", 0.120, 0.5, -0.4),
    ("3-5-2", -0.024, -0.3, 0.0),
    ("5-3-2", -0.162, -1.2, -1.7),
    ("4-5-1", -0.003, 0.8, 0.7),
    ("3-4-3", 0.226, 1.9, 1.7),
];

/// Which weights make the five-zone engine read a shape the way M12 does, at equal players.
/// Prints the sum of squared differences (points in units of the study's standard error) for each.
#[test]
#[ignore = "measurement study"]
fn p6_fit_numbers_to_m12() {
    let matches = matches_per_cell().max(3_000);
    eprintln!("\n== Fit of the numbers weights to M12 ({matches} matches a shape) ==");
    eprintln!(
        "{:>5} {:>5} | {:>8} | per shape: points (M12) | possession (M12) | shots (M12)",
        "mid", "att", "misfit"
    );
    for mid in [0.0, 0.2, 0.4, 0.6, 0.8] {
        for att in [0.0, 0.2, 0.4, 0.6, 0.8, 1.0] {
            let config = priced(mid, att);
            let reference = against_442("4-4-2", matches, &config);
            let mut misfit = 0.0;
            let mut cells = String::new();
            for (name, points, shots, possession) in M12_AGAINST_442 {
                let got = against_442(name, matches, &config);
                let (dp, ds, dq) = (
                    got.points - reference.points,
                    got.shots_for - reference.shots_for,
                    got.possession - reference.possession,
                );
                misfit += ((dp - points) / 0.08).powi(2)
                    + ((ds - shots) / 0.5).powi(2)
                    + ((dq - possession) / 1.0).powi(2);
                cells.push_str(&format!(
                    " | {:+.2} ({:+.2}) {:+.1} ({:+.1}) {:+.1} ({:+.1})",
                    dp, points, dq, possession, ds, shots
                ));
            }
            eprintln!("{mid:>5.1} {att:>5.1} | {misfit:>8.1}{cells}");
        }
    }
}
