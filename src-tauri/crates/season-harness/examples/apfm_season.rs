//! Mission 4: whole-season scenarios in the FM24 world.
//!
//! `apfm_season <club id> <seasons> <out.json> [--everything] [--live] [--seed N] [--no-save-test]`
//!
//! Plays the career through the production day/rollover entry points (`season_harness::driver`),
//! holds it to the harness invariants, and records what a report needs: matches, scores, tables,
//! injuries, moves, finances, development, timings, and a save/reload comparison at mid-season,
//! at the end of a season and at the start of the next. The world file comes from `APFM_WORLD_JSON`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use chrono::{Datelike, NaiveDate};
use db::save_manager::SaveManager;
use domain::league::{CompetitionType, FixtureCompetition, FixtureStatus, League};
use domain::stats::{StatsState, TeamMatchStatsRecord};
use ofm_core::end_of_season::{self, EndOfSeasonSummary};
use ofm_core::game::Game;
use season_harness::driver::{DayPath, RunOptions, SeasonObserver, play_day, roll_season_over};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;
use serde_json::{Value, json};

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

    let t_build = Instant::now();
    let mut game = if club == "native" {
        // The world OFM ships (440 generated clubs), for a baseline the FM24 numbers are read against.
        season_harness::worlds::WorldSpec::shipped(seed)
            .build()
            .expect("native world builds")
    } else {
        let path = world_path_from_env().expect("APFM_WORLD_JSON is not set");
        build_fm24_career(&path, &club, seed).expect("career builds")
    };
    let build_s = t_build.elapsed().as_secs_f64();
    if flag("--everything") {
        game.active_region_ids.clear();
        game.active_competition_ids.clear();
    }
    let mut options = RunOptions::seasons(seasons);
    if flag("--no-tenure-guard") {
        options.keep_manager_employed = false;
    }
    if flag("--live") {
        options.day_path = DayPath::UserMatchLive;
    }

    let report = run(
        &mut game,
        options,
        seasons,
        !flag("--no-save-test"),
        build_s,
        seed,
    );
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("{}", serde_json::to_string(&report["headline"]).unwrap());
}

// ---------------------------------------------------------------------------------------------
// Observer
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
struct MatchRow {
    season: u32,
    comp: FixtureCompetition,
    home: String,
    away: String,
    hg: u8,
    ag: u8,
    h_shots: u16,
    a_shots: u16,
    h_sot: u16,
    a_sot: u16,
    h_poss: u8,
    yellow: u32,
    red: u32,
    fouls: u32,
}

struct PlayerSnap {
    team: Option<String>,
    contract_end: Option<String>,
    loaned: bool,
}

#[derive(Default)]
struct Collector {
    rows: Vec<MatchRow>,
    pending: HashMap<String, TeamMatchStatsRecord>,
    strength: HashMap<String, f64>,
    season_strength: BTreeMap<u32, HashMap<String, f64>>,
    injured: HashSet<String>,
    new_injuries: u32,
    injury_days: u64,
    max_injured: usize,
    snap: HashMap<String, PlayerSnap>,
    moves: BTreeMap<String, BTreeMap<String, u32>>,
    expired_attached_max: usize,
    min_squad: usize,
    user_squad: Vec<(String, usize)>,
    ovr_start: HashMap<String, (u8, i32)>,
    progression: Vec<Value>,
    comp_checks: Vec<Value>,
    comp_faults: Vec<String>,
    memberships: HashMap<String, Vec<String>>,
    movement: Vec<Value>,
    finance_rows: Vec<Value>,
    last_season: u32,
    max_active_loans: usize,
    stats: StatsState,
    jersey_clashes: Vec<Value>,
    jersey_clash_days: u32,
}

fn top14(game: &Game) -> HashMap<String, f64> {
    let mut by_team: HashMap<&str, Vec<u8>> = HashMap::new();
    for p in &game.players {
        if let Some(t) = p.team_id.as_deref()
            && p.injury.is_none()
        {
            by_team.entry(t).or_default().push(p.ovr);
        }
    }
    by_team
        .into_iter()
        .map(|(t, mut v)| {
            v.sort_unstable_by(|a, b| b.cmp(a));
            let n = v.len().clamp(1, 14);
            (
                t.to_string(),
                v.iter().take(14).map(|x| *x as f64).sum::<f64>() / n as f64,
            )
        })
        .collect()
}

fn age_on(dob: &str, today: NaiveDate) -> i32 {
    NaiveDate::parse_from_str(dob, "%Y-%m-%d")
        .map(|d| {
            let mut a = today.year() - d.year();
            if (today.month(), today.day()) < (d.month(), d.day()) {
                a -= 1;
            }
            a
        })
        .unwrap_or(-1)
}

impl Collector {
    fn new(game: &Game) -> Self {
        let mut c = Collector {
            min_squad: usize::MAX,
            ..Default::default()
        };
        c.strength = top14(game);
        c
    }

    fn snapshot_ovr(&mut self, game: &Game) {
        let today = game.clock.current_date.date_naive();
        self.ovr_start = game
            .players
            .iter()
            .map(|p| (p.id.clone(), (p.ovr, age_on(&p.date_of_birth, today))))
            .collect();
    }

    fn progression_report(&mut self, game: &Game, label: &str) {
        let today = game.clock.current_date.date_naive();
        let mut buckets: BTreeMap<&str, (i64, i64, i64)> = BTreeMap::new(); // sum, count, pot_gap
        let mut moved: Vec<(i32, String)> = Vec::new();
        for p in &game.players {
            if let Some((o0, age)) = self.ovr_start.get(&p.id) {
                let b = match *age {
                    a if a < 0 => "?",
                    a if a <= 18 => "<=18",
                    19..=21 => "19-21",
                    22..=25 => "22-25",
                    26..=29 => "26-29",
                    30..=33 => "30-33",
                    _ => "34+",
                };
                let d = p.ovr as i64 - *o0 as i64;
                let e = buckets.entry(b).or_default();
                e.0 += d;
                e.1 += 1;
                moved.push((
                    d as i32,
                    format!("{} ({}→{}, {} ans)", p.match_name, o0, p.ovr, age),
                ));
            }
        }
        let _ = today;
        moved.sort();
        let table: BTreeMap<&str, Value> = buckets
            .into_iter()
            .map(|(k, (s, n, _))| (k, json!({"n": n, "mean_delta": (s as f64 / n.max(1) as f64 * 100.0).round() / 100.0})))
            .collect();
        self.progression.push(json!({
            "label": label,
            "by_age": table,
            "biggest_falls": moved.iter().take(3).map(|x| x.1.clone()).collect::<Vec<_>>(),
            "biggest_rises": moved.iter().rev().take(3).map(|x| x.1.clone()).collect::<Vec<_>>(),
            "players_compared": moved.len(),
        }));
    }

