//! Mission 6: the transfer market, the free-agent pool and the clubs' books, season by season.
//!
//! `apfm_economy <club id> <seasons> <out.json> [--everything] [--seed N] [--no-tenure-guard]`
//!
//! Plays the career through the production day/rollover entry points and, every day, compares each
//! player with the day before, so every move is *counted where it happened* rather than inferred
//! from a weekly snapshot: AI-to-AI transfers, free signings (with the wage the club agreed, against
//! what it already pays), releases, expiries, loans started/ended, extensions. At each season's end
//! it reads the state the books are in: cash, wage bill against budget, squad sizes, the free-agent
//! pool, and the cash journal by kind. The world file comes from `APFM_WORLD_JSON`.
//!
//! The AI clubs are the 239 that are not the player's; the player's club is passive here, as in
//! every harness run, and is reported apart.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use chrono::NaiveDate;
use domain::finance::CashKind;
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use serde_json::{Value, json};

struct Snap {
    team: Option<String>,
    parent: Option<String>,
    loaned: bool,
    end: Option<String>,
    wage: u32,
}

#[derive(Default)]
struct Economy {
    user: String,
    snap: HashMap<String, Snap>,
    /// season -> event name -> count
    events: BTreeMap<u32, BTreeMap<String, u64>>,
    /// season -> sampled free signings
    signings: BTreeMap<u32, Vec<Value>>,
    /// season -> sampled AI-to-AI transfers
    transfers: BTreeMap<u32, Vec<Value>>,
    signing_ratios: BTreeMap<u32, Vec<f64>>,
    signing_wage_over_budget_pct: BTreeMap<u32, Vec<f64>>,
    extension_wage_change: BTreeMap<u32, Vec<f64>>,
    extension_ages: BTreeMap<u32, BTreeMap<String, u64>>,
    season_state: Vec<Value>,
    prev_cum: HashMap<(String, CashKind), i64>,
    season_started: Option<Instant>,
    season_seconds: Vec<f64>,
    day_millis: Vec<f64>,
    current_season: u32,
    last_day_started: Option<Instant>,
}

