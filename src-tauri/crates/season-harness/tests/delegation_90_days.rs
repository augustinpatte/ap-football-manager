//! Ninety days of full delegation, played through the real daily turn.
//!
//! The delegation scenarios in `ofm_core::staff_system` hand the assistant one situation and one day.
//! This file plays the whole chain the manager is promised when he hands the club over, on the days
//! the game itself plays (`play_day`: AI clubs bidding, windows opening, registrations, the finance
//! posting): a player nobody needs is put on the list, an offer for him is accepted, the position
//! that is now short is named and a matching player is signed, a protected player is left alone,
//! nobody is dealt twice and the squad and the books stay inside their limits.
//!
//! A run that produces no transfers proves nothing, so every scenario first requires that the
//! circumstances arose (a sale, a signing) and only then asserts what the assistant did with them.
//! The source-backed scenario skips, loudly, when `APFM_WORLD_JSON` is not set.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use domain::delegation::DelegationMode;
use domain::player::Position;
use domain::staff::StaffPost;
use ofm_core::contracts::squad_capacity;
use ofm_core::game::Game;
use ofm_core::squad_floor::MIN_SENIOR_PLAYERS;
use ofm_core::staff_system::{market, profile};
use season_harness::driver::{RunOptions, play_day};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::invariants::Invariants;
use season_harness::lifecycle::Lifecycle;
use season_harness::worlds::WorldSpec;

const DAYS: u32 = 90;
const MAX_SENIORS: usize = 30;

fn user_team(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn ensure_assistant(game: &mut Game) {
    let team = user_team(game);
    if profile::team_staff(game, &team)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager)
    {
        return;
    }
    let reputation = game
        .teams
        .iter()
        .find(|t| t.id == team)
        .map_or(0, |t| t.reputation);
    let (id, wage) = game
        .staff
        .iter()
        .filter(|s| s.team_id.is_none() && s.post() == StaffPost::AssistantManager)
        .max_by_key(|s| (s.overall(), s.id.clone()))
        .map(|s| (s.id.clone(), market::asking_wage(s, reputation)))
        .expect("a free assistant coach exists in the world");
    market::hire(game, &team, &id, wage, 3).expect("the assistant can be hired");
}

/// A club the assistant can act for: a real transfer budget and cash, so that "no money" is never
/// the reason nothing happens.
fn give_the_club_a_budget(game: &mut Game, cash: i64, transfer_budget: i64) {
    let team = user_team(game);
    for club in game.teams.iter_mut().filter(|t| t.id == team) {
        club.finance = cash;
        club.transfer_budget = transfer_budget;
    }
}

/// A back line cut to four: a position the squad review names as short. The others go to another club.
fn cut_the_back_line_to_four(game: &mut Game) {
    let team = user_team(game);
    let other = game
        .teams
        .iter()
        .find(|t| t.id != team)
        .map(|t| t.id.clone())
        .expect("another club");
    let mut defenders: Vec<(u8, String)> = game
        .players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(team.as_str())
                && p.natural_position.to_group_position() == Position::Defender
        })
        .map(|p| (p.ovr, p.id.clone()))
        .collect();
    defenders.sort_by(|a, b| b.cmp(a));
    for (_, id) in defenders.into_iter().skip(4) {
        if let Some(club) = game.teams.iter_mut().find(|t| t.id == team) {
            club.remove_player_references(&id);
        }
        if let Some(player) = game.players.iter_mut().find(|p| p.id == id) {
            player.team_id = Some(other.clone());
            player.jersey_number = None;
        }
    }
}

/// One assistant decision as the log keeps it: (date, title key, parameters).
type Decision = (String, String, Vec<(String, String)>);

/// One transfer the user's club was party to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Move {
    date: String,
    player_id: String,
    from: String,
    to: String,
    fee: u64,
}

/// What the club's books and squad looked like at the end of a day.
#[derive(Debug, Clone)]
struct DayRow {
    day: u32,
    cash: i64,
    transfer_budget: i64,
    wage_bill: i64,
    wage_budget: i64,
    seniors: usize,
}

