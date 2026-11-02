//! Free-agent negotiations: an offer is a talk that takes days, is weighed against what the player
//! can command, competes with other clubs' offers, survives a save, and ends in one contract.

#[path = "support/free_agent_talks.rs"]
mod free_agent_support;

use chrono::{TimeZone, Utc};
use domain::free_agent_talk::{FreeAgentTalk, FreeAgentTalkReason, FreeAgentTalkStatus};
use domain::manager::Manager;
use domain::player::PlayerAttributes;
use domain::player::{Player, PlayerMovementKind, Position};
use domain::season::TransferWindowStatus;
use domain::team::Team;
use free_agent_support::{DAYS_TO_AN_ANSWER, run_days};
use ofm_core::clock::GameClock;
use ofm_core::contracts::{
    FreeAgentOfferDecision, InterestLevel, OfferTerms, accept_free_agent_counter,
    assess_free_agent_interest, free_agent_talks_awaiting_manager, process_contract_expiries,
    submit_free_agent_offer, withdraw_free_agent_offer,
};
use ofm_core::game::Game;

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

fn team(id: &str, reputation: u32) -> Team {
    let mut team = Team::new(
        id.to_string(),
        format!("{id} FC"),
        id.chars().take(3).collect(),
        "England".to_string(),
        "London".to_string(),
        format!("{id} Ground"),
        30_000,
    );
    team.reputation = reputation;
    team.finance = 20_000_000;
    team.transfer_budget = 10_000_000;
    team.wage_budget = 5_000_000;
    team
}

fn set_date(game: &mut Game, year: i32, month: u32, day: u32) {
    game.clock.current_date = Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap();
}

/// The user's club and two others, each with sixteen inert seniors so that no departure trips the
/// squad floor, and the one player under test.
fn game_with(player: Player) -> Game {
    let mut manager = Manager::new(
        "manager-1".to_string(),
        "Jane".to_string(),
        "Doe".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("team-1".to_string());
    let mut user = team("team-1", 100);
    user.manager_id = Some("manager-1".to_string());
    let mut game = Game::new(
        GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()),
        manager,
        vec![user, team("big-club", 400), team("small-club", 90)],
        vec![player],
        vec![],
        vec![],
    );
    let team_ids: Vec<String> = game.teams.iter().map(|team| team.id.clone()).collect();
    for team_id in team_ids {
        for ((group, _), count) in ofm_core::squad_floor::MIN_PLAYERS_PER_GROUP
            .into_iter()
            .zip([3, 5, 5, 3])
        {
            for index in 0..count {
                let id = format!("depth-{team_id}-{group:?}-{index}");
                let mut depth = Player::new(
                    id.clone(),
                    id.clone(),
                    id,
                    "1996-01-01".to_string(),
                    "England".to_string(),
                    group.clone(),
                    attrs(),
                );
                depth.team_id = Some(team_id.clone());
                depth.stage_contract_end(Some("2031-06-30".to_string()));
                depth.stage_wage(0);
                depth.market_value = 0;
                game.players.push(depth);
            }
        }
    }
    game
}

const STAR: &str = "star-1";
const USER: &str = "team-1";

fn free_agent(id: &str, ovr: u8, date_of_birth: &str, value: u64, last_wage: u32) -> Player {
    let mut player = Player::new(
        id.to_string(),
        id.to_string(),
        format!("Player {id}"),
        date_of_birth.to_string(),
        "England".to_string(),
        Position::Midfielder,
        attrs(),
    );
    player.team_id = None;
    player.ovr = ovr;
    player.potential = ovr;
    player.market_value = value;
    player.morale = 70;
    player.stage_wage(last_wage);
    player
}

