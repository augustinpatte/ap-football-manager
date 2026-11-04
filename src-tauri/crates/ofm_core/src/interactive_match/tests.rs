//! Controller scenarios: authority, order translation, the computer managers, finalisation
//! input, determinism, rendering-speed independence and save/reload.
use super::ai::Group;
use super::plan::TacticalChange;
use super::*;
use crate::spatial_runner::tests::game_with_substitutes;
use domain::team::PlayStyle;

/// `MatchReport` has no `PartialEq`; two reports are the same when they serialise the same.
fn as_json(report: &MatchReport) -> serde_json::Value {
    serde_json::to_value(report).expect("a report serialises")
}

fn identity() -> FixtureIdentity {
    FixtureIdentity {
        fixture_index: 0,
        competition_id: "competition-m12-1".into(),
        fixture_id: "fixture-m12-1".into(),
    }
}

fn game(seed: u64) -> Game {
    let mut game = game_with_substitutes(7);
    game.seed = seed;
    game
}

fn start(seed: u64) -> InteractiveMatch {
    InteractiveMatch::start(&game(seed), identity(), "session-1").expect("starts")
}

/// Given an interactive match just started,
/// Then the engine session it holds plays the F4.4 football (run F4.4: the instant path was covered,
/// the interactive one could revert unseen).
#[test]
fn an_interactive_match_is_started_with_the_f44_football() {
    use apfm_match_lab::sim::football::FootballConfig;
    let cp = start(1).checkpoint().expect("checkpoint");
    assert_eq!(cp.session.football, FootballConfig::full_match_f44());
    assert!(cp.session.football.m12.continuous_line);
}

/// Given an interactive match restored from its checkpoint,
/// Then it keeps the football the checkpoint recorded (a save made under F4.4 reloads as F4.4).
#[test]
fn a_restored_interactive_match_keeps_the_recorded_football() {
    use apfm_match_lab::sim::football::FootballConfig;
    let restored =
        InteractiveMatch::restore(start(1).checkpoint().expect("checkpoint")).expect("restores");
    let again = restored.checkpoint().expect("checkpoint");
    assert_eq!(again.session.football, FootballConfig::full_match_f44());
}

fn run_for(m: &mut InteractiveMatch, minutes: u32) -> AdvanceOutcome {
    m.advance(AdvanceRequest {
        target: AdvanceTarget::ForMinutes(minutes),
        stop_at_key_events: false,
    })
    .unwrap()
}

fn run_to_end(m: &mut InteractiveMatch) {
    loop {
        let out = m
            .advance(AdvanceRequest {
                target: AdvanceTarget::ToEnd,
                stop_at_key_events: false,
            })
            .unwrap();
        if out.stop == StopReason::Finished {
            return;
        }
        assert_ne!(out.stop, StopReason::Paused);
    }
}

fn order(m: &InteractiveMatch, id: &str, order: UserOrder) -> UserCommand {
    UserCommand {
        command_id: id.into(),
        expected_revision: m.session().unwrap().revision(),
        order,
    }
}

/// Given a fixture of the user's club (home),
/// When the match starts,
/// Then the user manages the home side, the opponent is the computer's, only formations M12
/// can build are offered, and nothing has been played.
#[test]
fn a_match_starts_with_the_user_on_his_own_side() {
    let m = start(1);
    let v = m.view().unwrap();
    assert_eq!(v.user_side, Some(Side::Home));
    assert!(!v.assistant_active);
    assert_eq!(v.session.clock_ms, 0);
    assert_eq!(
        v.session.home.on_pitch.len() + v.session.away.on_pitch.len(),
        22
    );
    assert!(v.supported_formations.iter().any(|f| f == "4-3-3"));
    assert!(m.command_log().is_empty());
}

/// Given an attacking style ordered by the manager,
/// When the order is sent,
/// Then the engine's mentality is the one the adapter's single conversion gives that style,
/// and the opponent's tactics are untouched.
#[test]
fn a_manager_order_reaches_the_engine_through_the_adapters_mapping() {
    let mut m = start(2);
    let away_before = m.view().unwrap().session.away.tactics;
    let receipt = m
        .submit(order(
            &m,
            "o-1",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Attacking),
                ..Default::default()
            }),
        ))
        .unwrap();
    assert!(receipt.accepted, "{receipt:?}");
    let v = m.view().unwrap();
    let expected =
        crate::spatial_adapter::tactics(&PlayStyle::Attacking, &m.plans[0].phase).mentality;
    assert_eq!(v.session.home.tactics.mentality, expected);
    assert_eq!(v.session.away.tactics, away_before);
    assert_eq!(v.home_plan.style, PlayStyle::Attacking);
}

