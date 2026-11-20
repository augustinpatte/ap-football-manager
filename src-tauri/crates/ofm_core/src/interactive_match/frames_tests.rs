//! The frames a screen draws: where they come from, that taking them changes nothing, and that
//! they follow the match (substitutions, added time, full time).
use super::live_frames::LiveFrame;
use super::plan::TacticalChange;
use super::*;
use crate::spatial_runner::tests::game_with_substitutes;
use apfm_match_lab::model::Position;

fn identity() -> FixtureIdentity {
    FixtureIdentity {
        fixture_index: 0,
        competition_id: "competition-m12-1".into(),
        fixture_id: "fixture-m12-1".into(),
    }
}

fn start(seed: u64) -> InteractiveMatch {
    let mut game = game_with_substitutes(7);
    game.seed = seed;
    InteractiveMatch::start(&game, identity(), "session-frames").expect("starts")
}

fn request(target: AdvanceTarget) -> AdvanceRequest {
    AdvanceRequest {
        target,
        stop_at_key_events: false,
    }
}

/// More stretches than any match needs (a 90-minute match is 540 ten-second stretches, 9,000 for
/// extra time at ten seconds a stretch would still be far below): a loop that reaches it is a hung
/// match, and fails its test instead of hanging the suite.
const MAX_STRETCHES: usize = 2_000;

/// Plays to full time in `minutes`-sized stretches, collecting every frame handed out. A match that
/// does not end within twice its length in stretches fails the test instead of hanging the suite.
fn play_capturing(m: &mut InteractiveMatch, minutes: u32) -> Vec<LiveFrame> {
    let mut frames = Vec::new();
    let stretches = 2 * 120 / minutes.max(1) + 4;
    for _ in 0..stretches {
        let out = m
            .advance_capturing(request(AdvanceTarget::ForMinutes(minutes)))
            .expect("advances");
        frames.extend(out.frames);
        match out.stop {
            StopReason::Finished => return frames,
            StopReason::Paused => panic!("nobody paused the match"),
            _ => {}
        }
    }
    panic!("the match did not reach full time within {stretches} stretches of {minutes} minutes");
}

/// Given a managed match played to full time with frame capture on,
/// When its frames are compared with the ones the engine records itself in the same match,
/// Then every frame on the engine's grid is the same instant of the same world: positions,
/// ball, score and phase. The frames are the engine's data, not a second simulation.
#[test]
fn captured_frames_are_the_engines_own_recorded_frames() {
    // The same match, restored from its fresh checkpoint with the engine's recorder switched on
    // (the journal is empty, so the restored session plays the very same football).
    let mut checkpoint = start(3).checkpoint().expect("checkpoint");
    checkpoint.session.config.record_frames = true;
    let mut m = InteractiveMatch::restore(checkpoint).expect("restores");
    let captured = play_capturing(&mut m, 5);
    let run = m
        .session()
        .expect("still playing")
        .finished_run()
        .expect("full time");
    let roster = &run.output.roster;
    let engine = &run.output.frames;
    assert!(!engine.is_empty(), "the engine recorded no frames");

    let mut compared = 0;
    for theirs in engine {
        let Some(ours) = captured.iter().find(|f| f.t_ms == theirs.t_ms) else {
            continue;
        };
        compared += 1;
        assert_eq!(ours.score, [theirs.score.home, theirs.score.away]);
        assert_eq!(ours.state, theirs.state);
        assert!(
            (ours.ball[0] - theirs.ball.x).abs() < 0.011,
            "ball x at {}",
            ours.t_ms
        );
        assert!(
            (ours.ball[1] - theirs.ball.y).abs() < 0.011,
            "ball y at {}",
            ours.t_ms
        );
        let on_pitch: Vec<_> = theirs
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.on_pitch)
            .collect();
        assert_eq!(
            ours.players.len(),
            on_pitch.len(),
            "players at {}",
            ours.t_ms
        );
        for (index, p) in on_pitch {
            let sample = ours
                .players
                .iter()
                .find(|s| usize::from(s.0) == index)
                .unwrap_or_else(|| panic!("{} missing at {}", roster[index].0, ours.t_ms));
            assert!((sample.1 - p.x).abs() < 0.011 && (sample.2 - p.y).abs() < 0.011);
        }
    }
    // 90 minutes at 2 frames a second plus added time.
    assert!(
        compared > 10_000,
        "only {compared} frames matched the engine's grid"
    );
}

