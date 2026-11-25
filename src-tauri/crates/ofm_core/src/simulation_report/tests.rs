//! Scenario tests for the report, on small hand-built games. The end-to-end proof, with the real
//! daily turn and a hired assistant, is `season-harness/tests/simulation_report.rs`.

use chrono::{NaiveDate, TimeZone, Utc};
use domain::contract_ledger::{ContractRecord, ContractSource, ReleaseReason};
use domain::delegation::{DecisionOutcome, DelegationLogEntry, Duty};
use domain::finance::CashKind;
use domain::league::{Fixture, FixtureCompetition, FixtureStatus, League, MatchResult};
use domain::manager::Manager;
use domain::message::{InboxMessage, MessageCategory};
use domain::player::{
    Injury, Player, PlayerAttributes, PlayerMovementEntry, PlayerMovementKind, Position,
};
use domain::team::Team;

use super::*;
use crate::clock::GameClock;
use crate::finances::journal;
use crate::game::Game;

const HOME: &str = "reds";
const AWAY: &str = "blues";

fn attrs() -> PlayerAttributes {
    PlayerAttributes {
        pace: 60,
        stamina: 60,
        strength: 60,
        agility: 60,
        passing: 60,
        shooting: 60,
        tackling: 60,
        dribbling: 60,
        defending: 60,
        positioning: 60,
        vision: 60,
        decisions: 60,
        composure: 60,
        aggression: 60,
        teamwork: 60,
        leadership: 60,
        handling: 30,
        reflexes: 30,
        aerial: 60,
    }
}

fn player(id: &str, club: Option<&str>) -> Player {
    let mut player = Player::new(
        id.into(),
        format!("Player {id}"),
        format!("Full {id}"),
        "1999-01-01".into(),
        "GB".into(),
        Position::Midfielder,
        attrs(),
    );
    player.team_id = club.map(str::to_string);
    player
}

fn team(id: &str, name: &str) -> Team {
    Team::new(
        id.into(),
        name.into(),
        id[..3].to_uppercase(),
        "England".into(),
        "Town".into(),
        "Ground".into(),
        10_000,
    )
}

/// Two clubs, the manager at `reds`, the clock on 2026-08-01, 1,000,000 in the bank.
fn game() -> Game {
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
    let mut manager = Manager::new(
        "mgr".into(),
        "Test".into(),
        "Manager".into(),
        "1980-01-01".into(),
        "England".into(),
    );
    manager.hire(HOME.into());
    let mut reds = team(HOME, "Reds FC");
    reds.finance = 1_000_000;
    let blues = team(AWAY, "Blues FC");
    Game::new(clock, manager, vec![reds, blues], vec![], vec![], vec![])
}

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

/// Plays `days` quiet days: the clock moves and the tracker is told, as the run does.
fn pass_days(game: &mut Game, tracker: &mut ReportTracker, days: u32) {
    for _ in 0..days {
        game.clock.advance_days(1);
        tracker.after_day(game);
    }
}

fn the_club(report: &SimulationReport) -> &ClubReport {
    assert_eq!(report.clubs.len(), 1, "{report:?}");
    &report.clubs[0]
}

fn fixture(id: &str, date: &str, home: &str, away: &str, goals: (u8, u8)) -> Fixture {
    Fixture {
        id: id.into(),
        competition_id: "league".into(),
        date: date.into(),
        home_team_id: home.into(),
        away_team_id: away.into(),
        competition: FixtureCompetition::League,
        status: FixtureStatus::Completed,
        result: Some(MatchResult {
            home_goals: goals.0,
            away_goals: goals.1,
            home_scorers: vec![],
            away_scorers: vec![],
            report: None,
            home_penalties: None,
            away_penalties: None,
        }),
        ..Default::default()
    }
}

fn league_with(fixtures: Vec<Fixture>) -> League {
    let mut league = League::new(
        "league".into(),
        "Test League".into(),
        2026,
        &[HOME.to_string(), AWAY.to_string()],
    );
    league.fixtures = fixtures;
    league
}

fn movement(
    date: &str,
    kind: PlayerMovementKind,
    from: Option<(&str, &str)>,
    to: Option<(&str, &str)>,
) -> PlayerMovementEntry {
    PlayerMovementEntry {
        from_team_id: from.map(|(id, _)| id.to_string()),
        from_team_name: from.map(|(_, name)| name.to_string()),
        to_team_id: to.map(|(id, _)| id.to_string()),
        to_team_name: to.map(|(_, name)| name.to_string()),
        ..PlayerMovementEntry::new(date, kind)
    }
}

fn record(player: &mut Player, entry: PlayerMovementEntry) {
    player.record_movement(entry).unwrap();
}

