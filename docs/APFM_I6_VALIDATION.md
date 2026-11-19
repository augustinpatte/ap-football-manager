# APFM I6: validation

Branch `claude/i6-match2d`, from `11bbb6d4`. Design and behaviour: [`APFM_I6_2D_MATCH_EXPERIENCE.md`](APFM_I6_2D_MATCH_EXPERIENCE.md).
Nothing was pushed or merged. `crates/engine`, `crates/domain` and `crates/db` are byte-identical to the baseline.

## 1. Verdict

| Requirement | Status |
|---|---|
| A complete spatial match in the desktop app, kickoff to full time, on authentic engine data | **Built and tested through the real Rust path and the real frontend code; not driven in a GUI** (section 5) |
| Pause / resume, speeds, key / extended / full highlights, half-time, added time, full time | Met, unit- and component-tested |
| Substitutions and tactical changes that change the football that follows | Met: the existing F4 interactive session, now with a picture; orders only at the engine's head |
| One official finalisation, once | Met, unchanged path, tested for the watched path including a repeated finish |
| Delegated-tactics fitness carry-over | **Reproduced; mechanism not established; no code changed** (section 6) |
| Real GUI smoke test | **Not performed** (section 5) |

## 2. Quality gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean |
| same with `--features mcp` | clean |
| `cargo test --workspace` | 5,186 passed, 0 failed, 130 ignored (160 suites); baseline was 5,173 passed |
| `cargo test --features mcp --lib` | 678 passed, 0 failed, 11 ignored |
| `npm test` | 3,111 passed, 0 failed, 4 skipped (352 files); baseline was 3,066 |
| `npm run build` (tsc + vite) | clean |
| `npm run lint`, `format:check`, `knip` | clean (3 pre-existing `info` notes in `transfers/`) |
| `vitest run src/i18n` (the real i18n gate) | 35 passed; 27 new keys x 12 locales |

## 3. What the tests prove

Backend, `interactive_match/frames_tests.rs` (10) and `application/interactive_match.rs` (3 new):

| Scenario | Test |
|---|---|
| Frames are the engine's own: every frame on the engine's grid (over 10,000) equals the engine's own recording of the same match, to the centimetre | `captured_frames_are_the_engines_own_recorded_frames` |
| Capturing changes nothing: result, every event and every command of the computer managers are identical with and without capture (only the revision counter differs, by design) | `capturing_frames_does_not_change_the_match` |
| Timeline validity: strictly increasing, never more than 1 s between frames, nobody outside the stadium, never more than 11 a side, the ball holder is on the pitch, the last frame is full time with the engine's score | `the_frames_of_a_match_are_ordered_complete_and_on_the_pitch` |
| Goals and the picture agree: over five matches the frame score moves exactly when the stream has a goal | `the_score_in_the_frames_moves_with_the_goals_in_the_event_stream` |
| A substitution is in later frames and never in earlier ones | `a_substitution_appears_in_the_frames_after_it_and_not_before` |
| An order never rewrites frames already handed out; the engine's clock does not move | `an_order_never_rewrites_frames_already_handed_out` |
| Clock: nominal end, added time announced, second-half base, extra-time bases | `the_clock_view_follows_the_periods_and_their_added_time`, `a_period_clock_reports_the_minutes_played_before_it` |
| Feed carries shots, fouls and corners, in stream order | `the_feed_carries_shots_fouls_and_corners_as_well_as_the_key_events` |
| Seconds-granular advance | `a_match_can_be_advanced_by_seconds` |
| A watched match (30-45 s stretches, an order at the break) finalises once with the score of the last frame; a second finish is refused and changes nothing | `a_match_watched_on_the_2d_screen_is_finalised_once_with_the_score_it_showed` |
| Wire shape the screen reads (arrays, roster with sides, clock) | `the_wire_format_of_a_captured_advance_is_what_the_screen_reads` |
| A screen that takes over a match under way is given the current frame | `a_screen_that_takes_over_a_match_under_way_is_given_the_current_frame` |

The existing F4/I9 tests (two simultaneous finishes, a failed finish kept for retry, save mid-match and
resume, checkpoints recorded under F4.4/F5.2 restoring as such, a fixture of another match never applied,
five-zone and M12 consequences identical) all still pass unchanged.

Frontend:

