//! The scouting dossier: reports as estimates, the shortlist, assignments that can be cancelled and
//! handed on, and the screen projection. Nothing here reads a hidden value the club has not earned.

use chrono::{TimeZone, Utc};
use domain::manager::Manager;
use domain::player::{Player, PlayerAttributes, Position};
use domain::staff::{Staff, StaffAttributes, StaffRole};
use domain::team::Team;
use ofm_core::clock::GameClock;
use ofm_core::game::Game;
use ofm_core::scouting::{cancel_scouting, process_scouting, reassign_scouting, send_scout};
use ofm_core::scouting_dossier::{Confidence, SHORTLIST_MAX, shortlist_add, shortlist_remove};
use ofm_core::scouting_view::{ReportStatus, dossier_view, targets_view};

fn attrs() -> PlayerAttributes {
    PlayerAttributes {
        pace: 70,
        stamina: 65,
        strength: 60,
        agility: 68,
        passing: 72,
        shooting: 66,
        tackling: 58,
        dribbling: 74,
        defending: 55,
        positioning: 62,
        vision: 70,
        decisions: 64,
        composure: 60,
        aggression: 50,
        teamwork: 66,
        leadership: 55,
        handling: 30,
        reflexes: 30,
        aerial: 58,
    }
}

fn player(id: &str, team: Option<&str>, ovr: u8, potential: u8) -> Player {
    let mut p = Player::new(
        id.to_string(),
        id.to_string(),
        format!("Player {id}"),
        "2005-03-15".to_string(),
        "BR".to_string(),
        Position::Midfielder,
        attrs(),
    );
    p.team_id = team.map(str::to_string);
    p.ovr = ovr;
    p.potential = potential;
    p
}

fn team(id: &str) -> Team {
    Team::new(
        id.to_string(),
        format!("{id} FC"),
        id[..3].to_string(),
        "England".to_string(),
        "London".to_string(),
        "Stadium".to_string(),
        40_000,
    )
}

fn scout(id: &str, ability: u8, potential: u8) -> Staff {
    let mut s = Staff::new(
        id.to_string(),
        "Scout".to_string(),
        format!("Nr{id}"),
        "1985-01-01".to_string(),
        StaffRole::Scout,
        StaffAttributes {
            coaching: 30,
            judging_ability: ability,
            judging_potential: potential,
            physiotherapy: 20,
        },
    );
    s.team_id = Some("team1".to_string());
    s
}

fn game_with(scouts: Vec<Staff>, targets: usize) -> Game {
    let mut manager = Manager::new(
        "mgr1".into(),
        "Test".into(),
        "Manager".into(),
        "1980-01-01".into(),
        "England".into(),
    );
    manager.hire("team1".to_string());
    let mut players = vec![player("own", Some("team1"), 60, 70)];
    for n in 0..targets {
        players.push(player(&format!("t{n}"), Some("team2"), 62, 78));
    }
    Game::new(
        GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()),
        manager,
        vec![team("team1"), team("team2")],
        players,
        scouts,
        vec![],
    )
}

fn finish(game: &mut Game) {
    for _ in 0..12 {
        process_scouting(game);
        game.clock.advance_days(1);
    }
}

fn truth(game: &Game, id: &str) -> (u8, u8) {
    let p = game.players.iter().find(|p| p.id == id).unwrap();
    (p.ovr, p.potential)
}

/// Given a scout sent to a player
/// When the job finishes
/// Then the dossier holds a report whose ranges contain the true ability and potential but are
///      not centred on them, and the player is no longer unscouted.
#[test]
fn a_finished_job_writes_a_report_with_ranges_that_hold_the_truth() {
    let mut off_centre = 0;
    for n in 0..20 {
        let mut game = game_with(vec![scout("s1", 70, 70)], 1);
        // A different assignment day gives a different draw.
        game.clock.advance_days(n);
        send_scout(&mut game, "s1", "t0").unwrap();
        finish(&mut game);
        let report = game.scouting_dossier.report_on("t0").expect("a report");
        let (ovr, potential) = truth(&game, "t0");
        assert!(report.ability.contains(ovr));
        assert!(report.potential.unwrap().contains(potential));
        if report.ability.mid() != ovr {
            off_centre += 1;
        }
    }
    assert!(off_centre > 5, "the estimate was centred on the truth");
}