const REDS: (&str, &str) = (HOME, "Reds FC");
const BLUES: (&str, &str) = (AWAY, "Blues FC");

/// Given a run that was asked for ten days but stopped after three
/// When the report is read
/// Then it covers the three days played, from the first to the first day not played
#[test]
fn a_run_that_stops_early_reports_the_days_it_played() {
    let mut game = game();
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 3);

    let report = build_report(&game, tracker);

    let club = the_club(&report);
    assert_eq!(
        (club.from.as_str(), club.until.as_str()),
        ("2026-08-01", "2026-08-04")
    );
    assert_eq!(club.days_played, 3);
    assert_eq!(club.ended, StretchEnd::Continues);
    assert_eq!(club.club_id, HOME);
}

/// Given a manager with no club
/// When a run is reported
/// Then there is nothing to attribute: the report has no club
#[test]
fn a_manager_without_a_club_has_an_empty_report() {
    let mut game = game();
    game.manager.team_id = None;
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 5);

    assert!(build_report(&game, tracker).clubs.is_empty());
}

/// Given matches inside and outside the run, home and away, one a knockout the club lost
/// When the report is read
/// Then only the club's finished matches inside the run are counted, from the club's side
#[test]
fn matches_are_the_clubs_finished_fixtures_inside_the_days_played() {
    let mut game = game();
    let mut league = league_with(vec![
        fixture("before", "2026-07-30", HOME, AWAY, (9, 0)),
        fixture("won", "2026-08-02", HOME, AWAY, (3, 1)),
        fixture("lost", "2026-08-05", AWAY, HOME, (2, 0)),
        fixture("drawn", "2026-08-07", HOME, AWAY, (1, 1)),
        fixture("after", "2026-08-12", HOME, AWAY, (4, 0)),
        fixture("elsewhere", "2026-08-03", "x", "y", (1, 0)),
        // The first day not played (the run covers 08-01 to 08-10 inclusive): not the run's.
        fixture("first-unplayed-day", "2026-08-11", HOME, AWAY, (7, 0)),
        // The first day played: the run's (it is played during the run, not before it).
        fixture("first-day", "2026-08-01", AWAY, HOME, (0, 1)),
    ]);
    league.fixtures[1].competition = FixtureCompetition::Cup;
    let first_day = league.fixtures.len() - 1;
    let played = league.fixtures[first_day].clone();
    league.fixtures[first_day].status = FixtureStatus::Scheduled;
    league.fixtures[first_day].result = None;
    game.competitions.push(league);
    let mut tracker = ReportTracker::begin(&game);
    game.competitions[0].fixtures[first_day] = played;
    pass_days(&mut game, &mut tracker, 10);

    let report = build_report(&game, tracker);

    let matches = &the_club(&report).matches;
    assert_eq!(
        (matches.played, matches.won, matches.drawn, matches.lost),
        (4, 2, 1, 1)
    );
    assert_eq!((matches.goals_for, matches.goals_against), (5, 4));
    let ids: Vec<_> = matches
        .fixtures
        .items
        .iter()
        .map(|m| m.fixture_id.as_str())
        .collect();
    assert_eq!(ids, ["first-day", "won", "lost", "drawn"]);
    let lost = &matches.fixtures.items[2];
    assert!(!lost.at_home);
    assert_eq!((lost.goals_for, lost.goals_against), (0, 2));
    assert_eq!(lost.opponent_name, "Blues FC");
}

/// Given a knockout tie the club lost on penalties after a draw
/// When the report is read
/// Then the match is a draw by its goals, the shootout is shown beside it and the club is out
#[test]
fn a_shootout_defeat_is_a_draw_with_a_penalty_score_and_an_elimination() {
    let mut game = game();
    let mut tie = fixture("tie", "2026-08-02", HOME, AWAY, (1, 1));
    tie.competition = FixtureCompetition::Cup;
    if let Some(result) = tie.result.as_mut() {
        result.home_penalties = Some(3);
        result.away_penalties = Some(4);
    }
    let mut cup = league_with(vec![tie]);
    cup.id = "cup".into();
    cup.name = "Test Cup".into();
    cup.knockout_rounds = vec![domain::league::KnockoutRoundState {
        id: "final".into(),
        name: "Final".into(),
        fixture_ids: vec!["tie".into()],
        ..Default::default()
    }];
    game.competitions.push(cup);
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 3);

    let report = build_report(&game, tracker);

    let club = the_club(&report);
    let line = &club.matches.fixtures.items[0];
    assert_eq!(line.outcome, MatchOutcome::Draw);
    assert_eq!(
        (line.penalties_for, line.penalties_against),
        (Some(3), Some(4))
    );
    assert!(club.events.iter().any(|e| matches!(
        e,
        ReportEvent::KnockoutEliminated { fixture_id, .. } if fixture_id == "tie"
    )));
}

