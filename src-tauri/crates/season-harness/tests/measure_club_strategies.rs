//! Run F3: do AI clubs buy and borrow according to what they are?
//!
//! The FM24 world is played for a season with every league active. Each club's strategy (the
//! profile the market reads) is taken at the opening; every permanent transfer and loan of the
//! season is then attributed to its buyer and grouped by strategy. The properties asserted are the
//! ones the profiles promise: clubs short of money buy fewer players and pay less for them,
//! youth-built clubs sign younger players, small clubs borrow more than big ones, and no profile
//! lets a squad collapse. The "contenders" row is printed but not asserted: a season does not
//! show contenders signing differently from the rest (documented in docs/APFM_RUN_F3_AUTONOMY.md).
//!
//! The FM24 world is the player's own file (`APFM_WORLD_JSON`), never in the repository: the test
//! returns at once when it is not set. Run with `-- --nocapture` to read the table.

use std::collections::{BTreeMap, HashMap};

use domain::player::{PlayerMovementKind, SquadRole};
use ofm_core::game::Game;
use ofm_core::transfers::{ClubStrategyReport, strategy_reports};
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::fm24::{build_fm24_career, world_path_from_env};

const LENS: &str = "fm-c-501";

#[derive(Default, Clone)]
struct Acquisitions {
    buys: u32,
    loans: u32,
    fee_sum: f64,
    value_sum: f64,
    age_sum: f64,
    young_buys: u32,
    seller_reputation_sum: f64,
}

impl Acquisitions {
    fn add(&mut self, other: &Acquisitions) {
        self.buys += other.buys;
        self.loans += other.loans;
        self.fee_sum += other.fee_sum;
        self.value_sum += other.value_sum;
        self.age_sum += other.age_sum;
        self.young_buys += other.young_buys;
        self.seller_reputation_sum += other.seller_reputation_sum;
    }

    fn mean_fee(&self) -> f64 {
        self.fee_sum / f64::from(self.buys.max(1))
    }

    fn mean_age(&self) -> f64 {
        self.age_sum / f64::from(self.buys.max(1))
    }

    fn young_share(&self) -> f64 {
        f64::from(self.young_buys) / f64::from(self.buys.max(1))
    }
}

#[derive(Default)]
struct Watch {
    seen: HashMap<String, usize>,
    by_club: BTreeMap<String, Acquisitions>,
    reputation: HashMap<String, u32>,
}

impl SeasonObserver for Watch {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        let today = game.clock.current_date.date_naive();
        let manager_club = game.manager.team_id.clone().unwrap_or_default();
        for player in &game.players {
            let before = self.seen.get(&player.id).copied().unwrap_or(0);
            let history = &player.movement_history;
            for entry in history.iter().skip(before) {
                let Some(to) = entry.to_team_id.as_deref() else {
                    continue;
                };
                if to == manager_club
                    || entry.from_team_id.as_deref() == Some(manager_club.as_str())
                {
                    continue;
                }
                let acquisitions = self.by_club.entry(to.to_string()).or_default();
                match entry.kind {
                    PlayerMovementKind::PermanentTransfer => {
                        acquisitions.buys += 1;
                        acquisitions.fee_sum += entry.fee.unwrap_or(0) as f64;
                        acquisitions.value_sum += player.market_value as f64;
                        let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
                        acquisitions.age_sum += f64::from(age);
                        if age <= 22 {
                            acquisitions.young_buys += 1;
                        }
                        let seller = entry.from_team_id.as_deref().unwrap_or_default();
                        acquisitions.seller_reputation_sum +=
                            f64::from(self.reputation.get(seller).copied().unwrap_or(0));
                    }
                    PlayerMovementKind::LoanStart => acquisitions.loans += 1,
                    _ => {}
                }
            }
            self.seen.insert(player.id.clone(), history.len());
        }
    }
}

/// The clubs matching `keep`, and what they acquired together.
fn group(
    reports: &[ClubStrategyReport],
    by_club: &BTreeMap<String, Acquisitions>,
    keep: impl Fn(&ClubStrategyReport) -> bool,
) -> (usize, Acquisitions) {
    let mut total = Acquisitions::default();
    let mut clubs = 0;
    for report in reports.iter().filter(|r| keep(r)) {
        clubs += 1;
        if let Some(acquired) = by_club.get(&report.team_id) {
            total.add(acquired);
        }
    }
    (clubs, total)
}

fn per_club(count: u32, clubs: usize) -> f64 {
    f64::from(count) / clubs.max(1) as f64
}