    fn weekly(&mut self, game: &Game) {
        let today = game.clock.current_date.date_naive();
        let month = format!("{}-{:02}", today.year(), today.month());
        let mut cur: HashMap<String, PlayerSnap> = HashMap::with_capacity(game.players.len());
        let mut loans = 0;
        let mut expired = 0;
        for p in &game.players {
            let loaned = p.active_loan.is_some();
            if loaned {
                loans += 1;
            }
            if p.team_id.is_some()
                && let Some(end) = p.contract_end()
                && let Ok(e) = NaiveDate::parse_from_str(end, "%Y-%m-%d")
                && e < today
                && !loaned
            {
                expired += 1;
            }
            cur.insert(
                p.id.clone(),
                PlayerSnap {
                    team: p.team_id.clone(),
                    contract_end: p.contract_end().map(str::to_string),
                    loaned,
                },
            );
        }
        self.max_active_loans = self.max_active_loans.max(loans);
        self.expired_attached_max = self.expired_attached_max.max(expired);
        let m = self.moves.entry(month).or_default();
        let mut bump = |k: &str| *m.entry(k.to_string()).or_default() += 1;
        for (id, now) in &cur {
            match self.snap.get(id) {
                None => bump("new_player"),
                Some(prev) => {
                    match (&prev.team, &now.team) {
                        (Some(a), Some(b)) if a != b => {
                            if now.loaned && !prev.loaned {
                                bump("loan_out")
                            } else if prev.loaned && !now.loaned {
                                bump("loan_return_or_buy")
                            } else {
                                bump("transfer")
                            }
                        }
                        (Some(_), None) => bump("released_to_free_agency"),
                        (None, Some(_)) => bump("free_agent_signed"),
                        _ => {}
                    }
                    if prev.team == now.team
                        && let (Some(a), Some(b)) = (&prev.contract_end, &now.contract_end)
                        && b > a
                    {
                        bump("contract_extended")
                    }
                }
            }
        }
        for id in self.snap.keys() {
            if !cur.contains_key(id) {
                *m.entry("player_gone_retired".into()).or_default() += 1;
            }
        }
        self.snap = cur;
    }