/// Given a purchase, a sale, a loan, a free signing, and a purchase whose fee never reached the journal
/// When the report is read
/// Then each is a transfer line with its fee and counterparty, and the fee carries whether the
/// journal holds the payment
#[test]
fn transfers_carry_fee_counterparty_and_whether_the_journal_paid_them() {
    let mut game = game();
    let mut bought = player("bought", Some(HOME));
    record(
        &mut bought,
        movement(
            "2026-08-03",
            PlayerMovementKind::PermanentTransfer,
            Some(BLUES),
            Some(REDS),
        ),
    );
    let mut sold = player("sold", Some(AWAY));
    record(
        &mut sold,
        movement(
            "2026-08-04",
            PlayerMovementKind::PermanentTransfer,
            Some(REDS),
            Some(BLUES),
        ),
    );
    let mut loaned = player("loaned", Some(HOME));
    record(
        &mut loaned,
        PlayerMovementEntry {
            loan_end_date: Some("2027-06-30".into()),
            ..movement(
                "2026-08-05",
                PlayerMovementKind::LoanStart,
                Some(BLUES),
                Some(REDS),
            )
        },
    );
    let mut free = player("free", Some(HOME));
    record(
        &mut free,
        movement(
            "2026-08-06",
            PlayerMovementKind::FreeAgentSigning,
            None,
            Some(REDS),
        ),
    );
    let mut unpaid = player("unpaid", Some(HOME));
    record(
        &mut unpaid,
        movement(
            "2026-08-07",
            PlayerMovementKind::PermanentTransfer,
            Some(BLUES),
            Some(REDS),
        ),
    );
    game.players = vec![bought, sold, loaned, free, unpaid];
    // The fees are written into the ledgers by rebuilding the entries with their fee.
    set_fee(&mut game, "bought", 2_000_000);
    set_fee(&mut game, "sold", 500_000);
    set_fee(&mut game, "unpaid", 300_000);
    let mut tracker = ReportTracker::begin(&game);
    for (date, amount, kind) in [
        ("2026-08-03", -2_000_000, CashKind::TransferFeeOut),
        ("2026-08-04", 500_000, CashKind::TransferFeeIn),
    ] {
        journal::post(&mut game, HOME, amount, kind, day(date)).unwrap();
    }
    pass_days(&mut game, &mut tracker, 10);

    let report = build_report(&game, tracker);

    let transfers = &the_club(&report).transfers;
    assert_eq!(
        (
            transfers.purchases,
            transfers.sales,
            transfers.loans_in,
            transfers.free_signings
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(
        (transfers.fees_paid, transfers.fees_received),
        (2_300_000, 500_000)
    );
    let line = |id: &str| {
        transfers
            .moves
            .items
            .iter()
            .find(|m| m.player_id == id)
            .unwrap()
    };
    assert_eq!(
        line("bought").counterparty_name.as_deref(),
        Some("Blues FC")
    );
    assert_eq!(line("bought").fee_in_journal, Some(true));
    assert_eq!(line("sold").kind, TransferKind::Sale);
    assert_eq!(line("sold").fee_in_journal, Some(true));
    assert_eq!(
        line("unpaid").fee_in_journal,
        Some(false),
        "a fee with no payment is shown as such"
    );
    assert_eq!(line("free").fee, None);
    assert_eq!(line("free").fee_in_journal, None);
    assert_eq!(line("loaned").loan_end_date.as_deref(), Some("2027-06-30"));
}

/// Writes the fee into the last movement of a player (the ledger has no setter: it is rebuilt).
fn set_fee(game: &mut Game, id: &str, fee: u64) {
    let player = game.players.iter_mut().find(|p| p.id == id).unwrap();
    let mut entries: Vec<PlayerMovementEntry> = player.movement_history.iter().cloned().collect();
    if let Some(last) = entries.last_mut() {
        last.fee = Some(fee);
    }
    player.movement_history =
        domain::contract_ledger::MovementLedger::restore(entries, None, None, 0);
}

/// Given a renewal on better terms and a contract that ran out
/// When the report is read
/// Then the renewal shows the wage and end date before and after, and the expiry its reason
#[test]
fn contract_changes_show_the_terms_before_and_after() {
    let mut game = game();
    let mut renewed = player("renewed", Some(HOME));
    record(
        &mut renewed,
        PlayerMovementEntry {
            contract: Some(ContractRecord {
                start: Some("2024-07-01".into()),
                end: Some("2026-12-31".into()),
                weekly_wage: 10_000,
                source: ContractSource::Transfer,
            }),
            ..movement(
                "2024-07-01",
                PlayerMovementKind::PermanentTransfer,
                Some(BLUES),
                Some(REDS),
            )
        },
    );
    record(
        &mut renewed,
        PlayerMovementEntry {
            contract: Some(ContractRecord {
                start: Some("2026-08-03".into()),
                end: Some("2029-06-30".into()),
                weekly_wage: 14_000,
                source: ContractSource::Renewal,
            }),
            ..movement(
                "2026-08-03",
                PlayerMovementKind::Renewal,
                Some(REDS),
                Some(REDS),
            )
        },
    );
    let mut expired = player("expired", None);
    record(
        &mut expired,
        PlayerMovementEntry {
            contract: Some(ContractRecord {
                start: Some("2023-07-01".into()),
                end: Some("2026-08-04".into()),
                weekly_wage: 8_000,
                source: ContractSource::Transfer,
            }),
            ..movement(
                "2023-07-01",
                PlayerMovementKind::FreeAgentSigning,
                None,
                Some(REDS),
            )
        },
    );
    record(
        &mut expired,
        PlayerMovementEntry {
            release_reason: Some(ReleaseReason::Expired),
            ..movement("2026-08-04", PlayerMovementKind::Released, Some(REDS), None)
        },
    );
    game.players = vec![renewed, expired];
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 5);

    let report = build_report(&game, tracker);

    let contracts = &the_club(&report).contracts;
    assert_eq!(
        (contracts.renewed, contracts.expired, contracts.terminated),
        (1, 1, 0)
    );
    let renewal = &contracts.changes.items[0];
    assert_eq!(renewal.kind, ContractChangeKind::Renewal);
    assert_eq!(
        (renewal.weekly_wage_before, renewal.weekly_wage_after),
        (Some(10_000), Some(14_000))
    );
    assert_eq!(renewal.end_before.as_deref(), Some("2026-12-31"));
    assert_eq!(renewal.end_after.as_deref(), Some("2029-06-30"));
    let expiry = &contracts.changes.items[1];
    assert_eq!(expiry.kind, ContractChangeKind::Expired);
    assert_eq!(
        (expiry.weekly_wage_before, expiry.weekly_wage_after),
        (Some(8_000), None)
    );
}

/// Given payments in the journal over the run, one before it and one dated on the first day not played
/// When the report is read
/// Then the totals by kind are the journal's own posts of the days played, they explain the change in
/// cash exactly except for the one payment dated on a day that was not played, which is left out of
/// the totals and shows as unexplained (the first day played counts, the first day not played does not)
#[test]
fn finances_reconcile_with_the_journal() {
    let mut game = game();
    journal::post(
        &mut game,
        HOME,
        -50_000,
        CashKind::PlayerWages,
        day("2026-07-25"),
    )
    .unwrap();
    let opening = game.teams[0].finance;
    let mut tracker = ReportTracker::begin(&game);
    journal::post(
        &mut game,
        HOME,
        -120_000,
        CashKind::PlayerWages,
        day("2026-08-02"),
    )
    .unwrap();
    journal::post(
        &mut game,
        HOME,
        80_000,
        CashKind::Matchday,
        day("2026-08-03"),
    )
    .unwrap();
    journal::post(
        &mut game,
        HOME,
        -2_000_000,
        CashKind::TransferFeeOut,
        day("2026-08-04"),
    )
    .unwrap();
    journal::post(
        &mut game,
        HOME,
        300_000,
        CashKind::TransferFeeIn,
        day("2026-08-05"),
    )
    .unwrap();
    // On the first day played a post is the run's; one dated on the first day not played is not.
    journal::post(&mut game, HOME, 11_000, CashKind::Upkeep, day("2026-08-01")).unwrap();
    journal::post(&mut game, HOME, 999, CashKind::Other, day("2026-08-08")).unwrap();
    pass_days(&mut game, &mut tracker, 7);

    let report = build_report(&game, tracker);

    let finances = &the_club(&report).finances;
    assert_eq!(finances.opening_cash, opening);
    assert_eq!(
        finances.closing_cash,
        opening - 120_000 + 80_000 - 2_000_000 + 300_000 + 11_000 + 999
    );
    assert_eq!(
        finances.change,
        finances.closing_cash - finances.opening_cash
    );
    assert_eq!(
        finances.unexplained, 999,
        "only the payment dated on a day not played is left out of the journal's explanation"
    );
    assert!(finances.detail_complete);
    assert_eq!(
        finances.posts, 5,
        "the post on the first day not played is not counted"
    );
    assert_eq!(
        (finances.transfer_fees_out, finances.transfer_fees_in),
        (-2_000_000, 300_000)
    );
    let wages = finances
        .by_kind
        .iter()
        .find(|k| k.kind == CashKind::PlayerWages)
        .unwrap();
    assert_eq!(
        (wages.income, wages.expense, wages.net),
        (0, -120_000, -120_000)
    );
    assert_eq!(
        finances.by_kind[0].kind,
        CashKind::TransferFeeOut,
        "largest movement first"
    );
}

/// Given cash that changed with no journal post
/// When the report is read
/// Then the difference is reported as unexplained, not absorbed into a category
#[test]
fn money_the_journal_does_not_explain_is_reported_as_unexplained() {
    let mut game = game();
    let mut tracker = ReportTracker::begin(&game);
    journal::post(
        &mut game,
        HOME,
        10_000,
        CashKind::Matchday,
        day("2026-08-02"),
    )
    .unwrap();
    game.teams[0].finance += 777;
    pass_days(&mut game, &mut tracker, 3);

    let finances = the_club(&build_report(&game, tracker)).finances.clone();

    assert_eq!(finances.unexplained, 777);
    assert_eq!(finances.change, 10_777);
}

/// Given the squad before and after the run
/// When the report is read
/// Then arrivals and departures are the difference of the two, each with the cause the ledger
/// records, and none where the ledger records nothing
#[test]
fn squad_changes_are_the_difference_with_the_ledgers_cause() {
    let mut game = game();
    let stays = player("stays", Some(HOME));
    let mut leaves = player("leaves", Some(HOME));
    let mut joins = player("joins", None);
    let appears = player("appears", None);
    record(
        &mut joins,
        movement(
            "2026-08-03",
            PlayerMovementKind::FreeAgentSigning,
            None,
            Some(REDS),
        ),
    );
    record(
        &mut leaves,
        movement(
            "2026-08-04",
            PlayerMovementKind::PermanentTransfer,
            Some(REDS),
            Some(BLUES),
        ),
    );
    game.players = vec![stays, leaves, joins, appears];
    let mut tracker = ReportTracker::begin(&game);
    game.players[1].team_id = Some(AWAY.into());
    game.players[2].team_id = Some(HOME.into());
    game.players[3].team_id = Some(HOME.into());
    pass_days(&mut game, &mut tracker, 5);

    let squad = the_club(&build_report(&game, tracker)).squad.clone();

    assert_eq!((squad.size_before, squad.size_after), (2, 3));
    let arrival = |id: &str| {
        squad
            .arrivals
            .items
            .iter()
            .find(|m| m.player_id == id)
            .unwrap()
    };
    assert_eq!(
        arrival("joins").cause,
        Some(PlayerMovementKind::FreeAgentSigning)
    );
    assert_eq!(
        arrival("appears").cause,
        None,
        "no ledger entry, no invented cause"
    );
    assert_eq!(squad.departures.items.len(), 1);
    assert_eq!(
        squad.departures.items[0].cause,
        Some(PlayerMovementKind::PermanentTransfer)
    );
    assert_eq!(
        squad.departures.items[0].counterparty_name.as_deref(),
        Some("Blues FC")
    );
}

/// Given players injured inside the run, one of whom recovers and one of whom is sold while hurt
/// When the report is read
/// Then each injury is listed with when it was first seen and whether he recovered at the club
#[test]
fn injuries_are_the_ones_the_run_saw_begin_and_end() {
    let mut game = game();
    game.players = vec![
        player("hurt", Some(HOME)),
        player("healed", Some(HOME)),
        player("sold", Some(HOME)),
        player("already", Some(HOME)),
    ];
    game.players[3].injury = Some(Injury {
        name: "Old knock".into(),
        days_remaining: 9,
    });
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 1);
    for id in ["hurt", "healed", "sold"] {
        let p = game.players.iter_mut().find(|p| p.id == id).unwrap();
        p.injury = Some(Injury {
            name: format!("{id} injury"),
            days_remaining: 12,
        });
    }
    pass_days(&mut game, &mut tracker, 1);
    game.players
        .iter_mut()
        .find(|p| p.id == "healed")
        .unwrap()
        .injury = None;
    let seller = game.players.iter_mut().find(|p| p.id == "sold").unwrap();
    seller.team_id = Some(AWAY.into());
    pass_days(&mut game, &mut tracker, 2);

    let injuries = the_club(&build_report(&game, tracker))
        .squad
        .injuries
        .clone();

    let line = |id: &str| injuries.items.iter().find(|i| i.player_id == id).unwrap();
    assert_eq!(injuries.total, 4);
    assert_eq!(
        line("already").first_seen,
        "2026-08-02",
        "seen at the end of the first day"
    );
    assert_eq!(line("hurt").first_seen, "2026-08-03");
    assert_eq!(line("hurt").recovered_by, None);
    assert_eq!(line("healed").recovered_by.as_deref(), Some("2026-08-04"));
    assert_eq!(
        line("sold").recovered_by,
        None,
        "he did not recover at the club"
    );
    assert!(!line("sold").still_at_club);
}