/// Given an order id,
/// When it is sent twice, or sent against an old revision,
/// Then the retry gets the same answer without a second effect, and the stale one is refused.
#[test]
fn orders_are_idempotent_and_stale_ones_refused() {
    let mut m = start(3);
    let change = || {
        UserOrder::Tactics(TacticalChange {
            style: Some(PlayStyle::Defensive),
            ..Default::default()
        })
    };
    let first = m.submit(order(&m, "o-1", change())).unwrap();
    let log_len = m.command_log().len();
    let again = m
        .submit(UserCommand {
            command_id: "o-1".into(),
            expected_revision: 0,
            order: change(),
        })
        .unwrap();
    assert!(again.duplicate && again.accepted == first.accepted);
    assert_eq!(m.command_log().len(), log_len, "no second effect");

    run_for(&mut m, 2);
    let stale = m
        .submit(UserCommand {
            command_id: "o-2".into(),
            expected_revision: 1,
            order: UserOrder::Tactics(TacticalChange {
                tempo: Some(domain::team::Tempo::Patient),
                ..Default::default()
            }),
        })
        .unwrap();
    assert!(!stale.accepted);
    assert_eq!(stale.rejection.as_deref(), Some("stale_revision"));
}

/// Given a formation the engine cannot build,
/// When it is ordered together with a valid change,
/// Then the whole order is refused and nothing was applied.
#[test]
fn an_order_with_an_unsupported_formation_is_all_or_nothing() {
    let mut m = start(4);
    let before = m.view().unwrap().session.home.tactics;
    let err = m
        .submit(order(
            &m,
            "o-1",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Attacking),
                formation: Some("2-3-5".into()),
                ..Default::default()
            }),
        ))
        .unwrap_err();
    assert!(matches!(err, InteractiveError::UnsupportedFormation));
    assert_eq!(
        err.translation_key(),
        "be.error.interactive.unsupportedFormation"
    );
    assert_eq!(m.view().unwrap().session.home.tactics, before);
}

/// Given a match the player has not handed over,
/// When the assistant is switched on,
/// Then it is refused; with the tactics duty delegated it works and can be taken back.
#[test]
fn the_assistant_needs_the_tactics_duty_to_be_delegated() {
    let mut m = start(5);
    assert!(matches!(
        m.set_assistant(true),
        Err(InteractiveError::AssistantNotDelegated)
    ));
    assert!(!m.view().unwrap().assistant_active);

    let mut delegated = game(5);
    delegated.delegation.set(Duty::Tactics, true, false);
    let mut m = InteractiveMatch::start(&delegated, identity(), "s").unwrap();
    assert!(m.view().unwrap().assistant_available);
    m.set_assistant(true).unwrap();
    assert!(m.view().unwrap().assistant_active);
    m.set_assistant(false).unwrap();
    assert!(!m.view().unwrap().assistant_active);
}

/// Given the assistant in charge of the player's side,
/// When the manager gives a tactical order,
/// Then the assistant stands back from tactics for the stand-back window, and the manager's
/// order is in force (a human decision is never overwritten).
#[test]
fn the_assistant_stands_back_from_what_the_manager_ordered() {
    let mut delegated = game(6);
    delegated.delegation.set(Duty::Tactics, true, false);
    let mut m = InteractiveMatch::start(&delegated, identity(), "s").unwrap();
    m.set_assistant(true).unwrap();
    run_for(&mut m, 5);
    let r = m
        .submit(order(
            &m,
            "o-1",
            UserOrder::Tactics(TacticalChange {
                pressing: Some(domain::team::PressingIntensity::Aggressive),
                ..Default::default()
            }),
        ))
        .unwrap();
    assert!(r.accepted);
    let now = m.session().unwrap().now_ms();
    assert_eq!(
        m.assistant_stand_back_until(Group::Tactics),
        Some(now + ASSISTANT_STAND_BACK_MS)
    );
    assert_eq!(m.assistant_stand_back_until(Group::Substitution), None);
}

/// Given matches played to the end with the computer managing the opponent,
/// When every command its manager sent is read back,
/// Then it only ever commanded its own side, under its own actor, and the engine accepted
/// everything it sent: its substitutions are legal by construction.
#[test]
fn the_opponents_manager_only_commands_its_own_side_and_never_breaks_a_rule() {
    let mut any_substitution = false;
    for seed in 1..=12 {
        let mut m = start(seed);
        run_to_end(&mut m);
        let log = m.command_log();
        for entry in log {
            assert_eq!(
                entry.envelope.actor.id,
                opponent_actor(Side::Away),
                "seed {seed}: only the opponent's manager acts (nobody gave orders)"
            );
            assert_eq!(entry.envelope.actor.side, Side::Away);
            assert!(
                entry.receipt.outcome.is_accepted(),
                "seed {seed}: refused {:?}: {:?}",
                entry.envelope.command,
                entry.receipt.outcome
            );
            any_substitution |= matches!(
                entry.envelope.command,
                MatchCommand::RequestSubstitution { .. }
            );
        }
        let v = m.view().unwrap();
        assert!(v.session.away.substitutions_used <= 5);
        assert_eq!(
            v.session.home.substitutions_used, 0,
            "the player's side was not touched"
        );
    }
    assert!(any_substitution, "the computer manager does substitute");
}

