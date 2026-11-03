# APFM Run F4 — a stateful, causal, interactive M12 match

**Not pushed: the M12 repository (`~/apfm-match-lab`) has no git remote configured**, so its eight F4
commits exist only on the local branch `f4/interactive-session`. `ofm_core` depends on that checkout by
path (`../../../../../apfm-match-lab`), so the pushed OFM branch builds only with M12 on that branch.
Add a remote and push it before anyone else builds this. The M12 working tree also still holds the
pre-existing, uncommitted licence edits (`Cargo.toml`, untracked `LICENSE`/`NOTICE`), which were left
alone and not committed.

Branches: OFM `claude/run-f4-interactive-engine` (from the verified F3 branch `claude/run-f3-autonomy`,
`dd166c94`), M12 `f4/interactive-session` (from `main`, `482e09a`). The viewer repository was not
changed. Nothing was merged into `apfm/main` or an integration branch.

## 1. Verdict

A manager can play a real M12 match: pause it, change mentality, pressing, tempo and formation,
substitute, hand the side to the assistant, and finish it through the career's one finalisation.
The match is one stateful simulation, not a replay.

| Acceptance rule | Status | Evidence |
|---|---|---|
| A tactical change alters future football | **Met** | §6: 200 paired seeds per comparison |
| A substitution changes who plays | **Met** | §7, `tests/f4_squad.rs` |
| A red card changes strength and future play | **Met** | 1 red: own shots −2.12, conceded +1.84; 2 reds: −2.82 / +3.24, all 95% CIs exclude 0 |
| Fatigue evolves and persists | **Met** | `fatigue_accumulates_and_belongs_to_each_player`; OFM E2E: 90 min wears more than 45 more than 0 |
| Past events stay immutable | **Met** | `a_tactical_change_never_rewrites_what_already_happened`; append-only stream |
| One canonical finalisation | **Met** | `finish_spatial_match` is the only path; §9 |
| *Required causal link* mentality → defensive exposure | **Not met** | §6.3: the engine has no such mechanism; reported, not tuned |

Not claimed: visual polish (explicitly out of scope), a mid-match save in the career file (§11), an MCP
tool for the interactive match (§13).

## 2. Where the engine already was, and what F4 did to it

The task asked to *extract* a stepping function. It already existed:

| Original entry point | Role |
|---|---|
| `Simulation::step` (`src/sim/runner.rs`) | one tick; the only simulation loop |
| `run_match` / `Simulation::finish` / `simulate_autonomous_with` | drove `step` to full time; observers were locals of the call |
| `LiveSession` (`src/sim/live.rs`, Mission 10) | a command queue in front of `Simulation` |

What did **not** exist, and what F4 added in M12:

* `MatchSession` (`src/sim/session.rs`): owns the `Simulation` **and its accumulators** (event
  recorder, statistics observer), a revision, the command ledger and journal, actors and pause.
* `simulate_autonomous_with` is now `MatchSession::new` + `advance_to_end` + `finish`. Post-processing
  moved, unchanged, into `assemble_autonomous_run`. There is one simulation loop.
* `Simulation::set_hold_at_period_breaks`: `end_period` is split into "close the period" and
  `begin_next_period`, so a manager can act in the half-time break. Off, behaviour is identical.
* `live::apply_to_simulation`: the one command → `Simulation` mapping, now typed (`ApplyError`), shared
  by `LiveSession` and `MatchSession`.
* `Simulation::cancel_substitution`, `pending_substitutions`, `MatchSession::finished_run`.
* Found by the F4 tests and fixed: a team reduced by a dismissal could not change formation
  (`FormationNeedsElevenPlayers` fired on any empty slot).

**Proof that the refactor changed no football.** Digests of 36 full matches (world, output JSON,
physical record, substitutions, ticks; plain, managed and extra-time/shoot-out scenarios) captured
from the untouched engine (`examples/f4_golden.rs`) are byte-identical afterwards, again after the
formation change, and the 41 pre-existing suites (533 tests) still pass.

## 3. Session state (Layer A and B)

