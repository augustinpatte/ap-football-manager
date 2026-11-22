//! REALISM3: the transfer market, window by window, for contrasting clubs and several seeds.
//!
//! `realism3_market_bench <world.json> <out.json> [--clubs fm-c-498,fm-c-1075] [--seeds 1,2,3] [--days 90]`
//!
//! For each seed the FM24 world is built once; for each club a copy of it is given a manager who
//! makes **no decisions** (offers are not answered, so every one is counted where it arrives), and
//! the career is played through the production day path for `--days` days from the start date.
//! Cumulative counters are taken at day 30, 60 and 90: what the user's players were offered, what
//! the AI clubs did among themselves, what the sweep's funnel excluded, and anything invalid.
//!
//! The world file is the owner's own (`APFM_WORLD_JSON`-style path, an argument here); nothing of it
//! is written out beyond aggregates and the club ids asked for.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use domain::player::{PlayerMovementKind, TransferOfferStatus};
use ofm_core::career::{CareerScope, begin_career};
use ofm_core::game::Game;
use ofm_core::transfers::{
    FUNNEL_NAMES, enable_deal_log, market_funnel, reset_market_funnel, take_deal_log,
};
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::fm24::build_fm24_world;
use serde_json::{Value, json};

const CHECKPOINTS: [u32; 3] = [30, 60, 90];

#[derive(Default)]
struct Counters {
    /// Offers to the user's players, by id, with the day they arrived.
    offers: BTreeMap<String, Value>,
    loan_offers: BTreeSet<String>,
    ai_transfers: u32,
    ai_fee_over_value: Vec<f64>,
    ai_fee_over_capacity: Vec<f64>,
    invalid_offers: Vec<String>,
    duplicate_pending: u32,
    double_owned: u32,
    loans_started: u32,
}

struct Bench {
    user_team: String,
    seen_len: HashMap<String, usize>,
    counters: Counters,
    checkpoints: BTreeMap<u32, Value>,
    funnel_base: [u64; ofm_core::transfers::FUNNEL_STAGES],
}

impl Bench {
    fn new(game: &Game) -> Self {
        let user_team = game.manager.team_id.clone().unwrap_or_default();
        let seen_len = game
            .players
            .iter()
            .map(|p| (p.id.clone(), p.movement_history.len()))
            .collect();
        Self {
            user_team,
            seen_len,
            counters: Counters::default(),
            checkpoints: BTreeMap::new(),
            funnel_base: market_funnel(),
        }
    }

    fn snapshot(&self) -> Value {
        let funnel = market_funnel();
        let funnel_since: serde_json::Map<String, Value> = FUNNEL_NAMES
            .iter()
            .enumerate()
            .map(|(i, name)| ((*name).to_string(), json!(funnel[i] - self.funnel_base[i])))
            .collect();
        let ratios = |v: &Vec<f64>| -> Value {
            let mut s = v.clone();
            s.sort_by(f64::total_cmp);
            let q = |p: f64| {
                s.get(((s.len().max(1) - 1) as f64 * p).round() as usize)
                    .copied()
            };
            json!({"n": s.len(), "p10": q(0.1), "p50": q(0.5), "p90": q(0.9)})
        };
        let offer_ratio: Vec<f64> = self
            .counters
            .offers
            .values()
            .filter_map(|o| o["fee_over_value"].as_f64())
            .collect();
        json!({
            "incoming_permanent_offers": self.counters.offers.len(),
            "incoming_loan_offers": self.counters.loan_offers.len(),
            "distinct_players_offered_for": self.counters.offers.values().filter_map(|o| o["player"].as_str()).collect::<BTreeSet<_>>().len(),
            "incoming_fee_over_value": ratios(&offer_ratio),
            "ai_to_ai_permanent_transfers": self.counters.ai_transfers,
            "ai_loans_started": self.counters.loans_started,
            "ai_fee_over_value": ratios(&self.counters.ai_fee_over_value),
            "ai_fee_over_buyer_capacity": ratios(&self.counters.ai_fee_over_capacity),
            "invalid_incoming_offers": self.counters.invalid_offers.len(),
            "duplicate_pending_offers": self.counters.duplicate_pending,
            "players_owned_twice": self.counters.double_owned,
            "funnel": funnel_since,
        })
    }
}

