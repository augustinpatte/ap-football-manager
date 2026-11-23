//! REALISM3B: are the incoming permanent offers for the user's players repeated approaches?
//!
//! `realism3b_offer_audit <world.json> <out.json> [--clubs fm-c-498] [--seeds 1,2,3] [--days 120]`
//!
//! For each club and seed the career is played through the production day path with a manager who
//! either **ignores** every offer or **rejects** each one the day it arrives. Every offer is followed
//! from the day it is first seen to its end: who made it, for whom, at what fee, and how it closed.
//! Reported per run: offers per player and unique bidding clubs, repeated approaches by the same club
//! for the same player (and the gap since the earlier one closed against the rebid cooldown), how the
//! offers fall over the days and over the transfer window, and how the offers ended.
//!
//! Aggregates only; the world file is read, never written.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use domain::player::TransferOfferStatus;
use ofm_core::career::{CareerScope, begin_career};
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, play_day};
use season_harness::fm24::build_fm24_world;
use serde_json::{Value, json};

/// The rebid cooldown the game applies, in days (`transfers::consts::REBID_COOLDOWN_DAYS`).
const REBID_COOLDOWN_DAYS: i64 = 30;

#[derive(Clone)]
struct Offer {
    player: String,
    from: String,
    first_day: u32,
    date: String,
    fee: u64,
    status: TransferOfferStatus,
    closed_on: Option<String>,
    /// Days left in the transfer window when the offer was first seen.
    days_left: Option<i64>,
}

/// The rebid cooldown for an offer arriving with this many days left in the window
/// (`WindowPhase::rebid_cooldown_days`: 30, shortened to 15, 7 and 3 as the window closes).
fn phase_cooldown(days_left: Option<i64>) -> (i64, &'static str) {
    match days_left {
        // An offer is only made while the window is open; one that the end-of-day context calls
        // "no window" was made on the closing day itself.
        None => (REBID_COOLDOWN_DAYS / 10, "deadline_day"),
        Some(d) if d <= 0 => (REBID_COOLDOWN_DAYS / 10, "deadline_day"),
        Some(1..=2) => (REBID_COOLDOWN_DAYS / 4, "last_72h"),
        Some(3..=7) => (REBID_COOLDOWN_DAYS / 2, "last_week"),
        Some(_) => (REBID_COOLDOWN_DAYS, "mid_or_early"),
    }
}

#[derive(Default)]
struct Watch {
    user_team: String,
    offers: BTreeMap<String, Offer>,
    new_by_day: BTreeMap<u32, u32>,
    window_open_days: u32,
    offers_on_closed_window_days: u32,
}

impl SeasonObserver for Watch {
    fn on_day_end(&mut self, game: &Game, _season: u32, day: u32) {
        let window_open = game.season_context.transfer_window.status
            == domain::season::TransferWindowStatus::Open;
        self.window_open_days += u32::from(window_open);
        self.record(game, day, window_open);
    }
}

impl Watch {
    /// Reads every offer on the user's players: new ones are noted, known ones keep their latest
    /// status. Called again after the manager answers, so the answer's status is the one kept.
    fn record(&mut self, game: &Game, day: u32, window_open: bool) {
        for player in game
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(self.user_team.as_str()))
        {
            for offer in &player.transfer_offers {
                let entry = self.offers.entry(offer.id.clone()).or_insert_with(|| {
                    *self.new_by_day.entry(day).or_default() += 1;
                    self.offers_on_closed_window_days += u32::from(!window_open);
                    Offer {
                        player: player.id.clone(),
                        from: offer.from_team_id.clone(),
                        first_day: day,
                        date: offer.date.clone(),
                        fee: offer.fee,
                        status: offer.status.clone(),
                        closed_on: None,
                        days_left: game.season_context.transfer_window.days_remaining,
                    }
                });
                entry.status = offer.status.clone();
                entry.closed_on = offer.closed_on.clone();
            }
        }
    }
}