fn quantile(values: &mut [f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let index = ((values.len() - 1) as f64 * q).round() as usize;
    values[index]
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn age_on(dob: &str, today: NaiveDate) -> i32 {
    domain::age::age_on_or_unknown(today, dob)
}

impl Economy {
    fn bump(&mut self, season: u32, key: &str) {
        *self
            .events
            .entry(season)
            .or_default()
            .entry(key.to_string())
            .or_default() += 1;
    }

    fn take_snapshot(game: &Game) -> HashMap<String, Snap> {
        game.players
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    Snap {
                        team: p.team_id.clone(),
                        parent: p.active_loan.as_ref().map(|l| l.parent_team_id.clone()),
                        loaned: p.active_loan.is_some(),
                        end: p.contract_end().map(str::to_string),
                        wage: p.wage(),
                    },
                )
            })
            .collect()
    }

    /// Median weekly wage of the club's other registered players.
    fn club_median_wage(game: &Game, team_id: &str, except: &str) -> f64 {
        let mut wages: Vec<f64> = game
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(team_id) && p.id != except)
            .map(|p| f64::from(p.wage()))
            .collect();
        quantile(&mut wages, 0.5)
    }

    fn club_bill_pct_of_budget(game: &Game, team_id: &str) -> f64 {
        let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
            return 0.0;
        };
        if team.wage_budget <= 0 {
            return 0.0;
        }
        ofm_core::finances::calc_wages(game, team_id) as f64 / team.wage_budget as f64 * 100.0
    }

    fn classify_day(&mut self, game: &Game, season: u32) {
        let today = game.clock.current_date.date_naive();
        let today_s = game.clock.current_date.format("%Y-%m-%d").to_string();
        let now = Self::take_snapshot(game);
        let index: HashMap<&str, usize> = game
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.as_str(), i))
            .collect();
        let mut keys: Vec<(String, &'static str)> = Vec::new();
        for (id, cur) in &now {
            let Some(prev) = self.snap.get(id) else {
                keys.push((id.clone(), "new_player"));
                continue;
            };
            let player = &game.players[index[id.as_str()]];
            match (&prev.team, &cur.team) {
                (Some(a), Some(b)) if a != b => {
                    if cur.loaned && !prev.loaned {
                        keys.push((id.clone(), "loan_started"));
                    } else if prev.loaned && !cur.loaned {
                        if prev.parent.as_deref() == Some(b.as_str()) {
                            keys.push((id.clone(), "loan_returned"));
                        } else {
                            keys.push((id.clone(), "loan_bought"));
                        }
                    } else if *a == self.user || *b == self.user {
                        keys.push((id.clone(), "transfer_user_club"));
                    } else {
                        keys.push((id.clone(), "transfer_ai"));
                        let sample = self.transfers.entry(season).or_default();
                        if sample.len() < 400 {
                            let team = |id: &str| game.teams.iter().find(|t| t.id == id);
                            sample.push(json!({
                                "date": today_s,
                                "player_ovr": player.ovr,
                                "age": age_on(&player.date_of_birth, today),
                                "pos": format!("{:?}", player.position),
                                "market_value": player.market_value,
                                "wage_before": prev.wage,
                                "wage_after": cur.wage,
                                "from": team(a).map(|t| json!([t.name, t.reputation])),
                                "to": team(b).map(|t| json!([t.name, t.reputation])),
                            }));
                        }
                    }
                }
                (Some(_), None) => {
                    let expired = prev.end.as_deref().is_some_and(|e| e <= today_s.as_str());
                    keys.push((
                        id.clone(),
                        if expired {
                            "contract_expired"
                        } else {
                            "released_early"
                        },
                    ));
                }
                (None, Some(club)) => {
                    keys.push((id.clone(), "free_agent_signed"));
                    let median = Self::club_median_wage(game, club, id);
                    let ratio = if median > 0.0 {
                        f64::from(cur.wage) / median
                    } else {
                        0.0
                    };
                    let pct = Self::club_bill_pct_of_budget(game, club);
                    self.signing_ratios.entry(season).or_default().push(ratio);
                    self.signing_wage_over_budget_pct
                        .entry(season)
                        .or_default()
                        .push(pct);
                    let sample = self.signings.entry(season).or_default();
                    if sample.len() < 400 {
                        let team = game.teams.iter().find(|t| &t.id == club);
                        sample.push(json!({
                            "date": today_s,
                            "club": team.map(|t| t.name.clone()),
                            "club_rep": team.map(|t| t.reputation),
                            "player_ovr": player.ovr,
                            "age": age_on(&player.date_of_birth, today),
                            "pos": format!("{:?}", player.position),
                            "wage": cur.wage,
                            "club_median_wage": median.round(),
                            "wage_over_median": round2(ratio),
                            "bill_pct_of_budget_after": pct.round(),
                        }));
                    }
                }
                (Some(a), Some(b)) if a == b => {
                    if let (Some(pe), Some(ce)) = (&prev.end, &cur.end)
                        && ce > pe
                        && !cur.loaned
                    {
                        keys.push((id.clone(), "contract_extended"));
                        let change = if prev.wage > 0 {
                            f64::from(cur.wage) / f64::from(prev.wage) - 1.0
                        } else {
                            0.0
                        };
                        self.extension_wage_change
                            .entry(season)
                            .or_default()
                            .push(change * 100.0);
                        let age = age_on(&player.date_of_birth, today);
                        let band = match age {
                            ..=21 => "<=21",
                            22..=29 => "22-29",
                            30..=33 => "30-33",
                            _ => "34+",
                        };
                        *self
                            .extension_ages
                            .entry(season)
                            .or_default()
                            .entry(band.to_string())
                            .or_default() += 1;
                    }
                }
                _ => {}
            }
        }
        for id in self.snap.keys() {
            if !now.contains_key(id) {
                keys.push((id.clone(), "player_removed"));
            }
        }
        for (_, key) in keys {
            self.bump(season, key);
        }
        self.snap = now;
    }

    fn season_state(&mut self, game: &Game, season: u32, label: &str) {
        let user = self.user.clone();
        let team_index: HashMap<&str, usize> = game
            .teams
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.as_str(), i))
            .collect();
        let bills = ofm_core::finances::wage_bills_by_club(game);

        let mut squad_all: HashMap<&str, usize> = HashMap::new();
        let mut squad_senior: HashMap<&str, usize> = HashMap::new();
        for p in &game.players {
            if let Some(t) = p.team_id.as_deref() {
                *squad_all.entry(t).or_default() += 1;
                if p.squad_role == domain::player::SquadRole::Senior {
                    *squad_senior.entry(t).or_default() += 1;
                }
            }
        }
        let ai: Vec<&domain::team::Team> = game.teams.iter().filter(|t| t.id != user).collect();

        let mut cash: Vec<f64> = ai.iter().map(|t| t.finance as f64).collect();
        let negative = ai.iter().filter(|t| t.finance < 0).count();
        let mut bill_pct: Vec<f64> = ai
            .iter()
            .filter(|t| t.wage_budget > 0)
            .map(|t| bills.get(&t.id).copied().unwrap_or(0) as f64 / t.wage_budget as f64 * 100.0)
            .collect();
        let over100 = bill_pct.iter().filter(|p| **p > 100.0).count();
        let over110 = bill_pct.iter().filter(|p| **p > 110.0).count();
        let over150 = bill_pct.iter().filter(|p| **p > 150.0).count();
        let mut all_sizes: Vec<f64> = ai
            .iter()
            .map(|t| squad_all.get(t.id.as_str()).copied().unwrap_or(0) as f64)
            .collect();
        let mut senior_sizes: Vec<f64> = ai
            .iter()
            .map(|t| squad_senior.get(t.id.as_str()).copied().unwrap_or(0) as f64)
            .collect();
        let total_bill: i64 = ai
            .iter()
            .map(|t| bills.get(&t.id).copied().unwrap_or(0))
            .sum();

        // Journal since the last reading, by kind and by club.
        let mut cum: HashMap<(String, CashKind), i64> = HashMap::new();
        for post in game.cash_journal.iter() {
            *cum.entry((post.club_id.clone(), post.kind)).or_default() += post.amount;
        }
        let mut by_kind_ai: BTreeMap<String, i64> = BTreeMap::new();
        let mut op_result: HashMap<String, i64> = HashMap::new();
        let mut matchday: HashMap<String, i64> = HashMap::new();
        for ((club, kind), total) in &cum {
            let delta = total
                - self
                    .prev_cum
                    .get(&(club.clone(), *kind))
                    .copied()
                    .unwrap_or(0);
            if *club != user {
                *by_kind_ai.entry(format!("{kind:?}")).or_default() += delta;
            }
            if kind.counts_toward_season_totals() {
                *op_result.entry(club.clone()).or_default() += delta;
            }
            if *kind == CashKind::Matchday {
                *matchday.entry(club.clone()).or_default() += delta;
            }
        }
        self.prev_cum = cum;
        let ai_ids: Vec<&str> = ai.iter().map(|t| t.id.as_str()).collect();
        let mut ops: Vec<f64> = ai_ids
            .iter()
            .map(|id| op_result.get(*id).copied().unwrap_or(0) as f64)
            .collect();
        let deficit = ops.iter().filter(|v| **v < 0.0).count();
        let no_gate = ai_ids
            .iter()
            .filter(|id| matchday.get(**id).copied().unwrap_or(0) == 0)
            .count();

        let free: Vec<&domain::player::Player> = game
            .players
            .iter()
            .filter(|p| p.team_id.is_none() && !p.retired)
            .collect();
        let mut free_ovr: Vec<f64> = free.iter().map(|p| f64::from(p.ovr)).collect();
        let free_ge_60 = free.iter().filter(|p| p.ovr >= 60).count();
        let free_ge_70 = free.iter().filter(|p| p.ovr >= 70).count();
        let mut pool_profile: BTreeMap<String, u64> = BTreeMap::new();
        let on = game.clock.current_date.date_naive();
        for p in &free {
            let origin = if p.id.starts_with("fm-p-") {
                "fm24"
            } else {
                "generated"
            };
            let age = age_on(&p.date_of_birth, on);
            let age_band = match age {
                ..=20 => "<=20",
                21..=25 => "21-25",
                26..=30 => "26-30",
                _ => "31+",
            };
            let ovr_band = match p.ovr {
                ..=49 => "<50",
                50..=59 => "50-59",
                60..=69 => "60-69",
                _ => "70+",
            };
            *pool_profile
                .entry(format!("{origin} {age_band} ovr{ovr_band}"))
                .or_default() += 1;
        }
        let loans = game
            .players
            .iter()
            .filter(|p| p.active_loan.is_some())
            .count();
        let user_state = team_index.get(user.as_str()).map(|&i| {
            let t = &game.teams[i];
            json!({
                "cash": t.finance,
                "weekly_wage_bill": bills.get(&user).copied().unwrap_or(0),
                "wage_budget": t.wage_budget,
                "squad": squad_all.get(user.as_str()).copied().unwrap_or(0),
                "operating_result_season": op_result.get(&user).copied().unwrap_or(0),
            })
        });

        // The clubs furthest over their wage budget, with what could explain it.
        let mut worst: Vec<(f64, Value)> = ai
            .iter()
            .filter(|t| t.wage_budget > 0)
            .map(|t| {
                let bill = bills.get(&t.id).copied().unwrap_or(0);
                let pct = bill as f64 / t.wage_budget as f64 * 100.0;
                (
                    pct,
                    json!({
                        "club": t.name, "rep": t.reputation, "wage_budget": t.wage_budget,
                        "weekly_bill": bill, "pct": pct.round(), "cash": t.finance,
                        "squad": squad_all.get(t.id.as_str()).copied().unwrap_or(0),
                        "seniors": squad_senior.get(t.id.as_str()).copied().unwrap_or(0),
                        "operating_result": op_result.get(&t.id).copied().unwrap_or(0),
                        "sponsor_weekly": t.sponsorship.as_ref().map(|s| s.base_value),
                    }),
                )
            })
            .collect();
        worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let worst: Vec<Value> = worst.into_iter().take(12).map(|x| x.1).collect();
        // The clubs that lose most money in the season, with what their income is made of.
        let mut losers: Vec<(i64, Value)> = ai
            .iter()
            .map(|t| {
                let result = op_result.get(&t.id).copied().unwrap_or(0);
                (
                    result,
                    json!({
                        "club": t.name, "rep": t.reputation, "stadium": t.stadium_capacity,
                        "operating_result": result,
                        "gate": matchday.get(&t.id).copied().unwrap_or(0),
                        "sponsor_weekly": t.sponsorship.as_ref().map(|s| s.base_value),
                        "weekly_bill": bills.get(&t.id).copied().unwrap_or(0),
                        "wage_budget": t.wage_budget, "cash": t.finance,
                    }),
                )
            })
            .filter(|(result, _)| *result < 0)
            .collect();
        losers.sort_by_key(|row| row.0);
        let deficit_clubs: Vec<Value> = losers.into_iter().take(10).map(|row| row.1).collect();

        // What the three worst-off clubs actually pay, dearest first, and how each came to be there.
        let worst_names: Vec<String> = worst
            .iter()
            .take(3)
            .filter_map(|w| w["club"].as_str().map(str::to_string))
            .collect();
        let worst_players: Vec<Value> = worst_names
            .iter()
            .map(|name| {
                let team = game.teams.iter().find(|t| &t.name == name);
                let mut rows: Vec<(u32, Value)> = game
                    .players
                    .iter()
                    .filter(|p| team.is_some_and(|t| p.team_id.as_deref() == Some(t.id.as_str())))
                    .map(|p| {
                        let last = p.movement_history.last();
                        (
                            p.wage(),
                            json!({
                                "name": p.match_name, "ovr": p.ovr, "wage": p.wage(),
                                "role": format!("{:?}", p.squad_role),
                                "last_move": last.map(|m| format!("{:?} {}", m.kind, m.date)),
                                "source": last.and_then(|m| m.contract.as_ref()).map(|c| format!("{:?}", c.source)),
                                "ends": p.contract_end(),
                            }),
                        )
                    })
                    .collect();
                rows.sort_by_key(|row| std::cmp::Reverse(row.0));
                json!({"club": name, "players": rows.into_iter().take(10).map(|r| r.1).collect::<Vec<_>>()})
            })
            .collect();

        self.season_state.push(json!({
            "season": season,
            "label": label,
            "worst_wage_bill_clubs": worst,
            "worst_clubs_payroll": worst_players,
            "largest_deficits": deficit_clubs,
            "date": game.clock.current_date.format("%Y-%m-%d").to_string(),
            "players_total": game.players.len(),
            "free_agents": {
                "count": free.len(),
                "ovr_ge_60": free_ge_60,
                "ovr_ge_70": free_ge_70,
                "ovr_median": quantile(&mut free_ovr, 0.5),
                "profile": pool_profile,
            },
            "active_loans": loans,
            "ai_clubs": ai.len(),
            "cash": {
                "negative_clubs": negative,
                "negative_pct": round2(negative as f64 / ai.len().max(1) as f64 * 100.0),
                "p10": quantile(&mut cash, 0.1),
                "median": quantile(&mut cash, 0.5),
                "p90": quantile(&mut cash, 0.9),
                "sum": cash.iter().sum::<f64>(),
            },
            "wage_bill": {
                "total_weekly_ai": total_bill,
                "pct_of_budget_p10": round2(quantile(&mut bill_pct, 0.1)),
                "pct_of_budget_median": round2(quantile(&mut bill_pct, 0.5)),
                "pct_of_budget_p90": round2(quantile(&mut bill_pct, 0.9)),
                "clubs_over_100": over100,
                "clubs_over_110": over110,
                "clubs_over_150": over150,
            },
            "squads_all": {
                "min": quantile(&mut all_sizes, 0.0),
                "p10": quantile(&mut all_sizes, 0.1),
                "median": quantile(&mut all_sizes, 0.5),
                "p90": quantile(&mut all_sizes, 0.9),
                "max": quantile(&mut all_sizes, 1.0),
            },
            "squads_senior": {
                "min": quantile(&mut senior_sizes, 0.0),
                "p10": quantile(&mut senior_sizes, 0.1),
                "median": quantile(&mut senior_sizes, 0.5),
                "p90": quantile(&mut senior_sizes, 0.9),
            },
            "journal_since_last_reading_ai": by_kind_ai,
            "operating_result": {
                "deficit_clubs": deficit,
                "deficit_pct": round2(deficit as f64 / ai.len().max(1) as f64 * 100.0),
                "p10": quantile(&mut ops, 0.1),
                "median": quantile(&mut ops, 0.5),
                "p90": quantile(&mut ops, 0.9),
                "clubs_with_no_matchday_income": no_gate,
            },
            "user_club": user_state,
        }));
    }
}

