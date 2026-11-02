# Match viewing: quick simulation, 2D watch, replay (Run D, part A)

Base: `validation/runs1-4-m12`. Scope: the React/Tauri integration around the M12 replay. The M12
engine (`~/apfm-match-lab`) was **not available** in the Cloud session, so nothing below was run
against real M12 output. See [§5](#5-what-must-be-validated-locally-with-the-real-m12-engine).

## 1. Why the player saw only a result

Observation: a match was simulated and only the final score appeared; no 22-player animation.

There is no bug in the viewer or in replay loading. The cause is that **a replay only exists if the
match went through one of the two M12 commands**, and the UI sent almost every match elsewhere:

| How the match was started | Engine | Replay stored | 2D reachable? |
|---|---|---|---|
| Dashboard "live" → **Start Match** | classic live engine (`live_match_manager`, text commentary) | no | no |
| Dashboard "spectator" | classic live engine | no | **no**: the pre-match screen was skipped (`isSpectator → first_half`), and the only M12 buttons lived on that screen |
| Dashboard "delegate" | classic engine, whole match day in one call (`ofm_core::matchday`) | no | no |
| Pre-match screen → "Simulate with M12" (`simulate_spatial_match`) | M12 | yes | only afterwards, from the schedule |
| Pre-match screen → "Watch in 2D with M12" (`prepare_spatial_replay` → `finish_spatial_replay`) | M12 | yes | yes |

Three things made this invisible:

1. The two M12 actions were two buttons in a `fixed right-4 top-4` overlay floating over the
   pre-match screen, labelled with the engine's code name ("…with M12"), with no explanation of how
   they differ from **Start Match**.
2. Spectators never saw them at all.
3. The schedule's "Review match" button appears on every completed user match, but for a match
   played by any path other than M12 the backend correctly returns `None`, and the UI reported
   *"This replay is unavailable or corrupted."* — wrong for a replay that was never recorded.

`load_spatial_replay` is read-only (it queries `match_replays` and never simulates); that was
already correct and is now covered by tests.

## 2. What changed

* **`MatchViewModePanel`** (new), rendered inside the pre-match screen through a `viewModeSlot`
  prop, replaces the floating overlay. It names and explains the three ways to play:
  *Start Match* (classic live, no 2D pitch), *Watch in 2D* (22 players; the result counts only when
  the manager chooses "Go to result"), *Quick simulation* (instant result, 2D replay stored).
* **`SpectatorViewChoice`** (new): spectators get the same choice, plus Start Match, instead of
  being sent straight into the live screen.
* **Post-match**: after a quick simulation or a watched match, `PostMatchScreen` offers "Replay in
  2D", which only reads the stored replay (`load_spatial_replay`) and opens the viewer read-only.
* **Explicit absence messages** instead of one generic error:
  * no replay stored → `match.replayNotRecorded` (says which paths keep one);
  * stored but unplayable → `match.replayUnavailable` (unreadable / malformed / inconsistent);
  * schema other than 1 → `match.replayUnsupportedSchema`;
  * a result with events but no frames → `match.replayNoFrames`. **No frame is ever invented.**
* **`m12Replay.ts`** (new, pure): parsing with a typed rejection reason, `frameAtOrBefore`,
  `playersOnPitch`. The viewer draws exactly the players whose `on_pitch` flag is set (bench and
  sent-off players are not painted), shows "players on the pitch: n home · n away", and lists events
  with translated types and player names (names come from the match snapshot; an unknown event
  type is shown as the engine spelled it).
* The viewer accepts `homePlayerIds`. When the match is still live the home side is taken from the
  snapshot instead of assuming the roster is split in half (which breaks when the two benches differ
  in size). A stored replay opened later has no such source and still uses the half split.
* User-facing wording no longer mentions "M12" (values only; the translation keys keep their old
  names to avoid churn). All 12 locales updated.

Unchanged by design: every Rust command, the replay schema, persistence, finalisation. Viewing never
finalises: *Back* from the 2D viewer before confirming only drops the front-end state (the prepared
replay stays in `StateManager` and is overwritten on the next preparation); the read-only viewer's
only exit is a callback that closes it.

## 3. Behaviour guaranteed by tests

| Scenario | Test |
|---|---|
| Quick simulation vs 2D watch call different commands; double clicks are ignored | `MatchSimulation.test.tsx` |
| Leaving the 2D viewer without confirming never calls `finish_spatial_replay` / `finish_live_match` | `MatchSimulation.test.tsx` (player and spectator) |
| Confirming finalises the prepared replay exactly once | `MatchSimulation.test.tsx` (existing) |
| Post-match replay reads `load_spatial_replay` with the stable competition/fixture ids, finalises nothing | `MatchSimulation.test.tsx` |
| Missing replay → explicit "not recorded" message, no empty viewer | `MatchSimulation.test.tsx`, `ScheduleTab.test.tsx` |
| 22 players + ball painted; 21 after a red card; bench never painted; uneven benches | `M12ReplayViewer.test.tsx`, `m12Replay.test.ts` |
| Pause / play / speed / timeline seek | `M12ReplayViewer.test.tsx` |
| Frames-less, wrong-schema, non-JSON, inconsistent documents | `m12Replay.test.ts`, `M12ReplayViewer.test.tsx` |
| Spectators see the three options; only Start Match enters the live stage | `MatchSimulation.test.tsx` |

The fixtures (`m12Replay.fixtures.ts`) are hand-built to the shape that `spatial_match_output.rs`
validates. **They are not M12 output** and prove nothing about the engine.

## 4. Known limitations (not fixed here)

* **Live and delegated matches never have a 2D replay.** They run on the classic engine; producing
  spatial data for them would mean running M12, which changes the result of the match. Out of scope.
* `SpatialMatchResponse` returns the game but not the `round_summary` / `cup_round_summaries` that
  `finish_live_match` returns, so the round-digest screen after an M12 match has no summary. This
  predates Run D and is a Rust change; left alone to keep Runs A–C's surfaces untouched.
* The half-split home/away fallback for stored replays (see §2).

## 5. What must be validated locally with the real M12 engine

On the machine that has `~/apfm-match-lab`:

1. `scripts/apfm/run-m12-tests.sh` — the six M12 tests that fail by construction against a stub
   must pass.
2. Play one match through each of: Start Match, Watch in 2D → Go to result, Quick simulation. Check
   in the schedule that only the last two offer a replay, and that the message for the first is
   "No 2D replay was recorded".
3. Watch in 2D: confirm at kick-off **22 players** are painted (11 per colour) and the line
   "Players on the pitch" reads 11 home · 11 away. Seek to a card/substitution and confirm the count
   and the shirt labels follow it.
4. **Roster order**: the real replay's `roster` must list every home player before every away
   player. A stored replay relies on that (and on equal benches) because the document carries no
   side per roster entry. If it does not, the stored-replay viewer will colour players wrongly; the
   fix is for M12 to emit the side per roster entry.
5. **Event vocabulary**: only `goal`, `card`, `substitution`, `injury` are translated; list the
   other `type` values M12 really emits and add the ones worth a label.
6. Coordinate system: the viewer assumes a 105 × 68 m pitch with the origin bottom-left, as the
   contract enforces (`PITCH_LENGTH_M`/`PITCH_WIDTH_M`).
7. Quick simulation then "Replay in 2D": the score on the scoreboard at the last frame must equal
   the stored result.
8. Re-run `cargo test --lib` and the `ofm_core` M12 tests to confirm finalisation is untouched.