| Owned by | State |
|---|---|
| M12 `Simulation` | clock, phase, ball, players, possession, referee, injuries, fatigue, substitutions, tactics, RNG (derived per decision from the seed, so no free-running generator to persist) |
| M12 `MatchSession` | event stream (append-only; an event's id is its index), statistics accumulator, revision, ledger of every command and answer, journal of accepted commands with the tick boundary they were applied at, actors, pause |
| OFM `InteractiveMatch` | fixture identity, user side, OFM tactical plans, assistant state and stand-back windows, order ledger, AI cursor and review clock |
| UI | nothing but what the last answer said |

The controller does not calculate goals, cards or results; the UI mutates nothing.

## 4. Commands and validation

`MatchCommand`: `SetMentality`, `SetPressing`, `SetTempo`, `SetFormation`, `SetTacticalInstructions`,
`RequestSubstitution`, `CancelPendingSubstitution`. Players are stable ids. Every command travels in a
`CommandEnvelope { session_id, command_id, actor, expected_revision, command }`.

`MatchSession::submit` checks, in this order: session identity → duplicate id (answered, not
re-applied; the same id for a different command is refused) → phase (finished) → actor authorised for
the side (`grant`/`revoke`) → revision → parameters → player eligibility → effective time. A rejection
changes nothing (digest, journal and revision are asserted unchanged). A tactical command is in force at
the tick boundary the session is at; a substitution is queued to the next stoppage, or to the break
(using no window). No command carries a UI timestamp; the clock is the engine's. Commands and advances
are serialised by `&mut` in the engine and by the state slot's mutex in OFM.

Tests (`tests/f4_session_commands.rs`, 15): duplicates, id reuse, wrong session, stale revision,
unauthorised/revoked actors, invalid players and parameters, commands after full time, queuing for the
next stoppage, half-time substitutions, cancellation, deterministic application, checkpoint/restore
including the duplicate memory, and three threads issuing commands against an advancing thread
(journal replays to the same world). Mutation check: disabling idempotency and the revision guard turns
four tests red.

## 5. Segmented = whole (Deliverable 1)

`tests/f4_session_equivalence.rs`: 28 seeds in each of four scenarios (plain, both sides managed,
knockout with extra time and a shoot-out, high press), full 90-minute matches, **112 seeds**, each played
three ways: a bare `Simulation::run_to_end` with the hold off (the pre-session oracle), the whole-match
wrapper, and a session advanced to 30', 60', full time. Compared bit-for-bit: score, every event, cards,
active players, minutes, final condition, statistics, substitutions, world digest, ticks, end status.
A second test stops at every key event and every minute (8 seeds × 4 scenarios): rendering speed cannot
change the match. Mutation check: resetting the statistics accumulator at each `advance_to` fails at
`statistics`, as predicted.

## 6. Tactics: causal evidence (Deliverable 3)

`tests/f4_tactics.rs`. 200 paired seeds per comparison, full matches; the same snapshot is played with
and without an order from the home manager at the same instant; everything before it is bit-identical.
No score multiplier exists: the engine reads the current sliders in its pressing (`defend.rs`: range
8 + 22 p, 1/2/3 pressers, trigger, leash), passing (`wanted_arrival_speed`, progress weight
×(0.8 + 0.4 m)) and off-ball decisions, and the formation planner. Metrics come from the read-only
probes (`m10.probe`) and the event stream. **The criterion was fixed before the runs: the 95% interval of
the paired difference excludes zero, in the direction the code's mechanism implies. No threshold was
changed after seeing a result.** Asserted: pressing → press attempts and energy; tempo → passes attempted;
mentality → forward pass progression. Everything else is reported, not asserted.

### 6.1 Asserted effects (mean paired difference, home team; 200 pairs)

| Order | Press attempts | Energy (eu) | Passes | Forward pass progression (m) |
|---|---:|---:|---:|---:|
| Pressing 90 | **+247.7** [+230, +265] | **+11,879** [+11.0k, +12.7k] | −12.8 | −48.7 |
| Pressing 10 | **−585.7** [−599, −573] | **−21,050** [−21.7k, −20.4k] | −7.6 | +167.2 |
| Tempo 90 | −4.8 (n.s.) | −317 (n.s.) | **+10.7** [+4.9, +16.5] | −48.9 (n.s.) |
| Tempo 10 | +56.4 | +3,314 | **−43.4** [−49.1, −37.6] | +178.7 |
| Mentality 90 | +9.5 (n.s.) | +716 (n.s.) | −11.2 | **+71.1** [+17.4, +124.8] |
| Mentality 10 | −0.5 (n.s.) | −420 (n.s.) | +8.2 | **−58.7** [−115.2, −2.3] |

### 6.2 Trade-offs the engine really has (reported, not asserted)

| Order | Shots for | Shots conceded | Recoveries | Possession changes |
|---|---:|---:|---:|---:|
| Pressing 90 | −2.02 | +1.74 | −1.2 (n.s.) | −1.1 |
| Pressing 10 | +1.38 | **+6.60** (+48%) | −5.7 | −0.1 (n.s.) |
| Tempo 90 | −1.76 | +0.79 | **+27.6** | +1.1 |
| Tempo 10 | −2.10 | +0.5 (n.s.) | **−22.1** | **+3.9** |
| Formation 5-3-2 (kickoff) | −3.27 | +0.1 (n.s.) | −4.5 | — |
| Formation 4-3-3 (kickoff) | **+2.79** | −0.2 (n.s.) | +3.0 | — |
| Pressing 90 at half-time | −0.69 | +0.49 | −1.5 | — |

High pressing costs ~8% more energy and leaves space (more shots conceded); a low block spends 14% less
energy but concedes ~48% more shots. Formation changes are large and structural: 5-3-2 shortens the team
by 3.5 m and moves the front line back 4.9 m; 4-3-3 lengthens it by 1.75 m. The half-time change adds
about half of the kickoff effect (press attempts +125 vs +248) and the first half is bit-identical.
The low-tempo result (more forward progression, fewer passes) is an observation about this engine's
passing model, not an expectation I set.

### 6.3 Not met: mentality → defensive exposure

The spec lists "defensive exposure" and "positional risk" as required consequences of mentality. In this
engine mentality only reaches pass selection (progress weight) and the off-ball "hold depth" weight. With
200 pairs, an attacking mentality changes none of: shots conceded (−0.38, CI spans 0), counter-attack
shots conceded (+0.07), back-line depth (+0.05 m), front-line depth (+0.06 m), team length (−0.04 m).
Adding that link means changing how the planner positions every team and recalibrating the M10–M12
football; I did not do it silently. **Decision for the owner**: add a bounded mentality term to the block
shift in `formation.rs` behind a config value, then re-run this study.

### 6.4 Cheap causal tests

`a_formation_change_moves_nobody_and_resets_nothing` (no agent moved by the command; ball, owner and
possession side unchanged; nobody covers >12 m in the next second), `a_tactical_change_never_rewrites_what_already_happened`
(events before the change are identical, the future differs), `a_formation_change_reassigns_roles`,
`the_engine_always_reads_the_latest_orders`.

## 7. Substitutions, dismissals, injuries (Deliverables 4 and 5)

`tests/f4_squad.rs` (17). Highlights: the eleven before and after differ by exactly the swap; minutes
add up to the swap time; the outgoing player's statistics freeze; the incoming player's slot profile is
his own (and over 60 pairs a strong passer completes more of his passes than a weak one); invalid
substitutions (no allowance, no window, a goalkeeper for an outfielder, a player who already played)
leave no trace; several substitutions at the break are all made and use no window; a straight red and a
second yellow leave ten, nothing replaces the player by itself, and he is in **no later event** (checked
by scanning every string of every later event); a red after substitutions refunds nothing; ten can
reorganise (the engine fix above); an injured player is replaced when substitutions remain, and when none
remain the team plays on short-handed; nobody who left can be brought back; fatigue accumulates and
belongs to each player.

Engine behaviour worth knowing: a forced sanction is shown at the next whistle, goal or period end, not at
a throw-in (natural cards come from fouls, which are whistles, so this only affects scripted scenarios).

## 8. Command model in OFM

OFM vocabulary in, M12 out: the manager picks a `PlayStyle`, `PressingIntensity`, `Tempo` and a formation;
`interactive_match::plan::commands_between` sends one command per slider that moved, computed through
the adapter's **single** conversion `spatial_adapter::tactics`. An unsupported formation refuses the whole
order before any command exists. Offered formations are exactly M12's presets.

## 9. OFM integration path

```
start_live_match            (existing: fixture identity, locking)
 └─ start_interactive_match      InteractiveMatch::start: adapt_selected_fixture → M12 MatchSession
     ├─ advance_interactive_match   M12 advance; computer managers consulted at checkpoints
     ├─ submit_match_command        manager's order → M12 commands under the actor "manager"
     ├─ set_interactive_pause / set_interactive_assistant
     └─ finish_interactive_match    complete(): report = M12 output → decode → project
                                    → finish_spatial_match  (the existing finalisation)
```
No second finalisation path exists. `finish_interactive_match_internal` holds the application gate, refuses
a match that has not reached full time, refuses (and discards) a match whose fixture is not the one the
live session now identifies, and keeps the finished match when the career cannot apply it, so the same
result can be retried. Two simultaneous finishes apply it once. Standings are untouched before
finalisation (asserted). The end-to-end test (`a_managed_match_is_finalised_once_and_survives_a_reload`)
plays a real fixture to half-time, changes mentality, makes a legal substitution, finishes, writes the
career to SQLite and reads it back with a fresh reader: fixture, score, table, appearances, the substitution
in the stored record (once), goals credited = score, and fatigue ordering (90' more than half more than
none) all survive; a second finish changes nothing.

What the career records follows the existing projection: appearances, goals, assists, cards, injuries,
ratings and stamina scaled by minutes played. The engine's per-player end condition is **not** projected
into the career (`MatchReport` has no field for it); stamina after the match is the existing minutes-based
depletion.

## 10. Computer managers and delegation (Deliverables 6–7)

`interactive_match::ai::decide` is a pure function of what a manager can see: score, clock, cards,
injuries, fatigue, the bench and the club's tactical identity (its kickoff plan). No RNG, no engine handle,
no future. It acts at the half-time break, after a dismissal, after an injury and every 10 simulated
minutes (rate-limited: 6 minutes between discretionary tactical changes). Its orders go through the same
pipeline under a per-side actor, so M12 enforces every legality rule: over 12 full matches the engine
refused **none** of the opponent manager's commands, it only ever commanded its own side, and it did
substitute.

Authority: `manager` (the player's side, always), `assistant` (the player's side, only if `Duty::Tactics`
is delegated and the switch is on), `opponent-ai-*` (the other side). Taking the assistant back always
works. The assistant stands back for 15 simulated minutes from any kind of order the manager just gave, so
a human decision is never overwritten.

## 11. Persistence

After full time the career keeps what the existing finalisation keeps (§9) and nothing else: no ball or
player coordinates, no replay, no tick log. A running session serialises to `InteractiveCheckpoint`: the
input, the configuration and the **journal of accepted commands with the tick each was applied at**.
Because the simulation is deterministic, restoring replays it and verifies the world digest; a divergence
is an error. At 30' with 200 commands the checkpoint is ~100 KB and restores in ~230 ms. This is tested
(`a_saved_match_resumes_exactly`) but **not wired into the career save**: the application already refuses to
save while a live session exists, so a match in progress is lost when the app closes.

## 12. Verification

| Check | Result |
|---|---|
| M12 `cargo test --release` | 45 suites, 577 tests, 0 failed (533 pre-existing + 44 F4) |
| M12 clippy `--all-targets` / fmt | 0 warnings / clean |
| OFM `cargo test --workspace` | 113 suites, 4,394 tests, 0 failed |
| OFM `cargo test --features mcp --lib` | 622 tests, 0 failed |
| OFM clippy default and `--features mcp`, `-D warnings`, `--locked --workspace --all-targets` | clean |
| `cargo fmt --check` | clean |
| `npm test` (vitest) | 329 files, 2,906 tests, 0 failed |
| `npm run build`, `lint`, `format:check`, `knip`, `tsc --noEmit` | clean |
| i18n gates (`vitest run src/i18n`) | pass; 37 new keys in 12 locales |

### 12.1 Final numbers

| Suite | Result |
|---|---|
| OFM `cargo test --workspace` (default features) | 113 suites, 4,394 tests, 0 failed |
| OFM `cargo test --features mcp --lib` | 622 tests, 0 failed |
| `npm test` | 329 files, 2,906 tests (4 skipped, pre-existing), 0 failed |
| M12 `cargo test --release` | 45 suites, 577 tests, 0 failed |

Commands:

```
cd ~/apfm-match-lab && cargo test --release
cd ~/apfm-match-lab && cargo test --release --test f4_tactics -- --ignored --nocapture report_   # formation / half-time arms
cd ~/apfm-migration/openfootmanager && cargo test --manifest-path src-tauri/Cargo.toml --workspace
cd ~/apfm-migration/openfootmanager && cargo test --manifest-path src-tauri/Cargo.toml --features mcp --lib
cd ~/apfm-migration/openfootmanager && npm test && npm run build && npm run lint && npm run knip
```

`tests/f4_tactics.rs` runs six 200-pair studies (~280 s together in release); run it with `--release`.

## 13. Performance and memory (`examples/f4_perf.rs`, release, Apple silicon)

| Measure | Result |
|---|---|
| Whole match, baseline engine (`482e09a`) | 604–610 ms (54,000 ticks) |
| Whole match through the session wrapper | 603–616 ms (no measurable overhead) |
| Session advanced in 1-minute slices (91 calls) | 613 ms (+0.4% vs whole) |
| `view()` + JSON | 13 µs, 11.5 KB |
| One tactical command | 154 µs |
| Checkpoint at 30' (200 commands) / restore | 99.5 KB / 230 ms |
| Peak RSS, headless full session | 5.9 MB (budget 16 MB) |
| Peak RSS after 20 vs 200 sequential sessions | 5.85 vs 6.39 MB (+0.5 MB for 180 more sessions; not a linear leak, but above the 5% drift target in `PERFORMANCE_TARGETS.md`; I did not run longer) |

## 14. UI (minimal by design)

`InteractiveMatchScreen`: clock, score, phase, both lineups with condition and cards, formation, the
manager's tactics selects, substitution selects with the pending queue and cancellation, pause/resume,
play-to-end, speed, the assistant switch, a newest-first event feed (reusing the replay viewer's wording),
and Finish. Entry: "Manage the match" in the pre-match panel. It holds no football state. Tests use a
stateful fake of the service that feeds values back (ten scenarios, including a failure that resynchronises
with the engine and a refusal explained in words).

## 15. Known limitations and open decisions

1. **Mentality → defensive exposure is not an engine mechanism** (§6.3).
2. A half-time formation change re-lays players at the *old* formation's kickoff positions and they walk to
   the new shape; it is a genuine restart, but not the new shape's geometry.
3. Two computer-manager policies now exist: M12's autonomous manager (decides inside the engine at dead
   balls, used by whole-match runs) and `interactive_match::ai` (decides through commands, as F4 requires).
   The latter reuses M12's `quality`, `fit` and `line_of` but not its `need_v2`/`formation_utility`;
   converging them is a candidate for epic #589. Two projection-context builders (`projection_context`,
   `session_projection_context`) exist because OFM's mirror input and M12's input are different types.