/// Given a sharp scout and a poor one on the same players
/// When both report
/// Then the sharp scout's ranges are narrower and his confidence higher, and the poor one cannot
///      judge potential at all when he is below the floor.
#[test]
fn scout_quality_sets_the_width_and_the_confidence() {
    let report_of = |ability: u8, potential: u8| {
        let mut game = game_with(vec![scout("s1", ability, potential)], 1);
        send_scout(&mut game, "s1", "t0").unwrap();
        finish(&mut game);
        game.scouting_dossier.report_on("t0").unwrap().clone()
    };
    let sharp = report_of(90, 90);
    let poor = report_of(35, 20);
    assert!(sharp.ability.width() < poor.ability.width());
    assert_eq!(sharp.confidence(), Confidence::High);
    assert_eq!(poor.confidence(), Confidence::Low);
    assert!(sharp.potential.is_some());
    assert!(poor.potential.is_none());
}

/// Given a player scouted twice in the same season
/// When the second report lands
/// Then the range is no wider than the first and still holds the truth, and the count is two;
///      a report from a later season replaces the old one instead.
#[test]
fn a_second_report_narrows_in_the_same_season_and_replaces_across_seasons() {
    let mut game = game_with(vec![scout("s1", 50, 50)], 1);
    game.season_context.season_start = Some("2025-06-01".to_string());
    send_scout(&mut game, "s1", "t0").unwrap();
    finish(&mut game);
    let first = game.scouting_dossier.report_on("t0").unwrap().clone();
    send_scout(&mut game, "s1", "t0").unwrap();
    finish(&mut game);
    let second = game.scouting_dossier.report_on("t0").unwrap().clone();
    assert_eq!(second.reports, 2);
    assert!(second.ability.width() <= first.ability.width());
    assert!(second.ability.contains(truth(&game, "t0").0));
    assert_eq!(game.scouting_dossier.reports.len(), 1);

    game.season_context.season_start = Some("2026-06-01".to_string());
    game.clock.advance_days(400);
    send_scout(&mut game, "s1", "t0").unwrap();
    finish(&mut game);
    assert_eq!(game.scouting_dossier.report_on("t0").unwrap().reports, 1);
}

/// Given the same career state twice
/// When the same scout is sent to the same player
/// Then the two dossiers are identical (the report is part of the assignment, not of the clock).
#[test]
fn the_same_assignment_writes_the_same_report() {
    let run = || {
        let mut game = game_with(vec![scout("s1", 60, 60)], 1);
        send_scout(&mut game, "s1", "t0").unwrap();
        finish(&mut game);
        serde_json::to_string(&game.scouting_dossier).unwrap()
    };
    assert_eq!(run(), run());
}

/// Given a player outside the manager's division
/// When a scout is sent
/// Then the job takes two days longer than the scout's own time, so scouting costs game time.
#[test]
fn a_distant_player_takes_longer_to_scout() {
    let mut game = game_with(vec![scout("s1", 90, 90)], 1);
    send_scout(&mut game, "s1", "t0").unwrap();
    assert_eq!(game.scouting_assignments[0].days_remaining, 2 + 2);
}

/// Given a player being scouted
/// When the manager cancels the job, or hands it to a second scout
/// Then the job disappears (no report is written), or moves with its days intact; unknown jobs,
///      the same scout and a busy scout are refused.
#[test]
fn a_player_job_can_be_cancelled_and_reassigned() {
    let mut game = game_with(vec![scout("s1", 70, 70), scout("s2", 70, 70)], 2);
    send_scout(&mut game, "s1", "t0").unwrap();
    let id = game.scouting_assignments[0].id.clone();
    assert_eq!(
        reassign_scouting(&mut game, &id, "s1").unwrap_err(),
        "be.error.scouting.scoutAlreadyAssignedToSearch"
    );
    reassign_scouting(&mut game, &id, "s2").unwrap();
    assert_eq!(game.scouting_assignments[0].scout_id, "s2");
    send_scout(&mut game, "s1", "t1").unwrap();
    assert!(
        reassign_scouting(&mut game, &id, "s1")
            .unwrap_err()
            .starts_with("be.error.scouting.scoutAssignmentFull")
    );
    cancel_scouting(&mut game, &id).unwrap();
    assert_eq!(
        cancel_scouting(&mut game, &id).unwrap_err(),
        "be.error.scouting.assignmentNotFound"
    );
    finish(&mut game);
    assert!(game.scouting_dossier.report_on("t0").is_none());
    assert!(game.scouting_dossier.report_on("t1").is_some());
}

