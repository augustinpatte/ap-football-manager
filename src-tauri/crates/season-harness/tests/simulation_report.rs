//! The simulation report, checked against a run of the real game: ninety days of Total
//! delegation through `fast_forward::run` (the daily turn, AI clubs, windows, finance posting),
//! then every figure of the report compared with a count made another way from the same game.
//!
//! The report reads the game's records; these tests do not trust it. Each figure is recomputed
//! here from the competitions' transfer logs, the cash journal's post ids and the delegation log,
//! and a run that produced nothing to report fails instead of passing on empty lists.

use std::collections::{BTreeSet, HashSet};

use chrono::{Duration, NaiveDate};
use domain::delegation::DelegationMode;
use domain::finance::CashKind;
use domain::player::PlayerMovementKind;
use domain::staff::StaffPost;
use ofm_core::end_of_season;
use ofm_core::fast_forward::{
    self, FastForwardRequest, FastForwardSummary, InterruptPolicy, PendingDecision, StopReason,
};
use ofm_core::game::Game;
use ofm_core::simulation_report::{
    ClubReport, ReportEvent, ReportTracker, SquadMove, StretchEnd, TransferKind, build_report,
};
use ofm_core::staff_system::{market, profile};
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use season_harness::worlds::WorldSpec;

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

/// A generated career with a real assistant, a budget and every duty handed over.
fn full_delegation_world(seed: u64) -> Game {
    let mut game = WorldSpec::gate(seed).build().expect("the world builds");
    ensure_assistant(&mut game);
    let team = user_team(&game);
    for club in game.teams.iter_mut().filter(|t| t.id == team) {
        club.finance = 40_000_000;
        club.transfer_budget = 30_000_000;
    }
    game.delegation.apply_mode(DelegationMode::Full);
    game
}

fn no_host(_: &Game) -> Vec<PendingDecision> {
    Vec::new()
}

fn run_days(game: &mut Game, days: i64, policy: InterruptPolicy) -> FastForwardSummary {
    let target = game.clock.current_date.date_naive() + Duration::days(days);
    let request = FastForwardRequest {
        target_date: target.format("%Y-%m-%d").to_string(),
        policy,
    };
    fast_forward::run(game, &request, &mut no_host, &mut |_| {}).expect("the request is valid")
}

fn the_club(summary: &FastForwardSummary) -> &ClubReport {
    assert_eq!(summary.report.clubs.len(), 1, "one club was managed");
    &summary.report.clubs[0]
}

fn day_text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// The club's posts of the run, taken by id against the journal as it stood before.
fn posts_since(game: &Game, club: &str, known: &HashSet<String>) -> Vec<(CashKind, i64)> {
    game.cash_journal
        .iter()
        .filter(|post| post.club_id == club && !known.contains(&post.id))
        .filter(|post| post.kind != CashKind::OpeningBalance)
        .map(|post| (post.kind, post.amount))
        .collect()
}

/// One line of what a report holds, for the evidence the audit quotes (`--nocapture`).
fn describe(label: &str, club: &ClubReport) -> String {
    format!(
        "{label}: {} days, {} matches ({}W {}D {}L, {}-{}), transfers {} bought ({}) {} sold ({}) \
         {} loans in {} loans out {} free, {} contract changes, squad {} -> {} ({} in, {} out), {} injuries, \
         delegation {} executed {} declined {} left, cash {} -> {} ({} posts, {} unexplained)",
        club.days_played,
        club.matches.played,
        club.matches.won,
        club.matches.drawn,
        club.matches.lost,
        club.matches.goals_for,
        club.matches.goals_against,
        club.transfers.purchases,
        club.transfers.fees_paid,
        club.transfers.sales,
        club.transfers.fees_received,
        club.transfers.loans_in,
        club.transfers.loans_out,
        club.transfers.free_signings,
        club.contracts.changes.total,
        club.squad.size_before,
        club.squad.size_after,
        club.squad.arrivals.total,
        club.squad.departures.total,
        club.squad.injuries.total,
        club.delegation.executed,
        club.delegation.declined,
        club.delegation.left_to_manager,
        club.finances.opening_cash,
        club.finances.closing_cash,
        club.finances.posts,
        club.finances.unexplained,
    )
}

