//! REALISM3B: how the market values of the *active world's* players move over several seasons, and
//! what that does to fees, wages and club books.
//!
//! ```text
//! realism3b_valuation_audit --world <world.json> --club fm-c-498 --out out.json [--seasons 5] [--seed 1]
//! realism3b_valuation_audit --generated --out out.json [--seasons 5] [--seed 1]
//! ```
//!
//! The career is played through the production day and rollover path (the manager decides nothing).
//! At the start and after every rollover every club player is read: rating, ceiling, age, value, wage.
//! Between two readings the same player's change in value is set against the change in his rating and
//! age. Completed transfers are read the day they happen (fee against the player's value that day).
//! Aggregates only: no player name is written, and the world file is only read.
//!
//! It touches no API that the revaluation added, so the same file runs on the baseline commit for a
//! like-for-like before/after.

use std::collections::{BTreeMap, HashMap};

use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::build_fm24_career;
use season_harness::worlds::WorldSpec;
use serde_json::{Value, json};

#[derive(Clone)]
struct Row {
    ovr: u8,
    potential: u8,
    age: i32,
    value: u64,
    wage: u32,
}

type Snapshot = BTreeMap<String, Row>;

fn snapshot(game: &Game) -> Snapshot {
    let on = game.clock.current_date.date_naive();
    game.players
        .iter()
        .filter(|p| p.team_id.is_some() && !p.retired)
        .map(|p| {
            (
                p.id.clone(),
                Row {
                    ovr: p.ovr,
                    potential: p.potential,
                    age: domain::age::age_on_or_unknown(on, &p.date_of_birth),
                    value: p.market_value,
                    wage: p.wage(),
                },
            )
        })
        .collect()
}

fn pct(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn quant(values: &[f64]) -> Value {
    json!({
        "n": values.len(),
        "p10": pct(values, 0.10),
        "p50": pct(values, 0.50),
        "p90": pct(values, 0.90),
        "p99": pct(values, 0.99),
        "max": pct(values, 1.0),
    })
}

fn age_group(age: i32) -> &'static str {
    match age {
        ..=20 => "a_<=20",
        21..=23 => "b_21-23",
        24..=26 => "c_24-26",
        27..=29 => "d_27-29",
        30..=32 => "e_30-32",
        _ => "f_33+",
    }
}

fn delta_group(delta: i32) -> &'static str {
    match delta {
        ..=-5 => "a_drop>=5",
        -4..=-1 => "b_drop1-4",
        0 => "c_flat",
        1..=4 => "d_gain1-4",
        _ => "e_gain>=5",
    }
}

fn spread(values: &[f64]) -> Value {
    json!({"n": values.len(), "p10": pct(values, 0.1), "p50": pct(values, 0.5), "p90": pct(values, 0.9)})
}

/// One transition between two readings.
fn transition(before: &Snapshot, after: &Snapshot) -> Value {
    let (mut moved, mut n) = (0usize, 0usize);
    let mut by_age: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut by_delta: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut gain_pairs: Vec<(f64, f64)> = Vec::new();
    let mut wage_ratios: Vec<f64> = Vec::new();
    let mut worst: Option<(i32, Value)> = None;
    for (id, now) in after {
        let Some(then) = before.get(id) else { continue };
        if then.value == 0 {
            continue;
        }
        n += 1;
        moved += usize::from(now.value != then.value);
        let ratio = now.value as f64 / then.value as f64;
        let delta = i32::from(now.ovr) - i32::from(then.ovr);
        if then.wage > 0 {
            wage_ratios.push(f64::from(now.wage) / f64::from(then.wage));
        }
        by_age.entry(age_group(then.age)).or_default().push(ratio);
        by_delta.entry(delta_group(delta)).or_default().push(ratio);
        gain_pairs.push((f64::from(delta as i16), ratio.ln()));
        if then.ovr >= 80 && delta <= -4 {
            let drop = -delta;
            if worst.as_ref().is_none_or(|(d, _)| drop > *d) {
                worst = Some((
                    drop,
                    json!({
                        "ovr": [then.ovr, now.ovr], "age": [then.age, now.age],
                        "potential": [then.potential, now.potential],
                        "value": [then.value, now.value], "ratio": ratio,
                    }),
                ));
            }
        }
    }
    json!({
        "survivors": n,
        "share_value_changed": moved as f64 / n.max(1) as f64,
        "by_age_group_value_ratio": by_age.iter().map(|(k, v)| (k.to_string(), spread(v))).collect::<BTreeMap<_, _>>(),
        "by_ovr_change_value_ratio": by_delta.iter().map(|(k, v)| (k.to_string(), spread(v))).collect::<BTreeMap<_, _>>(),
        "wage_ratio_same_player": spread(&wage_ratios),
        "corr_ovr_change_vs_log_value_ratio": pearson(&gain_pairs),
        "largest_decline_among_80plus": worst.map(|(_, v)| v),
    })
}