| Area | Tests |
|---|---|
| Frame store, interpolation, restarts, gaps, clock labels, highlight selection, pacing | `livePlayback.test.ts` (21) |
| Director: picture never overtakes the engine, pause catches up before orders, one request in flight, break and second half without a duplicated or missing frame, full time, failures, malformed frames, events revealed as reached, key mode shows the half far faster and never the quiet stretch, mode change keeps the picture | `liveDirector.test.ts` (13), against a stateful fake engine |
| Real engine data: 361 real frames, 11 a side in every one, nobody over 12 m/s, clock and possession, drawn, played through the director | `liveMatch.real.test.ts` (6), from `liveMatch.real.generated.json` written by the backend test `write_live_fixture` (the first three minutes of a real match, serialised as the IPC does) |
| The screen with a stateful fake service: kickoff, play, goal shown when the picture reaches it, pause then orders against the revision shown, substitution and cancel, refusal, half-time stop and full time, finish exactly once on a double click, skip ahead, failure resynchronises, one status line, no raw ids, unknown stoppage reason, modes, assistant | `InteractiveMatchScreen.test.tsx` (15) |
| The screen through the real service module (only Tauri's `invoke` replaced) on the real payloads: command names, argument names (`captureFrames`, `eventCursor`), the `for_seconds` request, real lineups | `InteractiveMatchScreen.real.test.tsx` (2) |

Mutations were not run by the author on this change (see section 8: the test reviewer did not complete).

## 4. Performance

Backend, one whole match played in 10-second stretches (`frames_cost`, test profile of this repository):

| Measure | Result |
|---|---|
| Plain advance | 1.37 s |
| With frame capture and JSON serialisation of every reply | 1.52 s (+0.16 s for the whole match) |
| Frames in a match | 11,044 (2 a second, added time included) |
| JSON | 639 bytes a frame, 7.07 MB for the whole match; the largest 10-second reply is 13.6 KB |

The engine does not wait for the picture: the full match costs about the same whether it is watched or
not, and the F4 figure for the whole match (about 0.6 s in release) is the floor.

Frontend, Node/jsdom, a synthetic full-match store of 11,400 frames made by repeating the real ones:

| Measure | Result |
|---|---|
| Appending 11,400 frames | 11.6 ms |
| Reading the world between two frames (`sampleAt`) | 1.3 us |
| Highlight selection over the whole match (extended, 200 events) | 0.6 ms |
| Statistics at the end of a full match | 0.15 ms |

The pitch is drawn from a canvas by the animation loop; React renders about four times a second while
the match runs and not at all while it is paused (a test asserts no engine calls and no draws while
paused). Memory for 11,400 frames in the browser was **not measured** (the Node heap reading was
meaningless); the wire size above bounds the data, and the frames are plain arrays.

The Tauri IPC cost of a 10-second reply (13.6 KB, a few hundred microseconds to serialise) is not
measured in a running app.

## 5. Real application validation: what was and was not verified

**Not verified: any interaction with the running desktop app.** No UI automation reaches this
environment's windows (no computer-use or window-control tool was available; earlier runs recorded the
same), so none of the nine steps of the requested smoke test (open a career, reach match preparation,
start the match, see the pitch, reach half-time, reach full time, read the post-match report, save and
reload, check the result is recorded once) was performed in a GUI. Nothing in this report is a claim that
they were.

What stands in for it, and what each does and does not show:

* The Rust application layer drives the real path with real M12 output and a real career and saves:
  start, advance with frames, an order at the break, full time, finish, standings, appearances, the
  stored report, a second finish refused, save/reload of a match in progress (the F4/I9 tests above).
  This is the backend a Tauri command calls; the commands themselves are thin adapters over these
  functions and compile into the app (clippy and the build are clean).
* The screen runs through the real service module on real engine payloads, with only `invoke` replaced.
  This catches wire mismatches (shapes, command and argument names). It does not exercise a webview, a
  real canvas, real layout or real timing.
* `npm run tauri dev` was started twice. The first attempt failed at once because port 1420 was held by
  another session's dev server (the I7 worktree's); that process was left alone. The second attempt, on
  port 1431, is reported in section 5.1.

### 5.1 Launch attempt

The second attempt compiled and started the debug app from this worktree (`target/debug/openfootmanager`,
Vite on port 1431). The window's webview loaded this branch's frontend and made its first backend calls
(`get_active_game`, `list_installed_packages`; ten `[cmd]` lines in the log, no error, warning or panic).
That shows that the branch builds into a running app whose frontend and backend talk. It shows nothing
about a match: no career was opened and nothing was clicked, because nothing here can drive the window.
The instance was then stopped; only this worktree's processes were touched.