/// A world with the user's club at `reputation`, its seniors rated `squad_ovr`, and a free agent.
fn world(player: Player, reputation: u32, squad_ovr: u8) -> Game {
    let mut game = game_with(player);
    game.season_context.transfer_window.status = TransferWindowStatus::Open;
    rate_squad(&mut game, USER, reputation, squad_ovr);
    // The other clubs are deep in quality: no free agent here would improve them, so none bids
    // unless a test makes it so.
    rate_squad(&mut game, "big-club", 400, 95);
    rate_squad(&mut game, "small-club", 90, 95);
    game
}

fn rate_squad(game: &mut Game, club_id: &str, reputation: u32, squad_ovr: u8) {
    game.teams
        .iter_mut()
        .find(|team| team.id == club_id)
        .unwrap()
        .reputation = reputation;
    for player in game
        .players
        .iter_mut()
        .filter(|player| player.team_id.as_deref() == Some(club_id))
    {
        player.ovr = squad_ovr;
        player.potential = squad_ovr;
    }
}

fn terms(weekly_wage: u32, contract_years: u32) -> OfferTerms {
    OfferTerms {
        weekly_wage,
        contract_years,
        signing_bonus: 0,
        promise_stepping_stone: false,
    }
}

fn player<'a>(game: &'a Game, id: &str) -> &'a Player {
    game.players.iter().find(|player| player.id == id).unwrap()
}

fn talks<'a>(game: &'a Game, id: &str) -> &'a [FreeAgentTalk] {
    &player(game, id).morale_core.free_agent_talks
}

fn interest(game: &Game, id: &str, offer: OfferTerms) -> ofm_core::contracts::FreeAgentInterest {
    assess_free_agent_interest(game, id, offer).expect("the player can be assessed")
}

/// What an elite free agent asks of the user's club, as the club's own assessment reports it.
fn his_ask(game: &Game) -> OfferTerms {
    let ask = interest(game, STAR, terms(1, 3));
    terms(ask.expected_wage, ask.expected_years)
}

fn star_world(reputation: u32, squad_ovr: u8) -> Game {
    let mut game = world(
        free_agent(STAR, 82, "1998-01-01", 20_000_000, 100_000),
        reputation,
        squad_ovr,
    );
    game.teams[0].wage_budget = 500_000_000;
    game
}

/// Scenario 1. Given a modest club (reputation 300, squad of 60s) and an 82-rated free agent
/// When the manager offers exactly what he asks
/// Then he is told it is unlikely, and when he answers he declines: no contract, a reason that
///      names the club's level, and a message for the manager.
#[test]
fn a_modest_club_cannot_sign_an_elite_free_agent_just_by_meeting_his_ask() {
    let mut game = star_world(300, 60);
    let ask = his_ask(&game);

    let advice = interest(&game, STAR, ask);
    assert_eq!(advice.verdict, InterestLevel::Unlikely);
    assert_eq!(
        advice.main_obstacle,
        Some(FreeAgentTalkReason::ClubBelowHisLevel)
    );
    let outcome = submit_free_agent_offer(&mut game, STAR, ask).expect("the offer is taken");
    assert_eq!(outcome.decision, FreeAgentOfferDecision::Pending);
    assert_eq!(player(&game, STAR).team_id, None, "nothing happens at once");
    run_days(&mut game, DAYS_TO_AN_ANSWER);

    assert_eq!(player(&game, STAR).team_id, None);
    let talk = &talks(&game, STAR)[0];
    assert_eq!(talk.status, FreeAgentTalkStatus::Rejected);
    assert_eq!(talk.reason, Some(FreeAgentTalkReason::ClubBelowHisLevel));
    assert!(
        game.messages
            .iter()
            .any(|message| message.subject_key.as_deref()
                == Some("be.msg.freeAgentTalk.rejected.subject")),
        "the manager is told"
    );
}