fn show(name: &str, clubs: usize, a: &Acquisitions) {
    println!(
        "{name:<20} {clubs:>6} {:>9.2} {:>10.2} {:>10.2} {:>9.1} {:>10.2} {:>11.0}",
        per_club(a.buys, clubs),
        per_club(a.loans, clubs),
        a.fee_sum / a.value_sum.max(1.0),
        a.mean_age(),
        a.young_share(),
        a.seller_reputation_sum / f64::from(a.buys.max(1)),
    );
}

#[test]
fn clubs_buy_and_borrow_according_to_their_strategy() {
    let Some(path) = world_path_from_env() else {
        return;
    };
    let mut game = build_fm24_career(&path, LENS, 7).expect("the career builds");
    game.active_region_ids.clear();
    game.active_competition_ids.clear();
    let reports = strategy_reports(&game);
    let reputation: HashMap<String, u32> = game
        .teams
        .iter()
        .map(|t| (t.id.clone(), t.reputation))
        .collect();
    let mut watch = Watch {
        reputation: reputation.clone(),
        seen: game
            .players
            .iter()
            .map(|p| (p.id.clone(), p.movement_history.len()))
            .collect(),
        ..Watch::default()
    };
    run_seasons(&mut game, RunOptions::seasons(1), &mut watch).expect("a season plays");

    println!(
        "\nstrategy group        clubs  buys/club loans/club  fee/value  mean age  <=22 share  seller rep"
    );
    let by_means = |means: &'static str| group(&reports, &watch.by_club, |r| r.means == means);
    let (n_wealthy, wealthy) = by_means("wealthy");
    let (n_comfortable, comfortable) = by_means("comfortable");
    let (n_constrained, constrained) = by_means("constrained");
    show("wealthy", n_wealthy, &wealthy);
    show("comfortable", n_comfortable, &comfortable);
    show("constrained", n_constrained, &constrained);

    let (_, contenders) = group(&reports, &watch.by_club, |r| r.ambition == "contender");
    let (n_others, others) = group(&reports, &watch.by_club, |r| r.ambition != "contender");
    show("contenders", reports.len() - n_others, &contenders);
    show("everyone else", n_others, &others);

    let mut youth: Vec<f64> = reports.iter().map(|r| r.youth_focus).collect();
    youth.sort_by(f64::total_cmp);
    let (low, high) = (youth[youth.len() / 3], youth[youth.len() * 2 / 3]);
    let (n_low, least_youthful) = group(&reports, &watch.by_club, |r| r.youth_focus <= low);
    let (n_high, most_youthful) = group(&reports, &watch.by_club, |r| r.youth_focus >= high);
    show("least youth-built", n_low, &least_youthful);
    show("most youth-built", n_high, &most_youthful);

    let mut reputations: Vec<u32> = reputation.values().copied().collect();
    reputations.sort_unstable();
    let (small_line, big_line) = (
        reputations[reputations.len() / 3],
        reputations[reputations.len() * 2 / 3],
    );
    let (n_small, small) = group(&reports, &watch.by_club, |r| {
        reputation[&r.team_id] <= small_line
    });
    let (n_big, big) = group(&reports, &watch.by_club, |r| {
        reputation[&r.team_id] >= big_line
    });
    show("smallest third (rep)", n_small, &small);
    show("biggest third (rep)", n_big, &big);

    assert!(
        n_wealthy > 0 && n_constrained > 0,
        "the world has both wealthy and constrained clubs"
    );
    assert!(
        constrained.mean_fee() < wealthy.mean_fee(),
        "constrained clubs pay less per signing than wealthy ones"
    );
    assert!(
        per_club(constrained.buys, n_constrained) < per_club(comfortable.buys, n_comfortable),
        "clubs short of money buy fewer players than comfortable ones"
    );
    assert!(
        most_youthful.young_share() > least_youthful.young_share() + 0.10,
        "youth-built clubs sign clearly more young players"
    );
    assert!(
        most_youthful.mean_age() < least_youthful.mean_age(),
        "and their signings are younger on average"
    );
    assert!(
        per_club(small.loans, n_small) > per_club(big.loans, n_big),
        "small clubs borrow more than big ones"
    );

    let manager_club = game.manager.team_id.clone().unwrap_or_default();
    let below_the_floor = game
        .teams
        .iter()
        .filter(|t| t.id != manager_club)
        .filter(|t| {
            game.players
                .iter()
                .filter(|p| {
                    p.team_id.as_deref() == Some(t.id.as_str()) && p.squad_role == SquadRole::Senior
                })
                .count()
                < 15
        })
        .count();
    assert_eq!(
        below_the_floor, 0,
        "no strategy leaves a club below the squad floor"
    );
}