/// Everything observed over the ninety days.
#[derive(Debug, Default)]
struct Run {
    rows: Vec<DayRow>,
    /// Each distinct assistant decision, once, with the day it was first seen.
    decisions: BTreeMap<Decision, u32>,
    moves: BTreeSet<Move>,
    /// Players seen on the transfer list of the club on any day, with the first day.
    listed_on: BTreeMap<String, u32>,
    /// Players with a pending or accepted sale offer on any day.
    offered_for_sale: BTreeSet<String>,
    /// Proposals the assistant made (a proposal is the Prudent path; Total does not make them).
    proposed_players: BTreeSet<String>,
    /// The club's cash journal rows posted during the run, by kind: (count, total).
    cash: BTreeMap<String, (usize, i64)>,
    /// The club's cash when the run started, and the journal's net of the run.
    cash_at_start: i64,
    journal_net: i64,
}

impl Run {
    fn decisions_titled(&self, title: &str) -> Vec<(&Decision, &u32)> {
        self.decisions
            .iter()
            .filter(|(k, _)| k.1 == title)
            .collect()
    }

    fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn sales(&self, user: &str) -> Vec<&Move> {
        self.moves.iter().filter(|m| m.from == user).collect()
    }

    fn signings(&self, user: &str) -> Vec<&Move> {
        self.moves.iter().filter(|m| m.to == user).collect()
    }

    /// A readable account of the run for the evidence report (`--nocapture`).
    fn describe(&self, label: &str, game: &Game, user: &str) -> String {
        let first = self.rows.first();
        let last = self.rows.last();
        let sold: u64 = self.sales(user).iter().map(|m| m.fee).sum();
        let bought: u64 = self.signings(user).iter().map(|m| m.fee).sum();
        let name = |id: &str| {
            game.players
                .iter()
                .find(|p| p.id == id)
                .map_or_else(|| id.to_string(), |p| p.match_name.clone())
        };
        let mut text = format!(
            "{label}: {} sales (income {sold}), {} signings (spent {bought}); cash {} -> {}, transfer budget {} -> {}, seniors {} -> {}\n",
            self.sales(user).len(),
            self.signings(user).len(),
            first.map_or(0, |r| r.cash),
            last.map_or(0, |r| r.cash),
            first.map_or(0, |r| r.transfer_budget),
            last.map_or(0, |r| r.transfer_budget),
            first.map_or(0, |r| r.seniors),
            last.map_or(0, |r| r.seniors),
        );
        text.push_str(&format!("    cash journal by kind: {:?}\n", self.cash));
        for m in &self.moves {
            let direction = if m.to == user { "IN " } else { "OUT" };
            text.push_str(&format!(
                "    {} {direction} {} fee {}\n",
                m.date,
                name(&m.player_id),
                m.fee
            ));
        }
        text
    }
}

fn user_moves(game: &Game, user: &str) -> BTreeSet<Move> {
    game.competitions
        .iter()
        .flat_map(|c| &c.transfer_log)
        .filter(|t| t.from_team_id == user || t.to_team_id == user)
        .map(|t| Move {
            date: t.date.clone(),
            player_id: t.player_id.clone(),
            from: t.from_team_id.clone(),
            to: t.to_team_id.clone(),
            fee: t.fee,
        })
        .collect()
}

fn every_player_has_one_club(game: &Game, day: u32) {
    let mut seen = HashSet::new();
    for player in &game.players {
        assert!(
            seen.insert(&player.id),
            "day {day}: {} appears twice",
            player.id
        );
    }
}

/// Plays `DAYS` days of the career with the daily checks and records what happened.
fn play_ninety_days(game: &mut Game, season_label: &str) -> Run {
    play_ninety_days_with(game, season_label, |_, _, _| {})
}