/// Scenario 1 (exception). Given the same elite player has gone eighty days without a club
/// When the club offers well over his ask and he would be its undisputed best player
/// Then he accepts: the exceptional case needs all three conditions, none of them free.
#[test]
fn an_elite_free_agent_accepts_a_modest_club_only_under_exceptional_conditions() {
    let build = |days_without_a_club: i64, raise_pct: u32| {
        let mut star = free_agent(STAR, 82, "1998-01-01", 20_000_000, 100_000);
        star.team_id = Some("big-club".to_string());
        star.stage_contract_end(Some("2026-05-10".to_string()));
        let mut game = world(star, 300, 60);
        // `world` rated everyone at big-club, where he still is: he is an 82.
        for rated in game.players.iter_mut().filter(|p| p.id == STAR) {
            rated.ovr = 82;
            rated.potential = 82;
        }
        game.teams[0].wage_budget = 500_000_000;
        set_date(&mut game, 2026, 5, 10);
        process_contract_expiries(&mut game);
        assert_eq!(player(&game, STAR).team_id, None, "released on expiry");
        let released = game.clock.current_date;
        game.clock.current_date = released + chrono::Duration::days(days_without_a_club);
        let ask = his_ask(&game);
        let offer = terms(
            ask.weekly_wage * (100 + raise_pct) / 100,
            ask.contract_years,
        );
        (game, offer)
    };

    // A fresh free agent turns the same raise down: no terms could make up the difference, so he
    // does not even counter, and says what stands in the way ...
    let (mut fresh, offer) = build(5, 40);
    submit_free_agent_offer(&mut fresh, STAR, offer).unwrap();
    run_days(&mut fresh, DAYS_TO_AN_ANSWER);
    assert_eq!(player(&fresh, STAR).team_id, None);
    assert_eq!(talks(&fresh, STAR)[0].status, FreeAgentTalkStatus::Rejected);
    assert_eq!(
        talks(&fresh, STAR)[0].reason,
        Some(FreeAgentTalkReason::ClubBelowHisLevel)
    );

    // ... and so does a long-waiting one who is only offered his ask.
    let (mut waited, offer) = build(80, 0);
    submit_free_agent_offer(&mut waited, STAR, offer).unwrap();
    run_days(&mut waited, DAYS_TO_AN_ANSWER);
    assert_eq!(player(&waited, STAR).team_id, None);

    // Eighty days without a club and a forty per cent raise is enough.
    let (mut game, offer) = build(80, 40);
    submit_free_agent_offer(&mut game, STAR, offer).unwrap();
    run_days(&mut game, DAYS_TO_AN_ANSWER);
    assert_eq!(
        player(&game, STAR).team_id.as_deref(),
        Some(USER),
        "{:?}",
        talks(&game, STAR)
    );
}

/// Scenario 2. Given a club whose board's wage policy cannot carry the wage
/// When the manager offers it
/// Then the backend refuses before any talk exists, and a bonus the club cannot pay is refused too.
#[test]
fn a_financially_impossible_offer_is_refused_by_the_backend() {
    let mut game = star_world(900, 80);
    game.teams[0].wage_budget = 1_000;
    let before = game.clone();

    let error = submit_free_agent_offer(&mut game, STAR, terms(400_000, 3)).unwrap_err();
    assert_eq!(error, "be.error.contracts.boardWagePolicy?budget=1000");
    assert!(talks(&game, STAR).is_empty(), "no talk was opened");

    game.teams[0].wage_budget = 500_000_000;
    game.teams[0].finance = 10_000;
    let broke = OfferTerms {
        signing_bonus: 1_000_000,
        ..terms(100_000, 3)
    };
    assert_eq!(
        submit_free_agent_offer(&mut game, STAR, broke).unwrap_err(),
        "be.error.transfers.insufficientFunds"
    );
    assert!(talks(&game, STAR).is_empty());
    assert_eq!(game.cash_journal.len(), before.cash_journal.len());
}

