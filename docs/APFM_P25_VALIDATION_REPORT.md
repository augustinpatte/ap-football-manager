# APFM P2.5 validation report

Branch `claude/apfm-playable-career`. Starts from `38d42f31`; the commits below are all on top of it.
Not merged into `apfm/main`. M12 and the external 2D viewer were not touched.

## Verdict

**The automated evidence is green. The desktop application has still not been played by anyone.**
No tool for driving a window was available in this session, so no GUI journey was performed. What
was done on the GUI side is a launch smoke test (below), which proves the real binary starts and
stays inside its isolated data root, and nothing about the screens. "A new player can complete two
seasons from the window" remains **unverified** until a person fills in
`APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`.

## Commits (oldest first, all on this branch)

| Commit | Change | Kind |
|---|---|---|
| `3ce15a34` | The mirrored side-pair swap no longer runs on save writes; on load it runs only for saves below format 9; legacy-blob migration still runs it | fix (confirmed bug) |
| `a64e30a8` | Staff tab opens on Management when proposals are pending, so the dashboard alert lands on Accept/Decline; flow test `DelegationAlertFlow.test.tsx` | fix (confirmed gap) |
| `6d2ee5b6` | The match-day lineup alert is listed above the proposals alert | fix (reviewer finding) |
| `19ea02c4` | Proposals-alert wording: pt register, zh-CN term | i18n polish |
| `1c923621` | The flow test's fake proposal carries the params the real string takes; French raw-key check widened | test |
| `6c661ee3` | Inverted-XI tests for every save writer; gate constant renamed `FIRST_FORMAT_WITHOUT_XI_SIDE_REPAIR`; reuses `stamp_format` | test, rename |
| `38dac225` | Journey tests that discriminate: real formation change, join-on-registration-date, reject/accept proposal pair | test |

## 1. Validation run

Counts are as printed by the tools. "Not run" means not run.

| Check | Result | At |
|---|---|---|
| `cargo test --locked --workspace --no-fail-fast` | **122 targets, 4,710 passed, 0 failed, 76 ignored**, no warnings (previous audit baseline: 4,698 passed, 75 ignored) | `3ce15a34` + the frontend commits; the later Rust commits are test-only plus a behaviour-neutral constant rename, and are covered by the lib lines below |
| `cargo test --locked -p db --lib` | 441 passed, 0 failed, 1 ignored | final Rust state |
| `cargo test --locked --lib` (root package) | 563 passed, 0 failed, 8 ignored | final Rust state |
| `cargo test --locked --lib --features mcp` | 649 passed, 0 failed, 8 ignored | final Rust state; **lib target only, not the whole workspace with `mcp`** |
| `cargo test --release --lib a_reloaded_career_completes_two_seasons -- --ignored` | passed, 13.4 s (2 min 17 s build) | final Rust state |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean | final Rust state |
| same with `--features mcp` | clean | before the last test-only commit |
| `cargo fmt --all -- --check` | clean | final |
| `npm run build` | pass | final |
| `npm test` | 333 files passed, 1 skipped; **2,930 passed**, 4 skipped (baseline 2,920 at the start of the branch work, 2,923 at the last report) | final |
| `npm run format:check`, `npm run lint` | clean | final |
| `npm run knip` | exit 0 | final |
| `npm exec --no -- vitest run src/i18n` (locale and key-coverage gates) | 9 files, 35 tests passed | after the pt/zh-CN edits |
| `scripts/check-toolchain-pin.sh` | agrees on 1.95.0 | final |
| `scripts/gui-validation/run-isolated.test.sh` | all passed | final |
| `npm run audit:i18n` | advisory; flags `playable_journey.rs` test `expect` messages, which are out of scope per CLAUDE.md | final |
| `cargo deny`, `cargo machete` | **not run** (neither is installed here) | |
| `/preflight` as a single skill invocation | **not run**; its component scripts (format, lint, knip, build, test, clippy default and mcp) were run individually as listed | |
| Full workspace `--features mcp` suite | **not run** (lib target only, above) | |

### Mutation checks (the named test must fail without the fix)

* Starting-XI fix: with the load-time gate removed (`|| true`), `test_load_game_keeps_an_inverted_starting_xi_in_a_current_format_save` and `test_inverted_wingers_survive_save_quit_and_reload` fail with the manager's order swapped. This also proves the winger fixture actually triggers the swap. The tests for the original write path were red before the fix for the predicted reason (the write path swapped the pair).
* Staff-tab fix: the flow test failed (4 of 6) before the fix with "no Accept button after the alert click"; the test reviewer independently removed the fix in a disposable copy and saw the same four fail.
* Alert order: the ordering test failed with `3 < 2` before the move.