/// What the report costs on this world, for the audit: the read at the end of a run (fixtures,
/// every player's ledger, the journal) and the watch the run keeps each day. Printed, not asserted.
fn report_cost(game: &Game) -> String {
    let tracker = ReportTracker::begin(game);
    let started = std::time::Instant::now();
    let _ = build_report(game, tracker.clone());
    let read = started.elapsed();
    let mut watch = tracker;
    let started = std::time::Instant::now();
    for _ in 0..100 {
        watch.after_day(game);
    }
    format!(
        "report cost on {} players: read at the end {:?}, watch per day {:?}",
        game.players.len(),
        read,
        started.elapsed() / 100
    )
}

/// A run of ninety days with what was true before it, to compare the report with afterwards.
struct Observed {
    label: String,
    summary: FastForwardSummary,
    game: Game,
    before_ids: HashSet<String>,
    before_squad: BTreeSet<String>,
    before_cash: i64,
}

fn ninety_days(label: &str, mut game: Game) -> Observed {
    let user = user_team(&game);
    let before_ids = game.cash_journal.iter().map(|p| p.id.clone()).collect();
    let before_squad = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(user.as_str()))
        .map(|p| p.id.clone())
        .collect();
    let before_cash = game.teams.iter().find(|t| t.id == user).unwrap().finance;
    let summary = run_days(&mut game, 90, InterruptPolicy::ToTargetDate);
    println!("{}", describe(label, the_club(&summary)));
    Observed {
        label: label.to_string(),
        summary,
        game,
        before_ids,
        before_squad,
        before_cash,
    }
}

fn gate_run(seed: u64) -> Observed {
    ninety_days(&format!("seed {seed}"), full_delegation_world(seed))
}

/// The report's days, matches and transfer totals against the run's own counts and the
/// competitions' transfer logs. Returns how many logged transfers the club was party to.
fn check_matches_and_transfers(run: &Observed) -> usize {
    let club = the_club(&run.summary);
    let user = user_team(&run.game);
    let label = &run.label;

    assert_eq!(run.summary.stop, StopReason::TargetReached, "{label}");
    assert_eq!(club.days_played, 90, "{label}");
    assert_eq!(
        club.until,
        day_text(run.game.clock.current_date.date_naive())
    );
    assert_eq!(
        club.matches.played, run.summary.progress.user_matches_played,
        "{label}: the report's matches are the matches the run played"
    );
    assert!(
        club.matches.played >= 3,
        "{label}: too few matches to prove anything"
    );
    assert_eq!(
        club.matches.won + club.matches.drawn + club.matches.lost,
        club.matches.played
    );

    let logged: Vec<_> = run
        .game
        .competitions
        .iter()
        .flat_map(|c| &c.transfer_log)
        .filter(|t| t.date.as_str() >= club.from.as_str() && t.date.as_str() < club.until.as_str())
        .filter(|t| t.from_team_id == user || t.to_team_id == user)
        .collect();
    let bought: u64 = logged
        .iter()
        .filter(|t| t.to_team_id == user)
        .map(|t| t.fee)
        .sum();
    let sold: u64 = logged
        .iter()
        .filter(|t| t.from_team_id == user)
        .map(|t| t.fee)
        .sum();
    assert_eq!(
        (
            club.transfers.purchases as usize,
            club.transfers.sales as usize
        ),
        (
            logged.iter().filter(|t| t.to_team_id == user).count(),
            logged.iter().filter(|t| t.from_team_id == user).count()
        ),
        "{label}: the report's purchases and sales are the transfer log's"
    );
    assert_eq!(
        (club.transfers.fees_paid, club.transfers.fees_received),
        (bought, sold),
        "{label}"
    );
    for line in &club.transfers.moves.items {
        if let Some(paid) = line.fee_in_journal {
            assert!(
                paid,
                "{label}: the fee of {} is in no journal post",
                line.player_id
            );
        }
        assert!(
            run.game.players.iter().any(|p| p.id == line.player_id),
            "{label}: {} is not a player of the game",
            line.player_id
        );
    }
    logged.len()
}