/// Scenario 2 (later). Given an offer the club could pay when it was made
/// When its wage budget collapses before the player answers
/// Then he does not sign: the offer lapses and the roster is unchanged.
#[test]
fn an_offer_the_club_can_no_longer_pay_lapses_instead_of_signing() {
    let mut game = world(free_agent("fa", 60, "1998-01-01", 400_000, 3_000), 300, 58);
    submit_free_agent_offer(&mut game, "fa", terms(4_000, 3)).unwrap();

    game.teams[0].wage_budget = 1;
    run_days(&mut game, DAYS_TO_AN_ANSWER);

    assert_eq!(player(&game, "fa").team_id, None);
    let talk = &talks(&game, "fa")[0];
    assert_eq!(talk.status, FreeAgentTalkStatus::Lapsed);
    assert_eq!(talk.reason, Some(FreeAgentTalkReason::ClubCannotAfford));
}

/// Scenario 3. Given a player a little below the club's level
/// When the manager offers his ask
/// Then he answers within three days and signs once: one club, one contract, one record.
#[test]
fn a_reasonable_player_signs_with_a_suitable_club_within_three_days() {
    let mut game = world(free_agent("fa", 58, "1998-01-01", 400_000, 3_000), 300, 60);
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    let outcome = submit_free_agent_offer(&mut game, "fa", ask).unwrap();
    let respond_on = outcome.respond_on.expect("he gives a day he will answer");
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    assert!(respond_on > today, "not the same day");

    run_days(&mut game, 1);
    let answered_by_now = player(&game, "fa").team_id.is_some();
    run_days(&mut game, DAYS_TO_AN_ANSWER - 1);

    assert_eq!(player(&game, "fa").team_id.as_deref(), Some(USER));
    assert!(
        respond_on <= game.clock.current_date.format("%Y-%m-%d").to_string(),
        "answered by the promised day (already on day one: {answered_by_now})"
    );
    let signings = player(&game, "fa")
        .movement_history
        .iter()
        .filter(|entry| entry.kind == PlayerMovementKind::FreeAgentSigning)
        .count();
    assert_eq!(signings, 1);
    assert_eq!(talks(&game, "fa")[0].status, FreeAgentTalkStatus::Signed);
}

/// Scenario 4. Given a player and an offer well below what he can command, but not an insult
/// When he answers
/// Then he declines for the pay, signs nothing, and the manager is told.
#[test]
fn a_player_turns_down_an_inadequate_contract() {
    let mut game = world(
        free_agent("fa", 70, "1998-01-01", 3_000_000, 15_000),
        300,
        62,
    );
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    let low = terms(ask.weekly_wage * 70 / 100, 1);

    let outcome = submit_free_agent_offer(&mut game, "fa", low).unwrap();
    assert_eq!(
        outcome.decision,
        FreeAgentOfferDecision::Pending,
        "not an insult"
    );
    run_days(&mut game, DAYS_TO_AN_ANSWER);

    assert_eq!(player(&game, "fa").team_id, None);
    assert_eq!(talks(&game, "fa")[0].status, FreeAgentTalkStatus::Rejected);
    assert_eq!(
        talks(&game, "fa")[0].reason,
        Some(FreeAgentTalkReason::WageTooLow)
    );
    assert!(player(&game, "fa").movement_history.is_empty());
}