/// Given delegation entries of every outcome, inside and outside the run
/// When the report is read
/// Then each outcome is counted, only the run's entries are listed and none is invented
#[test]
fn delegation_counts_each_outcome_inside_the_run() {
    let mut game = game();
    let entry = |date: &str, outcome| DelegationLogEntry {
        date: date.into(),
        duty: Duty::Scouting,
        title_key: "delegation.log.test".into(),
        params: vec![("player".into(), "p1".into())],
        outcome,
    };
    game.delegation.log = vec![
        entry("2026-07-30", DecisionOutcome::Executed),
        entry("2026-08-02", DecisionOutcome::Executed),
        entry("2026-08-03", DecisionOutcome::Declined),
        entry("2026-08-04", DecisionOutcome::LeftToManager),
        entry("2026-08-20", DecisionOutcome::Executed),
    ];
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 6);

    let delegation = the_club(&build_report(&game, tracker)).delegation.clone();

    assert_eq!(
        (
            delegation.executed,
            delegation.declined,
            delegation.left_to_manager,
            delegation.approved
        ),
        (1, 1, 1, 0)
    );
    assert_eq!(delegation.entries.items.len(), 3);
    assert_eq!(
        delegation.entries.items[0].params,
        vec![("player".to_string(), "p1".to_string())]
    );
}