/// Opening plus the journal's posts is closing, and the totals by kind and the transfer fees are
/// the journal's own, counted here by post id against the journal as it stood before.
fn check_finances(run: &Observed) {
    let club = the_club(&run.summary);
    let user = user_team(&run.game);
    let label = &run.label;
    let finances = &club.finances;
    let posts = posts_since(&run.game, &user, &run.before_ids);

    assert_eq!(finances.opening_cash, run.before_cash, "{label}");
    let net: i64 = posts.iter().map(|(_, amount)| amount).sum();
    assert_eq!(
        finances.closing_cash - finances.opening_cash,
        net,
        "{label}: the journal explains the cash"
    );
    assert_eq!(finances.unexplained, 0, "{label}");
    assert!(finances.detail_complete);
    assert_eq!(finances.posts as usize, posts.len(), "{label}");
    for total in &finances.by_kind {
        let of_kind: i64 = posts
            .iter()
            .filter(|(k, _)| *k == total.kind)
            .map(|(_, a)| a)
            .sum();
        assert_eq!(total.net, of_kind, "{label}: {:?}", total.kind);
    }
    assert_eq!(
        finances.transfer_fees_out,
        -(club.transfers.fees_paid as i64),
        "{label}"
    );
    assert_eq!(
        finances.transfer_fees_in, club.transfers.fees_received as i64,
        "{label}"
    );
    assert!(finances.posts > 0, "{label}: no payment in ninety days");
}

/// The squad is the difference of the squad before and after, with the ledger's cause on every
/// sale; the delegation counts are the delegation log's by outcome. Returns the log's size.
fn check_squad_and_delegation(run: &Observed) -> usize {
    let club = the_club(&run.summary);
    let user = user_team(&run.game);
    let label = &run.label;
    let after: BTreeSet<String> = run
        .game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(user.as_str()))
        .map(|p| p.id.clone())
        .collect();

    assert_eq!(club.squad.size_before as usize, run.before_squad.len());
    assert_eq!(club.squad.size_after as usize, after.len());
    let arrived: BTreeSet<_> = after.difference(&run.before_squad).cloned().collect();
    let left: BTreeSet<_> = run.before_squad.difference(&after).cloned().collect();
    let reported = |moves: &[SquadMove]| -> BTreeSet<String> {
        moves.iter().map(|m| m.player_id.clone()).collect()
    };
    assert_eq!(reported(&club.squad.arrivals.items), arrived, "{label}");
    assert_eq!(reported(&club.squad.departures.items), left, "{label}");
    for line in club
        .transfers
        .moves
        .items
        .iter()
        .filter(|l| l.kind == TransferKind::Sale && left.contains(&l.player_id))
    {
        let departure = club
            .squad
            .departures
            .items
            .iter()
            .find(|m| m.player_id == line.player_id)
            .unwrap();
        assert_eq!(
            departure.cause,
            Some(PlayerMovementKind::PermanentTransfer),
            "{label}"
        );
        assert_eq!(departure.fee, line.fee);
    }

    let in_run = |date: &str| date >= club.from.as_str() && date < club.until.as_str();
    let log: Vec<_> = run
        .game
        .delegation
        .log
        .iter()
        .filter(|e| in_run(&e.date))
        .collect();
    let counted = club.delegation.executed
        + club.delegation.approved
        + club.delegation.declined
        + club.delegation.left_to_manager;
    assert_eq!(counted as usize, log.len(), "{label}");
    assert_eq!(club.delegation.entries.total as usize, log.len());
    log.len()
}

