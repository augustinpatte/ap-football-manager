//! M3: the youth academy and the manager's decisions over several seasons, in an isolated database.
//!
//! A scripted manager runs the academy the way the screen lets a human: it reads the review, and it
//! acts only on what the review says (promotes a youngster the advice recommends while a senior
//! place is free, lists a loan candidate). Every season boundary the game is written to a real
//! `.db` file in a temporary directory, read back by a fresh `SaveManager`, and the career
//! *continues from the reloaded copy*, so a reload that changed or lost anything would show in the
//! seasons after it.
//!
//! What must hold on every day and every week, whatever the world does:
//!
//! * no player appears twice, and no player the academy has held ever leaves `game.players`;
//! * the registered seniors never exceed the limit, so no promotion bypasses registration;
//! * the review never carries a potential, and every assessed band holds the true ceiling the
//!   simulation still keeps, within the widths the youth department can give;
//! * the review reads the same twice running and the same after a reload;
//! * advice and the game's rules agree: a promotion the advice calls blocked is refused for a real
//!   reason (out on loan, or no senior place), and a recommended one the manager acts on succeeds.

use std::collections::HashSet;

use db::save_manager::SaveManager;
use domain::delegation::{DelegationMode, Duty};
use domain::player::SquadRole;
use domain::staff::StaffPost;
use domain::stats::StatsState;
use ofm_core::academy_advice::AdviceState;
use ofm_core::academy_review::{AcademyReview, academy_review, promotion_place_refusal};
use ofm_core::end_of_season::is_season_complete;
use ofm_core::game::Game;
use ofm_core::rating_view::RatingView;
use ofm_core::staff_system::{market, profile};
use season_harness::driver::{RunOptions, SeasonObserver, play_day, roll_season_over};
use season_harness::worlds::WorldSpec;

struct Quiet;
impl SeasonObserver for Quiet {}

fn user_team(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

/// The club's administration is handed to an assistant, except the academy and scouting, which the
/// scripted manager keeps. The same setup as the I7 study (`i7_scouting_youth.rs`); the two files
/// are separate test targets and cannot share a helper without a support module both would adopt.
fn administered(game: &mut Game) {
    let team = user_team(game);
    let has_assistant = profile::team_staff(game, &team)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager);
    if !has_assistant {
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
            .expect("a free assistant manager exists in the world");
        market::hire(game, &team, &id, wage, 3).expect("the assistant can be hired");
    }
    game.delegation.apply_mode(DelegationMode::Full);
    game.delegation.set(Duty::StaffRecruitment, false, false);
    game.delegation.set(Duty::StaffRenewals, false, false);
    game.delegation.set(Duty::Scouting, false, false);
    game.delegation.set(Duty::Youth, false, false);
    for member in game
        .staff
        .iter_mut()
        .filter(|s| s.team_id.as_deref() == Some(team.as_str()))
    {
        member.contract_end = Some("2099-01-01".to_string());
    }
}

/// What the scripted manager did, and what it saw, over the career.
#[derive(Default)]
struct Watch {
    /// Every academy player the club has held: none may ever leave `game.players`.
    seen_in_academy: HashSet<String>,
    promoted: usize,
    loan_listed: usize,
    reviews_read: usize,
    /// Reviews that had at least one row whose promotion the advice called blocked.
    blocked_rows: usize,
    reloads: usize,
    seasons_with_new_intake: usize,
    /// Academy players lent out to another club during the career, to be back by the end of it.
    lent: Vec<String>,
}

fn review_json(review: &AcademyReview) -> String {
    serde_json::to_string(review).expect("the review serializes")
}

/// The invariants that must hold at the end of every day.
fn check_day(game: &Game, watch: &mut Watch, label: &str) {
    let mut ids = HashSet::new();
    for player in &game.players {
        assert!(
            ids.insert(player.id.as_str()),
            "{label}: {} appears twice",
            player.id
        );
    }
    let team = user_team(game);
    let capacity = ofm_core::contracts::squad_capacity(game, &team);
    assert!(
        capacity.registered_seniors <= capacity.senior_limit,
        "{label}: {} registered seniors against a limit of {}",
        capacity.registered_seniors,
        capacity.senior_limit
    );
    for id in &watch.seen_in_academy {
        assert!(
            ids.contains(id.as_str()),
            "{label}: academy player {id} is no longer in the game"
        );
    }
    for player in &game.players {
        if player.contract_club_id() == Some(team.as_str()) && player.squad_role == SquadRole::Youth
        {
            watch.seen_in_academy.insert(player.id.clone());
        }
    }
}