    fn check_leagues(&mut self, game: &Game, season: u32) {
        let mut tables = Vec::new();
        for c in &game.competitions {
            if c.kind != CompetitionType::League
                || c.rules.format != domain::league::CompetitionFormat::LeagueTable
            {
                continue;
            }
            let n = c.participant_ids.len();
            let mut faults: Vec<String> = Vec::new();
            let uniq: HashSet<&String> = c.participant_ids.iter().collect();
            if uniq.len() != n {
                faults.push(format!("{}: doublons dans les participants", c.id));
            }
            // calendar shape: every ordered pair once
            let mut pairs: HashMap<(&str, &str), u32> = HashMap::new();
            let mut home: HashMap<&str, u32> = HashMap::new();
            let mut away: HashMap<&str, u32> = HashMap::new();
            let mut by_day: HashMap<(&str, &str), u32> = HashMap::new();
            let mut done = 0;
            let (mut gf, mut ga) = (0u64, 0u64);
            let mut recomputed: HashMap<&str, [u32; 6]> = HashMap::new(); // p w d l gf ga
            let league_fixtures: Vec<&domain::league::Fixture> = c
                .fixtures
                .iter()
                .filter(|f| f.competition == FixtureCompetition::League)
                .collect();
            let friendlies = c.fixtures.len() - league_fixtures.len();
            for f in league_fixtures.iter().copied() {
                *pairs.entry((&f.home_team_id, &f.away_team_id)).or_default() += 1;
                *home.entry(&f.home_team_id).or_default() += 1;
                *away.entry(&f.away_team_id).or_default() += 1;
                *by_day.entry((&f.date, &f.home_team_id)).or_default() += 1;
                *by_day.entry((&f.date, &f.away_team_id)).or_default() += 1;
                if f.home_team_id == f.away_team_id {
                    faults.push(format!("{}: {} joue contre lui-même", c.id, f.home_team_id));
                }
                if f.status == FixtureStatus::Completed {
                    done += 1;
                    if let Some(r) = &f.result {
                        gf += r.home_goals as u64;
                        ga += r.away_goals as u64;
                        let h = recomputed.entry(&f.home_team_id).or_default();
                        h[0] += 1;
                        h[4] += r.home_goals as u32;
                        h[5] += r.away_goals as u32;
                        match r.home_goals.cmp(&r.away_goals) {
                            std::cmp::Ordering::Greater => h[1] += 1,
                            std::cmp::Ordering::Equal => h[2] += 1,
                            _ => h[3] += 1,
                        }
                        let a = recomputed.entry(&f.away_team_id).or_default();
                        a[0] += 1;
                        a[4] += r.away_goals as u32;
                        a[5] += r.home_goals as u32;
                        match r.away_goals.cmp(&r.home_goals) {
                            std::cmp::Ordering::Greater => a[1] += 1,
                            std::cmp::Ordering::Equal => a[2] += 1,
                            _ => a[3] += 1,
                        }
                    }
                }
            }
            let complete = done == league_fixtures.len();
            let expected = n * (n.saturating_sub(1));
            if league_fixtures.len() != expected {
                faults.push(format!(
                    "{}: {} rencontres de championnat au lieu de {}",
                    c.id,
                    league_fixtures.len(),
                    expected
                ));
            }
            if pairs.values().any(|v| *v != 1) || pairs.len() != expected {
                faults.push(format!(
                    "{}: couples domicile/extérieur non uniques ({} couples)",
                    c.id,
                    pairs.len()
                ));
            }
            if home.values().any(|v| *v as usize != n - 1)
                || away.values().any(|v| *v as usize != n - 1)
            {
                faults.push(format!("{}: domiciles/extérieurs non équilibrés", c.id));
            }
            let double_booked = by_day.values().filter(|v| **v > 1).count();
            if double_booked > 0 {
                faults.push(format!(
                    "{}: {} équipes jouent 2 fois le même jour",
                    c.id, double_booked
                ));
            }
            let (sf, sa): (u64, u64) = c.standings.iter().fold((0, 0), |a, s| {
                (a.0 + s.goals_for as u64, a.1 + s.goals_against as u64)
            });
            if sf != sa {
                faults.push(format!(
                    "{}: buts marqués {} ≠ encaissés {} au classement",
                    c.id, sf, sa
                ));
            }
            if sf != gf + ga {
                faults.push(format!(
                    "{}: {} buts au classement pour {} dans les rencontres",
                    c.id,
                    sf,
                    gf + ga
                ));
            }
            let mut standings_ok = true;
            for s in &c.standings {
                let r = recomputed
                    .get(s.team_id.as_str())
                    .copied()
                    .unwrap_or_default();
                if (
                    s.played,
                    s.won,
                    s.drawn,
                    s.lost,
                    s.goals_for,
                    s.goals_against,
                ) != (r[0], r[1], r[2], r[3], r[4], r[5])
                    || s.points != 3 * s.won + s.drawn
                    || s.played != s.won + s.drawn + s.lost
                {
                    standings_ok = false;
                    faults.push(format!(
                        "{}: classement de {} incohérent avec les rencontres",
                        c.id, s.team_id
                    ));
                    break;
                }
            }
            if c.standings.len() != n {
                faults.push(format!(
                    "{}: {} lignes de classement pour {} équipes",
                    c.id,
                    c.standings.len(),
                    n
                ));
            }
            let sorted = c.sorted_standings();
            let champion = sorted
                .first()
                .map(|s| s.team_id.clone())
                .unwrap_or_default();
            let ties = sorted
                .windows(2)
                .filter(|w| {
                    w[0].points == w[1].points
                        && w[0].goal_difference() == w[1].goal_difference()
                        && w[0].goals_for == w[1].goals_for
                })
                .count();
            let team_name = |id: &str| {
                game.teams
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.name.clone())
                    .unwrap_or(id.to_string())
            };
            let top: Vec<Value> = sorted
                .iter()
                .take(3)
                .chain(
                    sorted
                        .iter()
                        .rev()
                        .take(3)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev(),
                )
                .map(|s| {
                    json!([
                        team_name(&s.team_id),
                        s.played,
                        s.points,
                        s.goals_for as i64 - s.goals_against as i64
                    ])
                })
                .collect();
            let table_full: Vec<Value> = sorted
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    json!([
                        i + 1,
                        team_name(&s.team_id),
                        s.played,
                        s.won,
                        s.drawn,
                        s.lost,
                        s.goals_for,
                        s.goals_against,
                        s.points
                    ])
                })
                .collect();
            self.comp_faults.extend(faults.iter().cloned());
            tables.push(json!({
                "id": c.id, "name": c.name, "season": c.season, "teams": n,
                "participants": c.participant_ids.iter().map(|id| { let t = game.teams.iter().find(|t| &t.id == id); json!([id, t.map(|t| t.name.clone()), t.map(|t| t.reputation)]) }).collect::<Vec<_>>(), "league_fixtures": league_fixtures.len(), "friendlies": friendlies,
                "completed": done, "complete": complete, "first_fixture": league_fixtures.iter().map(|f| f.date.as_str()).min(), "last_fixture": league_fixtures.iter().map(|f| f.date.as_str()).max(), "now": game.clock.current_date.format("%Y-%m-%d").to_string(), "standings_consistent": standings_ok,
                "goals": gf, "champion": team_name(&champion), "exact_ties_after_gd_gf": ties,
                "edges": top, "table": table_full, "faults": faults,
            }));
            self.memberships.insert(
                format!("{season}:{}", c.id),
                sorted.iter().map(|s| s.team_id.clone()).collect(),
            );
        }
        self.comp_checks
            .push(json!({"season": season, "leagues": tables}));
    }

    fn record_finances(&mut self, game: &Game, label: &str) {
        let user = game.manager.team_id.clone().unwrap_or_default();
        let mut vals: Vec<i64> = game.teams.iter().map(|t| t.finance).collect();
        vals.sort_unstable();
        let neg = vals.iter().filter(|v| **v < 0).count();
        let u = game.teams.iter().find(|t| t.id == user);
        let wage_bill: i64 = game
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(&user))
            .map(|p| p.wage() as i64 * 52)
            .sum();
        let mut sizes: HashMap<&str, usize> = HashMap::new();
        for p in &game.players {
            if let (Some(t), None) = (p.team_id.as_deref(), p.active_loan.as_ref()) {
                *sizes.entry(t).or_default() += 1;
            }
        }
        let mut sz: Vec<usize> = game
            .teams
            .iter()
            .map(|t| sizes.get(t.id.as_str()).copied().unwrap_or(0))
            .collect();
        sz.sort_unstable();
        let free_agents = game.players.iter().filter(|p| p.team_id.is_none()).count();
        let mut by_kind: BTreeMap<String, i64> = BTreeMap::new();
        for post in game.cash_journal.iter().filter(|post| post.club_id == user) {
            *by_kind.entry(format!("{:?}", post.kind)).or_default() += post.amount;
        }
        self.finance_rows.push(json!({
            "label": label,
            "user_cash_journal_total_by_kind": by_kind,
            "free_agents": free_agents,
            "squad_size_min_p10_median_max": [sz.first(), sz.get(sz.len() / 10), sz.get(sz.len() / 2), sz.last()],
            "user_finance": u.map(|t| t.finance),
            "user_season_income": u.map(|t| t.season_income),
            "user_season_expenses": u.map(|t| t.season_expenses),
            "user_transfer_budget": u.map(|t| t.transfer_budget),
            "user_wage_budget": u.map(|t| t.wage_budget),
            "user_annual_wage_bill": wage_bill,
            "clubs": vals.len(), "clubs_negative": neg,
            "min": vals.first(), "median": vals.get(vals.len() / 2), "max": vals.last(),
        }));
    }
}