### Reviewer agents (read-only, run on `5f5a4dfc..` the branch)

| Agent | Outcome |
|---|---|
| `i18n-auditor` | No blocking defect. Gates pass; key present in all 12 locales; placeholders intact. Advisory: pt register (fixed), zh-CN term (fixed), and wording that names the Staff area differently from the tab in es/pt/de/cs/tr (**left as is**; needs a native-speaker decision). |
| `ui-accessibility-reviewer` | Nothing introduced by the diff beyond the ordering note (fixed). Pre-existing, in the not-yet-finished Staff folder: view toggle has no `aria-pressed` and no project focus ring, raw palette colours, search box without a label, focus lost when an alert navigates, no live region for alert changes. **Not fixed**; listed under remaining blockers. |
| `ofm-architecture-reviewer` | The format-9 gate is sound: format 9 postdates the repair, every pre-fix writer canonicalised, so no format-9 save can hold an unrepaired pair. Constant name was misleading (fixed). `load_game` is ~230 lines, pre-existing size debt. |
| `ofm-dedup-reviewer` | No hard duplicates. Duplicate test helper (fixed). Not fixed, for epic #589: load sequence copied into `playable_journey.rs` instead of a shared `load_career`; `play_interactive` and standings helpers repeated from `interactive_season.rs`; no shared `createGameState` in `src/test-utils`. |
| `ofm-test-reviewer` | Frontend mutations all killed. Findings acted on: write paths other than `create_save`/`save_game` were untested (now covered); 4-4-2 assertion could not discriminate (now 4-3-3); transfer test did not check the join date (now does); decline assertion was vacuous and accept was never exercised (now a pair). Not acted on: the host in the flow test replaces `Dashboard.handleNavigate`, so the real routing from alert to the `Staff` tab is an inferred seam. |

## 2. Starting XI persistence

**Cause.** `canonicalize_game_starting_xi_ids` ran on a clone of the career at every save write
(four writers) and again on every load, swapping a mirrored side pair (LB/RB, LWB/RWB, LM/RM, LW/RW)
whenever the swap raised positional fit. An explicitly chosen inverted full-back or winger was
therefore undone on the next reload, and the file never held the manager's order.

**Fix (`3ce15a34`).** No write path normalises any more. Load applies the repair only when the save's
stored format is below 9; legacy-blob migration still applies it. No schema change, no format bump,
no new field. Auto lineup generation is untouched (it already assigns slot by slot).

**Regression tests (db crate, all green).**

| Required | Test |
|---|---|
| 1. Inverted full-back | `test_create_save_keeps_a_deliberately_inverted_full_back_pair`, `..._save_game_keeps_..._when_overwriting`, and one each for `create_save_with_stats`, `save_game_with_stats`, `save_game_with_stats_and_match` |
| 2. Inverted winger | `test_inverted_wingers_survive_save_quit_and_reload` (4-3-3) |
| 3. Normal lineup | `test_a_naturally_sided_starting_xi_round_trips_unchanged` |
| 4. Old-save compatibility | `test_load_game_repairs_mirrored_starting_xi_order_in_a_pre_format_9_save`; `legacy_migration` test `..._canonicalizes_mirrored_starting_xi_order` still passes |
| 5. Save, quit, reload | the winger test writes twice, drops the manager, reopens with a fresh `SaveManager` and also reads the raw column; `test_load_game_keeps_an_inverted_starting_xi_in_a_current_format_save` loads a current-format save |

One consequence to know about: a save already at format 9 that an old build wrote with the repair
applied stays as written; nothing re-inverts it. A pre-9 save is repaired once on first load, then
restamped at 9.

## 3. Delegation alerts, end to end (frontend)

`src/components/dashboard/DelegationAlertFlow.test.tsx` mounts the real alert model
(`getDashboardAlerts`), the real `DashboardAlerts` and the real `StaffTab` with real i18n, over a
stateful backend fake that hands the changed game back as `resolve_delegation_proposal` does.

