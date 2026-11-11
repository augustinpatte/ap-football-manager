//! P4.7: do the positions whose value and wage were priced from the coarse rating (wingers, wide
//! and defensive/attacking midfielders) behave as market bargains? Measurement only (`#[ignore]`d).
//!
//! `P47_SEEDS=1,2,3 P47_SEASONS=10 cargo test --release -p season-harness --test p47_pricing -- --ignored --nocapture`
//!
//! "Under-priced" positions are the five the D6 audit named. For every completed permanent deal:
//! the share of deals for them against their share of the world's players; the fee as a share of
//! the player's market value, by class; and the weekly wage per rating point, by class, for all
//! rostered seniors, at the opening and at the end (does the gap close as the world trains and
//! re-prices its players, or widen?).

use std::collections::HashMap;

use domain::player::Position;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};

struct Nobody;

impl SeasonObserver for Nobody {}
use season_harness::env;

fn is_underpriced(position: &str) -> bool {
    matches!(
        position,
        "RightWinger"
            | "LeftWinger"
            | "DefensiveMidfielder"
            | "AttackingMidfielder"
            | "RightMidfielder"
            | "LeftMidfielder"
    )
}

fn wage_per_point(game: &Game) -> (f64, f64, usize, usize, f64, f64) {
    let mut sums = [(0.0f64, 0.0f64, 0usize); 2];
    for player in &game.players {
        if player.team_id.is_none() || player.squad_role != domain::player::SquadRole::Senior {
            continue;
        }
        let class = usize::from(is_underpriced(&format!("{:?}", player.natural_position)));
        if player.natural_position == Position::Goalkeeper || player.ovr == 0 {
            continue;
        }
        sums[class].0 += f64::from(player.wage()) / f64::from(player.ovr);
        sums[class].1 += player.market_value as f64 / f64::from(player.ovr).powi(2);
        sums[class].2 += 1;
    }
    (
        sums[1].0 / sums[1].2.max(1) as f64,
        sums[0].0 / sums[0].2.max(1) as f64,
        sums[1].2,
        sums[0].2,
        sums[1].1 / sums[1].2.max(1) as f64,
        sums[0].1 / sums[0].2.max(1) as f64,
    )
}

#[test]
#[ignore = "measurement study"]
fn p47_pricing() {
    let seasons = env::first("P47_SEASONS", "10") as u32;
    let mut deals_under = 0u64;
    let mut deals_other = 0u64;
    let mut fee_share: [(f64, f64); 2] = [(0.0, 0.0); 2];
    let mut players_under = 0usize;
    let mut players_other = 0usize;
    let mut opening = (0.0, 0.0);
    let mut closing = (0.0, 0.0);
    let mut seeds = 0.0;
    let mut value_open = (0.0, 0.0);
    let mut value_close = (0.0, 0.0);
    for seed in env::list("P47_SEEDS", "1") {
        let mut game = env::world("P47_WORLD", seed).build().expect("world builds");
        let (u, o, nu, no, vu, vo) = wage_per_point(&game);
        value_open.0 += vu;
        value_open.1 += vo;
        opening.0 += u;
        opening.1 += o;
        players_under += nu;
        players_other += no;
        ofm_core::transfers::enable_deal_log();
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut Nobody).expect("plays");
        let (u, o, _, _, vu, vo) = wage_per_point(&game);
        value_close.0 += vu;
        value_close.1 += vo;
        closing.0 += u;
        closing.1 += o;
        seeds += 1.0;
        let mut by_class: HashMap<bool, (u64, f64, f64)> = HashMap::new();
        for deal in ofm_core::transfers::take_deal_log() {
            let entry = by_class
                .entry(is_underpriced(&deal.natural_position))
                .or_default();
            entry.0 += 1;
            if deal.value > 0 {
                entry.1 += deal.fee as f64 / deal.value as f64;
                entry.2 += 1.0;
            }
        }
        for (class, (n, ratio, denom)) in by_class {
            let slot = usize::from(class);
            if class {
                deals_under += n;
            } else {
                deals_other += n;
            }
            fee_share[slot].0 += ratio;
            fee_share[slot].1 += denom;
        }
    }
    let total = (deals_under + deals_other).max(1) as f64;
    eprintln!(
        "opening world share of rostered senior outfielders in the five positions: {:.1}%",
        100.0 * players_under as f64 / (players_under + players_other).max(1) as f64
    );
    eprintln!(
        "share of completed permanent deals for them: {:.1}% ({deals_under} of {})",
        100.0 * deals_under as f64 / total,
        deals_under + deals_other
    );
    eprintln!(
        "fee as a share of market value: five positions {:.3} | others {:.3}",
        fee_share[1].0 / fee_share[1].1.max(1.0),
        fee_share[0].0 / fee_share[0].1.max(1.0)
    );
    eprintln!(
        "weekly wage per rating point: opening five positions {:.1} vs others {:.1} (ratio {:.3}) | end five positions {:.1} vs others {:.1} (ratio {:.3})",
        opening.0 / seeds,
        opening.1 / seeds,
        opening.0 / opening.1,
        closing.0 / seeds,
        closing.1 / seeds,
        closing.0 / closing.1
    );
    eprintln!(
        "market value per rating point squared: opening ratio {:.3} | end ratio {:.3} (five positions over the others)",
        value_open.0 / value_open.1,
        value_close.0 / value_close.1
    );
}