impl SeasonObserver for Collector {
    fn on_stats_captured(&mut self, stats: &StatsState) {
        self.stats.append(stats.clone());
        for r in &stats.team_matches {
            match self.pending.remove(&r.fixture_id) {
                None => {
                    self.pending.insert(r.fixture_id.clone(), r.clone());
                }
                Some(other) => {
                    let (h, a) = if r.team_id == r.home_team_id {
                        (r, &other)
                    } else {
                        (&other, r)
                    };
                    self.rows.push(MatchRow {
                        season: self.last_season,
                        comp: r.competition.clone(),
                        home: h.team_id.clone(),
                        away: a.team_id.clone(),
                        hg: h.goals_for,
                        ag: a.goals_for,
                        h_shots: h.shots,
                        a_shots: a.shots,
                        h_sot: h.shots_on_target,
                        a_sot: a.shots_on_target,
                        h_poss: h.possession_pct,
                        yellow: (h.yellow_cards + a.yellow_cards) as u32,
                        red: (h.red_cards + a.red_cards) as u32,
                        fouls: (h.fouls_committed + a.fouls_committed) as u32,
                    });
                }
            }
        }
    }

    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        if day == 1 || season != self.last_season {
            self.last_season = season;
            let s = top14(game);
            self.season_strength.insert(season, s.clone());
            self.strength = s;
            if day == 1 {
                self.snapshot_ovr(game);
                self.snap.clear();
                self.weekly(game);
                self.record_finances(game, &format!("s{season} début"));
            }
        }
        {
            let mut seen: HashMap<(&str, u8), &str> = HashMap::new();
            let mut clashes = Vec::new();
            for p in &game.players {
                if let (Some(t), Some(n)) = (p.team_id.as_deref(), p.jersey_number)
                    && let Some(other) = seen.insert((t, n), p.id.as_str())
                {
                    clashes.push((t.to_string(), n, other.to_string(), p.id.clone()));
                }
            }
            if !clashes.is_empty() {
                self.jersey_clash_days += 1;
                if self.jersey_clashes.len() < 12 {
                    let (t, n, a, b) = clashes[0].clone();
                    let kinds = |id: &str| {
                        game.players
                            .iter()
                            .find(|p| p.id == id)
                            .map(|p| {
                                p.movement_history
                                    .iter()
                                    .rev()
                                    .take(2)
                                    .map(|m| format!("{:?}@{}", m.kind, m.date))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    };
                    self.jersey_clashes.push(json!({"date": game.clock.current_date.format("%Y-%m-%d").to_string(), "team": t, "number": n, "players": [a.clone(), b.clone()], "recent_moves": [kinds(&a), kinds(&b)], "clashes_today": clashes.len()}));
                }
            }
        }
        let now: HashSet<String> = game
            .players
            .iter()
            .filter(|p| p.injury.is_some())
            .map(|p| p.id.clone())
            .collect();
        self.new_injuries += now.difference(&self.injured).count() as u32;
        self.injury_days += now.len() as u64;
        self.max_injured = self.max_injured.max(now.len());
        self.injured = now;
        if day.is_multiple_of(7) {
            self.weekly(game);
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for p in &game.players {
                if let Some(t) = p.team_id.as_deref()
                    && p.active_loan.is_none()
                {
                    *counts.entry(t).or_default() += 1;
                }
            }
            if let Some(m) = counts.values().min() {
                self.min_squad = self.min_squad.min(*m);
            }
            if let Some(u) = game.manager.team_id.as_deref() {
                self.user_squad.push((
                    game.clock.current_date.format("%Y-%m-%d").to_string(),
                    *counts.get(u).unwrap_or(&0),
                ));
            }
        }
        if day == 150 {
            self.record_finances(game, &format!("s{season} jour 150"));
        }
    }

    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        self.progression_report(game, &format!("s{season}: début de saison → fin de saison"));
        self.record_finances(game, &format!("s{season} fin (avant bascule)"));
        self.check_leagues(game, season);
        self.movement.push(json!({"season": season, "date": game.clock.current_date.format("%Y-%m-%d").to_string()}));
    }

    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, season: u32) {
        self.record_finances(game, &format!("s{season} après bascule"));
    }
}

// ---------------------------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------------------------

fn dir_size(p: &std::path::Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
                .sum()
        })
        .unwrap_or(0)
}

fn keyed_by_id(v: &[Value]) -> bool {
    !v.is_empty() && v.iter().all(|e| e.get("id").is_some_and(Value::is_string))
}

fn game_json(game: &Game) -> Value {
    serde_json::to_value(game).expect("game serialises")
}

/// Where two JSON values differ, as patterns (ids in brackets replaced by `*`) with a count and
/// one example each, so a save that changed ten thousand players reads as one line.
#[derive(Default)]
struct Diffs {
    patterns: BTreeMap<String, (u32, String)>,
}

impl Diffs {
    fn add(&mut self, path: &str, example: String) {
        let mut pattern = String::new();
        let mut depth = 0;
        for ch in path.chars() {
            match ch {
                '[' => {
                    depth += 1;
                    if depth == 1 {
                        pattern.push_str("[*]");
                    }
                }
                ']' => depth -= 1,
                c if depth == 0 => pattern.push(c),
                _ => {}
            }
        }
        let e = self.patterns.entry(pattern).or_insert((0, example));
        e.0 += 1;
    }

    fn lines(&self) -> Vec<String> {
        self.patterns
            .iter()
            .map(|(k, (n, ex))| format!("{k} ×{n} — ex. {ex}"))
            .collect()
    }
}