impl SeasonObserver for Bench {
    fn on_day_end(&mut self, game: &Game, _season: u32, day: u32) {
        let team_budget: HashMap<&str, (i64, i64)> = game
            .teams
            .iter()
            .map(|t| (t.id.as_str(), (t.transfer_budget, t.finance)))
            .collect();
        let mut owners: HashMap<&str, u32> = HashMap::new();
        for player in &game.players {
            *owners.entry(player.id.as_str()).or_default() += 1;
            if player.team_id.as_deref() == Some(self.user_team.as_str()) {
                let mut pending_from: HashMap<&str, u32> = HashMap::new();
                for offer in &player.transfer_offers {
                    if offer.status == TransferOfferStatus::Pending {
                        *pending_from.entry(offer.from_team_id.as_str()).or_default() += 1;
                    }
                    if !self.counters.offers.contains_key(&offer.id) {
                        let value = player.market_value.max(1);
                        let (budget, cash) = team_budget
                            .get(offer.from_team_id.as_str())
                            .copied()
                            .unwrap_or((0, 0));
                        // Invalid: free, from his own club, or beyond what the buyer could pay today.
                        if offer.fee == 0
                            || offer.from_team_id == self.user_team
                            || (offer.fee as i64) > budget.max(cash).max(0) + offer.fee as i64 / 20
                        {
                            self.counters.invalid_offers.push(offer.id.clone());
                        }
                        self.counters.offers.insert(
                            offer.id.clone(),
                            json!({
                                "day": day, "player": player.id, "from": offer.from_team_id,
                                "fee": offer.fee, "fee_over_value": offer.fee as f64 / value as f64,
                                "ovr": player.ovr, "listed": player.transfer_listed,
                            }),
                        );
                    }
                }
                self.counters.duplicate_pending +=
                    pending_from.values().filter(|n| **n > 1).count() as u32;
                for offer in &player.loan_offers {
                    self.counters.loan_offers.insert(offer.id.clone());
                }
            }
            // New ledger entries since yesterday: transfers and loans between AI clubs.
            let before = self.seen_len.get(&player.id).copied().unwrap_or(0);
            let entries = &player.movement_history[..];
            if entries.len() > before {
                for entry in &entries[before..] {
                    let (from, to) = (entry.from_team_id.as_deref(), entry.to_team_id.as_deref());
                    let involves_user = from == Some(self.user_team.as_str())
                        || to == Some(self.user_team.as_str());
                    match entry.kind {
                        PlayerMovementKind::PermanentTransfer if !involves_user => {
                            self.counters.ai_transfers += 1;
                            if let Some(fee) = entry.fee.filter(|f| *f > 0) {
                                self.counters
                                    .ai_fee_over_value
                                    .push(fee as f64 / player.market_value.max(1) as f64);
                                if let Some((budget_after, _)) = to.and_then(|t| team_budget.get(t))
                                {
                                    self.counters.ai_fee_over_capacity.push(
                                        fee as f64 / (*budget_after + fee as i64).max(1) as f64,
                                    );
                                }
                            }
                        }
                        PlayerMovementKind::LoanStart if !involves_user => {
                            self.counters.loans_started += 1;
                        }
                        _ => {}
                    }
                }
            }
            self.seen_len
                .insert(player.id.clone(), player.movement_history.len());
        }
        self.counters.double_owned += owners.values().filter(|n| **n > 1).count() as u32;
        if CHECKPOINTS.contains(&day) {
            let snapshot = self.snapshot();
            self.checkpoints.insert(day, snapshot);
        }
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(world), Some(out)) = (args.first(), args.get(1)) else {
        return Err("usage: realism3_market_bench <world.json> <out.json> [--clubs a,b] [--seeds 1,2] [--days 90]".into());
    };
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let clubs: Vec<String> = flag("--clubs")
        .unwrap_or_else(|| "fm-c-498".to_string())
        .split(',')
        .map(str::to_string)
        .collect();
    let seeds: Vec<u64> = flag("--seeds")
        .unwrap_or_else(|| "1".to_string())
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let days: u32 = flag("--days").and_then(|d| d.parse().ok()).unwrap_or(90);

    enable_deal_log();
    let mut runs = Vec::new();
    for seed in &seeds {
        let (world_game, stats) = build_fm24_world(std::path::Path::new(world), *seed)?;
        for club in &clubs {
            let mut game = world_game.clone();
            begin_career(&mut game, club, CareerScope::default(), stats.clone())?;
            let squad: Vec<Value> = {
                let mut mine: Vec<_> = game
                    .players
                    .iter()
                    .filter(|p| {
                        p.team_id.as_deref() == Some(club.as_str())
                            && p.squad_role == domain::player::SquadRole::Senior
                    })
                    .collect();
                mine.sort_by_key(|p| std::cmp::Reverse(p.market_value));
                mine.iter()
                    .take(5)
                    .map(|p| json!({"ovr": p.ovr, "age": domain::age::age_on_or_unknown(game.clock.current_date.date_naive(), &p.date_of_birth), "value": p.market_value, "listed": p.transfer_listed, "contract_end": p.contract_end()}))
                    .collect()
            };
            let team = game.teams.iter().find(|t| &t.id == club).cloned();
            reset_market_funnel();
            let _ = take_deal_log();
            let mut bench = Bench::new(&game);
            let started = std::time::Instant::now();
            let options = RunOptions::seasons(1);
            for day in 1..=days {
                play_day(&mut game, options, 2026, day, &mut bench).map_err(|e| e.to_string())?;
            }
            runs.push(json!({
                "club": club, "seed": seed, "seconds": started.elapsed().as_secs_f64(),
                "club_facts": team.map(|t| json!({"reputation": t.reputation, "transfer_budget": t.transfer_budget, "finance": t.finance, "wage_budget": t.wage_budget})),
                "top_five_by_value": squad,
                "checkpoints": bench.checkpoints,
                "deals": take_deal_log().len(),
            }));
            println!(
                "{club} seed {seed}: done in {:.0}s",
                started.elapsed().as_secs_f64()
            );
        }
    }
    std::fs::write(
        out,
        serde_json::to_string_pretty(&json!({"days": days, "runs": runs}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}
