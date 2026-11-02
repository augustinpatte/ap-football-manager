//! How many moves the AI transfer market makes in a season, and what they look like (Run E).
//!
//! Measurement only, `#[ignore]`d, run in release mode like the shipped game:
//!
//! `cargo test --release -p season-harness --test measure_ai_market -- --ignored --nocapture`
//!
//! It exists to compare the market before and after the player became a party to every move: the
//! same worlds and seeds, the same counters, read off the transfer logs at the end of each season.

use std::collections::HashMap;

use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

#[derive(Default)]
struct Moves {
    per_season: Vec<Tally>,
}

#[derive(Default, Clone)]
struct Tally {
    moves: usize,
    fees: u128,
    abroad: usize,
    up_the_ladder: usize,
}

impl SeasonObserver for Moves {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        let reputation: HashMap<&str, u32> = game
            .teams
            .iter()
            .map(|t| (t.id.as_str(), t.reputation))
            .collect();
        let country: HashMap<&str, &str> = game
            .teams
            .iter()
            .map(|t| (t.id.as_str(), t.country.as_str()))
            .collect();
        let mut tally = Tally::default();
        for transfer in game.competitions.iter().flat_map(|c| &c.transfer_log) {
            tally.moves += 1;
            tally.fees += u128::from(transfer.fee);
            if country.get(transfer.from_team_id.as_str())
                != country.get(transfer.to_team_id.as_str())
            {
                tally.abroad += 1;
            }
            if reputation.get(transfer.to_team_id.as_str())
                > reputation.get(transfer.from_team_id.as_str())
            {
                tally.up_the_ladder += 1;
            }
        }
        self.per_season.push(tally);
    }
}

#[test]
#[ignore = "measurement: prints the AI market's volume per season"]
fn ai_market_volume_per_season() {
    for seed in [1_u64, 2, 3] {
        let mut game = WorldSpec::gate(seed).build().expect("the world builds");
        let mut moves = Moves::default();
        run_seasons(&mut game, RunOptions::seasons(3), &mut moves).expect("the run completes");
        for (index, season) in moves.per_season.iter().enumerate() {
            eprintln!(
                "MARKET seed {seed} season {}: {} moves, mean fee {}, {} abroad, {} up the ladder",
                index + 1,
                season.moves,
                season.fees.checked_div(season.moves as u128).unwrap_or(0),
                season.abroad,
                season.up_the_ladder,
            );
        }
    }
}