fn diff_json(a: &Value, b: &Value, path: &str, out: &mut Diffs) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => diff_json(p, q, &format!("{path}.{k}"), out),
                    (p, q) => out.add(
                        &format!("{path}.{k}"),
                        format!("{path}.{k}: présent {} → {}", p.is_some(), q.is_some()),
                    ),
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if keyed_by_id(x) && keyed_by_id(y) => {
            let ix: HashMap<&str, &Value> =
                x.iter().map(|v| (v["id"].as_str().unwrap(), v)).collect();
            let iy: HashMap<&str, &Value> =
                y.iter().map(|v| (v["id"].as_str().unwrap(), v)).collect();
            if x.len() != y.len() {
                out.add(
                    &format!("{path}.<longueur>"),
                    format!("{} → {}", x.len(), y.len()),
                );
            }
            let first_moved = x.iter().zip(y).position(|(p, q)| p["id"] != q["id"]);
            if let Some(i) = first_moved {
                out.add(
                    &format!("{path}.<ordre>"),
                    format!(
                        "premier écart à l'index {i}: {} → {}",
                        x[i]["id"], y[i]["id"]
                    ),
                );
            }
            for (id, p) in &ix {
                match iy.get(id) {
                    Some(q) if p != q => diff_json(p, q, &format!("{path}[{id}]"), out),
                    None => out.add(
                        &format!("{path}.<absent>"),
                        format!("{id} absent après rechargement"),
                    ),
                    _ => {}
                }
            }
            for id in iy.keys().filter(|k| !ix.contains_key(*k)) {
                out.add(
                    &format!("{path}.<apparu>"),
                    format!("{id} apparu après rechargement"),
                );
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.add(
                    &format!("{path}.<longueur>"),
                    format!("{} → {}", x.len(), y.len()),
                );
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if p != q {
                    diff_json(p, q, &format!("{path}[{i}]"), out);
                }
            }
        }
        _ => {
            if a != b {
                let s = |v: &Value| v.to_string().chars().take(50).collect::<String>();
                out.add(path, format!("{} → {}", s(a), s(b)));
            }
        }
    }
}

struct SaveCheck {
    label: String,
    save_secs: f64,
    load_secs: f64,
    bytes: u64,
    diffs: Vec<String>,
}

fn save_reload(
    game: &Game,
    stats: &StatsState,
    dir: &std::path::Path,
    label: &str,
) -> (Game, SaveCheck) {
    let mut sm = SaveManager::init(dir).expect("save dir");
    let t = Instant::now();
    let id = sm.create_save_with_stats(game, stats, label).expect("save");
    let save_secs = t.elapsed().as_secs_f64();
    let bytes = dir_size(dir);
    let mut sm2 = SaveManager::init(dir).expect("save dir (reopened)");
    sm2.load_saves().expect("index");
    let t = Instant::now();
    let mut loaded = sm2.load_game(&id).expect("load");
    ofm_core::ai_hiring::seed_ai_managers(&mut loaded);
    ofm_core::season_context::refresh_game_context(&mut loaded);
    let load_secs = t.elapsed().as_secs_f64();
    let mut dset = Diffs::default();
    diff_json(&game_json(game), &game_json(&loaded), "", &mut dset);
    let mut diffs = dset.lines();
    // Put the competitions back in the order the career had them, so what is compared next is the
    // simulation and not the list order (the difference is already in `diffs`).
    let order: HashMap<&str, usize> = game
        .competitions
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    loaded
        .competitions
        .sort_by_key(|c| order.get(c.id.as_str()).copied().unwrap_or(usize::MAX));
    let stats_back = sm2.load_stats_state(&id).expect("stats");
    if stats_back.team_matches.len() != stats.team_matches.len()
        || stats_back.player_matches.len() != stats.player_matches.len()
    {
        diffs.push(format!(
            "stats: team_matches {}→{}, player_matches {}→{}",
            stats.team_matches.len(),
            stats_back.team_matches.len(),
            stats.player_matches.len(),
            stats_back.player_matches.len()
        ));
    }
    (
        loaded,
        SaveCheck {
            label: label.into(),
            save_secs,
            load_secs,
            bytes,
            diffs,
        },
    )
}

/// What a replay has to reproduce, in an order that does not depend on how lists are stored:
/// every competition's table, every player's club, rating and contract end, every club's cash.
fn digest(g: &Game) -> Vec<String> {
    let mut lines = Vec::new();
    for c in &g.competitions {
        for s in &c.standings {
            lines.push(format!(
                "table {} {} p{} w{} d{} l{} gf{} ga{} pts{}",
                c.id,
                s.team_id,
                s.played,
                s.won,
                s.drawn,
                s.lost,
                s.goals_for,
                s.goals_against,
                s.points
            ));
        }
        let done = c
            .fixtures
            .iter()
            .filter(|f| f.status == FixtureStatus::Completed)
            .count();
        lines.push(format!(
            "fixtures {} completed {} of {}",
            c.id,
            done,
            c.fixtures.len()
        ));
    }
    for p in &g.players {
        lines.push(format!(
            "player {} {:?} ovr{} end{:?} inj{}",
            p.id,
            p.team_id,
            p.ovr,
            p.contract_end(),
            p.injury.as_ref().map_or(0, |i| i.days_remaining)
        ));
    }
    for t in &g.teams {
        lines.push(format!(
            "team {} cash{} rep{}",
            t.id, t.finance, t.reputation
        ));
    }
    lines.sort();
    lines
}

fn first_digest_difference(a: &[String], b: &[String]) -> String {
    let sa: HashSet<&String> = a.iter().collect();
    let sb: HashSet<&String> = b.iter().collect();
    let only_a: Vec<&&String> = sa.difference(&sb).take(3).collect();
    let only_b: Vec<&&String> = sb.difference(&sa).take(3).collect();
    format!(
        "{} lignes seulement dans la suite sans rechargement (ex. {:?}); {} seulement après rechargement (ex. {:?})",
        sa.difference(&sb).count(),
        only_a,
        sb.difference(&sa).count(),
        only_b
    )
}

fn run_rest_of_season(
    mut g: Game,
    options: RunOptions,
    season: u32,
    start_day: u32,
    rollover: bool,
) -> Vec<String> {
    let mut sink = Invariants::new(&g);
    let mut day = start_day;
    while !end_of_season::is_season_complete(&g) {
        day += 1;
        play_day(&mut g, options, season, day, &mut sink).expect("continues");
    }
    if rollover {
        roll_season_over(&mut g, options, season, &mut sink).expect("rolls over");
    }
    digest(&g)
}

