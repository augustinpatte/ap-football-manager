//! Mission 4, phase 4: do the AI clubs' purchases of young players follow the hidden ceiling?
//!
//! `apfm_ai_info <seed> <gate|shipped> <seasons>` plays the career and, on every day, notes each
//! player who changed club (not on loan, not retired), with his age, his rating and his true
//! potential on that day. For each young buyer-side move it compares the player's potential gap
//! (`potential - ovr`) with the average gap of every senior of the same age and a rating within
//! three points, in the same world on the same day, and prints the difference: if the AI's
//! recruiting is blind to the ceiling, the difference is about zero; if it reads it, it is
//! positive. The AI's own rule is "read the world as it is" (`squad_intelligence`), so this is a
//! measurement of what that costs or gives, not a test of a rule.
use std::collections::HashMap;

use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;
use serde_json::json;

#[derive(Default)]
struct Watch {
    club_of: HashMap<String, Option<String>>,
    /// (age, gap above the pool's gap, was he signed from another club)
    moves: Vec<(i32, f64, bool, i32)>,
}

fn age_of(game: &Game, dob: &str) -> i32 {
    domain::age::age_on_or_unknown(game.clock.current_date.date_naive(), dob)
}

impl SeasonObserver for Watch {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        let user = game.manager.team_id.clone();
        let mut pool: HashMap<(i32, u8), (f64, u32)> = HashMap::new();
        let mut movers = Vec::new();
        for player in &game.players {
            if player.retired || player.active_loan.is_some() {
                continue;
            }
            let club = player.team_id.clone();
            let before = self.club_of.insert(player.id.clone(), club.clone());
            let age = age_of(game, &player.date_of_birth);
            if age <= 23 && club.is_some() && player.squad_role == domain::player::SquadRole::Senior
            {
                let entry = pool.entry((age, player.ovr / 3)).or_default();
                entry.0 += f64::from(player.potential) - f64::from(player.ovr);
                entry.1 += 1;
            }
            if let Some(Some(previous)) = &before
                && club.as_deref() != Some(previous.as_str())
                && club.is_some()
                && club != user
                && age <= 23
            {
                movers.push((
                    player.id.clone(),
                    age,
                    player.ovr,
                    f64::from(player.potential) - f64::from(player.ovr),
                ));
            }
        }
        for (_, age, ovr, gap) in movers {
            let (mut sum, mut count) = (0.0, 0u32);
            for bucket in (ovr / 3).saturating_sub(1)..=(ovr / 3 + 1) {
                if let Some((s, c)) = pool.get(&(age, bucket)) {
                    sum += s;
                    count += c;
                }
            }
            if count >= 10 {
                self.moves
                    .push((age, gap - sum / f64::from(count), true, i32::from(ovr)));
            }
        }
    }
}

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let world = std::env::args().nth(2).unwrap_or_else(|| "gate".into());
    let seasons: u32 = std::env::args()
        .nth(3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let spec = if world == "shipped" {
        WorldSpec::shipped(seed)
    } else {
        WorldSpec::gate(seed)
    };
    let mut game = spec.build().expect("world");
    let mut watch = Watch::default();
    run_seasons(&mut game, RunOptions::seasons(seasons), &mut watch).expect("seasons");
    let diffs: Vec<f64> = watch.moves.iter().map(|m| m.1).collect();
    let n = diffs.len().max(1) as f64;
    let mean = diffs.iter().sum::<f64>() / n;
    let sd = (diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / n).sqrt();
    let positive = diffs.iter().filter(|d| **d > 0.0).count() as f64 / n;
    println!(
        "{}",
        json!({
            "world": world, "seed": seed, "seasons": seasons,
            "young_moves_compared": diffs.len(),
            "mean_gap_over_pool": mean, "sd": sd, "se": sd / n.sqrt(),
            "share_above_pool": positive,
        })
    );
}
