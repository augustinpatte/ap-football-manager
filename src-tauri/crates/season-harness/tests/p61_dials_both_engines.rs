//! P6.1: does a tactical dial push the match the same way in the five-zone engine as in M12?
//!
//! P6 compared the two engines' prices for a *formation*. A person watches a match in M12 and plays a
//! career on the five-zone engine, so what has to agree is the direction of every other tactical
//! choice too. Two clubs are forced to identical players; the subject moves one setting at a time
//! against a neutral 4-4-2, home and away, on the same seeds in each engine, and the change in
//! points, goals, shots and possession is read against the subject's own neutral baseline.
//!
//! `P61_MATCHES=3000 P61_M12_MATCHES=400 P61_SETTINGS=Possession,VeryLow cargo test --release -p season-harness --test p61_dials_both_engines -- --ignored --nocapture`
//!
//! M12 costs about a second a match, so a points effect of a few hundredths is not resolvable there;
//! shots and possession are the quantities to compare, and the table carries their standard errors.

use domain::team::{
    DefensiveLine, PitchWidth, PlayStyle, PressingIntensity, TacticsPhaseSettings, Tempo,
};
use ofm_core::game::Game;
use season_harness::env;
use season_harness::match_study::{
    Football, SideResult, make_uniform, mean_se, set_formation, subject_results, two_clubs,
};

type Setting = (&'static str, fn(&mut domain::team::Team));

const SETTINGS: [Setting; 13] = [
    ("Neutral", |_| {}),
    ("Defensive", |t| t.play_style = PlayStyle::Defensive),
    ("Attacking", |t| t.play_style = PlayStyle::Attacking),
    ("Possession", |t| t.play_style = PlayStyle::Possession),
    ("Counter", |t| t.play_style = PlayStyle::Counter),
    ("HighPress", |t| t.play_style = PlayStyle::HighPress),
    ("VeryLow", |t| {
        t.tactics_phase.defensive_line = DefensiveLine::VeryLow;
    }),
    ("HighLine", |t| {
        t.tactics_phase.defensive_line = DefensiveLine::High;
    }),
    ("Passive", |t| {
        t.tactics_phase.pressing_intensity = PressingIntensity::Passive;
    }),
    ("Aggressive", |t| {
        t.tactics_phase.pressing_intensity = PressingIntensity::Aggressive;
    }),
    ("Narrow", |t| t.tactics_phase.width = PitchWidth::Narrow),
    ("Wide", |t| t.tactics_phase.width = PitchWidth::Wide),
    ("Patient", |t| t.tactics_phase.tempo = Tempo::Patient),
];

fn reset(game: &mut Game, subject: &str, rating: u8) {
    make_uniform(game, subject, rating);
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == subject) {
        team.play_style = PlayStyle::Balanced;
        team.tactics_phase = TacticsPhaseSettings::default();
    }
}

/// `setting` less the neutral baseline on the same seeds, in each of the columns the table shows.
fn effect(
    setting: &[SideResult],
    neutral: &[SideResult],
    read: fn(&SideResult) -> f64,
) -> (f64, f64) {
    if setting.is_empty() || neutral.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let diffs: Vec<f64> = setting
        .iter()
        .zip(neutral)
        .map(|(a, b)| read(a) - read(b))
        .collect();
    mean_se(&diffs)
}

#[test]
#[ignore = "measurement study"]
fn p61_dials_both_engines() {
    let five_matches = env::first("P61_MATCHES", "3000");
    let m12_matches = env::first("P61_M12_MATCHES", "0");
    let rating = env::first("P6_RATING", "70") as u8;
    let only: Option<Vec<String>> = std::env::var("P61_SETTINGS")
        .ok()
        .map(|list| list.split(',').map(|s| s.trim().to_string()).collect());
    let seed = env::first("P6_SEEDS", "1");
    let mut game = env::world("P6_WORLD", seed).build().expect("world builds");
    let (subject, reference) = two_clubs(&game);
    make_uniform(&mut game, &reference, rating);
    set_formation(&mut game, &reference, "4-4-2");
    set_formation(&mut game, &subject, "4-4-2");

    let play = |game: &mut Game, football: Football, matches: u64| {
        subject_results(game, football, &subject, &reference, matches, seed)
    };
    reset(&mut game, &subject, rating);
    let neutral_five = play(&mut game, Football::FiveZone, five_matches);
    let neutral_m12 = play(&mut game, Football::M12, m12_matches);

    eprintln!(
        "\n== one setting moved at equal players, 4-4-2 v 4-4-2: {five_matches} five-zone / {m12_matches} M12 matches, paired seeds =="
    );
    eprintln!(
        "{:<11}| {:>7} {:>7} {:>7} {:>7} | {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "",
        "5z pts",
        "5z gf",
        "5z shot",
        "5z poss",
        "M12 pts",
        "se",
        "M12 gf",
        "M12 sht",
        "se",
        "M12 pos"
    );
    for (name, apply) in SETTINGS {
        if name == "Neutral" || only.as_ref().is_some_and(|o| !o.iter().any(|s| s == name)) {
            continue;
        }
        reset(&mut game, &subject, rating);
        if let Some(team) = game.teams.iter_mut().find(|t| t.id == subject) {
            apply(team);
        }
        let five = play(&mut game, Football::FiveZone, five_matches);
        let m12 = play(&mut game, Football::M12, m12_matches);
        let (fp, _) = effect(&five, &neutral_five, |r| r.points);
        let (fg, _) = effect(&five, &neutral_five, |r| r.goals_for);
        let (fs, _) = effect(&five, &neutral_five, |r| r.shots);
        let (fo, _) = effect(&five, &neutral_five, |r| r.possession);
        let (mp, mp_se) = effect(&m12, &neutral_m12, |r| r.points);
        let (mg, _) = effect(&m12, &neutral_m12, |r| r.goals_for);
        let (ms, ms_se) = effect(&m12, &neutral_m12, |r| r.shots);
        let (mo, _) = effect(&m12, &neutral_m12, |r| r.possession);
        eprintln!(
            "{name:<11}| {fp:>+7.3} {fg:>+7.3} {fs:>+7.2} {fo:>+7.2} | {mp:>+7.3} {mp_se:>7.3} {mg:>+7.3} {ms:>+7.2} {ms_se:>7.2} {mo:>+7.2}"
        );
    }
}