/// [`play_ninety_days`], with a hook run after each day's observation: a scenario that needs a
/// circumstance the world did not produce on its own (a buyer's bid) creates it there, on the day
/// the assistant has made the listing that makes it plausible, and the days that follow are the
/// game's own.
fn play_ninety_days_with(
    game: &mut Game,
    season_label: &str,
    mut after_day: impl FnMut(&mut Game, &Run, u32),
) -> Run {
    let user = user_team(game);
    let options = RunOptions::seasons(1);
    let mut invariants = Invariants::new(game);
    let mut lifecycle = Lifecycle::new(game);
    let mut run = Run {
        cash_at_start: game
            .teams
            .iter()
            .find(|t| t.id == user)
            .map_or(0, |t| t.finance),
        ..Run::default()
    };
    // The journal folds old rows away as days pass, so each day's new rows are taken by id.
    let mut seen_posts: HashSet<String> = game.cash_journal.iter().map(|p| p.id.clone()).collect();
    for day in 1..=DAYS {
        play_day(
            game,
            options,
            1,
            day,
            &mut (&mut invariants, &mut lifecycle),
        )
        .unwrap_or_else(|error| panic!("{season_label} day {day}: {error}"));
        every_player_has_one_club(game, day);
        observe(game, &user, day, &mut run);
        run.moves = user_moves(game, &user);
        for post in game.cash_journal.iter() {
            if post.club_id == user
                && post.kind != domain::finance::CashKind::OpeningBalance
                && seen_posts.insert(post.id.clone())
            {
                let entry = run.cash.entry(format!("{:?}", post.kind)).or_default();
                entry.0 += 1;
                entry.1 += post.amount;
                run.journal_net += post.amount;
            }
        }
        after_day(game, &run, day);
    }
    assert!(
        invariants.is_clean(),
        "{season_label}: {}",
        invariants.report()
    );
    assert!(
        lifecycle.is_clean(),
        "{season_label}: {}",
        lifecycle.report()
    );
    run.moves = user_moves(game, &user);
    run
}

fn observe(game: &Game, user: &str, day: u32, run: &mut Run) {
    let club = game.teams.iter().find(|t| t.id == user).expect("club");
    let wage_bill = ofm_core::finances::wage_bills_by_club(game)
        .get(user)
        .copied()
        .unwrap_or(0);
    run.rows.push(DayRow {
        day,
        cash: club.finance,
        transfer_budget: club.transfer_budget,
        wage_bill,
        wage_budget: club.wage_budget,
        seniors: squad_capacity(game, user).registered_seniors,
    });
    for entry in &game.delegation.log {
        run.decisions
            .entry((
                entry.date.clone(),
                entry.title_key.clone(),
                entry.params.clone(),
            ))
            .or_insert(day);
    }
    for player in game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(user))
    {
        if player.transfer_listed {
            run.listed_on.entry(player.id.clone()).or_insert(day);
        }
        if !player.transfer_offers.is_empty() {
            run.offered_for_sale.insert(player.id.clone());
        }
    }
    for proposal in &game.delegation.proposals {
        if let Some(id) = Run::param(&proposal.params, "player_id") {
            run.proposed_players.insert(id.to_string());
        }
    }
}

/// The promises that hold on every day of every run, whatever happened in it.
fn assert_the_club_stayed_inside_its_limits(run: &Run, user: &str) {
    let started_with = run.rows.first().map_or(0, |r| r.seniors);
    for row in &run.rows {
        assert!(row.cash >= 0, "day {}: cash is {}", row.day, row.cash);
        assert!(
            row.transfer_budget >= 0,
            "day {}: transfer budget is {}",
            row.day,
            row.transfer_budget
        );
        assert!(
            row.wage_bill <= row.wage_budget,
            "day {}: wage bill {} is over the budget {}",
            row.day,
            row.wage_bill,
            row.wage_budget
        );
        assert!(
            row.seniors <= MAX_SENIORS,
            "day {}: {} seniors registered, the limit is {MAX_SENIORS}",
            row.day,
            row.seniors
        );
        assert!(
            row.seniors >= started_with.min(MIN_SENIOR_PLAYERS),
            "day {}: the squad fell to {} seniors",
            row.day,
            row.seniors
        );
    }
    // The books reconcile: what the cash journal posted is exactly how the cash moved, and every
    // fee in the transfer log is a journal row of the right kind and size.
    let cash_now = run.rows.last().map_or(0, |r| r.cash);
    assert_eq!(
        run.cash_at_start + run.journal_net,
        cash_now,
        "the cash journal does not explain the cash: {:?}",
        run.cash
    );
    let fees = |incoming: bool| -> i64 {
        run.moves
            .iter()
            .filter(|m| (m.to == user) == incoming)
            .map(|m| i64::try_from(m.fee).unwrap_or(i64::MAX))
            .sum()
    };
    assert_eq!(
        run.cash.get("TransferFeeOut").map_or(0, |c| c.1),
        -fees(true),
        "fees paid for signings are not what the journal posted"
    );
    assert_eq!(
        run.cash.get("TransferFeeIn").map_or(0, |c| c.1),
        fees(false),
        "fees received for sales are not what the journal posted"
    );
    // Nobody is dealt twice: one sale and one signing per player, and no player both ways.
    let mut sold = HashSet::new();
    let mut signed = HashSet::new();
    for m in &run.moves {
        let set = if m.from == user {
            &mut sold
        } else {
            &mut signed
        };
        assert!(
            set.insert(m.player_id.clone()),
            "{} was dealt twice ({} -> {})",
            m.player_id,
            m.from,
            m.to
        );
    }
    assert!(
        sold.is_disjoint(&signed),
        "a player was sold and bought back inside {DAYS} days"
    );
}