fn run(
    game: &mut Game,
    options: RunOptions,
    seasons: u32,
    save_test: bool,
    build_s: f64,
    seed: u64,
) -> Value {
    let user_club = game.manager.team_id.clone().unwrap();
    let user_name = game
        .teams
        .iter()
        .find(|t| t.id == user_club)
        .map(|t| t.name.clone())
        .unwrap_or_default();
    let user_comp = game
        .competitions
        .iter()
        .find(|c| c.participant_ids.contains(&user_club) && c.kind == CompetitionType::League)
        .map(|c| (c.id.clone(), c.name.clone()))
        .unwrap_or_default();
    let mut invariants = Invariants::new(game);
    let mut collector = Collector::new(game);

    let start_date = game.clock.current_date.format("%Y-%m-%d").to_string();
    let initial_roster = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(&user_club))
        .count();
    let initial_expiring_june = game
        .players
        .iter()
        .filter(|p| {
            p.team_id.is_some()
                && p.contract_end()
                    .is_some_and(|e| e.as_bytes() <= b"2026-06-30")
        })
        .count();
    let initial_free = game.players.iter().filter(|p| p.team_id.is_none()).count();

    let mut day_times: Vec<f64> = Vec::new();
    let mut user_match_days: Vec<f64> = Vec::new();
    let mut season_rows: Vec<Value> = Vec::new();
    let mut save_checks: Vec<SaveCheck> = Vec::new();
    let mut save_equiv: Vec<Value> = Vec::new();
    let mut summaries: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let tmp =
        std::env::temp_dir().join(format!("apfm-season-{}-{}", std::process::id(), user_club));
    let _ = std::fs::remove_dir_all(&tmp);
    let total = Instant::now();
    let mut total_days = 0u32;

    'seasons: for season in 1..=seasons {
        let season_timer = Instant::now();
        let mut day = 0u32;
        let mut mid_done = false;
        let rows_before = collector.rows.len();
        while !end_of_season::is_season_complete(game) {
            day += 1;
            if day > 450 {
                errors.push(format!("saison {season}: non terminée en 450 jours"));
                break 'seasons;
            }
            let before_rows = collector.rows.len();
            let t = Instant::now();
            // The collector sits behind the tuple so it is told in the same order the driver tells it.
            let mut obs = (&mut invariants, &mut collector);
            if let Err(e) = play_day(game, options, season, day, &mut obs) {
                errors.push(format!("saison {season} jour {day}: {e}"));
                break 'seasons;
            }
            let dt = t.elapsed().as_secs_f64();
            day_times.push(dt);
            total_days += 1;
            if collector.rows.len() > before_rows {
                // matchday with at least one match
            }
            if game.clock.current_date.date_naive()
                >= NaiveDate::from_ymd_opt(2026 + season as i32, 1, 15).unwrap()
                && !mid_done
            {
                user_match_days.push(dt);
            }
            if save_test && season == 1 && !mid_done && day >= 200 {
                mid_done = true;
                let (reloaded, check) = save_reload(
                    game,
                    &collector.stats.clone(),
                    &tmp.join("mid"),
                    "mi-saison",
                );
                let date = game.clock.current_date.format("%Y-%m-%d").to_string();
                let fp_orig = run_rest_of_season(game.clone(), options, season, day, false);
                let fp_re = run_rest_of_season(reloaded.clone(), options, season, day, false);
                save_equiv.push(json!({
                    "point": "mi-saison", "date": date,
                    "continuation_identique_apres_rechargement": fp_orig == fp_re,
                    "premiere_difference": if fp_orig == fp_re { String::new() } else { first_digest_difference(&fp_orig, &fp_re) },
                }));
                save_checks.push(check);
                *game = reloaded; // the career carries on from the reloaded save
            }
        }
        // season is complete for the user's league
        let season_secs = season_timer.elapsed().as_secs_f64();
        let date_end = game.clock.current_date.format("%Y-%m-%d").to_string();
        let user_pos = user_summary(game, &user_club, &user_comp.0);
        season_rows.push(json!({
            "season": season, "days": day, "end_date": date_end, "seconds": season_secs,
            "matches_recorded": collector.rows.len() - rows_before, "user": user_pos,
        }));

        if save_test && season == 1 {
            let (reloaded, check) = save_reload(
                game,
                &collector.stats.clone(),
                &tmp.join("end"),
                "fin-de-saison-1",
            );
            let fp_orig = run_rest_of_season(game.clone(), options, season, day, true);
            let fp_re = run_rest_of_season(reloaded.clone(), options, season, day, true);
            save_equiv.push(json!({
                "point": "fin de saison (avant bascule)",
                "bascule_identique_apres_rechargement": fp_orig == fp_re,
                "premiere_difference": if fp_orig == fp_re { String::new() } else { first_digest_difference(&fp_orig, &fp_re) },
            }));
            save_checks.push(check);
            *game = reloaded;
        }

        let mut obs = (&mut invariants, &mut collector);
        let mut summary_holder: Option<Value> = None;
        {
            struct Grab<'a>(&'a mut Option<Value>);
            impl SeasonObserver for Grab<'_> {
                fn on_season_end(&mut self, _g: &Game, s: &EndOfSeasonSummary, _season: u32) {
                    *self.0 = Some(json!({
                        "league": s.league_name, "champion": s.champion_name, "user_position": s.user_position,
                        "user_points": s.user_points, "user_w_d_l": [s.user_won, s.user_drawn, s.user_lost],
                        "user_goals": [s.user_goals_for, s.user_goals_against], "golden_boot": [s.golden_boot_player, s.golden_boot_goals],
                        "teams": s.total_teams,
                    }));
                }
            }
            let mut grab = Grab(&mut summary_holder);
            let mut both = (&mut obs, &mut grab);
            if let Err(e) = roll_season_over(game, options, season, &mut both) {
                errors.push(format!("bascule saison {season}: {e}"));
                break 'seasons;
            }
        }
        summaries.push(summary_holder.unwrap_or(Value::Null));
        let date_new = game.clock.current_date.format("%Y-%m-%d").to_string();
        season_rows.last_mut().unwrap()["date_after_rollover"] = json!(date_new);

        if save_test && season == 1 {
            let (reloaded, check) = save_reload(
                game,
                &collector.stats.clone(),
                &tmp.join("start2"),
                "debut-saison-2",
            );
            save_checks.push(check);
            *game = reloaded;
        }
    }

    let total_secs = total.elapsed().as_secs_f64();
    let _ = std::fs::remove_dir_all(&tmp);

    // ---- sports aggregates ----
    let league_rows: Vec<&MatchRow> = collector
        .rows
        .iter()
        .filter(|r| r.comp == FixtureCompetition::League)
        .collect();
    let sport = sports(&league_rows, &collector);
    let mut by_comp: BTreeMap<String, u32> = BTreeMap::new();
    for r in &collector.rows {
        *by_comp.entry(format!("{:?}", r.comp)).or_default() += 1;
    }

    day_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| {
        day_times
            .get(((day_times.len().max(1) - 1) as f64 * p) as usize)
            .copied()
            .unwrap_or(0.0)
    };

    // promotion / relegation: compare final tables of the last completed season to memberships now
    let promotion = promotion_report(game, &collector, seasons);

    let violations = invariants.violations().len();
    let report = json!({
        "headline": {
            "club": user_name, "club_id": user_club, "competition": user_comp.1, "seasons_requested": seasons,
            "seasons_completed": summaries.len(), "days": total_days, "total_seconds": (total_secs * 100.0).round() / 100.0,
            "matches": collector.rows.len(), "invariant_violations": violations, "errors": errors,
            "date_start": start_date, "date_end": game.clock.current_date.format("%Y-%m-%d").to_string(),
        },
        "seed": seed, "build_seconds": build_s, "options": {"live_user_match": options.day_path == DayPath::UserMatchLive},
        "active_competitions": game.active_competition_ids.clone(),
        "seasons": season_rows, "end_of_season_summaries": summaries,
        "timing": {
            "days": day_times.len(), "mean_ms": day_times.iter().sum::<f64>() / day_times.len().max(1) as f64 * 1000.0,
            "p50_ms": pct(0.5) * 1000.0, "p95_ms": pct(0.95) * 1000.0, "max_ms": pct(1.0) * 1000.0,
            "week_mean_ms": day_times.iter().sum::<f64>() / day_times.len().max(1) as f64 * 7000.0,
        },
        "matches_by_competition": by_comp,
        "sport": sport,
        "jersey_clashes": {"days_with_a_clash": collector.jersey_clash_days, "first": collector.jersey_clashes},
        "injuries": {"new_injuries": collector.new_injuries, "injury_player_days": collector.injury_days, "max_simultaneous": collector.max_injured},
        "moves_by_month": collector.moves,
        "initial": {"user_roster": initial_roster, "contracts_ending_on_or_before_2026_06_30": initial_expiring_june, "free_agents": initial_free},
        "transfer_log_entries": game.competitions.iter().map(|c| c.transfer_log.len()).sum::<usize>(),
        "contracts": {"max_expired_but_attached": collector.expired_attached_max, "max_active_loans": collector.max_active_loans},
        "squads": {"min_squad_any_club_weekly": collector.min_squad, "user_squad_samples": collector.user_squad.iter().step_by(6).collect::<Vec<_>>()},
        "finances": collector.finance_rows,
        "progression": collector.progression,
        "competition_checks": collector.comp_checks,
        "competition_faults": collector.comp_faults,
        "promotion_relegation": promotion,
        "saves": save_checks.iter().map(|c| json!({
            "point": c.label, "save_seconds": c.save_secs, "load_seconds": c.load_secs, "bytes_on_disk": c.bytes,
            "difference_patterns": c.diffs.len(), "differences": c.diffs.iter().take(60).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "save_equivalence": save_equiv,
        "invariant_report": if violations == 0 { String::new() } else { invariants.report().chars().take(6000).collect::<String>() },
    });
    report
}

fn user_summary(game: &Game, user: &str, comp_id: &str) -> Value {
    let Some(c) = game.competitions.iter().find(|c| c.id == comp_id) else {
        return Value::Null;
    };
    let sorted = c.sorted_standings();
    let pos = sorted.iter().position(|s| s.team_id == user).map(|p| p + 1);
    let s = sorted.iter().find(|s| s.team_id == user);
    json!({"position": pos, "points": s.map(|s| s.points), "played": s.map(|s| s.played),
           "gd": s.map(|s| s.goals_for as i64 - s.goals_against as i64), "of": sorted.len()})
}

fn sports(rows: &[&MatchRow], col: &Collector) -> Value {
    let n = rows.len().max(1) as f64;
    let goals: f64 = rows.iter().map(|r| (r.hg + r.ag) as f64).sum();
    let hw = rows.iter().filter(|r| r.hg > r.ag).count() as f64;
    let dr = rows.iter().filter(|r| r.hg == r.ag).count() as f64;
    let aw = rows.iter().filter(|r| r.hg < r.ag).count() as f64;
    let hgoals: f64 = rows.iter().map(|r| r.hg as f64).sum();
    let shots: f64 = rows.iter().map(|r| (r.h_shots + r.a_shots) as f64).sum();
    let sot: f64 = rows.iter().map(|r| (r.h_sot + r.a_sot) as f64).sum();
    let poss: f64 = rows.iter().map(|r| r.h_poss as f64).sum();
    let yellow: f64 = rows.iter().map(|r| r.yellow as f64).sum();
    let red: f64 = rows.iter().map(|r| r.red as f64).sum();
    let fouls: f64 = rows.iter().map(|r| r.fouls as f64).sum();
    let zero = rows.iter().filter(|r| r.hg + r.ag == 0).count() as f64;
    let big = rows
        .iter()
        .filter(|r| (r.hg as i32 - r.ag as i32).abs() >= 4)
        .count() as f64;
    let mut dist: BTreeMap<u32, u32> = BTreeMap::new();
    for r in rows {
        *dist.entry((r.hg + r.ag).min(8) as u32).or_default() += 1;
    }
    // strength bins: stronger side by top-14 mean ovr
    let mut bins: BTreeMap<&str, [u32; 4]> = BTreeMap::new(); // n, stronger wins, draws, weaker wins
    let mut home_adv_equal = [0u32; 3];
    for r in rows {
        let Some(sm) = col.season_strength.get(&r.season) else {
            continue;
        };
        let (Some(hs), Some(as_)) = (sm.get(&r.home), sm.get(&r.away)) else {
            continue;
        };
        let d = hs - as_;
        let bin = match d.abs() {
            x if x < 2.0 => "écart < 2",
            x if x < 5.0 => "2 ≤ écart < 5",
            x if x < 8.0 => "5 ≤ écart < 8",
            _ => "écart ≥ 8",
        };
        let e = bins.entry(bin).or_default();
        e[0] += 1;
        let (sg, wg) = if d >= 0.0 { (r.hg, r.ag) } else { (r.ag, r.hg) };
        match sg.cmp(&wg) {
            std::cmp::Ordering::Greater => e[1] += 1,
            std::cmp::Ordering::Equal => e[2] += 1,
            std::cmp::Ordering::Less => e[3] += 1,
        }
        if d.abs() < 2.0 {
            match r.hg.cmp(&r.ag) {
                std::cmp::Ordering::Greater => home_adv_equal[0] += 1,
                std::cmp::Ordering::Equal => home_adv_equal[1] += 1,
                std::cmp::Ordering::Less => home_adv_equal[2] += 1,
            }
        }
    }
    // correlation between strength gap and goal difference
    let pts: Vec<(f64, f64)> = rows
        .iter()
        .filter_map(|r| {
            let sm = col.season_strength.get(&r.season)?;
            Some((
                sm.get(&r.home)? - sm.get(&r.away)?,
                r.hg as f64 - r.ag as f64,
            ))
        })
        .collect();
    let corr = correlation(&pts);
    let slope = if pts.len() > 2 {
        let mx = pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64;
        let my = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
        let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
        sxy / sxx.max(1e-9)
    } else {
        0.0
    };
    json!({
        "league_matches": rows.len(),
        "goals_per_match": goals / n, "home_goals_per_match": hgoals / n, "away_goals_per_match": (goals - hgoals) / n,
        "home_win_pct": hw / n * 100.0, "draw_pct": dr / n * 100.0, "away_win_pct": aw / n * 100.0,
        "shots_per_match": shots / n, "shots_on_target_per_match": sot / n, "home_possession_pct": poss / n,
        "yellow_per_match": yellow / n, "red_per_match": red / n, "fouls_per_match": fouls / n,
        "goalless_pct": zero / n * 100.0, "margin_4plus_pct": big / n * 100.0,
        "total_goals_distribution": dist,
        "by_strength_gap": bins.iter().map(|(k, v)| (k.to_string(), json!({"matches": v[0], "stronger_wins_pct": v[1] as f64 / v[0].max(1) as f64 * 100.0, "draws_pct": v[2] as f64 / v[0].max(1) as f64 * 100.0, "upsets_pct": v[3] as f64 / v[0].max(1) as f64 * 100.0}))).collect::<BTreeMap<_, _>>(),
        "equal_strength_home_draw_away": home_adv_equal,
        "strength_gap_vs_goal_diff": {"correlation": corr, "goals_per_strength_point": slope},
    })
}

fn correlation(p: &[(f64, f64)]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    let n = p.len() as f64;
    let mx = p.iter().map(|x| x.0).sum::<f64>() / n;
    let my = p.iter().map(|x| x.1).sum::<f64>() / n;
    let sxy: f64 = p.iter().map(|x| (x.0 - mx) * (x.1 - my)).sum();
    let sxx: f64 = p.iter().map(|x| (x.0 - mx).powi(2)).sum();
    let syy: f64 = p.iter().map(|x| (x.1 - my).powi(2)).sum();
    sxy / (sxx * syy).sqrt().max(1e-9)
}

/// Division moves between the first season's final tables and the second season's rosters.
fn promotion_report(game: &Game, col: &Collector, seasons: u32) -> Value {
    let mut out = Vec::new();
    for s in 1..seasons {
        let mut per_comp: Vec<(String, Vec<String>, Vec<String>)> = Vec::new(); // id, final table (s), new participants (s+1)
        for c in &game.competitions {
            let key = format!("{s}:{}", c.id);
            if let Some(table) = col.memberships.get(&key) {
                let next = col.memberships.get(&format!("{}:{}", s + 1, c.id));
                // roster at the start of s+1 is not stored; use the table of s+1 when it is complete, else the live roster.
                let roster: Vec<String> = match next {
                    Some(n) => n.clone(),
                    None => c.participant_ids.clone(),
                };
                per_comp.push((c.id.clone(), table.clone(), roster));
            }
        }
        let name = |id: &str| {
            game.teams
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.name.clone())
                .unwrap_or(id.into())
        };
        let mut moves = Vec::new();
        for (id, table, roster) in &per_comp {
            let before: HashSet<&String> = table.iter().collect();
            let after: HashSet<&String> = roster.iter().collect();
            let left: Vec<String> = table
                .iter()
                .filter(|t| !after.contains(t))
                .map(|t| name(t))
                .collect();
            let arrived: Vec<String> = roster
                .iter()
                .filter(|t| !before.contains(t))
                .map(|t| name(t))
                .collect();
            let comp_name = game
                .competitions
                .iter()
                .find(|c| &c.id == id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            moves.push(json!({"competition": comp_name, "id": id, "before": table.len(), "after": roster.len(), "left": left, "arrived": arrived,
                              "bottom_of_table": table.iter().rev().take(4).map(|t| name(t)).collect::<Vec<_>>(),
                              "top_of_table": table.iter().take(4).map(|t| name(t)).collect::<Vec<_>>()}));
        }
        out.push(json!({"from_season": s, "moves": moves}));
    }
    json!(out)
}

#[allow(dead_code)]
fn unused(_: &League) {}
