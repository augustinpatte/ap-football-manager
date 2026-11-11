//! P6: which clubs go into debt, and why. Measurement only (`#[ignore]`d).
//!
//! `P6_WORLD=fm24|gate|shipped P6_SEEDS=1 P6_SEASONS=5 [APFM_WORLD_JSON=<apfm-import output>]
//!  cargo test --release -p season-harness --test p6_finance_study -- --ignored --nocapture`
//!
//! The harness's manager makes no decisions, so the managed club's books say nothing about the AI's:
//! every figure here is of the clubs the game runs itself. For each season end the study lists the
//! clubs whose cash is below zero together with what the books say about them: whether they lose
//! money every year (operating result, which no signing rule can fix), whether the loss is a fee
//! (transfer result), and how far the wage bill sits above the board's budget.

use ofm_core::game::Game;
use season_harness::driver::{RunOptions, run_seasons};
use season_harness::env;
use season_harness::finance_study::{ClubYear, FinanceLog, report};
use season_harness::fm24::{build_fm24_career, world_path_from_env};

const FM24_USER_CLUB: &str = "fm-c-498";

fn build(world: &str, seed: u64) -> Option<Game> {
    match world {
        "fm24" => {
            let Some(path) = world_path_from_env() else {
                eprintln!("APFM_WORLD_JSON is not set: nothing to measure");
                return None;
            };
            Some(build_fm24_career(&path, FM24_USER_CLUB, seed).expect("the FM24 career builds"))
        }
        "shipped" => Some(
            season_harness::worlds::WorldSpec::shipped(seed)
                .build()
                .expect("world builds"),
        ),
        _ => Some(
            season_harness::worlds::WorldSpec::gate(seed)
                .build()
                .expect("world builds"),
        ),
    }
}

fn weekly_income(year: &ClubYear) -> f64 {
    year.income() as f64 / 52.0
}

fn debt_line(season: usize, year: &ClubYear, opening: Option<&ClubYear>) -> String {
    let opening_cash = opening.map_or(year.opening_cash, |o| o.closing_cash);
    format!(
        "  s{season} {:<10} {:<12} tier Q{} rep {:>4} | cash {:>7.1}M -> {:>7.1}M | op.result {:>6.1}M transfers {:>6.1}M | wage/budget {:>4.2} | commit*52/income {:>4.2} | squad {}",
        year.club_id,
        year.country,
        year.tier + 1,
        year.reputation,
        opening_cash as f64 / 1e6,
        year.closing_cash as f64 / 1e6,
        year.operating_result() as f64 / 1e6,
        year.transfer_result() as f64 / 1e6,
        year.weekly_commitment as f64 / year.wage_budget.max(1) as f64,
        year.weekly_commitment as f64 * 52.0 / (weekly_income(year) * 52.0).max(1.0),
        year.squad_size,
    )
}

#[test]
#[ignore = "measurement study"]
fn p6_who_goes_into_debt() {
    let world = std::env::var("P6_WORLD").unwrap_or_else(|_| "gate".to_string());
    let seasons = env::first("P6_SEASONS", "5") as u32;
    for seed in env::list("P6_SEEDS", "1") {
        let Some(mut game) = build(&world, seed) else {
            return;
        };
        let user = game.manager.team_id.clone();
        eprintln!(
            "\n== {world} world, seed {seed}: {} clubs ==",
            game.teams.len()
        );
        let mut dump = std::env::var("P6_DUMP").ok().map(|path| {
            let mut out = String::from(
                "kind\tseed\tseason\tclub\tcountry\treputation\tincome_weekly_est\twage_budget\tcommitment_weekly\tcash\ttransfer_budget\top_result\tfees_out\tseniors\n",
            );
            for team in &game.teams {
                let income = expected_annual_income(team) / 52.0;
                out.push_str(&format!(
                    "open\t{seed}\t0\t{}\t{}\t{}\t{income:.0}\t{}\t{}\t{}\t{}\t\t\t\n",
                    team.id,
                    team.country,
                    team.reputation,
                    team.wage_budget,
                    ofm_core::finances::calc_wages(&game, &team.id),
                    team.finance,
                    team.transfer_budget,
                ));
            }
            (path, out)
        });
        let mut log = FinanceLog::starting_from(&game);
        let started = std::time::Instant::now();
        run_seasons(&mut game, RunOptions::seasons(seasons), &mut log).expect("seasons play");
        eprintln!("== {seasons} seasons in {:.0?} ==", started.elapsed());
        eprintln!(
            "\nclub-seasons: season | clubs | in debt | deep (<-50M) | losing money (op.result<0) | over 110% of wage budget"
        );
        for (s, year) in log.years.iter().enumerate() {
            let clubs: Vec<&ClubYear> = year
                .iter()
                .filter(|c| Some(c.club_id.as_str()) != user.as_deref())
                .collect();
            eprintln!(
                "  s{s:>2} | {:>3} | {:>3} | {:>3} | {:>3} | {:>3}",
                clubs.len(),
                clubs.iter().filter(|c| c.closing_cash < 0).count(),
                clubs
                    .iter()
                    .filter(|c| c.closing_cash < -50_000_000)
                    .count(),
                if s == 0 {
                    0
                } else {
                    clubs.iter().filter(|c| c.operating_result() < 0).count()
                },
                clubs
                    .iter()
                    .filter(|c| c.wage_budget > 0
                        && c.weekly_commitment as f64 > c.wage_budget as f64 * 1.1)
                    .count(),
            );
        }
        eprintln!("\nclubs in debt at a season end:");
        for s in 0..log.years.len() {
            for year in log.years[s]
                .iter()
                .filter(|c| Some(c.club_id.as_str()) != user.as_deref() && c.closing_cash < 0)
            {
                let opening = (s > 0)
                    .then(|| log.years[s - 1].iter().find(|c| c.club_id == year.club_id))
                    .flatten();
                eprintln!("{}", debt_line(s, year, opening));
            }
        }
        if let Some((path, mut out)) = dump.take() {
            let end_budget: std::collections::HashMap<&str, i64> = game
                .teams
                .iter()
                .map(|t| (t.id.as_str(), t.transfer_budget))
                .collect();
            for (s, year) in log.years.iter().enumerate().skip(1) {
                for c in year {
                    out.push_str(&format!(
                        "year\t{seed}\t{s}\t{}\t{}\t{}\t{:.0}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                        c.club_id,
                        c.country,
                        c.reputation,
                        weekly_income(c),
                        c.wage_budget,
                        c.weekly_commitment,
                        c.closing_cash,
                        if s == log.years.len() - 1 {
                            end_budget.get(c.club_id.as_str()).copied().unwrap_or(0)
                        } else {
                            0
                        },
                        c.operating_result(),
                        -c.flows.get("TransferFeeOut").copied().unwrap_or(0),
                        c.squad_size,
                    ));
                }
            }
            std::fs::write(path, out).expect("the dump is written");
        }
        if std::env::var("P6_FULL").is_ok() {
            let all: Vec<usize> = (0..log.years.len()).collect();
            eprintln!("{}", report(&log, user.as_deref(), &all));
        }
    }
}