/// A player listed by the assistant who then left the club: the chain "list, offer, sell" for one
/// named player. Returns the players for whom it held.
fn listed_then_sold<'a>(run: &'a Run, user: &str) -> Vec<&'a str> {
    let listing_titles = [
        "delegation.log.listedSurplus",
        "delegation.log.listedFunds",
        "delegation.log.listedLeaving",
    ];
    let sold: HashSet<&str> = run
        .sales(user)
        .iter()
        .map(|m| m.player_id.as_str())
        .collect();
    listing_titles
        .iter()
        .flat_map(|title| run.decisions_titled(title))
        .filter_map(|((_, _, params), _)| Run::param(params, "player_id"))
        .filter(|id| sold.contains(id))
        .collect()
}

/// A signing the assistant announced (`recruitScheduled`: position and fee) that arrived: the move
/// in the transfer log has the announced fee, and the player is of the announced position group and
/// is on the club's books now. Returns the positions for which it held.
fn announced_and_signed(run: &Run, game: &Game, user: &str) -> Vec<String> {
    let mut positions = Vec::new();
    for ((_, _, params), _) in run.decisions_titled("delegation.log.recruitScheduled") {
        let (Some(position), Some(fee)) =
            (Run::param(params, "position"), Run::param(params, "fee"))
        else {
            continue;
        };
        let Ok(fee) = fee.parse::<u64>() else {
            continue;
        };
        let arrived = run.signings(user).into_iter().any(|m| {
            m.fee == fee
                && game.players.iter().any(|p| {
                    p.id == m.player_id
                        && format!("{:?}", p.natural_position.to_group_position()) == position
                        && p.team_id.as_deref() == Some(user)
                })
        });
        if arrived {
            positions.push(position.to_string());
        }
    }
    positions
}

fn full_delegation_world(seed: u64) -> Game {
    let mut game = WorldSpec::gate(seed).build().expect("the world builds");
    ensure_assistant(&mut game);
    give_the_club_a_budget(&mut game, 40_000_000, 30_000_000);
    game.delegation.apply_mode(DelegationMode::Full);
    game
}

/// Given the generated career, a real assistant manager, a transfer budget and an open market,
///       with every duty handed over (Total)
/// When ninety days are played through the real daily turn on three seeds
/// Then on every seed the assistant lists a player nobody needs and the club sells him, names a
///      position and signs a player of it who arrives, makes each deal once, and the squad and the
///      books stay inside their limits throughout.
#[test]
fn ninety_days_of_total_delegation_list_sell_recruit_and_stay_inside_the_limits() {
    let mut seeds_with_a_sale = 0;
    for seed in [1_u64, 2, 3] {
        let mut game = full_delegation_world(seed);
        let user = user_team(&game);
        let run = play_ninety_days(&mut game, &format!("seed {seed}"));
        println!("{}", run.describe(&format!("seed {seed}"), &game, &user));
        // Whether a club bids for a listed player within ninety days is the market's doing (seed 2
        // lists a player and nobody bids); the controlled scenarios below cover the sale itself.
        if !listed_then_sold(&run, &user).is_empty() {
            seeds_with_a_sale += 1;
        }
        assert!(
            !run.listed_on.is_empty() || !run.sales(&user).is_empty(),
            "seed {seed}: the assistant neither listed nor sold anyone in ninety days"
        );
        let signed = announced_and_signed(&run, &game, &user);
        assert!(
            !signed.is_empty(),
            "seed {seed}: no signing the assistant announced arrived, so the recruitment chain was not exercised"
        );
        assert_the_club_stayed_inside_its_limits(&run, &user);
        // Total sells and signs by itself: it never leaves a proposal waiting for the manager.
        assert!(
            run.proposed_players.is_empty(),
            "seed {seed}: Total left proposals for the manager: {:?}",
            run.proposed_players
        );
    }
    assert!(
        seeds_with_a_sale >= 2,
        "only {seeds_with_a_sale} of 3 careers listed a player and sold him"
    );
}

