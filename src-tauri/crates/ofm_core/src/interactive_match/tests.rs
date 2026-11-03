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
    let on = m.view().unwrap().session.home.bench[1].id.0.clone();
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
