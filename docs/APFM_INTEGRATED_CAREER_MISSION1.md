# Mission 1/5 — integrated playable career (2026-11-20)

Branch `apfm/main`, from `7114af8a` (the audit-corrections merge; `origin/apfm/main` is still at `48e07994`
and was not touched). Nothing was pushed, reset or deleted.

The three earlier corrections were confirmed present before any work started: mid-match save/exit
(`MatchSaveControls`, `useMatchSave`), crash-safe save index (`write_atomically`,
`load_index_or_quarantine`), fail-closed blocker checks (`BlockerPolicy`, `checkBlockingActions` rethrows).

## 1. Desktop verification status

**The real macOS window was NOT operated.** The session has no computer-use tools, `osascript` is refused
(`not allowed assistive access`, UI scripting disabled) and `screencapture` is refused (`could not create
image from display`). Every "window" cell below is therefore **NOT VERIFIED**, and nothing in this document
should be read as desktop playtesting. jsdom tests are not desktop playtesting.

What was done instead, and what it proves:

* A release binary of the final source (`target/release/openfootmanager`, built `--no-bundle`, so the earlier
  `.app` was not overwritten) was launched with `scripts/gui-validation/run-isolated.sh --smoke 25`. The webview
  loaded the bundled frontend (`AP Football Manager v0.3.0-nightly · 85532d5`), the backend answered
  `get_settings` and `get_active_game`, the game wrote only into the throw-away root and the harness found no
  change in any protected folder. **Launch only: nobody clicked anything.** The throw-away root was deleted.
* The Rust application layer was driven through the same functions the Tauri commands call, with the real engine
  (`application::playable_journey`, `interactive_match`, `time_advancement`; 51 passed, 10 ignored).
* A static cross-check of every frontend `invoke("…")` against `generate_handler!` and the Rust parameter names
  (200 calls, 204 commands): no unregistered command and no argument-name mismatch. The key response DTOs
  (`ScoutingTargetView`, `AcademyRow`, `AcademyReview`, `AdvanceTimeWithModeResponse`) match field for field.
* The shipped capability file grants `dialog:allow-message`, which the new error dialogs use.

## 2. Source audit

Classification: **CD** confirmed defect, **UR** unverified risk, **OK** working as designed.

| # | Area traced (frontend → Tauri → application → core) | Finding |
|---|---|---|
| 1 | Career creation, world, club: `MainMenu` → `start_game` → `useTeamSelection` → `select_team` | **OK.** Double-submit guarded, errors translated, portrait prewarm failure non-fatal. |
| 2 | Squad, tactics, training | **OK** for state (the screens render the backend game, so a refused change leaves a truthful view). **UR/P2:** `TrainingTab`, `TrainingGroupsCard`, `useTacticsLineup`, `TacticsRightPanel`, squad context-menu list toggles log a refused change and show nothing. Not fixed (no stale data, no lost progress). |
| 3 | Scout assignment, reports, shortlist, compare | **OK.** Dossier and review refetch on every game change; "No scouts free" replaces the button. Smoke step 4 says "choose a scout": the UI assigns the first free one (doc mismatch, not a defect). |
| 4 | Youth academy promotion and limits | Backend **OK** (`set_player_squad_role` calls `promotion_place_refusal`; 30-senior limit tested). **CD-1:** prospect list went stale after Promote on the review card (see §3). |
| 5 | Calendar, decisions, match prep | Blockers fail closed in the UI and in Rust (earlier correction); fast-forward stops on blockers inside its backend loop (**OK**). **CD-2:** a backend failure while advancing/skipping was invisible (see §3). Match day is deliberately exempt from the Rust blocker check; the pre-match flow owns the line-up. |
| 6 | Match orders, save, exit, load, resume | **OK** (single call queue, resync after failure, unique command ids, accepted orders survive a reload; `saving_and_returning_to_the_menu_keeps_the_match_resumable`). **CD-3:** pre-match line-up failures (see §3). **CD-4:** restore failure silent. |
| 7 | Match finalization, post-match effects | **OK** for finish-once (`hasFinalizedMatch`, `finishRequested`, `a_live_match_cannot_be_advanced_past_or_finalised_twice`). **CD-5:** a failed `finish_live_match` on the classic path showed nothing. |
| 8 | Finances, standings, statistics | **OK** by the backend integration tests; the bid form is disabled while in flight and offer responses are idempotent by offer id. Not exercised from a window. |
| 9 | Season rollover and next season | Backend **OK** (`seasonNotComplete` on early or repeated rollover). **CD-6:** the end-of-season screen swallowed a failed rollover. |
| 10 | Errors, loading, navigation, recovery | **CD-7:** a failing `get_saves` showed an empty Load list, which reads as lost saves. **UR/P2:** `Dashboard` initial state fetch and `handleDeleteSave` only log. |