| Required | Test (all pass) |
|---|---|
| 1. Pending proposal creates an alert | "raises a dashboard alert with the number of pending proposals"; "raises no alert when the assistant holds nothing" |
| 2. Click opens the correct Staff screen | "opens the Staff screen on the proposals when the alert is clicked". **This found a defect**: the Staff tab opened on "My staff" with the proposals one sub-tab away. Fixed in `a64e30a8`. |
| 3. Accept / reject updates state | "answers yes to one proposal and no to another, and the alert count follows" (checks the `accept` flag each call carried and that the count falls 2 to 1) |
| 4. Alert disappears at zero | "removes the alert once the last pending proposal is answered" |
| 5. English and French | "reads in French: ..." plus the English tests; no raw `dashboard.alerts`, `staff.delegation` or `delegation.(proposal|reason)` key left on screen |

Limit: the host stands in for `Dashboard`; the real `Dashboard.handleNavigate` is not in the path.

## 4. The delegated match response and `fixture_id`

**No defect, nothing changed.** Traced every consumer of the `advance_time_with_mode` response:

* `useAdvanceTime.ts:156` is the only place `result.fixture_id` is read, inside the `live_match`
  branch that navigates to the match screen. The backend always sets it there
  (`time_advancement.rs:172`, asserted in `commands/time.rs:1358`).
* The `advanced` branch (delegated or day-by-day) never reads it. It builds the digest from the
  returned game and `result.results` (`AdvanceMatchResultData`, which carries its own fixture
  identity). `MainMenu.tsx:497`'s `fixture_id` belongs to the resumable-match type, a different struct.
* The backend sets `fixture_id: None` for the delegate path by design (`time_advancement.rs:246`).

The engine and the response type were left alone.

## 5. GUI verification

| Item | Status |
|---|---|
| Computer or GUI automation tool available | **No.** Searched for one; none exists in this session. |
| `scripts/gui-validation/run-isolated.sh --no-build --smoke 25 --world-db ~/apfootballmanager/data/world.db`, after `npm run tauri -- build --debug --no-bundle` | **Passed.** The binary started and ran 25 s; it wrote into the isolated root (`saves/sync_info.json`); no protected folder changed; `world.db` byte-identical. Real saves untouched. |
| Career journey through the window | **Not performed.** |

The smoke run proves startup and isolation only. It says nothing about any screen.

## 6. Settings schema, column chooser

Not implemented. The schema change is authorised, but the whole chooser is a feature (per-table
state, a UI, tests), not a small independent change, and the instruction was not to build it in this
phase. No settings field was added, so no settings file changed.

## 7. Confirmed bugs fixed

1. Reload and save reordered a deliberately inverted starting XI (`3ce15a34`).
2. The delegation-proposals alert opened the wrong sub-view of the Staff tab (`a64e30a8`).
3. The proposals alert outranked the match-day lineup alert (`6d2ee5b6`), and two translation details (`19ea02c4`).

## 8. Manual GUI tests still outstanding

Everything in `APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`, all 10 sections, four passes (light/dark,
EN/FR). Updated rows: 2.6 (expect the eleven unchanged), 7.3 (alert sits below match-day alerts),
7.4 (lands on the Management view with Accept/Decline visible). Run with
`scripts/gui-validation/run-isolated.sh --world-db ~/apfootballmanager/data/world.db`.

## 9. Can prompt 3 safely begin?

**Not on the strength of automation alone.** Backend, persistence and the delegation flow are
covered and green, and the one XI bug is fixed. But the brief itself says a green suite is not a
verified desktop application, and no person has run the window. Prompt 3 (management UX
redesign) would be built on screens nobody has seen working end to end. Recommended gate: one
person completes checklist sections 1, 4, 5 and 8 (career start, calendar, match, save/reload) in
one theme and one language, and files any defect. If that comes back clean, prompt 3 can start while
the rest of the checklist continues. This is a recommendation; the decision is yours.

## 10. Remaining blockers and open items

* **Blocker:** no manual GUI pass (above).
* Staff folder accessibility debt (not in `FINISHED_FOLDERS`): toggle buttons without `aria-pressed` or the project focus ring, raw palette colours, unlabelled search input; focus is not moved after an alert navigates; no live region for alert changes.
* Translation wording of the alert versus the tab name in es/pt/de/cs/tr needs a native speaker.
* Journey tests play two seasons in `delegate` mode (in-house engine), not M12; M12 is exercised once in the live-match test. Tactic changes reaching the simulation rest on the F4 report, not re-run.
* Journey tests use the 80-club gate world, not the shipped 440-club world.
* `cargo deny`, `cargo machete` and the whole-workspace `--features mcp` suite were not run.
* Test-support duplication named for epic #589 (above).
* Open owner decisions carried over: model versus picker (audit §9.1), `delegate` using `engine` versus M12 (§9.3), column chooser scope.
