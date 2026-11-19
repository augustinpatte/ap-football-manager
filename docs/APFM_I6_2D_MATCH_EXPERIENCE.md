# APFM I6: the 2D match experience

Branch `claude/i6-match2d`, from `11bbb6d4`. Validation, numbers and the GUI statement are in
[`APFM_I6_VALIDATION.md`](APFM_I6_VALIDATION.md).

## 1. What a manager gets

"Manage the match" in the pre-match panel now opens one screen that shows the match being played on
a 2D pitch, from kickoff to the final whistle, and lets the manager act on it.

* A pitch drawn at 105 x 68 m with both teams' players on it (shirt numbers, the player on the ball
  ringed), the ball (lifted by its height), an arrow per team for the way it attacks (sides swap at
  half-time and before extra time), and a legend that says the same in words.
* A scoreboard whose score and stopwatch are those of the instant on screen, `45+2'` in added time,
  an "added time" badge once the referee has announced it, and Half Time / Full Time states.
* A caption: who has the ball, or why play is stopped (ball out, foul, offside, goal, injury,
  substitution, end of period).
* A feed (newest first) of goals, shots with how they ended, cards, fouls, corners, offsides,
  substitutions and injuries, and a statistics table (possession, shots, shots on target, corners,
  fouls, offsides, cards), both counted up to the instant on screen.
* Play / pause, speed 1x to 16x, three highlight modes (key moments, extended, full match), "skip to
  the next break", the assistant switch, Finish.
* Tactics, formation and substitutions, the lineups with each player's condition.

Everything under "the engine's" below is the M12 engine's own output. Nothing on the pitch is
invented by the screen.

## 2. Architecture as built

Nothing in the pinned M12 dependency (`apfm-match-lab` at `28f34b10`) was changed.

```
M12 MatchSession (F4)  ──read-only──▶  FrameSampler  ──▶ advance_interactive_match(capture_frames)
   │  orders: submit_match_command                           │  LiveFrame[], key events, view, clock
   ▼                                                         ▼
 InteractiveMatch (ofm_core)                         useLiveMatch ─▶ LiveDirector ─▶ LivePitch (canvas)
   │ finish_interactive_match                          (queue)       (frames, pace,    drawPitch (the
   ▼                                                                 highlights)       replay viewer's)
 finish_spatial_match  ◀── the one official finalisation (unchanged)
```

**Why it is one interactive session and not a replay.** The F4 work (`APFM_RUN_F4_INTERACTIVE_ENGINE.md`)
had already made the M12 match a stateful `MatchSession`: it can be advanced in slices, takes
commands between them, and equals the whole-match run bit for bit (112 seeds). That is what makes a
tactical change or a substitution made during playback able to change the football that follows,
without pretending anything about frames already played. What it lacked was a picture: the session
records no frames (`record_frames` off), and the 2D viewer only read a finished replay. I6 adds the
picture to the interactive session; it does not add a second simulator or a second viewer.

### Backend (`ofm_core::interactive_match`)

* `live_frames.rs`: `FrameSampler` copies, on the engine's own frame grid (500 ms), the positions,
  heading, ball, ball holder, possession side, score, game state and stoppage reason out of
  `session.simulation().world()`. It is read-only and holds no football state. Frames carry players
  by roster index (`[index, x, y, heading]`, on-pitch players only) so the roster, sent once in the
  view, is not repeated. `ClockView` derives the period's start, nominal end and announced added time
  from the engine's own `period_start` / `added_time` events.
* `InteractiveMatch::advance_capturing` is `advance` cut into frame-sized steps. The engine's
  segmented run equals its whole run, and the tests prove the controller's does too (section 7).
* `AdvanceTarget::ForSeconds` (the screen plays seconds ahead of the picture), a richer feed
  (`is_feed_event`: key events plus shots, fouls, corners, offsides, added time), and
  `reset_frame_sampling` so a screen that takes over a match under way is given the current frame.
* `application::interactive_match::advance_interactive_match_internal` gains `capture_frames`; the
  Tauri command takes it as an optional argument (older callers are unchanged).

### Frontend (`src/components/match`)

* `livePlayback.ts` (pure): `FrameStore` (validates and orders frames, counts what it refuses),
  `sampleAt` (the world between two frames), `highlightSpans`, `paceStep`, `clockLabel`.
* `liveDirector.ts` (plain TypeScript, no React): holds frames, events and the view; moves the
  picture; decides when the engine is asked for more and when orders are allowed.
* `useLiveMatch.ts`: the backend calls (one queue, so no two overlap), the animation loop and the
  order, assistant, skip and finish actions.
* `LivePitch.tsx`: a canvas redrawn by the animation loop; React state changes about four times a
  second, the pitch every display frame; while paused nothing animates and nothing is redrawn.
* `InteractiveMatchScreen.tsx`, `LiveMatchChrome.tsx`: composition. The existing `drawPitch`,
  `ReplayStatsPanel`, `feedEntryOf`/`statsAt` and the tactics, substitution and lineup panels are
  reused (`statsAt` and `drawPitch` were generalised to take only what they read).

The read-only replay viewer (`M12ReplayViewer`, "Watch in 2D") is unchanged and is still the way to
re-watch a finished match that was recorded as a replay.

## 3. Spatial data provenance