fn dist(values: &[f64]) -> Value {
    if values.is_empty() {
        return json!({"n": 0});
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let at = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    json!({"n": v.len(), "min": v[0], "p50": at(0.5), "p90": at(0.9), "max": v[v.len() - 1]})
}

fn days_between(a: &str, b: &str) -> Option<i64> {
    let (a, b) = (
        chrono::NaiveDate::parse_from_str(a, "%Y-%m-%d").ok()?,
        chrono::NaiveDate::parse_from_str(b, "%Y-%m-%d").ok()?,
    );
    Some((b - a).num_days())
}

fn report(watch: &Watch) -> Value {
    let mut by_player: HashMap<&str, Vec<&Offer>> = HashMap::new();
    for offer in watch.offers.values() {
        by_player
            .entry(offer.player.as_str())
            .or_default()
            .push(offer);
    }
    let per_player: Vec<f64> = by_player.values().map(|o| o.len() as f64).collect();
    let bidders_per_player: Vec<f64> = by_player
        .values()
        .map(|o| {
            o.iter()
                .map(|x| x.from.as_str())
                .collect::<BTreeSet<_>>()
                .len() as f64
        })
        .collect();
    let all_bidders: BTreeSet<&str> = watch.offers.values().map(|o| o.from.as_str()).collect();

    // The same club coming back for the same player.
    let (mut repeat_pairs, mut within_cooldown, mut repeats_total) = (0u32, 0u32, 0u32);
    let (mut breaks_phase_rule, mut shortened_by_phase) = (0u32, 0u32);
    let mut repeats_by_phase: BTreeMap<&str, u32> = BTreeMap::new();
    let mut gaps: Vec<f64> = Vec::new();
    let mut pair_counts: BTreeMap<(&str, &str), Vec<&Offer>> = BTreeMap::new();
    for offer in watch.offers.values() {
        pair_counts
            .entry((offer.player.as_str(), offer.from.as_str()))
            .or_default()
            .push(offer);
    }
    for offers in pair_counts.values_mut().filter(|o| o.len() > 1) {
        offers.sort_by(|a, b| a.date.cmp(&b.date).then(a.first_day.cmp(&b.first_day)));
        repeat_pairs += 1;
        repeats_total += offers.len() as u32 - 1;
        for pair in offers.windows(2) {
            // The earlier offer's end (when it closed) against the later one's arrival.
            let closed = pair[0]
                .closed_on
                .as_deref()
                .unwrap_or(pair[0].date.as_str());
            if let Some(gap) = days_between(closed, &pair[1].date) {
                gaps.push(gap as f64);
                within_cooldown += u32::from(gap < REBID_COOLDOWN_DAYS);
                let (required, phase) = phase_cooldown(pair[1].days_left);
                *repeats_by_phase.entry(phase).or_default() += 1;
                shortened_by_phase += u32::from(gap < REBID_COOLDOWN_DAYS && gap >= required);
                breaks_phase_rule += u32::from(gap < required);
            }
        }
    }
    let mut status: BTreeMap<String, u32> = BTreeMap::new();
    for offer in watch.offers.values() {
        *status.entry(format!("{:?}", offer.status)).or_default() += 1;
    }
    let mut by_bin: BTreeMap<u32, u32> = BTreeMap::new();
    for (day, n) in &watch.new_by_day {
        *by_bin.entry((day - 1) / 10 * 10 + 1).or_default() += n;
    }
    let fee_total: f64 = watch.offers.values().map(|o| o.fee as f64).sum();
    json!({
        "offers": watch.offers.len(),
        "distinct_players_offered_for": by_player.len(),
        "distinct_bidding_clubs": all_bidders.len(),
        "offers_per_player": dist(&per_player),
        "unique_bidders_per_player": dist(&bidders_per_player),
        "same_club_same_player_pairs_with_repeats": repeat_pairs,
        "repeat_offers": repeats_total,
        "gap_days_from_earlier_offer_closed_to_next_arrival": dist(&gaps),
        "repeats_inside_the_30_day_cooldown": within_cooldown,
        "of_which_allowed_by_the_closing_window_shortening": shortened_by_phase,
        "repeats_that_break_even_the_phase_cooldown": breaks_phase_rule,
        "repeats_by_window_phase_of_the_later_offer": repeats_by_phase,
        "offers_by_status_at_end": status,
        "new_offers_per_10_days": by_bin,
        "max_new_offers_in_one_day": watch.new_by_day.values().max().copied().unwrap_or(0),
        "window_open_days": watch.window_open_days,
        "offers_first_seen_on_a_day_the_window_was_closed": watch.offers_on_closed_window_days,
        "mean_fee": fee_total / watch.offers.len().max(1) as f64,
    })
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(world), Some(out)) = (args.first(), args.get(1)) else {
        return Err("usage: realism3b_offer_audit <world.json> <out.json> [--clubs a,b] [--seeds 1,2,3] [--days 120]".into());
    };
    let clubs: Vec<String> = flag(&args, "--clubs")
        .unwrap_or_else(|| "fm-c-498".to_string())
        .split(',')
        .map(str::to_string)
        .collect();
    let seeds: Vec<u64> = flag(&args, "--seeds")
        .unwrap_or_else(|| "1".to_string())
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let days: u32 = flag(&args, "--days")
        .and_then(|d| d.parse().ok())
        .unwrap_or(120);

    let mut runs = Vec::new();
    for seed in &seeds {
        let (world_game, stats) = build_fm24_world(std::path::Path::new(world), *seed)?;
        for club in &clubs {
            for answer in ["ignore", "reject"] {
                let mut game = world_game.clone();
                begin_career(&mut game, club, CareerScope::default(), stats.clone())?;
                let mut watch = Watch {
                    user_team: club.clone(),
                    ..Watch::default()
                };
                let options = RunOptions::seasons(1);
                let started = std::time::Instant::now();
                for day in 1..=days {
                    play_day(&mut game, options, 2026, day, &mut watch)
                        .map_err(|e| e.to_string())?;
                    if answer == "reject" {
                        let pending: Vec<(String, String)> = game
                            .players
                            .iter()
                            .filter(|p| p.team_id.as_deref() == Some(club.as_str()))
                            .flat_map(|p| {
                                p.transfer_offers
                                    .iter()
                                    .filter(|o| o.status == TransferOfferStatus::Pending)
                                    .map(|o| (p.id.clone(), o.id.clone()))
                                    .collect::<Vec<_>>()
                            })
                            .collect();
                        for (player_id, offer_id) in pending {
                            ofm_core::transfers::respond_to_offer(
                                &mut game, &player_id, &offer_id, false,
                            )?;
                        }
                        // Seen again so the status written by the answer is the one recorded.
                        let open = game.season_context.transfer_window.status
                            == domain::season::TransferWindowStatus::Open;
                        watch.record(&game, day, open);
                    }
                }
                let mut run = report(&watch);
                run["club"] = json!(club);
                run["seed"] = json!(seed);
                run["manager_answers"] = json!(answer);
                run["seconds"] = json!(started.elapsed().as_secs_f64());
                println!("{club} seed {seed} {answer}: {} offers", watch.offers.len());
                runs.push(run);
            }
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