/// Lends one academy player to another club for three weeks, as a loan deal would: the club keeps his
/// contract, he plays for the borrower, and the daily loan processing must bring him back. Gives the
/// scenario a youngster the advice must call blocked, which a scripted manager does not meet by
/// chance.
fn lend_one_academy_player(game: &mut Game, watch: &mut Watch) {
    let team = user_team(game);
    let borrower = game
        .teams
        .iter()
        .find(|t| t.id != team)
        .map(|t| t.id.clone())
        .expect("another club exists");
    let today = game.clock.current_date.date_naive();
    let Some(id) = academy_players(game)
        .into_iter()
        .filter(|id| {
            game.players
                .iter()
                .any(|p| &p.id == id && p.active_loan.is_none() && p.squad_role == SquadRole::Youth)
        })
        .min()
    else {
        return;
    };
    let player = game
        .players
        .iter_mut()
        .find(|p| p.id == id)
        .expect("the player exists");
    player.team_id = Some(borrower.clone());
    player.active_loan = Some(domain::player::ActiveLoan {
        parent_team_id: team,
        loan_team_id: borrower,
        loan_team_name: None,
        start_date: today.to_string(),
        end_date: (today + chrono::Duration::days(21)).to_string(),
        wage_contribution_pct: 50,
        buy_option_fee: None,
        loan_start_minutes: 0,
        loan_start_appearances: 0,
        development_reported_minutes: 0,
        development_reported_appearances: 0,
        terms_source: domain::player::LoanTermsSource::Agreed,
    });
    watch.lent.push(id);
}

/// What a week's review must satisfy, and the manager's reaction to it.
fn manager_reads_the_review(game: &mut Game, watch: &mut Watch, label: &str) {
    let first = academy_review(game).expect("the manager has a club");
    let again = academy_review(game).expect("the manager has a club");
    assert_eq!(
        review_json(&first),
        review_json(&again),
        "{label}: the review changed between two reads on the same day"
    );
    let wire = review_json(&first);
    for key in ["\"potential\"", "expected_ovr", "next_year"] {
        assert!(!wire.contains(key), "{label}: {key} in the review");
    }
    watch.reviews_read += 1;

    let team = user_team(game);
    let place_refusal = promotion_place_refusal(game, &team);
    for row in &first.rows {
        let player = game
            .players
            .iter()
            .find(|p| p.id == row.player_id)
            .expect("a row's player exists");
        let truth = player.potential.max(player.ovr);
        let RatingView::Estimate {
            low,
            high,
            confidence: _,
            current: true,
            ..
        } = row.ceiling
        else {
            panic!("{label}: {} has no assessed ceiling", row.player_id);
        };
        assert!(
            low <= truth && truth <= high && low >= player.ovr && high - low < 24 && high <= 99,
            "{label}: {} band {low}-{high} does not hold a true ceiling of {truth}",
            row.player_id
        );
        // The advice and the game's rules say the same thing.
        let blocked = row.advice.promote.state == AdviceState::Blocked;
        assert_eq!(
            blocked,
            row.on_loan || place_refusal.is_some(),
            "{label}: {} advice {:?} against loan {} / place {:?}",
            row.player_id,
            row.advice.promote,
            row.on_loan,
            place_refusal
        );
        watch.blocked_rows += usize::from(blocked);
    }

    // Act only on the advice.
    for row in &first.rows {
        if row.advice.promote.state == AdviceState::Recommended
            && promotion_place_refusal(game, &team).is_none()
        {
            let player = game
                .players
                .iter_mut()
                .find(|p| p.id == row.player_id)
                .expect("a row's player exists");
            player.squad_role = SquadRole::Senior;
            watch.promoted += 1;
        } else if row.advice.loan.state == AdviceState::Recommended
            && !row.loan_listed
            && !row.on_loan
        {
            let player = game
                .players
                .iter_mut()
                .find(|p| p.id == row.player_id)
                .expect("a row's player exists");
            player.loan_listed = true;
            watch.loan_listed += 1;
        }
    }
}

/// Writes the game to a `.db` file in `dir`, reads it back through a fresh manager, and returns
/// the reloaded copy, asserting the review reads the same on both sides.
fn save_and_reload(game: &Game, dir: &std::path::Path, watch: &mut Watch, label: &str) -> Game {
    let mut saves = SaveManager::init(dir).expect("the save manager opens");
    let save_id = saves
        .create_save_with_stats(game, &StatsState::default(), "m3")
        .expect("the save is created");
    saves
        .save_game_with_stats(game, &StatsState::default(), &save_id)
        .expect("the save is written");
    let reloaded = SaveManager::init(dir)
        .expect("a fresh manager opens the same directory")
        .load_game(&save_id)
        .expect("the save loads");
    assert_eq!(
        review_json(&academy_review(game).expect("a club")),
        review_json(&academy_review(&reloaded).expect("a club")),
        "{label}: the review changed across the reload"
    );
    assert_eq!(
        game.players.len(),
        reloaded.players.len(),
        "{label}: players lost"
    );
    watch.reloads += 1;
    reloaded
}