/// Given a club whose back line was cut to four (a position the squad review names as short) and
///       Total delegation with a transfer budget
/// When ninety days are played
/// Then the assistant names a defensive position, signs a defender who arrives, and does not go on
///      signing defenders beyond what the squad needed.
#[test]
fn a_short_position_is_identified_and_filled_with_a_matching_player() {
    let mut game = full_delegation_world(1);
    cut_the_back_line_to_four(&mut game);
    let user = user_team(&game);
    let defenders_before = game
        .players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(user.as_str())
                && p.natural_position.to_group_position() == Position::Defender
        })
        .count();
    assert_eq!(defenders_before, 4);

    let run = play_ninety_days(&mut game, "short back line");
    println!("{}", run.describe("short back line", &game, &user));

    let defenders_signed: Vec<&Move> = run
        .signings(&user)
        .into_iter()
        .filter(|m| {
            game.players.iter().any(|p| {
                p.id == m.player_id
                    && p.natural_position.to_group_position() == Position::Defender
                    && p.team_id.as_deref() == Some(user.as_str())
            })
        })
        .collect();
    assert!(
        !defenders_signed.is_empty(),
        "a back line of four was never filled"
    );
    assert!(
        announced_and_signed(&run, &game, &user).contains(&"Defender".to_string()),
        "the assistant never announced a defender he then signed"
    );
    // Four needs a fifth and a sixth at most; a dozen would be a shopping spree.
    assert!(
        defenders_signed.len() <= 4,
        "{} defenders signed for a back line that was four short of nothing",
        defenders_signed.len()
    );
    assert_the_club_stayed_inside_its_limits(&run, &user);
}

/// Given a control run in which the assistant lists and sells a player,
///       and the same career again with that player protected by the manager
/// When ninety days are played in each
/// Then the protected player is never listed, never offered for, never sold and still at the club,
///      while the assistant carries on with the rest of the squad.
#[test]
fn a_protected_player_stays_while_the_assistant_works_on_everyone_else() {
    let control_game = full_delegation_world(1);
    let user = user_team(&control_game);
    let mut control_game = control_game;
    let control = play_ninety_days(&mut control_game, "control");
    let candidate = listed_then_sold(&control, &user)
        .first()
        .map(|id| (*id).to_string())
        .expect("the control run sold a player the assistant had listed");

    let mut game = full_delegation_world(1);
    game.delegation.set_protected(&candidate, true);
    let run = play_ninety_days(&mut game, "protected");
    println!("{}", run.describe("protected", &game, &user));

    assert!(
        !run.listed_on.contains_key(&candidate),
        "the protected player was put on the transfer list on day {:?}",
        run.listed_on.get(&candidate)
    );
    assert!(
        !run.offered_for_sale.contains(&candidate) || {
            // An offer may arrive (clubs bid for anyone); what matters is that it was not accepted.
            run.sales(&user).iter().all(|m| m.player_id != candidate)
        },
        "an offer for the protected player was accepted"
    );
    assert!(run.sales(&user).iter().all(|m| m.player_id != candidate));
    assert!(!run.proposed_players.contains(&candidate));
    let player = game
        .players
        .iter()
        .find(|p| p.id == candidate)
        .expect("the player exists");
    assert_eq!(player.team_id.as_deref(), Some(user.as_str()));
    assert!(
        !run.sales(&user).is_empty() || !run.signings(&user).is_empty(),
        "the assistant did nothing at all, so protection was not tested against an active assistant"
    );
    assert_the_club_stayed_inside_its_limits(&run, &user);
}