/// Given the same career and the same orders,
/// When the match is played twice, once in one-minute slices and once in a single call,
/// Then the report is the same: how fast a screen asks for state changes nothing.
#[test]
fn rendering_speed_does_not_change_the_match() {
    let play = |step: Option<u32>| {
        let mut m = start(7);
        match step {
            None => run_to_end(&mut m),
            Some(minutes) => loop {
                let out = run_for(&mut m, minutes);
                if out.stop == StopReason::Finished {
                    break;
                }
            },
        }
        as_json(m.complete().unwrap())
    };
    let whole = play(None);
    assert_eq!(play(Some(1)), whole);
    assert_eq!(play(Some(15)), whole);
}

/// Given a match that has not reached full time,
/// When it is completed,
/// Then it is refused: a partial match is never presented as a result; at full time the report
/// is the engine's score, and completing again returns the very same report.
#[test]
fn a_partial_match_is_never_completed() {
    let mut m = start(8);
    run_for(&mut m, 30);
    assert!(matches!(m.complete(), Err(InteractiveError::NotFinished)));
    assert_eq!(
        m.complete().unwrap_err().translation_key(),
        "be.error.interactive.notFinished"
    );
    assert!(!m.is_completed());
    run_to_end(&mut m);
    let score = {
        let v = m.view().unwrap();
        (v.session.score_home, v.session.score_away)
    };
    let report = m.complete().unwrap().clone();
    assert_eq!((report.home_goals, report.away_goals), score);
    assert!(m.is_completed());
    assert_eq!(as_json(m.complete().unwrap()), as_json(&report));
    assert!(m.view().is_none(), "a completed match has no live view");
    assert!(matches!(
        m.advance(AdvanceRequest {
            target: AdvanceTarget::ToEnd,
            stop_at_key_events: false
        }),
        Err(InteractiveError::AlreadyCompleted)
    ));
}

/// Given orders from the manager (a tactical change and a substitution),
/// When the match is saved mid-way, restored, and both continue,
/// Then the restored match gives the same report: the compact state is enough.
#[test]
fn a_saved_match_resumes_exactly() {
    let mut m = start(9);
    run_for(&mut m, 12);
    let off = m.view().unwrap().session.home.on_pitch[4].id.0.clone();
    let on = m
        .view()
        .unwrap()
        .session
        .home
        .bench
        .iter()
        .find(|p| !format!("{:?}", p.natural_position).contains("Gk"))
        .expect("an outfield substitute")
        .id
        .0
        .clone();
    for (id, o) in [
        (
            "o-1",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Possession),
                ..Default::default()
            }),
        ),
        (
            "o-2",
            UserOrder::Substitute {
                player_off_id: off,
                player_on_id: on,
            },
        ),
    ] {
        let r = m.submit(order(&m, id, o)).unwrap();
        assert!(r.accepted, "{r:?}");
    }
    run_for(&mut m, 18);

    let json = serde_json::to_string(&m.checkpoint().unwrap()).unwrap();
    assert!(json.len() < 400_000, "compact: {} bytes", json.len());
    let mut restored = InteractiveMatch::restore(serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(restored.view().unwrap().session, m.view().unwrap().session);

    run_to_end(&mut m);
    run_to_end(&mut restored);
    assert_eq!(
        as_json(restored.complete().unwrap()),
        as_json(m.complete().unwrap())
    );
}

/// Given a user whose club is not in the fixture (nobody to take orders from),
/// When an order is sent,
/// Then it is refused as not his match, and both sides are the computer's.
#[test]
fn a_fixture_without_the_users_club_takes_no_orders() {
    let mut g = game(10);
    g.manager.team_id = Some("elsewhere".into());
    let mut m = InteractiveMatch::start(&g, identity(), "s").unwrap();
    assert_eq!(m.user_side(), None);
    let err = m
        .submit(UserCommand {
            command_id: "o".into(),
            expected_revision: 0,
            order: UserOrder::CancelSubstitution {
                player_off_id: "x".into(),
            },
        })
        .unwrap_err();
    assert!(matches!(err, InteractiveError::NotYourMatch));
    run_to_end(&mut m);
    for entry in m.command_log() {
        assert_eq!(
            entry.envelope.actor.id,
            opponent_actor(entry.envelope.actor.side),
            "each side is commanded only by its own computer manager"
        );
    }
}