/// Given a manager dismissed on the third day and a club he was not managing afterwards
/// When the report is read
/// Then the stretch ends on the dismissal day, the dismissal is an event, and what happened to
/// the club afterwards is not attributed to the manager
#[test]
fn a_dismissal_closes_the_stretch_and_later_days_are_not_attributed() {
    let mut game = game();
    game.competitions.push(league_with(vec![
        fixture("in", "2026-08-03", HOME, AWAY, (1, 0)),
        fixture("out", "2026-08-06", HOME, AWAY, (5, 0)),
    ]));
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 2);
    game.clock.advance_days(1);
    game.manager.team_id = None;
    tracker.after_day(&game);
    pass_days(&mut game, &mut tracker, 4);

    let report = build_report(&game, tracker);

    let club = the_club(&report);
    assert_eq!(club.ended, StretchEnd::Dismissed);
    assert_eq!(
        (club.from.as_str(), club.until.as_str()),
        ("2026-08-01", "2026-08-04")
    );
    assert_eq!(club.days_played, 3);
    assert_eq!(club.matches.played, 1, "the 5-0 came after the dismissal");
    assert!(
        club.events
            .iter()
            .any(|e| matches!(e, ReportEvent::Dismissed { date } if date == "2026-08-03"))
    );
}

/// Given a manager who moves to another club on the fourth day
/// When the report is read
/// Then there are two stretches, each with its own days and its own matches
#[test]
fn a_change_of_club_gives_each_club_its_own_stretch() {
    let mut game = game();
    game.competitions.push(league_with(vec![
        fixture("reds-game", "2026-08-02", HOME, "x", (2, 0)),
        fixture("blues-game", "2026-08-06", AWAY, "x", (3, 3)),
    ]));
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 3);
    game.clock.advance_days(1);
    game.manager.team_id = Some(AWAY.into());
    tracker.after_day(&game);
    pass_days(&mut game, &mut tracker, 4);

    let report = build_report(&game, tracker);

    assert_eq!(report.clubs.len(), 2);
    assert_eq!(
        (
            report.clubs[0].club_id.as_str(),
            report.clubs[0].days_played
        ),
        (HOME, 4)
    );
    assert_eq!(report.clubs[0].ended, StretchEnd::Moved);
    assert_eq!(
        (
            report.clubs[1].club_id.as_str(),
            report.clubs[1].days_played
        ),
        (AWAY, 4)
    );
    assert_eq!(
        report.clubs[0].matches.fixtures.items[0].fixture_id,
        "reds-game"
    );
    assert_eq!(
        report.clubs[1].matches.fixtures.items[0].fixture_id,
        "blues-game"
    );
    assert_eq!(report.clubs[1].from, report.clubs[0].until);
}