/// What a club can count on in a year: its recurring sponsorship and its estimated gate, as an AI
/// club's own signing rule reads it (`ai_finance_guard::expected_weekly_income`).
fn expected_annual_income(team: &domain::team::Team) -> f64 {
    let sponsor = team
        .sponsorship
        .as_ref()
        .map_or(0, |sponsorship| sponsorship.base_value);
    let gate = ofm_core::finances::calc_matchday(
        team.stadium_capacity,
        19,
        0.76,
        20.0 * ofm_core::club_economy::ticket_price_factor(team.reputation),
    );
    (sponsor * 52 + gate) as f64
}

/// The books of a world on day one: how much of what a club can count on its wage budget and its
/// wage bill already commit, by country. A club whose budget is above its income cannot spend its
/// budget without going into debt.
#[test]
#[ignore = "measurement study"]
fn p6_opening_books() {
    let world = std::env::var("P6_WORLD").unwrap_or_else(|_| "gate".to_string());
    let seed = env::first("P6_SEEDS", "1");
    let Some(game) = build(&world, seed) else {
        return;
    };
    let user = game.manager.team_id.clone();
    let mut by_country: std::collections::BTreeMap<String, Vec<(f64, f64, usize, i64)>> =
        std::collections::BTreeMap::new();
    for team in game.teams.iter().filter(|t| Some(&t.id) != user.as_ref()) {
        let income = expected_annual_income(team).max(1.0);
        let bill = ofm_core::finances::calc_wages(&game, &team.id) as f64 * 52.0;
        let seniors = game
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(team.id.as_str()))
            .count();
        by_country.entry(team.country.clone()).or_default().push((
            team.wage_budget as f64 * 52.0 / income,
            bill / income,
            seniors,
            team.finance,
        ));
    }
    eprintln!("\n== opening books, {world} world, seed {seed} ==");
    eprintln!(
        "country | clubs | budget/income P50 (max) | wages/income P50 (max) | over 1.0 | players P50 (max) | cash P50 M"
    );
    for (country, rows) in &by_country {
        let quantile = |values: Vec<f64>, q: f64| {
            let mut values = values;
            values.sort_by(f64::total_cmp);
            values[((values.len() - 1) as f64 * q) as usize]
        };
        let budget: Vec<f64> = rows.iter().map(|r| r.0).collect();
        let bill: Vec<f64> = rows.iter().map(|r| r.1).collect();
        let squad: Vec<f64> = rows.iter().map(|r| r.2 as f64).collect();
        let cash: Vec<f64> = rows.iter().map(|r| r.3 as f64 / 1e6).collect();
        eprintln!(
            "{country:<12} | {:>3} | {:>4.2} ({:>4.2}) | {:>4.2} ({:>4.2}) | {:>3} | {:>3.0} ({:>3.0}) | {:>6.1}",
            rows.len(),
            quantile(budget.clone(), 0.5),
            quantile(budget, 1.0),
            quantile(bill.clone(), 0.5),
            quantile(bill.clone(), 1.0),
            rows.iter().filter(|r| r.1 > 1.0).count(),
            quantile(squad.clone(), 0.5),
            quantile(squad, 1.0),
            quantile(cash, 0.5),
        );
    }
}