fn pearson(pairs: &[(f64, f64)]) -> f64 {
    let n = pairs.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let (ma, mb) = (
        pairs.iter().map(|p| p.0).sum::<f64>() / n,
        pairs.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (a, b) in pairs {
        cov += (a - ma) * (b - mb);
        va += (a - ma).powi(2);
        vb += (b - mb).powi(2);
    }
    if va <= 0.0 || vb <= 0.0 {
        0.0
    } else {
        cov / (va.sqrt() * vb.sqrt())
    }
}

/// Books and squad of the world and of the user's club at one reading.
fn books(game: &Game) -> Value {
    let user = game.manager.team_id.clone().unwrap_or_default();
    let finances: Vec<f64> = game.teams.iter().map(|t| t.finance as f64).collect();
    let bills: Vec<f64> = game
        .teams
        .iter()
        .map(|t| ofm_core::finances::calc_wages(game, &t.id) as f64)
        .collect();
    let negative = game.teams.iter().filter(|t| t.finance < 0).count();
    let debt: f64 = game
        .teams
        .iter()
        .filter(|t| t.finance < 0)
        .map(|t| -(t.finance as f64))
        .sum();
    let squad_value = |team: &str| -> u64 {
        game.players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(team) && !p.retired)
            .map(|p| p.market_value)
            .sum()
    };
    let user_team = game.teams.iter().find(|t| t.id == user);
    let players_in_clubs = game.players.iter().filter(|p| p.team_id.is_some()).count();
    json!({
        "date": game.clock.current_date.format("%Y-%m-%d").to_string(),
        "club_finance": quant(&finances),
        "weekly_wage_bill": quant(&bills),
        "world_weekly_wage_bill_total": bills.iter().sum::<f64>(),
        "clubs_in_debt": negative,
        "total_debt": debt,
        "players_total": game.players.len(),
        "players_in_clubs": players_in_clubs,
        "free_agents": game.players.iter().filter(|p| p.team_id.is_none() && !p.retired).count(),
        "on_loan": game.players.iter().filter(|p| p.active_loan.is_some()).count(),
        "academy_players": game.players.iter().filter(|p| p.squad_role == domain::player::SquadRole::Youth && p.team_id.is_some()).count(),
        "user_club": user_team.map(|t| json!({
            "finance": t.finance, "transfer_budget": t.transfer_budget, "wage_budget": t.wage_budget,
            "weekly_wage_bill": ofm_core::finances::calc_wages(game, &t.id),
            "squad_market_value": squad_value(&t.id),
        })),
    })
}

fn top_valued(snap: &Snapshot, n: usize) -> Value {
    let mut rows: Vec<&Row> = snap.values().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.value));
    Value::Array(
        rows.into_iter()
            .take(n)
            .map(
                |r| json!({"ovr": r.ovr, "potential": r.potential, "age": r.age, "value": r.value}),
            )
            .collect(),
    )
}

#[derive(Default)]
struct Audit {
    snaps: Vec<(String, Snapshot)>,
    books: Vec<Value>,
    seen: HashMap<String, usize>,
    /// (fee, value on the day, season label, age, rating)
    deals: Vec<(u64, u64, String, i32, u8)>,
    season: String,
}

impl Audit {
    fn collect_deals(&mut self, game: &Game) {
        for competition in &game.competitions {
            let seen = self.seen.entry(competition.id.clone()).or_insert(0);
            for deal in competition.transfer_log.iter().skip(*seen) {
                if let Some(player) = game.players.iter().find(|p| p.id == deal.player_id)
                    && deal.fee > 0
                    && player.market_value > 0
                {
                    let age = domain::age::age_on_or_unknown(
                        game.clock.current_date.date_naive(),
                        &player.date_of_birth,
                    );
                    self.deals.push((
                        deal.fee,
                        player.market_value,
                        self.season.clone(),
                        age,
                        player.ovr,
                    ));
                }
            }
            *seen = competition.transfer_log.len();
        }
    }
}