/// Given board messages and a rollover during the run
/// When the report is read
/// Then they are events with the id of the record they come from
#[test]
fn board_messages_and_season_rollovers_are_events() {
    let mut game = game();
    let board = |id: &str, date: &str, category: MessageCategory| {
        let mut message = InboxMessage::new(
            id.into(),
            "Board".into(),
            "Please improve".into(),
            "Board".into(),
            date.into(),
        );
        message.category = category;
        message.subject_key = Some("be.msg.board.test".into());
        message
    };
    game.messages.extend([
        board("msg-before", "2026-07-30", MessageCategory::BoardDirective),
        board("msg-1", "2026-08-03", MessageCategory::BoardDirective),
        board("msg-news", "2026-08-03", MessageCategory::Media),
        board("msg-after", "2026-08-12", MessageCategory::BoardDirective),
    ]);
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 2);
    let summary = crate::end_of_season::EndOfSeasonSummary {
        season: 2026,
        league_name: "Test League".into(),
        user_position: 4,
        user_points: 61,
        total_teams: 20,
        ..Default::default()
    };
    tracker.after_rollover(&game, &summary);
    pass_days(&mut game, &mut tracker, 2);

    let events = the_club(&build_report(&game, tracker)).events.clone();

    let messages: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            ReportEvent::BoardMessage {
                message_id,
                subject_key,
                ..
            } => Some((message_id.as_str(), subject_key.as_deref())),
            _ => None,
        })
        .collect();
    assert_eq!(
        messages,
        [("msg-1", Some("be.msg.board.test"))],
        "only the board's own messages inside the days played"
    );
    let rollovers: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                ReportEvent::SeasonCompleted {
                    season: 2026,
                    position: 4,
                    points: 61,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(rollovers.len(), 1);
}