## 3. Confirmed defects and fixes

Each fix was preceded by a failing regression test (seen red for the predicted reason). Translations are in
all 12 locales; the `src/i18n` gate passes.

| ID | Defect | Commit | Regression test(s) |
|---|---|---|---|
| CD-2 | `advance_time_with_mode` / `skip_to_match_day` rejections were only logged: Continue stopped spinning with no message (priority 6) | `ae3b33bf` | `useAdvanceTime.test.tsx`: "says so when the backend fails while advancing a day", "…while skipping to match day" |
| CD-4, CD-5, CD-7 | Restore failure bounced to the dashboard silently; a failed finish left the manager on a finished match; a failed save listing looked like lost saves | `0c73acab` | `MatchSimulation.test.tsx` (two), `MainMenu.test.tsx` (one) |
| CD-3 | Auto-select applies swaps one at a time and refreshed the screen only after the last: a refused swap left earlier swaps in the backend while the screen showed the old eleven (stale data after a partial mutation); formation, style, swap and set-piece failures were silent | `e1c11405` | `PreMatchSetup.test.tsx`: partial auto-select, refused formation. **Mutation:** removing the `finally` refresh makes the partial test fail; restored |
| CD-6 | `advance_to_next_season` failure swallowed on the screen that replaces the Home tab | `f89560e7` | `EndOfSeasonScreen.test.tsx` (two) |
| CD-1 | `YouthAcademyTab` was the only squad-caching tab not keyed on the game revision: after Promote on the review card the player stayed listed as a youth and in the totals until the clock moved | `85532d53` | `YouthAcademyTab.test.tsx`: "refreshes the prospects when the game changes without the clock moving" |

Design notes: the restore and finish paths read the translator through a ref so a language change cannot restart
a match (the earlier hardening rule). No new command, transaction or component family was added; every fix reuses
`showError`, `resolveBackendKeyError`, `useFetchedSquad`'s existing revision argument and the existing alert style.
No Rust production code changed.

## 4. Acceptance matrix (the 15 manual smoke steps)

"Automated" is what was actually run. "Window" is the real desktop window: **NOT VERIFIED on every row**, for
the reason in §1. A row is PASS only for the cited automated evidence.