## 6. Delegated tactics and match-eve fitness (carry-over)

**Question.** With training and the tactics duty delegated, mean match-eve condition of the likely
starters fell from about 85.4 to 80.9 over seeds 1 to 3 on the merged tree (I5 + I9).

**Reproduced.** On this baseline, 10 seasons, the generated `gate` world, the study's own arms: seed 1
manual 89.7, all delegated 83.8; seed 2 all delegated 78.0 (manual 85.4); seed 3 all delegated 80.8; with
`group_depth_weight` 0 (a temporary environment override of the engine's default, removed and never
committed) 85.8, 84.9 and 83.5. Every figure equals the integration report's to 0.1.

**What the tactics duty sets.** It writes the full plan the day before a match (`delegation_control::tactics`
-> `ai_tactics::plan_for_club`), and it is not one plan: seed 1 settles on Passive pressing and Patient
tempo from the start and never changes; seed 2 on Direct tempo with Aggressive pressing 57-71 % of
matches and Medium otherwise, **changing in 69-72 of about 425 matches**; seed 3 on Medium pressing and
Patient tempo. The style (Defensive, Attacking, Possession) and the formation (4-4-2) are the club's own.

**What reaches a player's condition.** A match charges condition only through
`player_wear::apply_match_wear_with_load`: minutes x the player's stamina attribute. The five-zone engine
reports no physical load (`physical_load` is `None`), so no tactic can wear a player more in a match. A
tactic could only reach the eve condition through who plays, for how long, or through injuries and
recovery.

**Controls (arm "manual training, passive and patient plan held by hand": the seed-1 delegated dials,
chosen by hand and never touched).**

| Eve condition | Seed 1 | Seed 2 | Seed 3 |
|---|---|---|---|
| Manual (default Medium / Direct), weight 1 | 89.7 | 85.4 | (86.4 in the integration report) |
| Passive / Patient held by hand, weight 1 | 91.4 | 85.9 | 88.9 |
| All delegated, weight 1 | 83.8 | 78.0 | 80.8 |
| Passive / Patient held by hand, weight 0 | 89.6 | 89.7 | 82.7 |
| All delegated, weight 0 | 85.8 | 84.9 | 83.5 |

Only seed 1's control is matched to what delegation chose; in seeds 2 and 3 the held plan is not the
delegated one.

**What the evidence says.**

1. In the one matched control (seed 1) the delegated pressing and tempo, held by hand, cost nothing
   (91.4 and 89.6 against 89.7 for the default plan, at both weights). Those two dials are not the cause.
2. The delegated club is less fit than the held-by-hand club at both weights (seed means: 80.9 against
   88.7 with the depth weight on, 84.7 against 87.3 off), so delegating does something beyond those dials.
   The remaining candidates are the duty's other dials (width, build-up, defensive line, shape, marking,
   counter-press) and, in seed 2, the weekly flipping of the plan. Not separated.
3. The gap is not explained by minutes. The eleven best seniors play **fewer** minutes a match when the
   tactics are delegated (seed 1: 66.3 against 71.4 manual and 69.3 held; seed 2: 70.0 against 73.7 held)
   and the engine's own manager makes more substitutions (1.99 against 1.42 and 1.64 a match in seed 1;
   2.63 against 2.34 in seed 2). They are out injured longer (seed 1: 15.2 days a season each against 12.1
   and 12.7; seed 2: 12.6 against 11.4). Fewer minutes with a lower condition points away from load and
   toward recovery or injury returns, but that is a reading, not a finding.
4. The depth weight's own share is within the noise. Taking weight 1 minus weight 0 per seed, the
   held-by-hand arm, with nothing about delegation involved, moves by +1.8, -3.8 and +6.2 points (mean
   +1.4), so a trajectory moves by several points whenever anything in the engine changes. The delegated
   arm moves by -2.0, -6.9 and -2.7 (mean -3.9, all the same sign). That is larger than the control's mean
   but not clearly outside its spread; three seeds of one club cannot settle it.