/// Given a shortlist
/// When players are added and removed
/// Then own players, duplicates, unknown players and a full list are refused with keys, and
///      removing someone who is not listed is refused.
#[test]
fn the_shortlist_refuses_what_it_cannot_hold() {
    let mut game = game_with(vec![], SHORTLIST_MAX + 1);
    assert_eq!(
        shortlist_add(&mut game, "own").unwrap_err(),
        "be.error.scouting.shortlistOwnPlayer"
    );
    assert_eq!(
        shortlist_add(&mut game, "nobody").unwrap_err(),
        "be.error.playerNotFound"
    );
    shortlist_add(&mut game, "t0").unwrap();
    assert_eq!(
        shortlist_add(&mut game, "t0").unwrap_err(),
        "be.error.scouting.shortlistAlready"
    );
    for n in 1..SHORTLIST_MAX {
        shortlist_add(&mut game, &format!("t{n}")).unwrap();
    }
    assert_eq!(
        shortlist_add(&mut game, &format!("t{SHORTLIST_MAX}")).unwrap_err(),
        "be.error.scouting.shortlistFull"
    );
    shortlist_remove(&mut game, "t0").unwrap();
    assert_eq!(
        shortlist_remove(&mut game, "t0").unwrap_err(),
        "be.error.scouting.shortlistNotListed"
    );
}

/// Given a shortlisted player who then signs for the manager's club, and another who retires
/// When a day is processed
/// Then both leave the shortlist and the one still pursuable stays.
#[test]
fn the_shortlist_drops_players_who_can_no_longer_be_pursued() {
    let mut game = game_with(vec![], 3);
    for id in ["t0", "t1", "t2"] {
        shortlist_add(&mut game, id).unwrap();
    }
    game.players
        .iter_mut()
        .find(|p| p.id == "t0")
        .unwrap()
        .team_id = Some("team1".into());
    game.players
        .iter_mut()
        .find(|p| p.id == "t1")
        .unwrap()
        .retired = true;
    process_scouting(&mut game);
    let left: Vec<&str> = game
        .scouting_dossier
        .shortlist
        .iter()
        .map(|e| e.player_id.as_str())
        .collect();
    assert_eq!(left, vec!["t2"]);
}

/// Given one scouted and one unscouted target
/// When the screen projection is built
/// Then the unscouted one has no estimate and says so as a risk, the scouted one has ranges and a
///      status, and neither carries the hidden potential as a field of its own.
#[test]
fn the_view_separates_what_is_known_from_what_is_not() {
    let mut game = game_with(vec![scout("s1", 80, 80)], 2);
    game.season_context.season_start = Some("2025-06-01".to_string());
    send_scout(&mut game, "s1", "t0").unwrap();
    finish(&mut game);
    shortlist_add(&mut game, "t1").unwrap();
    let views = targets_view(&game, &["t0".into(), "t1".into(), "ghost".into()]);
    assert_eq!(views.len(), 2);
    let scouted = &views[0];
    let unscouted = &views[1];
    assert_eq!(scouted.status, ReportStatus::Current);
    assert!(scouted.report.as_ref().unwrap().potential.is_some());
    assert_eq!(unscouted.status, ReportStatus::Unscouted);
    assert!(unscouted.report.is_none());
    assert!(unscouted.risks.contains(&"unscouted".to_string()));
    assert!(unscouted.shortlisted);
    // The true potential (78) is not a value the unscouted view can show.
    let json = serde_json::to_value(unscouted).unwrap().to_string();
    assert!(!json.contains("\"potential\""));

    let dossier = dossier_view(&game);
    assert_eq!(dossier.reports.len(), 1);
    assert_eq!(dossier.shortlist.len(), 1);
}

/// Given a report written before this season started
/// When the view is built
/// Then it is stale and carries the risk.
#[test]
fn a_report_from_an_earlier_season_is_stale() {
    let mut game = game_with(vec![scout("s1", 80, 80)], 1);
    send_scout(&mut game, "s1", "t0").unwrap();
    finish(&mut game);
    game.season_context.season_start = Some("2030-01-01".to_string());
    let view = &targets_view(&game, &["t0".into()])[0];
    assert_eq!(view.status, ReportStatus::Stale);
    assert!(view.risks.contains(&"staleReport".to_string()));
}

/// Given a player who is a free agent and one at a club
/// When their views are built
/// Then only the club's player has a fee to afford.
#[test]
fn only_a_player_with_a_club_has_a_fee() {
    let mut game = game_with(vec![], 1);
    game.players.push(player("free", None, 55, 60));
    let views = targets_view(&game, &["t0".into(), "free".into()]);
    assert!(views[0].fee_affordable.is_some());
    assert!(views[1].fee_affordable.is_none());
    assert!(views[1].is_free_agent);
}