4. The engine's end-of-match condition is not projected into the career (§9).
5. No MCP tool and no mid-match career save (§11).
6. Replays: an interactive match records no frames, so it has no 2D replay.
7. Translations were written for this run and have not had a native review; `de`, `fr`, `ru`, `tr`, `cs`,
   `id` use the polite or familiar form of their neighbours but a native check is advised.
8. `ProgressBar` has a transition with no reduced-motion escape (shared primitive, pre-existing).

## 16. Manual test

1. `npm run tauri dev`, start a career, open the next match so the pre-match screen shows.
2. Choose **Manage the match**. Press Play; the clock should advance in 3-minute slices and stop at half-time.
3. At the break: pick Attacking and Aggressive pressing, Apply; the lineup panel's formation and the plan
   update. Pick an outfield player and a bench player, Confirm substitution; it appears under queued
   substitutions and is made when play resumes (it appears in the feed once).
4. Press Pause, change something while paused, Resume. Try a second substitution with the same player: the
   refusal is explained.
5. Play to full time (or Play to full time), press **Finish match**: the post-match screen shows the score;
   the schedule shows the fixture completed; the table moved once; the substituted players have appearances.
6. If the club has match-day tactics delegated to the assistant (Staff), the assistant switch is enabled;
   your own orders stand for 15 simulated minutes.
7. Quit mid-match: the match is lost (§11).
