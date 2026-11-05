//! Run F5.1: where a club's money comes from and goes, by reputation tercile. Measurement only.
//!
//! `F51_SEED=1 F51_SEASONS=10 cargo test --release -p season-harness --test f51_finance_flows -- --ignored --nocapture`

use std::collections::{BTreeMap, HashMap};

use domain::finance::CashKind;
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

#[derive(Default)]
struct Flows {
    seen_posts: usize,
}

impl SeasonObserver for Flows {
    fn on_season_end(&mut self, game: &Game, _s: &EndOfSeasonSummary, season: u32) {
        let mut rep: Vec<(&str, u32)> = game
            .teams
            .iter()
            .map(|t| (t.id.as_str(), t.reputation))
            .collect();
        rep.sort_by_key(|r| std::cmp::Reverse(r.1));
        let n = rep.len();
        let tercile: HashMap<&str, usize> = rep
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i * 3 / n))
            .collect();
        let mut by: BTreeMap<(usize, String), i64> = BTreeMap::new();
        for post in game.cash_journal.iter().skip(self.seen_posts) {
            if post.kind == CashKind::OpeningBalance {
                continue;
            }
            let t = tercile.get(post.club_id.as_str()).copied().unwrap_or(2);
            *by.entry((t, format!("{:?}", post.kind))).or_default() += post.amount;
        }
        self.seen_posts = game.cash_journal.len();
        let clubs_per = (n / 3).max(1) as i64;
        eprintln!(
            "--- season {season}: per-club average over the season, M (terciles: top/mid/bottom reputation)"
        );
        let kinds: Vec<String> = by
            .keys()
            .map(|k| k.1.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        for kind in kinds {
            let v: Vec<f64> = (0..3)
                .map(|t| {
                    by.get(&(t, kind.clone())).copied().unwrap_or(0) as f64 / clubs_per as f64 / 1e6
                })
                .collect();
            eprintln!("   {kind:<20} {:>8.2} {:>8.2} {:>8.2}", v[0], v[1], v[2]);
        }
        let cash: Vec<f64> = (0..3)
            .map(|t| {
                let ids: Vec<&&str> = rep
                    .iter()
                    .map(|(id, _)| id)
                    .filter(|id| tercile[**id] == t)
                    .collect();
                ids.iter()
                    .map(|id| {
                        game.teams
                            .iter()
                            .find(|x| x.id == ***id)
                            .map_or(0.0, |x| x.finance as f64)
                    })
                    .sum::<f64>()
                    / ids.len().max(1) as f64
                    / 1e6
            })
            .collect();
        eprintln!(
            "   {:<20} {:>8.2} {:>8.2} {:>8.2}",
            "CASH (end)", cash[0], cash[1], cash[2]
        );
        let wb: Vec<f64> = (0..3)
            .map(|t| {
                let ids: Vec<&str> = rep
                    .iter()
                    .map(|(id, _)| *id)
                    .filter(|id| tercile[*id] == t)
                    .collect();
                ids.iter()
                    .map(|id| {
                        game.teams
                            .iter()
                            .find(|x| x.id == *id)
                            .map_or(0.0, |x| x.wage_budget as f64)
                    })
                    .sum::<f64>()
                    / ids.len().max(1) as f64
                    / 1e6
            })
            .collect();
        eprintln!(
            "   {:<20} {:>8.2} {:>8.2} {:>8.2}  (weekly wage_budget, M)",
            "WAGE BUDGET", wb[0], wb[1], wb[2]
        );
        let debtors: Vec<usize> = (0..3)
            .map(|t| {
                rep.iter()
                    .filter(|(id, _)| tercile[*id] == t)
                    .filter(|(id, _)| game.teams.iter().any(|x| x.id == *id && x.finance < 0))
                    .count()
            })
            .collect();
        eprintln!(
            "   {:<20} {:>8} {:>8} {:>8}  (clubs in debt)",
            "DEBT CLUBS", debtors[0], debtors[1], debtors[2]
        );
        let net: Vec<f64> = (0..3)
            .map(|t| {
                by.iter()
                    .filter(|((tt, k), _)| {
                        *tt == t && k != "TransferFeeIn" && k != "TransferFeeOut"
                    })
                    .map(|(_, v)| *v as f64)
                    .sum::<f64>()
                    / clubs_per as f64
                    / 1e6
            })
            .collect();
        eprintln!(
            "   {:<20} {:>8.2} {:>8.2} {:>8.2}  (operating net, M)",
            "NET (ex fees)", net[0], net[1], net[2]
        );
        let st: Vec<f64> = (0..3)
            .map(|t| {
                let ids: Vec<&str> = rep
                    .iter()
                    .map(|(id, _)| *id)
                    .filter(|id| tercile[*id] == t)
                    .collect();
                ids.iter()
                    .map(|id| {
                        game.teams
                            .iter()
                            .find(|x| x.id == *id)
                            .map_or(0.0, |x| f64::from(x.stadium_capacity))
                    })
                    .sum::<f64>()
                    / ids.len().max(1) as f64
            })
            .collect();
        eprintln!(
            "   {:<20} {:>8.0} {:>8.0} {:>8.0}  (stadium)",
            "STADIUM", st[0], st[1], st[2]
        );
    }
}

#[test]
#[ignore = "measurement study"]
fn f51_finance_flows() {
    let seed: u64 = std::env::var("F51_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let seasons: u32 = std::env::var("F51_SEASONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let mut game = WorldSpec::gate(seed).build().expect("world");
    run_seasons(
        &mut game,
        RunOptions::seasons(seasons),
        &mut Flows::default(),
    )
    .expect("play");
}