/// Given a payment folded into one line for the month the run started in
/// When the report is read
/// Then the totals still hold but the report says single payments are not all listed
#[test]
fn a_folded_post_inside_the_run_says_the_detail_is_not_complete() {
    let mut game = game();
    let mut tracker = ReportTracker::begin(&game);
    game.cash_journal.extend([domain::finance::CashPost {
        id: format!("{}reds:Upkeep:2026-08", domain::finance::FOLDED_ID_PREFIX),
        club_id: HOME.into(),
        amount: -1_000,
        kind: CashKind::Upkeep,
        date: "2026-08-01".into(),
    }]);
    pass_days(&mut game, &mut tracker, 3);

    let finances = the_club(&build_report(&game, tracker)).finances.clone();

    assert!(!finances.detail_complete);
    assert_eq!(finances.posts, 1);
}

/// Given a loan that came back, a loan that ended, a purchase through a purchase option, a contract the
/// club ended and a release with no reason recorded
/// When the report is read
/// Then each is its own kind of line, the option is marked, and none is mistaken for a plain transfer
#[test]
fn loan_returns_purchase_options_and_unexplained_releases_have_their_own_lines() {
    let mut game = game();
    let mut returned = player("returned", Some(HOME));
    record(
        &mut returned,
        movement(
            "2026-08-02",
            PlayerMovementKind::LoanReturn,
            Some(BLUES),
            Some(REDS),
        ),
    );
    let mut ended = player("ended", Some(AWAY));
    record(
        &mut ended,
        movement(
            "2026-08-03",
            PlayerMovementKind::LoanReturn,
            Some(REDS),
            Some(BLUES),
        ),
    );
    let mut option = player("option", Some(HOME));
    record(
        &mut option,
        movement(
            "2026-08-04",
            PlayerMovementKind::LoanToBuy,
            Some(BLUES),
            Some(REDS),
        ),
    );
    let mut terminated = player("terminated", None);
    record(
        &mut terminated,
        PlayerMovementEntry {
            release_reason: Some(ReleaseReason::Terminated),
            ..movement("2026-08-05", PlayerMovementKind::Released, Some(REDS), None)
        },
    );
    let mut unexplained = player("unexplained", None);
    record(
        &mut unexplained,
        movement("2026-08-06", PlayerMovementKind::Released, Some(REDS), None),
    );
    game.players = vec![returned, ended, option, terminated, unexplained];
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 8);

    let club = the_club(&build_report(&game, tracker)).clone();

    let kind = |id: &str| {
        club.transfers
            .moves
            .items
            .iter()
            .find(|m| m.player_id == id)
            .unwrap()
    };
    assert_eq!(kind("returned").kind, TransferKind::LoanReturned);
    assert_eq!(kind("ended").kind, TransferKind::LoanEnded);
    assert_eq!(kind("option").kind, TransferKind::Purchase);
    assert!(kind("option").via_purchase_option);
    assert!(!kind("returned").via_purchase_option);
    assert_eq!(
        (club.transfers.purchases, club.transfers.loans_in),
        (1, 0),
        "a loan return is not a loan"
    );
    assert_eq!(
        (
            club.contracts.terminated,
            club.contracts.released,
            club.contracts.expired
        ),
        (1, 1, 0)
    );
}