/// Scenario 5. Given a player who asks for more than he was offered
/// When the manager accepts his counter
/// Then he signs at once on his terms; accepting again is refused, and a counter left unanswered
///      lapses after three days.
#[test]
fn a_counter_offer_can_be_accepted_once_and_lapses_if_ignored() {
    let ask_of = |game: &Game| {
        let a = interest(game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    let build = || {
        let mut game = world(
            free_agent("fa", 70, "1998-01-01", 3_000_000, 15_000),
            330,
            62,
        );
        let ask = ask_of(&game);
        submit_free_agent_offer(&mut game, "fa", terms(ask.weekly_wage * 85 / 100, 3)).unwrap();
        run_days(&mut game, DAYS_TO_AN_ANSWER);
        game
    };

    let mut game = build();
    let counter = talks(&game, "fa")[0].clone();
    assert_eq!(counter.status, FreeAgentTalkStatus::CounterOffered);
    assert_eq!(free_agent_talks_awaiting_manager(&game), 1);
    assert!(
        game.messages
            .iter()
            .any(|m| m.subject_key.as_deref() == Some("be.msg.freeAgentTalk.counter.subject"))
    );

    let outcome = accept_free_agent_counter(&mut game, "fa").unwrap();
    assert_eq!(outcome.decision, FreeAgentOfferDecision::Accepted);
    assert_eq!(player(&game, "fa").team_id.as_deref(), Some(USER));
    assert_eq!(Some(player(&game, "fa").wage()), counter.counter_wage);
    assert_eq!(free_agent_talks_awaiting_manager(&game), 0);
    assert_eq!(
        accept_free_agent_counter(&mut game, "fa").unwrap_err(),
        "be.error.contracts.playerNotFreeAgent",
        "a second click cannot sign him again"
    );

    let mut ignored = build();
    run_days(&mut ignored, 5);
    assert_eq!(talks(&ignored, "fa")[0].status, FreeAgentTalkStatus::Lapsed);
    assert_eq!(
        accept_free_agent_counter(&mut ignored, "fa").unwrap_err(),
        "be.error.freeAgents.noCounterToAnswer"
    );
    assert_eq!(player(&ignored, "fa").team_id, None);
}

/// Given a player who keeps countering and a manager who keeps coming back a little short
/// When the fourth round is not met
/// Then he breaks the talks off, and the club is not heard for a fortnight, but is after it.
#[test]
fn a_player_breaks_off_after_four_rounds_and_the_club_is_heard_again_later() {
    let mut game = world(
        free_agent("fa", 70, "1998-01-01", 3_000_000, 15_000),
        330,
        62,
    );
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    let short = terms(ask.weekly_wage * 85 / 100, 3);

    for round in 1..=4 {
        submit_free_agent_offer(&mut game, "fa", short).unwrap();
        assert_eq!(talks(&game, "fa")[0].round, round);
        run_days(&mut game, DAYS_TO_AN_ANSWER);
    }

    let talk = &talks(&game, "fa")[0];
    assert_eq!(talk.status, FreeAgentTalkStatus::Rejected);
    assert_eq!(talk.reason, Some(FreeAgentTalkReason::TalksBrokenOff));
    assert_eq!(
        submit_free_agent_offer(&mut game, "fa", short).unwrap_err(),
        "be.error.freeAgents.talksBrokenOff"
    );
    run_days(&mut game, 15);
    assert!(submit_free_agent_offer(&mut game, "fa", short).is_ok());
}

/// The manager can take an offer back; he cannot then be answered.
#[test]
fn a_withdrawn_offer_is_never_answered() {
    let mut game = world(free_agent("fa", 58, "1998-01-01", 400_000, 3_000), 300, 60);
    submit_free_agent_offer(&mut game, "fa", terms(9_000, 3)).unwrap();

    let outcome = withdraw_free_agent_offer(&mut game, "fa").unwrap();
    run_days(&mut game, 10);

    assert_eq!(outcome.decision, FreeAgentOfferDecision::Rejected);
    assert_eq!(player(&game, "fa").team_id, None);
    assert_eq!(talks(&game, "fa")[0].status, FreeAgentTalkStatus::Withdrawn);
    assert_eq!(
        withdraw_free_agent_offer(&mut game, "fa").unwrap_err(),
        "be.error.freeAgents.noOpenTalk"
    );
}

/// A rival club's open offer to the player, due `respond_on`, as the market makes them.
fn rival_offer(game: &mut Game, id: &str, club_id: &str, wage: u32, respond_on: &str) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.players
        .iter_mut()
        .find(|player| player.id == id)
        .unwrap()
        .morale_core
        .free_agent_talks
        .push(FreeAgentTalk {
            id: format!("rival:{club_id}"),
            club_id: club_id.to_string(),
            weekly_wage: wage,
            contract_years: 3,
            opened_on: today.clone(),
            updated_on: today,
            respond_on: Some(respond_on.to_string()),
            ..FreeAgentTalk::default()
        });
}

/// Scenario 6. Given a strong free agent and a much bigger club whose side he would improve
/// When days pass and the manager has not bid
/// Then the bigger club makes an offer through the market, and he signs with it.
#[test]
fn a_bigger_club_that_needs_an_attractive_free_agent_bids_for_him() {
    let mut game = world(
        free_agent("fa", 70, "1998-01-01", 3_000_000, 15_000),
        300,
        62,
    );
    rate_squad(&mut game, "big-club", 850, 58);

    run_days(&mut game, 1);
    let rivals = interest(&game, "fa", terms(1, 3)).rival_clubs;
    run_days(&mut game, 6);

    assert_eq!(rivals.len(), 1, "one club made its move on the first day");
    assert_eq!(rivals[0].club_id, "big-club");
    assert_eq!(player(&game, "fa").team_id.as_deref(), Some("big-club"));
    assert!(talks(&game, "fa").iter().all(|talk| !talk.is_open()));
}

/// Scenario 6 (lost). Given the manager's offer is with the player until the fourth day
/// When a bigger club's offer, due earlier, is the better one
/// Then he signs with the rival, the manager's offer is lost to it, he is told who got him, and
///      there is one contract.
#[test]
fn a_better_rival_offer_wins_the_player_and_the_managers_is_lost() {
    let mut game = world(
        free_agent("fa", 70, "1998-01-01", 3_000_000, 15_000),
        300,
        62,
    );
    rate_squad(&mut game, "big-club", 850, 58);
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    submit_free_agent_offer(&mut game, "fa", ask).unwrap();
    for talk in &mut game
        .players
        .iter_mut()
        .find(|p| p.id == "fa")
        .unwrap()
        .morale_core
        .free_agent_talks
    {
        talk.respond_on = Some("2026-08-04".to_string());
    }
    rival_offer(&mut game, "fa", "big-club", ask.weekly_wage, "2026-08-02");

    run_days(&mut game, 2);

    assert_eq!(player(&game, "fa").team_id.as_deref(), Some("big-club"));
    let mine = talks(&game, "fa")
        .iter()
        .find(|talk| talk.club_id == USER)
        .unwrap();
    assert_eq!(mine.status, FreeAgentTalkStatus::LostToRival);
    let lost = game
        .messages
        .iter()
        .find(|m| m.subject_key.as_deref() == Some("be.msg.freeAgentTalk.lost.subject"))
        .expect("the manager is told");
    assert_eq!(
        lost.i18n_params.get("team").map(String::as_str),
        Some("big-club FC")
    );
    run_days(&mut game, 5);
    assert_eq!(
        player(&game, "fa")
            .movement_history
            .iter()
            .filter(|entry| entry.kind == PlayerMovementKind::FreeAgentSigning)
            .count(),
        1
    );
}

/// Scenario 6 (won). Given the manager's club is the bigger one
/// When a small rival's offer is on his desk too
/// Then the better offer wins — the manager's — and the rival is the one that is told no.
#[test]
fn the_better_offer_wins_when_it_is_the_managers() {
    let mut game = world(
        free_agent("fa", 70, "1998-01-01", 3_000_000, 15_000),
        900,
        75,
    );
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    submit_free_agent_offer(&mut game, "fa", ask).unwrap();
    rival_offer(&mut game, "fa", "small-club", ask.weekly_wage, "2026-08-04");

    run_days(&mut game, DAYS_TO_AN_ANSWER);

    assert_eq!(player(&game, "fa").team_id.as_deref(), Some(USER));
    let rival = talks(&game, "fa")
        .iter()
        .find(|talk| talk.club_id == "small-club")
        .unwrap();
    assert_eq!(rival.status, FreeAgentTalkStatus::LostToRival);
}

/// Scenario 7. Given a game saved in the middle of a negotiation
/// When it is reloaded and the days play on
/// Then it ends exactly as the game that was never saved: same answer, same day, same contract.
#[test]
fn a_negotiation_survives_a_save_and_reload() {
    let mut game = world(
        free_agent("fa", 70, "1998-01-01", 3_000_000, 15_000),
        330,
        62,
    );
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    submit_free_agent_offer(&mut game, "fa", terms(ask.weekly_wage * 85 / 100, 3)).unwrap();
    run_days(&mut game, 1);

    let saved = serde_json::to_string(&game).expect("the game serialises");
    let mut reloaded: Game = serde_json::from_str(&saved).expect("and reads back");
    assert_eq!(talks(&reloaded, "fa"), talks(&game, "fa"));

    run_days(&mut game, 6);
    run_days(&mut reloaded, 6);

    assert_eq!(talks(&reloaded, "fa"), talks(&game, "fa"));
    assert_eq!(player(&reloaded, "fa").team_id, player(&game, "fa").team_id);
    let ids = |game: &Game| -> Vec<(String, Option<String>)> {
        game.messages
            .iter()
            .map(|message| (message.id.clone(), message.subject_key.clone()))
            .collect()
    };
    assert_eq!(ids(&reloaded), ids(&game));
}

/// An older save has no talks on its players: they read back as none.
#[test]
fn a_save_from_before_negotiations_loads_with_no_talks() {
    let game = world(free_agent("fa", 58, "1998-01-01", 400_000, 3_000), 300, 60);
    let mut json: serde_json::Value = serde_json::to_value(&game).unwrap();
    for player in json["players"].as_array_mut().unwrap() {
        player["morale_core"]
            .as_object_mut()
            .unwrap()
            .remove("free_agent_talks");
    }

    let loaded: Game = serde_json::from_value(json).expect("an old save still loads");

    assert!(talks(&loaded, "fa").is_empty());
}

/// Scenario 8. Given the transfer window closes while a free agent still owes his answer
/// When the deadline sweep and the following days run
/// Then the talk is not swept away with the paid negotiations: free agents are not bound to the
///      window, so he still answers, and signs once.
#[test]
fn a_talk_open_at_the_transfer_deadline_still_resolves() {
    let mut game = world(free_agent("fa", 58, "1998-01-01", 400_000, 3_000), 300, 60);
    game.season_context.transfer_window.status = TransferWindowStatus::DeadlineDay;
    game.season_context.transfer_window.days_remaining = Some(0);
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    submit_free_agent_offer(&mut game, "fa", ask).unwrap();

    ofm_core::transfers::expire_negotiations_at_deadline(&mut game);
    game.season_context.transfer_window.status = TransferWindowStatus::Closed;
    run_days(&mut game, DAYS_TO_AN_ANSWER);

    assert_eq!(player(&game, "fa").team_id.as_deref(), Some(USER));
}

/// Scenario 9. Given an open talk
/// When the manager clicks again, a second club also bids, or the counter is accepted twice
/// Then there is never a second contract, a second talk for the club, or a second record.
#[test]
fn clicking_again_never_creates_a_duplicate_talk_or_contract() {
    let mut game = world(free_agent("fa", 58, "1998-01-01", 400_000, 3_000), 300, 60);
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };
    submit_free_agent_offer(&mut game, "fa", ask).unwrap();

    for _ in 0..3 {
        assert_eq!(
            submit_free_agent_offer(&mut game, "fa", ask).unwrap_err(),
            "be.error.freeAgents.talkAlreadyOpen"
        );
    }
    assert_eq!(talks(&game, "fa").len(), 1);

    run_days(&mut game, DAYS_TO_AN_ANSWER);
    assert_eq!(player(&game, "fa").team_id.as_deref(), Some(USER));
    assert_eq!(
        submit_free_agent_offer(&mut game, "fa", ask).unwrap_err(),
        "be.error.contracts.playerNotFreeAgent"
    );
    let on_the_books = game.players.iter().filter(|p| p.id == "fa").count();
    assert_eq!(on_the_books, 1);
    assert_eq!(
        player(&game, "fa")
            .movement_history
            .iter()
            .filter(|entry| entry.kind == PlayerMovementKind::FreeAgentSigning)
            .count(),
        1
    );
}