impl SeasonObserver for Economy {
    fn on_day_end(&mut self, game: &Game, season: u32, _day: u32) {
        if self.current_season != season {
            self.current_season = season;
            self.season_started = Some(Instant::now());
        }
        if self.snap.is_empty() {
            self.snap = Self::take_snapshot(game);
        } else {
            self.classify_day(game, season);
        }
        if let Some(t) = self.last_day_started.take() {
            self.day_millis.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        self.last_day_started = Some(Instant::now());
    }

    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        self.season_state(game, season, "avant bascule");
    }

    fn on_season_end(&mut self, _game: &Game, _summary: &EndOfSeasonSummary, season: u32) {
        // The rollover is not a day: the snapshot taken after it is the baseline of the next one,
        // and what it changed (retirements, expiries at the 30 June sweep) is counted with it.
        if let Some(t) = self.season_started.take() {
            self.season_seconds.push(t.elapsed().as_secs_f64());
        }
        let _ = season;
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let club = args.next().expect("club id");
    let seasons: u32 = args.next().expect("seasons").parse().expect("number");
    let out = args.next().expect("output path");
    let rest: Vec<String> = args.collect();
    let flag = |name: &str| rest.iter().any(|a| a == name);
    let seed: u64 = rest
        .iter()
        .position(|a| a == "--seed")
        .and_then(|i| rest.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);

    let path = world_path_from_env().expect("APFM_WORLD_JSON is not set");
    let t0 = Instant::now();
    let mut game = build_fm24_career(&path, &club, seed).expect("career builds");
    let build_s = t0.elapsed().as_secs_f64();
    if flag("--everything") {
        game.active_region_ids.clear();
        game.active_competition_ids.clear();
    }
    let active_clubs = game.active_team_ids().map(|ids| ids.len());
    let mut options = RunOptions::seasons(seasons);
    if flag("--no-tenure-guard") {
        options.keep_manager_employed = false;
    }
    let mut econ = Economy {
        user: game.manager.team_id.clone().unwrap_or_default(),
        ..Default::default()
    };
    econ.snap = Economy::take_snapshot(&game);
    econ.season_state(&game, 0, "ouverture");
    let started = Instant::now();
    let result = run_seasons(&mut game, options, &mut econ);
    let total_s = started.elapsed().as_secs_f64();
    // The state after the last rollover, for the opening of what would be the next season.
    econ.season_state(&game, seasons + 1, "après la dernière bascule");

    let mut per_season = Vec::new();
    for (season, events) in &econ.events {
        let mut ratios = econ.signing_ratios.get(season).cloned().unwrap_or_default();
        let mut bill_after = econ
            .signing_wage_over_budget_pct
            .get(season)
            .cloned()
            .unwrap_or_default();
        let mut ext = econ
            .extension_wage_change
            .get(season)
            .cloned()
            .unwrap_or_default();
        let heavy = ratios.iter().filter(|r| **r > 2.0).count();
        let over_budget = bill_after.iter().filter(|r| **r > 100.0).count();
        per_season.push(json!({
            "season": season,
            "events": events,
            "free_signings": {
                "count": ratios.len(),
                "wage_over_club_median_p50": round2(quantile(&mut ratios, 0.5)),
                "wage_over_club_median_p90": round2(quantile(&mut ratios, 0.9)),
                "signed_above_2x_club_median": heavy,
                "club_bill_pct_of_budget_after_p50": round2(quantile(&mut bill_after, 0.5)),
                "club_bill_pct_of_budget_after_p90": round2(quantile(&mut bill_after, 0.9)),
                "left_club_over_budget": over_budget,
            },
            "extensions": {
                "count": ext.len(),
                "wage_change_pct_p10": round2(quantile(&mut ext, 0.1)),
                "wage_change_pct_p50": round2(quantile(&mut ext, 0.5)),
                "wage_change_pct_p90": round2(quantile(&mut ext, 0.9)),
                "by_age": econ.extension_ages.get(season),
            },
        }));
    }
    let mut days = econ.day_millis.clone();
    let report = json!({
        "headline": {
            "club": club, "seasons": seasons, "seed": seed,
            "everything": flag("--everything"),
            "active_clubs_in_scope": active_clubs,
            "completed": result.is_ok(),
            "error": result.as_ref().err().map(|e| e.to_string()),
            "total_seconds": round2(total_s), "build_seconds": round2(build_s),
            "season_seconds": econ.season_seconds.iter().map(|s| round2(*s)).collect::<Vec<_>>(),
            "day_ms_p50": round2(quantile(&mut days, 0.5)),
            "day_ms_p95": round2(quantile(&mut days, 0.95)),
            "day_ms_max": round2(quantile(&mut days, 1.0)),
        },
        "per_season": per_season,
        "state": econ.season_state,
        "signings_sample": econ.signings,
        "transfers_sample": econ.transfers,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("{}", serde_json::to_string(&report["headline"]).unwrap());
}