/// Given the source-backed world (the player's own FM24 file) with Paris Saint-Germain's manager
///       handing over everything, a real assistant and a transfer budget
/// When ninety days are played
/// Then the same chain holds in the representative world, and the limits hold throughout.
#[test]
fn ninety_days_of_total_delegation_in_the_source_backed_world() {
    let Some(path) = world_path_from_env() else {
        println!("SKIPPED: APFM_WORLD_JSON is not set, so the source-backed world was not played");
        return;
    };
    let mut game = build_fm24_career(&path, "fm-c-501", 7).expect("the career builds");
    ensure_assistant(&mut game);
    give_the_club_a_budget(&mut game, 400_000_000, 150_000_000);
    game.delegation.apply_mode(DelegationMode::Full);
    let user = user_team(&game);

    let run = play_ninety_days(&mut game, "fm24");
    println!("{}", run.describe("fm24 PSG", &game, &user));

    assert!(
        !run.sales(&user).is_empty() || !run.signings(&user).is_empty(),
        "no transfer of any kind in ninety days: the source-backed run proved nothing"
    );
    assert_the_club_stayed_inside_its_limits(&run, &user);
}

/// The first player the assistant has put on the list who is still at the club and has no offer yet.
fn newly_listed_player(game: &Game, run: &Run, user: &str) -> Option<String> {
    let titles = [
        "delegation.log.listedSurplus",
        "delegation.log.listedFunds",
        "delegation.log.listedLeaving",
    ];
    titles
        .iter()
        .flat_map(|title| run.decisions_titled(title))
        .filter_map(|((_, _, params), _)| Run::param(params, "player_id"))
        .find(|id| {
            game.players.iter().any(|p| {
                p.id == *id
                    && p.team_id.as_deref() == Some(user)
                    && p.transfer_listed
                    && p.transfer_offers.is_empty()
            })
        })
        .map(str::to_string)
}

/// A bid from the richest other club for `player_id`, as the AI's own bids arrive.
fn bid_for(
    game: &mut Game,
    user: &str,
    player_id: &str,
    id: &str,
    fee_per_cent_of_value: u64,
) -> u64 {
    let buyer = game
        .teams
        .iter()
        .filter(|t| t.id != user)
        .max_by_key(|t| (t.finance, t.id.clone()))
        .map(|t| t.id.clone())
        .expect("another club");
    let today = game.clock.current_date.date_naive();
    let buyer_cash = game
        .teams
        .iter()
        .find(|t| t.id == buyer)
        .map_or(0, |t| t.finance);
    // Cash goes in through the journal, as a real credit would, so the books still reconcile.
    ofm_core::finances::credit_opening_cash(game, &buyer, (500_000_000 - buyer_cash).max(0), today)
        .expect("the buyer can be credited");
    if let Some(club) = game.teams.iter_mut().find(|t| t.id == buyer) {
        club.transfer_budget = club.transfer_budget.max(200_000_000);
    }
    let date = game.clock.current_date.format("%Y-%m-%d").to_string();
    let player = game
        .players
        .iter_mut()
        .find(|p| p.id == player_id)
        .expect("the listed player");
    let fee = player.market_value * fee_per_cent_of_value / 100;
    player.transfer_offers.push(domain::player::TransferOffer {
        id: id.to_string(),
        from_team_id: buyer,
        fee,
        wage_offered: player.wage() / 2,
        last_manager_fee: None,
        negotiation_round: 1,
        suggested_counter_fee: None,
        status: domain::player::TransferOfferStatus::Pending,
        date,
        registration_date: None,
        registration_failure_reason: None,
        closed_on: None,
        terms: Default::default(),
    });
    fee
}

fn offer_status(
    game: &Game,
    player_id: &str,
    offer_id: &str,
) -> Option<domain::player::TransferOfferStatus> {
    game.players
        .iter()
        .find(|p| p.id == player_id)?
        .transfer_offers
        .iter()
        .find(|o| o.id == offer_id)
        .map(|o| o.status.clone())
}