/// Given a finished match whose report cannot be built,
/// When it is completed,
/// Then the failure is reported, the match is left intact (it can still be read), and
/// completing again succeeds once the cause is gone: nothing was consumed.
#[test]
fn a_failed_completion_leaves_the_match_intact() {
    let mut m = start(11);
    run_to_end(&mut m);
    let real = m.input.home.id.clone();
    m.input.home.id = apfm_match_lab::ids::TeamId::new("someone-else".to_string());
    assert!(matches!(m.complete(), Err(InteractiveError::Run(_))));
    assert!(m.view().is_some(), "the match is still there");
    assert!(!m.is_completed());
    m.input.home.id = real;
    assert!(m.complete().is_ok());
    assert!(m.is_completed());
}

// ---- the assistant, in a match where it really has something to do

fn delegated(seed: u64) -> Game {
    let mut game = game(seed);
    game.delegation.set(Duty::Tactics, true, false);
    game
}

fn is_tactical(command: &MatchCommand) -> bool {
    matches!(
        command.name(),
        "set_mentality"
            | "set_pressing"
            | "set_tempo"
            | "set_formation"
            | "set_tactical_instructions"
    )
}

/// The times (ms) at which the assistant changed the player's tactics.
fn assistant_tactical_times(m: &InteractiveMatch) -> Vec<u32> {
    m.command_log()
        .iter()
        .filter(|e| e.envelope.actor.id == ASSISTANT_ACTOR && is_tactical(&e.envelope.command))
        .map(|e| e.receipt.at_ms)
        .collect()
}

fn played_with_assistant(seed: u64, order_at_minute: Option<u32>) -> InteractiveMatch {
    let mut m = InteractiveMatch::start(&delegated(seed), identity(), "s").unwrap();
    m.set_assistant(true).unwrap();
    if let Some(minute) = order_at_minute {
        m.advance(AdvanceRequest {
            target: AdvanceTarget::ToMinute(minute),
            stop_at_key_events: false,
        })
        .unwrap();
        let current = m.view().unwrap().home_plan.pressing;
        let other = if current == domain::team::PressingIntensity::Aggressive {
            domain::team::PressingIntensity::Passive
        } else {
            domain::team::PressingIntensity::Aggressive
        };
        let r = m
            .submit(order(
                &m,
                "manager-order",
                UserOrder::Tactics(TacticalChange {
                    pressing: Some(other),
                    ..Default::default()
                }),
            ))
            .unwrap();
        assert!(r.accepted, "{r:?}");
    }
    run_to_end(&mut m);
    m
}

/// Given the assistant in charge of the player's side over many matches,
/// When the commands it sent are read back,
/// Then it really acts (in at least one match), only ever for the player's side, and the
/// engine accepts everything it sends; with the assistant off it sends nothing at all.
#[test]
fn the_assistant_acts_for_the_players_side_and_only_when_switched_on() {
    let mut acted = false;
    for seed in 1..=30 {
        let m = played_with_assistant(seed, None);
        for entry in m
            .command_log()
            .iter()
            .filter(|e| e.envelope.actor.id == ASSISTANT_ACTOR)
        {
            acted = true;
            assert_eq!(entry.envelope.actor.side, Side::Home, "seed {seed}");
            assert!(
                entry.receipt.outcome.is_accepted(),
                "seed {seed}: {entry:?}"
            );
        }
    }
    assert!(acted, "in 30 matches the assistant never did anything");

    for seed in 1..=6 {
        let mut m = InteractiveMatch::start(&delegated(seed), identity(), "s").unwrap();
        run_to_end(&mut m);
        assert!(
            m.command_log()
                .iter()
                .all(|e| e.envelope.actor.id != ASSISTANT_ACTOR),
            "seed {seed}: the assistant is off, so it sends nothing"
        );
    }
}