| # | Step | Automated | Window |
|---|---|---|---|
| 1 | Launch, main menu, no error banner | **PASS (launch only):** release binary under the isolation harness, backend answered | NOT VERIFIED |
| 2 | New game → manager → world → club → dashboard | **PASS:** source trace + `TeamSelection`/`useTeamSelection`/`MainMenu` tests; career setup in `playable_journey` | NOT VERIFIED |
| 3 | Squad, Tactics, Training; change formation | **PASS:** `squad_and_tactic_choices_are_kept_and_survive_a_reload`, component tests. P2 silent failures noted | NOT VERIFIED |
| 4 | Scouting: assign a scout, days left | **PASS:** `ScoutingTab` tests, `send_idle_scouts` in the journey | NOT VERIFIED |
| 5 | Report with ranges, never an exact ability | **PASS:** `scouting_dossier` tests (`an_estimate_contains_the_truth_without_being_centred_on_it`, `the_dossier_and_the_academy_review_show_only_what_is_known`), run in the workspace pass | NOT VERIFIED |
| 6 | Shortlist two players, compare | **PASS:** I7 dossier/compare component tests | NOT VERIFIED |
| 7 | Youth academy, Promote | **PASS after CD-1:** `an_academy_promotion_survives_a_reload`, new stale-list test, limit tests | NOT VERIFIED |
| 8 | Continue to Match Day, auto-select XI | **PASS after CD-2/CD-3:** blocker tests, `a_live_match_kicks_off_through_the_squad_floor_gate`, new line-up tests | NOT VERIFIED |
| 9 | Manage the match, Play: pitch, clock | **PASS (engine/application only):** `interactive_match` tests with the real engine; the pitch rendering was not seen | NOT VERIFIED |
| 10 | Speed, highlight mode, pause/play | **PASS (logic only):** `liveDirector`/`useLiveMatch` tests; picture and timing not seen | NOT VERIFIED |
| 11 | Pause, substitution, tactics change | **PASS (logic only):** order-queue and stale-revision tests | NOT VERIFIED |
| 12 | Save mid-match, menu, Load, resume | **PASS:** `saving_and_returning_to_the_menu_keeps_the_match_resumable`, `a_watched_match_saved_midway_…` | NOT VERIFIED |
| 13 | Half time, full time, Finish once | **PASS after CD-5:** `a_live_match_cannot_be_advanced_past_or_finalised_twice`, finish-once component tests | NOT VERIFIED |
| 14 | Save, quit, Load; scouting and academy intact | **PASS:** `an_academy_promotion_survives_a_reload`, `a_watched_match_saved_midway_keeps_the_dossier_…`; old-save copies | NOT VERIFIED |
| 15 | Season end → next season, 15–30 seniors | **PASS after CD-6:** `a_reloaded_career_completes_two_seasons` (see §5), `season_advance_rolls_the_season_over` | NOT VERIFIED |

## 5. Validation

Run on the final code (`85532d53`; the only later change is this document).

| Gate | Result |
|---|---|
| Focused tests first (`useAdvanceTime`, `MainMenu`, `MatchSimulation`, `PreMatchSetup`, `EndOfSeasonScreen`, `YouthAcademyTab`, `src/i18n`) | each red before its fix, green after |
| `cargo test --locked --workspace` | exit 0; 162 suites, **5250 passed, 0 failed, 133 ignored** |
| `a_reloaded_career_completes_two_seasons` (ignored by default, run with `--release -- --ignored`) | **passed** (22 s) |
| `cargo test --lib application::` (journey, interactive match, time advancement, saving) | 51 passed, 0 failed, 10 ignored |
| `old_saves_load_and_play_on` on **copies** of 3 real saves (`APFM_OLD_SAVES_COPY`) | **passed**: loaded and migrated, 60 days played, a match finalised once, save and reload identical. SHA-256 of all 9 files in the real saves folder identical before and after |
| `cargo fmt --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | both clean |
| `npm test` | 358 files passed (1 skipped); **3176 tests passed**, 4 skipped, 0 failed |
| `vitest run src/i18n` | passes (also inside `npm test`) |
| `npm run build` (tsc + vite) | passes |
| `npm run lint` | passes (3 pre-existing infos, no warnings or errors) |
| `npm run format:check` | passes |
| `npm run knip` | passes (two configuration hints, no findings) |
| `npm run audit:i18n` | advisory only; lists out-of-scope MCP/CLI/adapter strings; not a gate |
| Release binary launch under the isolation harness (25 s) | started, backend answered, nothing outside the isolated root changed (launch only) |

The 133 ignored Rust tests are long-career studies and tests that need external files; only the two named above
were run. Not run: the four repository reviewer agents, and any window interaction.

Old-save note: the three copies loaded; one of them played 0 own matches in its 60 simulated days (it starts in
June, before the fixtures), which the test accepts. Real saves written by this build with a dossier or a match in
progress were not available to copy.

## 6. Remaining P0 / P1

**None known.** Open P2 (not fixed, none loses progress or shows false data): refused training, tactics, squad
list-toggle and academy-delete actions log without a message; the Dashboard's first state fetch logs without a
message; the style-size limits already recorded for `InteractiveMatchScreen`. The four repository reviewer agents
were **not run** on this diff.

**Not verified by anyone:** every on-screen behaviour (layout, pitch animation, timing, focus, theme, language).
The manual smoke test and `APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md` are still required.
