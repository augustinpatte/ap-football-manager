//! Mission 7 probe: play a career day by day and print the state of its national cup every month.
//! `apfm_cup_probe <club id> <days> [cup id]`
use domain::league::FixtureStatus;
use season_harness::driver::{RunOptions, play_day};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;

fn main() {
    let path = world_path_from_env().expect("APFM_WORLD_JSON");
    let mut args = std::env::args().skip(1);
    let club = args.next().unwrap_or_else(|| "fm-c-501".into());
    let days: u32 = args.next().and_then(|d| d.parse().ok()).unwrap_or(400);
    let cup_id = args.next().unwrap_or_else(|| "fm-cup-fr_coupe".into());
    let mut game = build_fm24_career(&path, &club, 7).expect("career builds");
    let mut inv = Invariants::new(&game);
    let options = RunOptions::seasons(1);
    for day in 1..=days {
        if play_day(&mut game, options, 1, day, &mut inv).is_err() {
            println!("day {day}: play_day failed");
            break;
        }
        if day % 20 == 0 {
            let cup = game.competitions.iter().find(|c| c.id == cup_id).unwrap();
            let done = cup
                .fixtures
                .iter()
                .filter(|f| f.status == FixtureStatus::Completed)
                .count();
            let next = cup
                .fixtures
                .iter()
                .filter(|f| f.status != FixtureStatus::Completed)
                .map(|f| f.date.clone())
                .min();
            println!(
                "{} day {day}: cup rounds {} fixtures {} completed {done} next unplayed {:?}",
                game.clock.current_date.format("%Y-%m-%d"),
                cup.knockout_rounds.len(),
                cup.fixtures.len(),
                next
            );
        }
    }
    let cup = game.competitions.iter().find(|c| c.id == cup_id).unwrap();
    for round in &cup.knockout_rounds {
        let dates: Vec<&str> = round
            .fixture_ids
            .iter()
            .filter_map(|id| cup.fixtures.iter().find(|f| &f.id == id))
            .map(|f| f.date.as_str())
            .collect();
        println!(
            "{}: {} ties, completed {}, dates {:?}",
            round.name,
            round.fixture_ids.len(),
            round.completed,
            dates.first()
        );
    }
}