/// Given a match in which the assistant changes the player's tactics,
/// When the same match is replayed with a manager's tactical order a few minutes before that
/// change,
/// Then the assistant sends no tactical command for the next 15 simulated minutes: a human
/// order is never overwritten, and the window ends.
#[test]
fn the_assistant_does_not_overwrite_a_manager_order_inside_the_window() {
    let (seed, change_at) = (1..=40)
        .find_map(|seed| {
            let times = assistant_tactical_times(&played_with_assistant(seed, None));
            times
                .into_iter()
                .find(|t| *t >= 20 * 60_000)
                .map(|t| (seed, t))
        })
        .expect("a match in which the assistant changes tactics late enough to test");

    let order_minute = change_at / 60_000 - 3;
    let m = played_with_assistant(seed, Some(order_minute));
    let order_ms = m
        .command_log()
        .iter()
        .find(|e| e.envelope.actor.id == MANAGER_ACTOR)
        .map(|e| e.receipt.at_ms)
        .expect("the manager's order is in the log");
    let inside = assistant_tactical_times(&m)
        .into_iter()
        .filter(|t| *t >= order_ms && *t < order_ms + ASSISTANT_STAND_BACK_MS)
        .count();
    assert_eq!(
        inside, 0,
        "seed {seed}: the assistant changed tactics inside the stand-back window"
    );
}

// ------------------------------------------------------------------ saved inside the career

use super::saved::{self, RestoreProblem, RoundBaselines};

fn baselines() -> RoundBaselines {
    RoundBaselines {
        round_matchday: 4,
        round_previous_standings: Vec::new(),
        league_round_context: Some(crate::matchday::LeagueRoundContext {
            date: "2026-08-22".into(),
            matchday: 4,
            previous_standings: Vec::new(),
        }),
    }
}

/// A match 25 minutes in with a tactical order and a substitution accepted.
fn played_a_while(seed: u64) -> InteractiveMatch {
    let mut m = start(seed);
    let (off, on) = {
        let v = m.view().unwrap();
        (
            v.session
                .home
                .on_pitch
                .iter()
                .find(|p| p.role == Some(apfm_match_lab::model::Position::Cm))
                .unwrap()
                .id
                .0
                .clone(),
            v.session
                .home
                .bench
                .iter()
                .find(|p| p.natural_position != apfm_match_lab::model::Position::Gk)
                .unwrap()
                .id
                .0
                .clone(),
        )
    };
    for (id, o) in [
        (
            "s-1",
            UserOrder::Tactics(TacticalChange {
                style: Some(PlayStyle::Attacking),
                ..Default::default()
            }),
        ),
        (
            "s-2",
            UserOrder::Substitute {
                player_off_id: off,
                player_on_id: on,
            },
        ),
    ] {
        assert!(m.submit(order(&m, id, o)).unwrap().accepted);
    }
    run_for(&mut m, 25);
    m
}

/// Given a match in progress with accepted orders,
/// When it is stored, read back and both are played to full time,
/// Then the restored match is the same match: same view now, same report at the end, and the
/// baselines are exactly the ones saved.
#[test]
fn a_saved_match_resumes_as_the_same_match() {
    let mut m = played_a_while(21);
    let text = saved::encode(&m, baselines()).unwrap();
    let (back, mut restored) = saved::decode(&text).unwrap();
    assert_eq!(back, baselines());
    assert_eq!(restored.view().unwrap().session, m.view().unwrap().session);
    assert_eq!(restored.identity(), m.identity());
    run_to_end(&mut m);
    run_to_end(&mut restored);
    assert_eq!(
        as_json(restored.complete().unwrap()),
        as_json(m.complete().unwrap())
    );
}

/// Given a stored match,
/// When it is restored twice,
/// Then both resumptions play out identically (restoring is deterministic, not merely possible).
#[test]
fn restoring_twice_gives_the_same_match() {
    let text = saved::encode(&played_a_while(22), baselines()).unwrap();
    let (_, mut a) = saved::decode(&text).unwrap();
    let (_, mut b) = saved::decode(&text).unwrap();
    run_to_end(&mut a);
    run_to_end(&mut b);
    assert_eq!(
        as_json(a.complete().unwrap()),
        as_json(b.complete().unwrap())
    );
}

/// Given a stored match that is saved again after being resumed and played on,
/// Then no accepted order is lost or applied twice: the order ledger still refuses a repeated id.
#[test]
fn a_resumed_match_does_not_apply_an_old_order_twice() {
    let m = played_a_while(23);
    let text = saved::encode(&m, baselines()).unwrap();
    let (_, mut restored) = saved::decode(&text).unwrap();
    let before = restored.command_log().len();
    let again = restored.submit(order(
        &restored,
        "s-1",
        UserOrder::Tactics(TacticalChange {
            style: Some(PlayStyle::Defensive),
            ..Default::default()
        }),
    ));
    // a repeated id answers with the earlier receipt (a retry is harmless) and applies nothing new
    assert!(again.is_ok());
    assert_eq!(restored.command_log().len(), before);
}

/// Given text that is not a stored match,
/// Then it is unreadable, and nothing panics.
#[test]
fn text_that_is_not_a_match_is_unreadable() {
    for text in ["", "not json", "{}", "[]", "{\"format_version\":1}"] {
        assert_eq!(
            saved::decode(text).err(),
            Some(RestoreProblem::Unreadable),
            "{text:?}"
        );
    }
}