/// Given more transfers than a report lists
/// When the report is read
/// Then the list is capped and the total beside it is exact
#[test]
fn a_long_list_is_capped_with_an_exact_total() {
    let mut game = game();
    let mut players = Vec::new();
    for n in 0..(MAX_LISTED + 25) {
        let mut p = player(&format!("p{n:04}"), Some(HOME));
        record(
            &mut p,
            movement(
                "2026-08-02",
                PlayerMovementKind::FreeAgentSigning,
                None,
                Some(REDS),
            ),
        );
        players.push(p);
    }
    game.players = players;
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 3);

    let transfers = the_club(&build_report(&game, tracker)).transfers.clone();

    assert_eq!(transfers.free_signings as usize, MAX_LISTED + 25);
    assert_eq!(transfers.moves.total as usize, MAX_LISTED + 25);
    assert_eq!(transfers.moves.items.len(), MAX_LISTED);
}

/// Given a finished run
/// When the report is built twice from the same game
/// Then both are identical: a report depends on nothing but the game and the tracker
#[test]
fn building_a_report_twice_gives_the_same_report() {
    let mut game = game();
    game.competitions.push(league_with(vec![fixture(
        "m",
        "2026-08-02",
        HOME,
        AWAY,
        (1, 0),
    )]));
    let mut tracker = ReportTracker::begin(&game);
    pass_days(&mut game, &mut tracker, 4);

    let first = build_report(&game, tracker.clone());
    let second = build_report(&game, tracker);

    assert_eq!(first, second);
}

/// Given things recorded on the run's first day BEFORE the run began (a payment, a finished match, a
/// signing, a delegation step, a board message), and the same kinds recorded on that day by the run
/// When the report is read
/// Then only what the run recorded is in it, and the books still reconcile: the first day is shared, so
/// the report separates what existed when the run began from what the run did
#[test]
fn what_was_recorded_before_the_run_on_its_first_day_is_not_the_runs() {
    let mut game = game();
    game.competitions.push(league_with(vec![fixture(
        "pre-match",
        "2026-08-01",
        HOME,
        AWAY,
        (9, 0),
    )]));
    journal::post(&mut game, HOME, -5_000, CashKind::Upkeep, day("2026-08-01")).unwrap();
    let mut before = player("pre", Some(HOME));
    record(
        &mut before,
        movement(
            "2026-08-01",
            PlayerMovementKind::FreeAgentSigning,
            None,
            Some(REDS),
        ),
    );
    game.players.push(before);
    let log = |title: &str| DelegationLogEntry {
        date: "2026-08-01".into(),
        duty: Duty::Scouting,
        title_key: title.into(),
        params: vec![],
        outcome: DecisionOutcome::Executed,
    };
    game.delegation.log.push(log("delegation.log.before"));
    let board = |id: &str| {
        let mut message = InboxMessage::new(
            id.into(),
            "Board".into(),
            "Directive".into(),
            "Board".into(),
            "2026-08-01".into(),
        );
        message.category = MessageCategory::BoardDirective;
        message
    };
    game.messages.push(board("msg-before"));
    let opening = game.teams[0].finance;
    let mut tracker = ReportTracker::begin(&game);

    journal::post(&mut game, HOME, -7_000, CashKind::Upkeep, day("2026-08-01")).unwrap();
    game.competitions[0]
        .fixtures
        .push(fixture("during-match", "2026-08-01", HOME, AWAY, (1, 0)));
    let mut during = player("during", Some(HOME));
    record(
        &mut during,
        movement(
            "2026-08-01",
            PlayerMovementKind::FreeAgentSigning,
            None,
            Some(REDS),
        ),
    );
    game.players.push(during);
    game.delegation.log.push(log("delegation.log.during"));
    game.messages.push(board("msg-during"));
    pass_days(&mut game, &mut tracker, 2);

    let club = the_club(&build_report(&game, tracker)).clone();

    assert_eq!(club.finances.opening_cash, opening);
    assert_eq!(club.finances.posts, 1, "only the run's own payment");
    assert_eq!(
        club.finances.unexplained, 0,
        "the earlier payment is in the opening cash, not counted twice"
    );
    let matches: Vec<_> = club
        .matches
        .fixtures
        .items
        .iter()
        .map(|m| m.fixture_id.as_str())
        .collect();
    assert_eq!(matches, ["during-match"]);
    let signings: Vec<_> = club
        .transfers
        .moves
        .items
        .iter()
        .map(|m| m.player_id.as_str())
        .collect();
    assert_eq!(signings, ["during"]);
    let steps: Vec<_> = club
        .delegation
        .entries
        .items
        .iter()
        .map(|e| e.title_key.as_str())
        .collect();
    assert_eq!(steps, ["delegation.log.during"]);
    let messages: Vec<_> = club
        .events
        .iter()
        .filter_map(|e| match e {
            ReportEvent::BoardMessage { message_id, .. } => Some(message_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(messages, ["msg-during"]);
}