fn run_career(seasons: u32) -> Watch {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    administered(&mut game);
    let options = RunOptions::seasons(seasons);
    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut watch = Watch::default();
    check_day(&game, &mut watch, "start");

    for season in 1..=seasons {
        let mut day = 0;
        while !is_season_complete(&game) {
            day += 1;
            assert!(day < 470, "season {season} did not complete");
            let label = format!("season {season} day {day}");
            if day == 50 {
                lend_one_academy_player(&mut game, &mut watch);
            }
            if day % 7 == 1 {
                manager_reads_the_review(&mut game, &mut watch, &label);
            }
            play_day(&mut game, options, season, day, &mut Quiet)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            check_day(&game, &mut watch, &label);
        }
        // Every youngster lent so far still exists exactly once; the one lent this season is back
        // at his club, with the contract still his club's.
        for id in &watch.lent {
            assert_eq!(
                game.players.iter().filter(|p| &p.id == id).count(),
                1,
                "season {season}: {id} must exist once"
            );
        }
        let id = watch.lent.last().expect("one was lent this season");
        let lent = game.players.iter().find(|p| &p.id == id).expect("exists");
        assert!(
            lent.active_loan.is_none(),
            "season {season}: {id} is still out on loan after his three weeks"
        );
        eprintln!(
            "M3 season {season}: lent {id} is back={} (club {:?}, contract club {:?}, role {:?})",
            lent.team_id.as_deref() == Some(user_team(&game).as_str()),
            lent.team_id,
            lent.contract_club_id(),
            lent.squad_role
        );
        let academy_before = academy_players(&game);
        // The season boundary: the career goes through a real database and continues from the copy.
        game = save_and_reload(
            &game,
            dir.path(),
            &mut watch,
            &format!("season {season} end"),
        );
        roll_season_over(&mut game, options, season, &mut Quiet)
            .unwrap_or_else(|e| panic!("season {season} rollover: {e}"));
        check_day(
            &game,
            &mut watch,
            &format!("season {season} after rollover"),
        );
        let academy_after = academy_players(&game);
        if academy_after.difference(&academy_before).next().is_some() {
            watch.seasons_with_new_intake += 1;
        }
    }
    watch
}

fn academy_players(game: &Game) -> HashSet<String> {
    let team = user_team(game);
    game.players
        .iter()
        .filter(|p| {
            p.contract_club_id() == Some(team.as_str())
                && p.squad_role == SquadRole::Youth
                && !p.retired
        })
        .map(|p| p.id.clone())
        .collect()
}

/// Given a generated career in which the manager acts only on the academy review, and the game goes
///       through a real database at the end of every season and continues from the reloaded copy
/// When several seasons are played, with ageing, season rollovers and each year's intake
/// Then nobody is duplicated or lost, the registered seniors never pass the limit, the review never
///      carries a potential, every assessed band holds the true ceiling, the review reads the same
///      twice and after each reload, and the advice always agrees with the game's own rules.
#[test]
#[ignore = "plays several full seasons: run with --release -- --ignored m3_academy"]
fn m3_academy_decisions_hold_across_seasons_and_reloads() {
    let seasons: u32 = std::env::var("M3_SEASONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let watch = run_career(seasons);

    assert_eq!(
        watch.reloads, seasons as usize,
        "a reload at every season end"
    );
    assert!(
        watch.reviews_read > 40,
        "the manager read the review every week"
    );
    assert!(
        watch.seen_in_academy.len() > 5,
        "the academy was never populated: the scenario would be vacuous"
    );
    assert!(
        watch.seasons_with_new_intake > 0,
        "no season's intake brought a new youngster into the academy"
    );
    assert_eq!(
        watch.lent.len(),
        seasons as usize,
        "one youngster lent out in each season"
    );
    assert!(
        watch.blocked_rows > 0,
        "the advice never met a blocked promotion: the loan injected each season should have"
    );
    assert!(
        watch.promoted > 0 && watch.loan_listed > 0,
        "the manager never acted on the advice: the scenario would be vacuous"
    );
    eprintln!(
        "M3: {seasons} seasons, {} reviews, {} promoted, {} loan-listed, {} blocked rows, {} academy players seen, {} intakes",
        watch.reviews_read,
        watch.promoted,
        watch.loan_listed,
        watch.blocked_rows,
        watch.seen_in_academy.len(),
        watch.seasons_with_new_intake
    );
}