/// Given ninety days of Total delegation on three seeds
/// When the run ends
/// Then the report covers the days played, its matches are the ones the run counted, and each
/// transfer total equals the same total taken from the competitions' transfer logs
#[test]
fn matches_and_transfers_agree_with_the_games_own_logs_over_ninety_days() {
    let transfers_seen: usize = [1_u64, 2, 3]
        .into_iter()
        .map(|seed| check_matches_and_transfers(&gate_run(seed)))
        .sum();
    assert!(
        transfers_seen > 0,
        "no transfer happened on any seed: the report was not exercised"
    );
}

/// Given the same ninety days
/// When the finances are read
/// Then opening plus the journal's posts is closing to the cent, the by-kind totals are the
/// journal's own, and the fees the report shows are the journal's transfer payments
#[test]
fn the_financial_summary_reconciles_with_the_journal() {
    for seed in [1_u64, 2, 3] {
        check_finances(&gate_run(seed));
    }
}

/// Given the same ninety days
/// When the squad and delegation sections are read
/// Then the squad is the difference of the squad before and after with the ledger's cause on
/// every transfer, and the delegation counts are the delegation log's, by outcome
#[test]
fn squad_and_delegation_are_the_games_own_differences_and_log() {
    let delegated: usize = [1_u64, 2, 3]
        .into_iter()
        .map(|seed| check_squad_and_delegation(&gate_run(seed)))
        .sum();
    assert!(
        delegated > 0,
        "Total delegation logged nothing in ninety days on any seed"
    );
}

/// Given the source-backed world (the player's own FM24 file) with Paris Saint-Germain's manager
/// handing over everything, a real assistant and a transfer budget
/// When ninety days are played
/// Then the same figures agree with the same logs in the representative world, where loans and
/// renewals exist; without `APFM_WORLD_JSON` it says so and plays nothing
#[test]
fn the_report_agrees_with_the_logs_in_the_source_backed_world() {
    let Some(path) = world_path_from_env() else {
        println!("SKIPPED: APFM_WORLD_JSON is not set, so the source-backed world was not played");
        return;
    };
    let mut game = build_fm24_career(&path, "fm-c-501", 7).expect("the career builds");
    ensure_assistant(&mut game);
    let team = user_team(&game);
    for club in game.teams.iter_mut().filter(|t| t.id == team) {
        club.finance = 400_000_000;
        club.transfer_budget = 150_000_000;
    }
    game.delegation.apply_mode(DelegationMode::Full);

    let run = ninety_days("fm24 PSG", game);

    let transfers = check_matches_and_transfers(&run);
    check_finances(&run);
    let delegated = check_squad_and_delegation(&run);
    println!("{}", report_cost(&run.game));
    assert!(
        transfers > 0 && delegated > 0,
        "nothing to report: {transfers} transfers, {delegated} delegation entries"
    );
}

/// Given a run asked for ninety days that a stopping policy ends early (here by a decision the
/// manager must make, the first thing to stop it)
/// When the report is read
/// Then it covers the days played, not the ninety asked for, and its books still reconcile
#[test]
fn a_run_that_stops_early_reports_the_days_it_played() {
    let mut game = full_delegation_world(1);
    let user = user_team(&game);
    let known: HashSet<String> = game.cash_journal.iter().map(|p| p.id.clone()).collect();
    let summary = run_days(&mut game, 90, InterruptPolicy::StopAtNextMatch);

    assert_ne!(
        summary.stop,
        StopReason::TargetReached,
        "it must have stopped early"
    );
    let club = the_club(&summary);
    assert!(club.days_played < 90, "the run stopped early");
    assert_eq!(club.days_played, summary.progress.days_done);
    assert_eq!(club.until, summary.progress.current_date);
    assert_eq!(club.finances.unexplained, 0);
    let net: i64 = posts_since(&game, &user, &known)
        .iter()
        .map(|(_, a)| a)
        .sum();
    assert_eq!(club.finances.change, net);
}