/// The manager may not open talks with the whole market at once.
#[test]
fn the_number_of_open_talks_is_bounded() {
    let mut game = world(
        free_agent("fa-0", 58, "1998-01-01", 400_000, 3_000),
        300,
        60,
    );
    for index in 1..8 {
        game.players.push(free_agent(
            &format!("fa-{index}"),
            58,
            "1998-01-01",
            400_000,
            3_000,
        ));
    }
    let mut refused = Vec::new();
    for index in 0..8 {
        if let Err(error) =
            submit_free_agent_offer(&mut game, &format!("fa-{index}"), terms(3_500, 3))
        {
            refused.push(error);
        }
    }
    assert_eq!(
        refused,
        vec!["be.error.freeAgents.tooManyOpenTalks".to_string(); 3]
    );
}

/// Scenario 10. Given a paid transfer bought in three instalments
/// When a free agent is negotiated and signed alongside it, and a year later the next instalment falls due
/// Then the scheduled obligations were untouched by the signing and are paid once, to the cent.
#[test]
fn free_agent_signings_leave_scheduled_transfer_obligations_alone() {
    use domain::transfer_terms::TransferTerms;
    use ofm_core::transfers::{make_transfer_bid_with_terms, process_transfer_obligations};

    let mut bought = free_agent("bought", 70, "1999-01-01", 1_000_000, 10_000);
    bought.team_id = Some("small-club".to_string());
    bought.stage_contract_end(Some("2029-06-30".to_string()));
    bought.potential = 70;
    let mut game = world(bought, 300, 60);
    game.players
        .push(free_agent("fa", 58, "1998-01-01", 400_000, 3_000));
    let fee = 3_000_000;
    let instalments = TransferTerms {
        instalments: 3,
        ..TransferTerms::default()
    };
    let bid = make_transfer_bid_with_terms(&mut game, "bought", fee, instalments).unwrap();
    assert_eq!(
        bid.decision,
        ofm_core::transfers::TransferNegotiationDecision::Accepted
    );
    let obligations = serde_json::to_string(&game.transfer_obligations).unwrap();
    assert_eq!(
        game.transfer_obligations.len(),
        2,
        "two instalments to come"
    );
    let (cash, journal) = (game.teams[0].finance, game.cash_journal.len());
    let ask = {
        let a = interest(&game, "fa", terms(1, 3));
        terms(a.expected_wage, a.expected_years)
    };

    submit_free_agent_offer(&mut game, "fa", ask).unwrap();
    run_days(&mut game, DAYS_TO_AN_ANSWER);

    accept_free_agent_counter(&mut game, "fa").expect("the counter is within the club's means");
    assert_eq!(player(&game, "fa").team_id.as_deref(), Some(USER));
    assert_eq!(
        serde_json::to_string(&game.transfer_obligations).unwrap(),
        obligations
    );
    assert_eq!(
        (game.teams[0].finance, game.cash_journal.len()),
        (cash, journal),
        "no bonus: no cash moved"
    );
    set_date(&mut game, 2027, 8, 2);
    process_transfer_obligations(&mut game);
    let share = fee / 3;
    assert_eq!(game.teams[0].finance, cash - share as i64);
}