/// Given a world in which the assistant lists a player and no club bids for him in ninety days (seed
///       2, observed), and Total delegation
/// When a buyer bids well above the player's value the day after the listing, then a second listed
///      player gets a derisory bid, and the days are played on
/// Then the generous bid is accepted by the assistant (the player leaves for that fee, once) and the
///      derisory one is declined with the player still at the club; both are in the history.
#[test]
fn a_generous_bid_for_a_listed_player_is_accepted_and_a_derisory_one_is_declined() {
    let mut game = full_delegation_world(2);
    let user = user_team(&game);
    let mut bids: Vec<(String, String, u64)> = Vec::new();
    let run = play_ninety_days_with(&mut game, "controlled sale", |game, run, _day| {
        if bids.len() >= 2 {
            return;
        }
        let taken: Vec<&str> = bids.iter().map(|(player, _, _)| player.as_str()).collect();
        let Some(player_id) =
            newly_listed_player(game, run, &user).filter(|id| !taken.contains(&id.as_str()))
        else {
            return;
        };
        // First a generous bid (130 % of his value), then a derisory one (30 %).
        let (offer_id, per_cent) = if bids.is_empty() {
            ("bid-generous", 130)
        } else {
            ("bid-derisory", 30)
        };
        let fee = bid_for(game, &user, &player_id, offer_id, per_cent);
        bids.push((player_id, offer_id.to_string(), fee));
    });
    println!("{}", run.describe("controlled sale", &game, &user));

    assert_eq!(
        bids.len(),
        2,
        "the assistant listed fewer than two players: {bids:?}"
    );
    let (sold_id, sold_offer, sold_fee) = &bids[0];
    let (kept_id, kept_offer, _) = &bids[1];
    assert_eq!(
        offer_status(&game, sold_id, sold_offer).map(|s| matches!(
            s,
            domain::player::TransferOfferStatus::Accepted
                | domain::player::TransferOfferStatus::PendingRegistration
        )),
        Some(true),
        "the generous bid was not accepted"
    );
    let departures: Vec<&Move> = run
        .sales(&user)
        .into_iter()
        .filter(|m| &m.player_id == sold_id)
        .collect();
    assert_eq!(departures.len(), 1, "the player must leave exactly once");
    assert_eq!(departures[0].fee, *sold_fee);
    assert!(
        run.decisions_titled("delegation.log.offerAcceptedSurplus")
            .len()
            + run
                .decisions_titled("delegation.log.offerAcceptedFunds")
                .len()
            + run
                .decisions_titled("delegation.log.offerAcceptedFair")
                .len()
            >= 1,
        "the acceptance is not in the assistant's history"
    );
    assert_eq!(
        offer_status(&game, kept_id, kept_offer),
        Some(domain::player::TransferOfferStatus::Rejected),
        "the derisory bid was not declined"
    );
    // He may leave later, on another club's better offer; he did not leave on this one.
    let derisory_fee = bids[1].2;
    assert!(
        run.sales(&user)
            .iter()
            .all(|m| &m.player_id != kept_id || m.fee != derisory_fee),
        "the player was sold for the derisory fee"
    );
    assert!(
        !run.decisions_titled("delegation.log.offerDeclined")
            .is_empty()
    );
    assert_the_club_stayed_inside_its_limits(&run, &user);
}

/// Given a player the assistant would otherwise list and sell, protected by the manager, and a buyer
///       who bids a fortune for him in the middle of the run
/// When the days are played
/// Then the bid is declined as a protected player's, and he is still at the club.
#[test]
fn a_fortune_for_a_protected_player_is_declined() {
    let mut game = full_delegation_world(1);
    let user = user_team(&game);
    let star = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(user.as_str()))
        .max_by_key(|p| (p.market_value, p.id.clone()))
        .map(|p| p.id.clone())
        .expect("a squad");
    game.delegation.set_protected(&star, true);
    let mut bid_fee = 0;
    let run = play_ninety_days_with(&mut game, "protected bid", |game, _run, day| {
        if day == 5 {
            bid_fee = bid_for(game, &user, &star, "bid-fortune", 400);
        }
    });

    assert!(bid_fee > 0);
    assert_eq!(
        offer_status(&game, &star, "bid-fortune"),
        Some(domain::player::TransferOfferStatus::Rejected)
    );
    assert!(
        !run.decisions_titled("delegation.log.offerDeclinedProtected")
            .is_empty(),
        "the refusal is not in the history"
    );
    assert!(run.sales(&user).iter().all(|m| m.player_id != star));
    let player = game.players.iter().find(|p| p.id == star).unwrap();
    assert_eq!(player.team_id.as_deref(), Some(user.as_str()));
    assert!(!run.listed_on.contains_key(&star));
}