| What is drawn | Where it comes from |
|---|---|
| Player positions, headings | `World::agents[i].pos`, `.heading`, at the instant of the frame, in centimetres |
| Which players are on the pitch | the world: substituted and sent-off players are absent from later frames |
| Ball position and height | `World::ball` |
| Ball holder, possession side | `World::football.owner`, `.possession_side` |
| Score, game state, stoppage reason, period | `World::score`, `.state`, `.stoppage`, `.period` |
| Events (goals, shots, cards...) | the canonical event stream, with its ids |
| Added time | the engine's `added_time` event |

Test `captured_frames_are_the_engines_own_recorded_frames` runs the same match with the engine's own
recorder switched on and compares every frame on the grid (over 10,000): positions, ball, score and
state agree to the centimetre. `liveMatch.real.generated.json` holds the first three minutes of a
real match, serialised as the IPC does; the frontend tests read it (11 a side in all 361 frames,
nobody faster than 12 m/s, a clean pass through the director, the drawing and the screen).

**The one thing the screen adds** is smoothness: between two recorded frames half a second apart it
draws a straight line, so movement is not a 2 Hz slideshow. A step that is a restart (everyone back at
the kickoff spots, the ball at the centre) is shown as a jump, a gap in the data (after "skip ahead")
is never bridged, and nothing is drawn past the last frame produced.

**Not available from the engine through this route.** The engine's per-team phase (attack, transition,
defence) and per-player action are recorded only in a whole-match replay and are not read; the caption
says who has the ball, which is the possession side the world holds. Player condition is shown from
the session view (the lineup panel), at the engine's head.

## 4. Playback and highlight modes

The engine runs a little ahead of the picture and the picture never gets ahead of the engine. In the
full match it asks for 10 to 60 simulated seconds at a time (more at higher speeds, so the request rate
stays low) and keeps 80 % of a stretch in hand. In the highlight modes it asks for 60 seconds and keeps
90 seconds in hand.

| Mode | What is shown |
|---|---|
| Full match | every instant, at 1x to 16x |
| Extended | every goal, shot, card, injury, penalty, corner, foul and offside with its run-up (15 s before, 8 s after), plus every spell of 3 s or more of possession in the last 28 m in front of a goal |
| Key moments | goals, shots, cards, injuries and penalties with 10 s before and 5 s after (15 s and 10 s for a goal); the quiet play between is cut |

Highlights are computed from the events and frames received so far. A cut jumps to the next
highlight's first frame; if none is known yet the picture parks 15 s behind the engine so a moment
that starts soon keeps its run-up; at a break or full time with none left it ends at the break. The
match played is the same in every mode and at every speed (`rendering_speed_does_not_change_the_match`,
`capturing_frames_does_not_change_the_match`): a mode changes what is shown, never what happens.

At half-time the engine stops, the picture reaches it, and the second half starts only when the
manager asks (the button reads "Start second half"). "Skip to the next break" plays the engine on
without the picture, asks for the frames of that stretch, and shows where it got to. The feed and the
statistics are then counted over the whole stretch.

## 5. Substitutions and tactical changes, truthfully

* An order is possible only when the picture is at the engine's head and the match is not running.
  "Pause" runs the picture on to the head (at 32x, at most a few seconds) before the panels enable;
  until then they are disabled with a note. The manager therefore always acts on the moment he sees.
* An order goes to the engine as M12 commands under the manager's actor. A tactical change is in force
  from the tick boundary the session is at; a substitution is queued to the next stoppage (or used at
  the break) and shown as queued; the engine's limits and rules (substitutions and windows left,
  injured and sent-off players, goalkeeper replacement, formations that need eleven players) decide,
  and a refusal is explained in words.
* An accepted order only changes frames not yet produced. `an_order_never_rewrites_frames_already_handed_out`
  and F4's `a_tactical_change_never_rewrites_what_already_happened` pin it; the engine's clock does not
  move because of an order, and the UI never edits a frame.
* Delegation and human control are as in F4/I9: the manager's side is changed only by his orders (or the
  assistant's, if the tactics duty is delegated and the switch is on, and never inside the 15 simulated
  minutes after an order of the same kind). The switch is available only when orders are.

## 6. Official finalisation

Unchanged, and shared with every other way of playing: `finish_interactive_match` hands the report of a
match that reached full time to `finish_spatial_match`. A match that has not reached full time is
refused; a failed application keeps the finished match for a retry; two simultaneous finishes apply it
once; a match of another fixture is never applied. The screen adds a second guard (a double click on
Finish is one call) and the final score on screen is the score of the last frame, which the application
test shows equals the recorded result.

Nothing is written to the career before full time (standings untouched, asserted), an error during
playback leaves the pre-match career as it was, and a match saved in the middle resumes from its
checkpoint (F4/I9: the journal of accepted commands replays to the same world and the digest is
verified). A resumed or re-opened match shows the current instant and carries on; the frames before it
are not re-sent.

## 7. Limitations

* The interactive 2D match is the only 2D live path that takes orders. A match played through the
  classic live engine, or delegated, has no spatial data (it never ran M12).
* No replay is stored for an interactive match (the frames are not persisted); "Watch in 2D" and "Quick
  simulation" still store one. Storing the captured frames at finish would be the natural next step.
* Spectators (no club in the fixture) do not get the live 2D screen from their choice panel; they keep
  "Watch in 2D" (a replay) and quick simulation. The screen itself works without a club (no orders).
* After an app restart the frames before the resume point are gone (the engine replays to the point; it
  does not re-emit the past).
* Per-team phases and per-player actions are not shown (section 3).
* Highlight selection is by event and ball position only; it does not rank moments by xG (the engine
  records none).
* The translations of the new strings were written for this change and have not had a native review.
* No GUI session was available to this run; see the validation report.