impl SeasonObserver for Audit {
    fn on_day_end(&mut self, game: &Game, season: u32, _day: u32) {
        self.season = format!("s{season}");
        self.collect_deals(game);
    }

    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, season: u32) {
        self.snaps.push((format!("s{season}"), snapshot(game)));
        self.books.push(books(game));
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = flag(&args, "--out")
        .ok_or("usage: --out <file> [--world w.json --club id | --generated]")?;
    let seasons: u32 = flag(&args, "--seasons")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let seed: u64 = flag(&args, "--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let (mut game, label) = if args.iter().any(|a| a == "--generated") {
        (
            WorldSpec::shipped(seed).build()?,
            "generated-shipped".to_string(),
        )
    } else {
        let world = flag(&args, "--world").ok_or("--world <world.json> or --generated")?;
        let club = flag(&args, "--club").unwrap_or_else(|| "fm-c-498".to_string());
        (
            build_fm24_career(std::path::Path::new(&world), &club, seed)?,
            format!("fm24:{club}"),
        )
    };
    let started = std::time::Instant::now();
    let mut audit = Audit {
        snaps: vec![("s0".to_string(), snapshot(&game))],
        books: vec![books(&game)],
        ..Audit::default()
    };
    run_seasons(&mut game, RunOptions::seasons(seasons), &mut audit).map_err(|e| e.to_string())?;
    let elapsed = started.elapsed().as_secs_f64();

    let mut transitions = serde_json::Map::new();
    let mut distributions = serde_json::Map::new();
    let mut tops = serde_json::Map::new();
    let mut stationarity = serde_json::Map::new();
    for (i, (name, snap)) in audit.snaps.iter().enumerate() {
        let values: Vec<f64> = snap
            .values()
            .map(|r| r.value as f64)
            .filter(|v| *v > 0.0)
            .collect();
        distributions.insert(name.clone(), quant(&values));
        let mean_ovr =
            snap.values().map(|r| f64::from(r.ovr)).sum::<f64>() / snap.len().max(1) as f64;
        let mean_age =
            snap.values().map(|r| f64::from(r.age)).sum::<f64>() / snap.len().max(1) as f64;
        let mut by_band: BTreeMap<u8, Vec<f64>> = BTreeMap::new();
        for r in snap
            .values()
            .filter(|r| r.value > 0 && (24..=29).contains(&r.age))
        {
            by_band
                .entry(r.ovr / 10 * 10)
                .or_default()
                .push(r.value as f64);
        }
        stationarity.insert(
            name.clone(),
            json!({
                "mean_ovr": mean_ovr, "mean_age": mean_age,
                "median_value_by_ovr_band_age_24_29": by_band.iter().map(|(k, v)| (k.to_string(), json!({"n": v.len(), "p50": pct(v, 0.5)}))).collect::<BTreeMap<_, _>>(),
                "median_value_academy_prospects_age_le_19": pct(&snap.values().filter(|r| r.age <= 19 && r.value > 0).map(|r| r.value as f64).collect::<Vec<_>>(), 0.5),
            }),
        );
        tops.insert(name.clone(), top_valued(snap, 10));
        if i > 0 {
            transitions.insert(
                format!("{}->{}", audit.snaps[i - 1].0, name),
                transition(&audit.snaps[i - 1].1, snap),
            );
        }
    }
    // First reading against the last: the whole run.
    let whole = transition(&audit.snaps[0].1, &audit.snaps[audit.snaps.len() - 1].1);

    let ratios: Vec<f64> = audit
        .deals
        .iter()
        .map(|(f, v, ..)| *f as f64 / *v as f64)
        .collect();
    let mut deals_by_season: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (f, v, s, ..) in &audit.deals {
        deals_by_season
            .entry(s.clone())
            .or_default()
            .push(*f as f64 / *v as f64);
    }
    let mut by_player_age: BTreeMap<String, Value> = BTreeMap::new();
    {
        let mut m: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
        for d in &audit.deals {
            let e = m.entry(age_group(d.3)).or_default();
            e.0 += 1;
            e.1 += d.0;
        }
        for (k, (n, f)) in m {
            by_player_age.insert(k.to_string(), json!({"n": n, "fees": f}));
        }
    }
    let mut by_fee_band: BTreeMap<String, usize> = BTreeMap::new();
    for d in &audit.deals {
        let band = match d.0 {
            0..=99_999 => "a_<100k",
            100_000..=999_999 => "b_100k-1M",
            1_000_000..=9_999_999 => "c_1M-10M",
            _ => "d_>=10M",
        };
        *by_fee_band.entry(band.to_string()).or_default() += 1;
    }
    let report = json!({
        "label": label, "seed": seed, "seasons": seasons, "elapsed_s": elapsed,
        "value_distribution_club_players": distributions,
        "top10_valued": tops,
        "stationarity": stationarity,
        "transitions": transitions,
        "whole_run": whole,
        "paid_transfers": {
            "n": audit.deals.len(),
            "fee_over_value": spread(&ratios),
            "total_fees": audit.deals.iter().map(|d| d.0).sum::<u64>(),
            "by_player_age": by_player_age,
            "by_fee_band": by_fee_band,
            "by_season": deals_by_season.iter().map(|(k, v)| (k.clone(), spread(v))).collect::<BTreeMap<_, _>>(),
        },
        "books": audit.books,
    });
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("wrote {out} ({label}, seed {seed}, {seasons} seasons, {elapsed:.1}s)");
    Ok(())
}