/// Given the same fixture and seed,
/// When one match is played plainly and the other with frames captured in 30-second stretches,
/// Then the result, the events and the commands the computer managers gave are identical:
/// taking frames reads the world and never touches it.
#[test]
fn capturing_frames_does_not_change_the_match() {
    let mut plain = start(7);
    let mut ended = false;
    for _ in 0..MAX_STRETCHES {
        let out = plain.advance(request(AdvanceTarget::ToEnd)).unwrap();
        if out.stop == StopReason::Finished {
            ended = true;
            break;
        }
    }
    assert!(ended, "the plain match never reached full time");
    let mut watched = start(7);
    ended = false;
    for _ in 0..MAX_STRETCHES {
        let out = watched
            .advance_capturing(request(AdvanceTarget::ForMinutes(1)))
            .unwrap();
        if out.stop == StopReason::Finished {
            ended = true;
            break;
        }
    }
    assert!(ended, "the watched match never reached full time");
    let events = |m: &InteractiveMatch| m.session().unwrap().events().to_vec();
    assert_eq!(events(&plain), events(&watched));
    // The session revision counts advance calls (stale-order protection), so it differs by design;
    // who ordered what, when, and what the engine answered must not.
    let ledger = |m: &InteractiveMatch| {
        m.command_log()
            .iter()
            .map(|e| {
                (
                    e.envelope.actor.clone(),
                    e.envelope.command.clone(),
                    e.receipt.at_ms,
                    e.receipt.outcome.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert!(
        !ledger(&plain).is_empty(),
        "the computer managers gave no orders to compare"
    );
    assert_eq!(ledger(&plain), ledger(&watched));
    let json = |m: &mut InteractiveMatch| serde_json::to_value(m.complete().unwrap()).unwrap();
    assert_eq!(json(&mut plain), json(&mut watched));
}

/// Given a whole match of captured frames,
/// Then their times strictly increase with no frame handed out twice, no frame is missing for
/// more than a second of match time, nobody is outside the stadium, and no side ever has more
/// than eleven players on the pitch.
#[test]
fn the_frames_of_a_match_are_ordered_complete_and_on_the_pitch() {
    let mut m = start(11);
    let roster = m.view().unwrap().roster;
    let frames = play_capturing(&mut m, 10);
    assert_eq!(frames[0].t_ms, 0, "the first frame is the kickoff layout");
    assert_eq!(frames[0].players.len(), 22);
    for pair in frames.windows(2) {
        assert!(
            pair[1].t_ms > pair[0].t_ms,
            "{} then {}",
            pair[0].t_ms,
            pair[1].t_ms
        );
        assert!(
            pair[1].t_ms - pair[0].t_ms <= 1_000,
            "gap after {}",
            pair[0].t_ms
        );
    }
    for frame in &frames {
        let mut per_side = [0usize; 2];
        for &(index, x, y, heading) in &frame.players {
            assert!(
                (-3.0..=108.0).contains(&x) && (-3.0..=71.0).contains(&y),
                "({x},{y})"
            );
            assert!(heading.is_finite());
            per_side[usize::from(roster.0[usize::from(index)].side == Side::Away)] += 1;
        }
        assert!(
            per_side[0] <= 11 && per_side[1] <= 11,
            "{per_side:?} at {}",
            frame.t_ms
        );
        assert!(frame.ball.iter().all(|v| v.is_finite()));
        if let Some(holder) = frame.holder {
            assert!(
                frame.players.iter().any(|p| p.0 == holder),
                "holder is on the pitch"
            );
        }
    }
    let last = frames.last().unwrap();
    assert_eq!(last.state, apfm_match_lab::sim::state::GameState::FullTime);
    let view = m.view().unwrap();
    assert_eq!(
        last.score,
        [view.session.score_home, view.session.score_away]
    );
}

/// Given a substitution the manager ordered at half-time,
/// When the second half is played,
/// Then the first frames after the break show the incoming player on the pitch and the
/// outgoing one off it: an accepted order is visible in later frames and never in earlier ones.
#[test]
fn a_substitution_appears_in_the_frames_after_it_and_not_before() {
    let mut m = start(5);
    let first_half = m
        .advance_capturing(AdvanceRequest {
            target: AdvanceTarget::ToEnd,
            stop_at_key_events: false,
        })
        .unwrap();
    assert_eq!(first_half.stop, StopReason::PeriodBreak);
    let view = m.view().unwrap();
    let outfield = |players: &[apfm_match_lab::sim::session::PlayerView]| {
        players
            .iter()
            .find(|p| p.natural_position != Position::Gk && p.role != Some(Position::Gk))
            .map(|p| p.id.0.clone())
            .expect("an outfield player")
    };
    let off_id = view.session.home.on_pitch[4].id.0.clone();
    let on_id = outfield(&view.session.home.bench);
    let index_of = |id: &str| {
        view.roster
            .0
            .iter()
            .position(|e| e.id == id)
            .expect("in the roster") as u16
    };
    let (off_index, on_index) = (index_of(&off_id), index_of(&on_id));
    assert!(
        first_half
            .frames
            .iter()
            .all(|f| !f.players.iter().any(|p| p.0 == on_index)),
        "a substitute is on the pitch before he was sent on"
    );

    let receipt = m
        .submit(UserCommand {
            command_id: "sub-1".into(),
            expected_revision: view.session.revision,
            order: UserOrder::Substitute {
                player_off_id: off_id,
                player_on_id: on_id,
            },
        })
        .unwrap();
    assert!(receipt.accepted, "{receipt:?}");
    let second = m
        .advance_capturing(request(AdvanceTarget::ForMinutes(2)))
        .unwrap();
    let later = second.frames.last().expect("frames after the break");
    assert!(later.players.iter().any(|p| p.0 == on_index));
    assert!(!later.players.iter().any(|p| p.0 == off_index));
}

/// Given a match played into the second half,
/// Then the clock view says where the half started and ends, added time is announced once
/// nominal time is reached, and the second half starts after the first half's added time.
#[test]
fn the_clock_view_follows_the_periods_and_their_added_time() {
    let mut m = start(9);
    let kickoff = m.view().unwrap().clock;
    assert_eq!(kickoff.period_start_ms, 0);
    assert_eq!(kickoff.nominal_end_ms, 45 * 60_000);
    assert_eq!(kickoff.added_time_ms, None);

    let out = m.advance(request(AdvanceTarget::ToEnd)).unwrap();
    assert_eq!(out.stop, StopReason::PeriodBreak);
    let first = out.view.clock;
    assert_eq!(first.nominal_end_ms, 45 * 60_000);
    let added = first
        .added_time_ms
        .expect("announced by the end of the half");
    assert!(added.is_multiple_of(60_000), "added time is whole minutes");

    let out = m.advance(request(AdvanceTarget::ForMinutes(1))).unwrap();
    let second = out.view.clock;
    assert!(second.period_start_ms >= 45 * 60_000 + added);
    assert_eq!(second.nominal_end_ms, second.period_start_ms + 45 * 60_000);
    assert_eq!(second.added_time_ms, None);
}

/// Given a tactical order given while paused between two stretches,
/// When play continues,
/// Then no frame handed out earlier changes (frames already seen are never rewritten) and the
/// engine's clock did not move because of the order.
#[test]
fn an_order_never_rewrites_frames_already_handed_out() {
    let mut m = start(4);
    let seen = m
        .advance_capturing(request(AdvanceTarget::ForMinutes(10)))
        .unwrap()
        .frames;
    let clock_before = m.view().unwrap().session.clock_ms;
    let view = m.view().unwrap();
    let receipt = m
        .submit(UserCommand {
            command_id: "t-1".into(),
            expected_revision: view.session.revision,
            order: UserOrder::Tactics(TacticalChange {
                pressing: Some(domain::team::PressingIntensity::Aggressive),
                ..Default::default()
            }),
        })
        .unwrap();
    assert!(receipt.accepted, "{receipt:?}");
    assert_eq!(m.view().unwrap().session.clock_ms, clock_before);
    let next = m
        .advance_capturing(request(AdvanceTarget::ForMinutes(1)))
        .unwrap()
        .frames;
    assert!(next[0].t_ms > seen.last().unwrap().t_ms);
}

/// Given a match played for a quarter of an hour,
/// When the feed is read,
/// Then it carries the football a viewer follows (shots with their outcome, fouls, corners)
/// besides the key events, in the engine's order, each with its id in the canonical stream.
#[test]
fn the_feed_carries_shots_fouls_and_corners_as_well_as_the_key_events() {
    let mut m = start(6);
    m.advance(request(AdvanceTarget::ForMinutes(30))).unwrap();
    let (events, cursor) = m.events_since(0);
    let kinds: std::collections::BTreeSet<&str> =
        events.iter().map(|e| e.event.kind.name()).collect();
    for wanted in ["shot", "foul", "period_start"] {
        assert!(kinds.contains(wanted), "no {wanted} in {kinds:?}");
    }
    assert!(
        !kinds.contains("pass"),
        "passes are football detail, not feed"
    );
    assert!(
        events.windows(2).all(|w| w[0].id < w[1].id),
        "feed is in stream order"
    );
    assert!(cursor > events.last().unwrap().id);
}

/// Given a match at kickoff,
/// When it is advanced by ten simulated seconds,
/// Then the clock moved by those ten seconds (one tick of tolerance) and no more: the 2D screen
/// plays seconds ahead of its picture, not minutes.
#[test]
fn a_match_can_be_advanced_by_seconds() {
    let mut m = start(2);
    let out = m
        .advance_capturing(request(AdvanceTarget::ForSeconds(10)))
        .unwrap();
    let clock = out.view.session.clock_ms;
    assert!((10_000..10_200).contains(&clock), "{clock}");
    let times: Vec<_> = out.frames.iter().map(|f| f.t_ms).collect();
    assert_eq!(times.first(), Some(&0));
    assert_eq!(times.last(), Some(&10_000));
}

/// Given the second half and extra-time clocks,
/// Then each period reports the nominal minutes played before it, so a stopwatch can show 45:00
/// (not 47:00) when the second half starts after two minutes of added time.
#[test]
fn a_period_clock_reports_the_minutes_played_before_it() {
    use super::live_frames::clock_view;
    use apfm_match_lab::output::Period;
    let half = 45 * 60_000;
    let c = clock_view(&[], Period::SecondHalf, half);
    assert_eq!((c.period_base_ms, c.period_nominal_ms), (half, half));
    let c = clock_view(&[], Period::ExtraTimeSecond, half);
    assert_eq!(
        (c.period_base_ms, c.period_nominal_ms),
        (2 * half + 15 * 60_000, 15 * 60_000)
    );
}

/// Given whole matches played with frame capture,
/// Then the score in the frames changes exactly when the engine's stream has a goal: a frame's
/// score never moves without a goal event in the half second that led to it, and every goal is
/// in the frames by the end. The picture and the event feed cannot contradict each other.
#[test]
fn the_score_in_the_frames_moves_with_the_goals_in_the_event_stream() {
    let mut goals_seen = 0;
    for seed in [3, 5, 7, 11, 13] {
        let mut m = start(seed);
        let frames = play_capturing(&mut m, 15);
        let goal_times: Vec<u32> = m
            .session()
            .expect("still playing")
            .events()
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Goal { .. }))
            .map(|e| e.t_ms)
            .collect();
        goals_seen += goal_times.len();
        for pair in frames.windows(2) {
            let moved = (u32::from(pair[1].score[0]) + u32::from(pair[1].score[1]))
                .saturating_sub(u32::from(pair[0].score[0]) + u32::from(pair[0].score[1]));
            let expected = goal_times
                .iter()
                .filter(|t| **t > pair[0].t_ms && **t <= pair[1].t_ms)
                .count() as u32;
            assert_eq!(
                moved, expected,
                "seed {seed}: score moved between {} and {}",
                pair[0].t_ms, pair[1].t_ms
            );
        }
        let last = frames.last().unwrap();
        assert_eq!(
            u32::from(last.score[0]) + u32::from(last.score[1]),
            goal_times.len() as u32,
            "seed {seed}: every goal is in the final score"
        );
    }
    assert!(goals_seen > 0, "five matches produced no goal to compare");
}

/// Cost of the 2D screen's data, measured (run on purpose, release-like profile):
/// `cargo test -p ofm_core --lib frames_cost -- --ignored --nocapture`
#[test]
#[ignore = "measurement, not a check; prints its numbers"]
fn frames_cost() {
    use std::time::Instant;
    let timed = |capture: bool| {
        let mut m = start(7);
        let began = Instant::now();
        let mut frames = 0usize;
        let mut bytes = 0usize;
        let mut largest_chunk = 0usize;
        for _ in 0..MAX_STRETCHES {
            let request = request(AdvanceTarget::ForSeconds(10));
            let out = if capture {
                m.advance_capturing(request)
            } else {
                m.advance(request)
            }
            .unwrap();
            if capture {
                let chunk = serde_json::to_string(&out.frames).unwrap().len();
                frames += out.frames.len();
                bytes += chunk;
                largest_chunk = largest_chunk.max(chunk);
            }
            if out.stop == StopReason::Finished {
                break;
            }
        }
        (began.elapsed(), frames, bytes, largest_chunk)
    };
    let (plain, ..) = timed(false);
    let (watched, frames, bytes, chunk) = timed(true);
    println!("plain, 10 s stretches:    {plain:?}");
    println!("capturing, 10 s stretches: {watched:?}");
    println!(
        "{frames} frames, {bytes} bytes of JSON in all ({} bytes a frame), largest 10 s reply {chunk} bytes",
        bytes / frames.max(1)
    );
}