**Decision: no code changed.** The task required a causal mechanism before any correction, and none was
established. Reading current condition in the delegated planner, as the integration report suggested,
is not supported by this evidence: the pressing and tempo that planner chose are not what makes the club
tired, and condition does not enter a match's wear at all. Disabling `group_depth_weight` was not done and
is not recommended on this evidence. Human tactics are untouched by any of this (a manager who has not
delegated the duty is never overwritten; the study's manual arms never changed plan).

**Remaining uncertainty and the experiment that would settle it.** (a) The size of the depth-weight
share needs 10 or more seeds per cell, because one cell moves 4-6 points on its own. (b) The duty's
other seven dials need a bisect: hold the exact delegated `TacticsPhaseSettings` of each seed by hand
(requires making `ai_tactics::plan_for_club` reachable from the harness, or recording the dials), then
undo one dial at a time. (c) The recovery and injury-return channel should be measured directly (condition
of a player on return from injury; the study already reports injured days). (d) Seed 2's weekly plan
flipping may itself be the problem (a review that changes the plan 16 % of the days). The study now
prints the plan histograms, plan changes, the lineup's own condition and turnover, the club's results,
substitutions, and the best eleven's minutes and injured days, and has the matched-control arm, so each of
those experiments is a rerun.

## 7. Save compatibility

* No `domain` or `db` file changed, so no serialized career field, SQLite column or migration changed.
* `InteractiveCheckpoint` (a match saved in progress) is unchanged. A checkpoint written before this
  change restores as before (the F5.2 and F4.4 compatibility tests pass), and a restored match gets a
  fresh frame sampler: the picture starts from the saved instant and the frames before it are not
  re-sent.
* The new fields are on replies, not on anything stored: `InteractiveView.roster` and `.clock`,
  `AdvanceSnapshot.frames`, `AdvanceTarget::ForSeconds` (a request). `capture_frames` is an optional
  command argument.
* The stored replay (`match_replays`) and the read-only viewer are untouched.

## 8. Reviews

| Reviewer | Outcome |
|---|---|
| `i18n-auditor` | Completed. No untranslated strings or missing keys; fixed: the dynamic stoppage key had no fallback (now `other`), the direction sentence was built from two fragments (now two whole sentences), Portuguese register (`pt` uses the familiar form like its neighbours), gendered phrasings in `pt-BR` and `cs`, a Turkish and a Chinese wording. Native review of the translations is still advised. |
| `ui-accessibility-reviewer` | Completed. Fixed: the feed's keys changed with every new event (the list was re-announced), a goal was announced twice and by a live region mounted already filled (now the one status line carries it and the banner is hidden from assistive technology with its height reserved), focus at full time. Open advisories: the pitch tells the teams apart by hue plus shirt numbers, a legend and captions (no shape cue), contrast of white labels on the away colour was not measured, the pitch palette is the replay viewer's pre-existing one. |
| `ofm-architecture-reviewer`, `ofm-test-reviewer`, `ofm-dedup-reviewer` | **Not completed.** All three failed part-way on the API usage limit and produced no report. Their questions (crate boundaries and size limits, test fixtures that discriminate, duplicated helpers such as `frameAtOrBefore` and `FrameStore.indexAtOrBefore`, `is_key_event` and `is_feed_event`) are therefore unanswered. They should be run before merging. |

## 9. Integration notes for I7

* I7 owns scouting and youth; none of its domain or frontend files were touched.
* Shared files changed: `src/i18n/locales/*.json` (new `match.liveView.*` group, changed
  `match.interactive.manageDesc`; both inside `match`, formatted as the existing files are, so a merge with
  I7's additions elsewhere in the same files should be mechanical), `src/i18n/INTENTIONAL_SAME.json` (one key
  in the `id` list), `docs/ARCHITECTURE.md` (one command row),
  `src/services/interactiveMatchService.ts` (types and a `captureFrames` argument; `setInteractivePause` was
  removed because nothing used it, the backend command remains).
* Youth and scouting do not reach the match screen. If I7 adds anything to the lineup or the squad panels
  of a match, those are `InteractiveMatchPanels.tsx`, which this work edited only to pass a minute formatter and
  an `announce` flag to the feed.
* The season-harness study (`i9_staff_training.rs`) gained diagnostics and one control arm; its assertions are
  unchanged.

## 10. Remaining limitations

See section 7 of the design document. In short: no stored replay for an interactive match, no live screen
for spectators, no per-team phase or per-player action overlay, no GUI verification, no run of three
reviewers, translations not natively reviewed, the delegated-fitness mechanism not established.