/// Given a two-day interval in which nothing is bought, sold or played
/// When the report is read
/// Then it is a report of nothing: empty lists, zero totals and books that still reconcile
#[test]
fn a_quiet_interval_is_a_report_of_nothing() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    let user = user_team(&game);
    let known: HashSet<String> = game.cash_journal.iter().map(|p| p.id.clone()).collect();
    let summary = run_days(&mut game, 2, InterruptPolicy::ToTargetDate);

    let club = the_club(&summary);
    assert_eq!(club.days_played, 2);
    assert_eq!(club.matches.played, 0);
    assert_eq!(club.transfers.moves.total, 0);
    assert_eq!(club.contracts.changes.total, 0);
    assert_eq!(club.delegation.entries.total, 0);
    assert_eq!(club.finances.unexplained, 0);
    let net: i64 = posts_since(&game, &user, &known)
        .iter()
        .map(|(_, a)| a)
        .sum();
    assert_eq!(club.finances.change, net);
}

/// How many days the generated career needs until its season is complete (probed on a copy).
fn days_until_season_complete(game: &Game) -> i64 {
    let mut probe = game.clone();
    let mut days = 0;
    while !end_of_season::is_season_complete(&probe) {
        ofm_core::turn::process_day(&mut probe);
        days += 1;
        assert!(days < 600, "the season never completes");
    }
    days
}

/// Given a run long enough to cross the end of the club's season, with the target-date policy
/// When `fast_forward::run` plays it
/// Then the rollover the run performed is an event of the report (with the season's result), the
/// days after it are still the same stretch, and the books still reconcile across the rollover
#[test]
fn a_run_across_the_end_of_the_season_reports_the_rollover() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    let user = user_team(&game);
    let known: HashSet<String> = game.cash_journal.iter().map(|p| p.id.clone()).collect();
    let days = days_until_season_complete(&game) + 5;

    let summary = run_days(&mut game, days, InterruptPolicy::ToTargetDate);

    assert!(
        summary.progress.seasons_rolled_over >= 1,
        "the run crossed a season"
    );
    let club = the_club(&summary);
    let rollovers: Vec<_> = club
        .events
        .iter()
        .filter(|e| matches!(e, ReportEvent::SeasonCompleted { .. }))
        .collect();
    assert_eq!(rollovers.len() as u32, summary.progress.seasons_rolled_over);
    assert_eq!(club.days_played as i64, summary.progress.days_done as i64);
    assert_eq!(club.ended, StretchEnd::Continues);
    assert_eq!(club.finances.unexplained, 0);
    let net: i64 = posts_since(&game, &user, &known)
        .iter()
        .map(|(_, a)| a)
        .sum();
    assert_eq!(club.finances.change, net);
}

/// Given a manager the board has already warned twice and whose satisfaction is at the floor
/// When `fast_forward::run` plays the days
/// Then the run stops as `ManagerFired`, the stretch ends `Dismissed`, the dismissal is an event, and
/// nothing after the dismissal is attributed to the club
#[test]
fn a_dismissal_during_the_run_ends_the_stretch_and_is_an_event() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    game.manager.satisfaction = 0;
    game.manager.warning_stage = 2;
    let days = 30;

    let summary = run_days(&mut game, days, InterruptPolicy::ToTargetDate);

    assert_eq!(summary.stop, StopReason::ManagerFired);
    let club = the_club(&summary);
    assert_eq!(club.ended, StretchEnd::Dismissed);
    assert!(
        club.events
            .iter()
            .any(|e| matches!(e, ReportEvent::Dismissed { .. }))
    );
    assert_eq!(club.until, summary.progress.current_date);
    assert!(club.days_played > 0 && (club.days_played as i64) < days);
}