/// Given a stored match from another envelope version,
/// Then it is reported as incompatible with that version and not misread.
#[test]
fn another_format_version_is_incompatible() {
    let text = saved::encode(&played_a_while(24), baselines()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["format_version"] = 99.into();
    assert_eq!(
        saved::decode(&v.to_string()).err(),
        Some(RestoreProblem::Incompatible(99))
    );
}

/// Given a checkpoint whose recorded world does not match what replaying it produces (a different
/// engine, or a damaged journal),
/// Then it is refused as unusable rather than resumed as a different match.
#[test]
fn a_checkpoint_that_does_not_replay_to_its_world_is_unusable() {
    let text = saved::encode(&played_a_while(25), baselines()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["checkpoint"]["session"]["world_digest"] = "0000000000000000".into();
    assert_eq!(
        saved::decode(&v.to_string()).err(),
        Some(RestoreProblem::Unusable)
    );
}

/// Given a finished match whose report has been built,
/// Then it cannot be stored: only a match that can still be played is checkpointed.
#[test]
fn a_completed_match_cannot_be_stored() {
    let mut m = start(26);
    run_to_end(&mut m);
    m.complete().unwrap();
    assert!(saved::encode(&m, baselines()).is_err());
}

/// Given matches played to full time with no orders, including seeds that contain a shot off the
/// woodwork (21) and an own goal (49, 56, 87),
/// When each is completed,
/// Then every one produces a report whose score matches the engine's: before Run F4.1 about one match
/// in four could not be finalised.
#[test]
fn every_match_that_reaches_full_time_can_be_finalised() {
    for seed in (1..=16).chain([21, 49, 56, 87]) {
        let mut m = start(seed);
        run_to_end(&mut m);
        let report = m
            .complete()
            .unwrap_or_else(|e| panic!("seed {seed} cannot be finalised: {e:?}"));
        let goals_listed = report.goals.len() as u8;
        assert!(
            goals_listed <= report.home_goals + report.away_goals,
            "seed {seed}: scorers never exceed the score"
        );
    }
}

/// Given full matches of OFM-adapted players with balanced tactics,
/// When each starter's measured load is read,
/// Then every line has starters and every one of them has a measured, finite load.
///
/// (Until Run F4.2 this test also required the mean ratio to the neutral workload to stay near 1 per
/// line. That held only for these synthetic players, whose attributes are nearly uniform; the neutral
/// workloads are now per line and come from real club pairs, so the drift check is
/// `fm24_workload_reference_study`.)
#[test]
fn every_starter_of_a_match_has_a_measured_load() {
    let mut per_line = [0u32; 3];
    for seed in 1..=8 {
        let g = game(seed);
        let mut m = InteractiveMatch::start(&g, identity(), "calibration").unwrap();
        run_to_end(&mut m);
        let report = m.complete().unwrap().clone();
        for (id, stats) in &report.player_stats {
            if stats.minutes_played < 85 {
                continue;
            }
            let Some(player) = g.players.iter().find(|p| &p.id == id) else {
                continue;
            };
            let load = stats
                .physical_load
                .expect("a player who played has a measured load");
            assert!(load.is_finite() && load >= 0.0);
            match player.position.to_group_position() {
                domain::player::Position::Defender => per_line[0] += 1,
                domain::player::Position::Midfielder => per_line[1] += 1,
                domain::player::Position::Forward => per_line[2] += 1,
                _ => {}
            }
        }
    }
    assert!(
        per_line.iter().all(|n| *n > 0),
        "every line measured: {per_line:?}"
    );
}

/// Given a finished match,
/// Then every player who played has a measured load and a player who did not play has none to
/// speak of: the charge is attached to the report, not guessed later.
#[test]
fn the_report_carries_the_load_of_everyone_who_played() {
    let mut m = start(31);
    run_to_end(&mut m);
    let report = m.complete().unwrap();
    let played: Vec<_> = report
        .player_stats
        .values()
        .filter(|s| s.minutes_played > 0)
        .collect();
    assert!(played.len() >= 22);
    assert!(played.iter().all(|s| s.physical_load.is_some()));
}

/// Given a finished match in which the manager made a substitution,
/// When the career's record is read,
/// Then it is internally consistent: the swapped players' minutes add up to a match, nobody is
/// credited with more goals than the score allows, every credited scorer has the goal in his own
/// statistics, and the cards in the timeline are the cards in the players' statistics.
#[test]
fn the_match_record_is_complete_and_consistent() {
    let mut m = played_a_while(40);
    run_to_end(&mut m);
    let report = m.complete().unwrap().clone();
    let swap = report
        .events
        .iter()
        .find(|e| e.event_type == engine::EventType::Substitution && e.side == engine::Side::Home)
        .expect("the substitution is in the record");
    let pair = [
        swap.player_id
            .clone()
            .expect("a player on the substitution"),
        swap.secondary_player_id.clone().expect("and the other"),
    ];
    let minutes = |id: &str| report.player_stats[id].minutes_played;
    // the match lasts as long as the players who never left (stoppage time included)
    let match_length = report
        .player_stats
        .values()
        .map(|s| s.minutes_played)
        .max()
        .unwrap();
    let [first, second] = [minutes(&pair[0]), minutes(&pair[1])];
    assert!(
        first > 0 && second > 0,
        "both of them played: {first}, {second}"
    );
    assert!(
        (i32::from(first) + i32::from(second) - i32::from(match_length)).abs() <= 2,
        "the two share one place for the whole match: {first} + {second} vs {match_length}"
    );
    assert!(first.min(second) < match_length);

    assert!(report.goals.len() as u8 <= report.home_goals + report.away_goals);
    for goal in &report.goals {
        assert!(
            report.player_stats[&goal.scorer_id].goals >= 1,
            "{} is credited in the list but not in his own record",
            goal.scorer_id
        );
    }

    let red_in_timeline = report
        .events
        .iter()
        .filter(|e| e.event_type == engine::EventType::RedCard)
        .count() as u8;
    let red_in_stats: u8 = report.player_stats.values().map(|s| s.red_cards).sum();
    assert_eq!(red_in_timeline, red_in_stats);
    assert!(
        report
            .player_stats
            .values()
            .filter(|s| s.minutes_played > 0)
            .count()
            >= 22,
        "everyone who played has a record"
    );
}

// ---------------------------------------------------------------------------------------------
// Run F4.2: the neutral workload re-measured on FM24-derived players
// ---------------------------------------------------------------------------------------------

/// Matches the engine's expenditure of the aerobic reserve per minute (raw, before the stamina
/// term) of every starter who played at least 85 minutes, in matches between real clubs of the
/// FM24 world, by line, stamina band and pressing intensity.
///
/// Needs the world file the owner builds with `apfm-import` (`APFM_WORLD_JSON`); read-only, nothing
/// is written. Run: `APFM_WORLD_JSON=… cargo test --release -p ofm_core --lib fm24_workload -- --ignored --nocapture`.
#[test]
#[ignore = "study: needs the FM24 world file (APFM_WORLD_JSON); minutes"]
fn fm24_workload_reference_study() {
    use crate::career::{CareerScope, begin_career};
    use crate::clock::GameClock;
    use crate::generator::{ReputationBasis, load_world_from_path};
    use crate::world::build::{PastHistory, build_game_from_imported_world};
    use crate::world::start_date_for_year;
    use domain::manager::Manager;
    use domain::team::PressingIntensity;

    let Some(path) = std::env::var_os("APFM_WORLD_JSON").map(std::path::PathBuf::from) else {
        println!("APFM_WORLD_JSON is not set: nothing measured");
        return;
    };
    let mut world = load_world_from_path(&path).expect("the FM24 world loads");
    world.generation_seed = Some(11);
    let clock = GameClock::new(start_date_for_year(2026).unwrap());
    let history = PastHistory {
        start_year: 2026,
        depth_years: 0,
    };
    let manager = Manager::new(
        "mgr_user".into(),
        "Scenario".into(),
        "Manager".into(),
        "1980-01-01".into(),
        "France".into(),
    );
    let (mut base, stats) = build_game_from_imported_world(
        clock,
        manager,
        history,
        world,
        ReputationBasis::Authored(&Default::default()),
    );
    let club = base.teams.first().expect("a club").id.clone();
    begin_career(&mut base, &club, CareerScope::default(), stats).expect("a career");

    // distinct fixtures of one competition: different pairs of real clubs
    let competition = base
        .competitions
        .iter()
        .find(|c| c.fixtures.len() >= 40)
        .expect("a league with fixtures");
    let picks: Vec<(usize, String, String)> = competition
        .fixtures
        .iter()
        .enumerate()
        .take(40)
        .map(|(i, f)| (i, competition.id.clone(), f.id.clone()))
        .collect();

    let (mut started, mut refused) = (0, 0);
    // load ratio to the line's neutral, default pressing: (sum, n) per line
    let mut ratios = [(0.0_f64, 0u32); 3];
    // pressing -> (matches, fouls, cautions, dismissals, of which second yellows)
    let mut discipline =
        std::collections::BTreeMap::<&'static str, (u32, u32, u32, u32, u32)>::new();
    // [line][stamina band][pressing] -> (sum of raw rate, n)
    let mut cell = std::collections::BTreeMap::<(usize, usize, &'static str), (f64, u32)>::new();
    for (pressing, name) in [
        (None, "default"),
        (Some(PressingIntensity::Passive), "passive"),
        (Some(PressingIntensity::Aggressive), "aggressive"),
    ] {
        for (index, competition_id, fixture_id) in &picks {
            let mut g = base.clone();
            if let Some(p) = &pressing {
                let (home, away) = {
                    let f = g
                        .competitions
                        .iter()
                        .find(|c| &c.id == competition_id)
                        .map(|c| &c.fixtures[*index])
                        .expect("the picked fixture");
                    (f.home_team_id.clone(), f.away_team_id.clone())
                };
                for t in g.teams.iter_mut().filter(|t| t.id == home || t.id == away) {
                    t.tactics_phase.pressing_intensity = p.clone();
                }
            }
            let id = FixtureIdentity {
                fixture_index: *index,
                competition_id: competition_id.clone(),
                fixture_id: fixture_id.clone(),
            };
            let mut m = match InteractiveMatch::start(&g, id, "fm24-workload") {
                Ok(m) => m,
                Err(error) => {
                    refused += 1;
                    if refused == 1 {
                        println!("first refusal: {error:?}");
                    }
                    continue;
                }
            };
            started += 1;
            run_to_end(&mut m);
            let report = m.complete().unwrap().clone();
            // discipline of this match (both teams)
            let d = discipline
                .entry(name)
                .or_insert((0u32, 0u32, 0u32, 0u32, 0u32));
            d.0 += 1;
            for ps in report.player_stats.values() {
                d.1 += u32::from(ps.fouls_committed);
                d.2 += u32::from(ps.yellow_cards);
                d.3 += u32::from(ps.red_cards);
            }
            d.4 += report
                .events
                .iter()
                .filter(|e| e.event_type == engine::EventType::SecondYellow)
                .count() as u32;
            for (pid, stats) in &report.player_stats {
                if stats.minutes_played < 85 {
                    continue;
                }
                let Some(p) = g.players.iter().find(|p| &p.id == pid) else {
                    continue;
                };
                let line = match p.position.to_group_position() {
                    domain::player::Position::Defender => 0,
                    domain::player::Position::Midfielder => 1,
                    domain::player::Position::Forward => 2,
                    _ => continue,
                };
                let load = stats.physical_load.expect("a load for a player who played");
                let band = usize::from(p.attributes.stamina >= 55)
                    + usize::from(p.attributes.stamina >= 70);
                let stamina_effect = 1.0 - 0.0125 * (f64::from(p.attributes.stamina) - 64.0);
                let raw = f64::from(load) / (f64::from(stats.minutes_played) * stamina_effect);
                let e = cell.entry((line, band, name)).or_insert((0.0, 0));
                e.0 += raw;
                e.1 += 1;
                if name == "default" {
                    ratios[line].0 += crate::player_wear::load_ratio(
                        &p.position,
                        p.attributes.stamina,
                        stats.minutes_played,
                        load,
                    );
                    ratios[line].1 += 1;
                }
            }
        }
    }
    println!("matches started {started}, refused {refused}");
    for (name, (n, fouls, yellows, reds, second)) in &discipline {
        let n = f64::from(*n);
        println!(
            "discipline ({name}): fouls {:.1}, cautions {:.2}, dismissals {:.3} (second yellows {:.3}) per match over {n}",
            f64::from(*fouls) / n,
            f64::from(*yellows) / n,
            f64::from(*reds) / n,
            f64::from(*second) / n,
        );
    }
    println!("reference: 0.0025 of the reserve per minute at stamina 64");
    println!(
        "line (0 def, 1 mid, 2 att) | stamina band (0 <55, 1 55-69, 2 >=70) | pressing | mean raw rate | n | ratio to reference"
    );
    for ((line, band, name), (sum, n)) in &cell {
        if *n > 0 {
            let mean = sum / f64::from(*n);
            println!(
                "{line} | {band} | {name} | {mean:.5} | {n} | {:.2}",
                mean / 0.0025
            );
        }
    }
    for (line, (sum, n)) in ratios.iter().enumerate() {
        assert!(*n > 0, "line {line} measured");
        let mean = sum / f64::from(*n);
        println!("line {line}: mean load ratio to its neutral {mean:.3} over {n} starters");
        assert!(
            (0.85..=1.2).contains(&mean),
            "line {line}: mean load ratio {mean:.3} is far from neutral; re-measure player_wear::reference_load_per_minute"
        );
    }
}
