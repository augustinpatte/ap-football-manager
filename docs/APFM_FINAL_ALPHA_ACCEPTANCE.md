# AP Football Manager — final alpha acceptance (mission 5/5)

Date 2026-11-20. Branch `apfm/main`, local only (nothing pushed, reset or deleted). Previous release candidate:
[`APFM_ALPHA_RELEASE_CANDIDATE.md`](APFM_ALPHA_RELEASE_CANDIDATE.md).

**Read this first.** Every automated gate is green and the backend journey was run end to end in one career, but
**nobody operated the game's window**: the session has no display and the accessibility API is refused
(section 4). Manual desktop acceptance is therefore **outstanding**, and no GUI step below is reported as passed.

## 1. Final SHA

| | |
|---|---|
| Last code commit (the release build and every gate below are of this) | `2c00d4003d1ccb8a1ff9e61a6c6ddae7d13f3de5`, `2c00d400` |
| Final HEAD | the commit that adds this file: `git rev-parse HEAD` on `apfm/main`. It changes documentation only |
| Commits by this mission | `2c00d400` (a test, the only change to Rust code: production code is identical to `83cb1e69`), plus this document |
| Missions 1-4 present | verified in `git log` (`2c894e6e` mission 1, `9cf4db5d`/`104fa882` mission 2, `585949d6` mission 3, `83cb1e69` mission 4) and by their tests in the green suites |

Not assumed from earlier reports: the four reviewers below read the whole `48e07994..83cb1e69` diff (44 commits,
140 files) and found the features wired in, and the journey test (section 5) drives them through the same
application-layer functions the commands call.

## 2. Validated gameplay systems

"Validated" means exercised by an automated test through the application layer, not seen in a window.

| System | Evidence |
|---|---|
| New career, club, world generation | `Session::begin` (generated 80-club world), `install_career`, first save |
| Squad, formation, preset, eleven, bench swap in its slot | `squad_and_tactic_choices_are_kept_and_survive_a_reload`, journey step 2 |
| Tactics, training focus, intensity, schedule | journey step 2 (set, read back) |
| Staff and delegation (approval mode, proposal accepted/rejected, manual always allowed, no assistant refused) | five `playable_journey` tests, journey step 3 |
| Scouting: scouts sent, report delivered, shortlist, dossier survives reload | `a_watched_match_saved_midway_keeps_the_dossier_and_is_finalised_once`, journey step 4; rating projection tests (mission 2) |
| Youth: ceiling band from the staff, advice, promotion, survives reload | `an_academy_promotion_survives_a_reload`, mission 3 tests, journey step 5 |
| Transfers: bid, contract demand, acceptance, registration on a later date, budget refusal | `a_transfer_is_scheduled...`, `a_bid_above_the_transfer_budget_is_refused...`, journey step 6 |
| Finances: cash equals its journal, the fee booked once | `journal_matches_cash`, one `TransferFeeOut` post per signing, unique post ids, at every stage of the journey |
| Calendar, blocker check before a match, one advance one day | blocker tests (mission 1), `a_live_match_cannot_be_advanced_past_or_finalised_twice` |
| Interactive match: frames, pause, tactical order, substitution, stale-revision refusal | journey steps 9-11, `interactive_match` tests |
| Save in the middle of a match, reload, match resumes | journey step 10, `a_watched_match_saved_midway...` |
| Match finalised exactly once, table moves once | journey step 11 (second finish refused; the league row moves by exactly 1) |
| Season completion, rollover, save and reload in the next season | journey steps 12-14, `a_reloaded_career_completes_two_seasons`, 20-season simulation |
| Save safety: crash-safe index, quarantined damaged index, failed write keeps the last good save | db tests (`a_failed_write_keeps_the_previous_index`, `a_failed_backup_leaves_the_run2_save_unrepaired`, rebuild tests) |

## 3. Automated test totals

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean (final HEAD) |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean, default and `--features mcp` (final HEAD) |
| `cargo test --workspace --locked` (code `83cb1e69`) | exit 0; 166 suites, **5320 passed, 0 failed, 135 ignored** (12 m 38 s) |
| `cargo test --workspace --locked --features mcp` (same code) | exit 0; 166 suites, **5407 passed, 0 failed, 135 ignored** |
| `cargo test --lib --locked`, final code `2c00d400` | **621 passed**, 0 failed, 14 ignored (includes the new journey test) |
| Same with `--features mcp` | **708 passed**, 0 failed, 14 ignored |
| `npm test` | 364 files passed (1 skipped); **3217 tests passed**, 0 failed, 4 skipped |
| `npm exec --no -- vitest run src/i18n` (the real i18n gate) | 9 files, 35 tests passed |
| `npm run build` (tsc + vite) | passes (not inferred from Vitest) |
| `npm run lint` | passes, 3 infos |
| `npm run format:check` | passes (Biome 2.5.5 from `node_modules`, checked with `./node_modules/.bin/biome --version`) |
| `npm run knip` | passes (two configuration hints, no findings) |

Honest notes: the full workspace run was made on `83cb1e69`; the only later change is a test inside
`playable_journey.rs`, so the lib runs above are the complete re-check of what changed. The first clippy pass
finished in under a second because the artifacts were fresh; the later pass compiled the changed crate and was
clean (it caught one `int_plus_one` in the new test, which was fixed). The 135 ignored Rust tests are long studies
and tests that need external files; the ones this mission needed were run explicitly (sections 5-7).

### Reviewers (read-only, static: they were told not to run cargo)

| Reviewer | P0/P1 | Notes |
|---|---|---|
| `ofm-architecture-reviewer` | none | blocker gate, save index durability, projection, perf paths, layering all sound |
| `ui-accessibility-reviewer` | none | P2s listed in section 9 |
| `ofm-test-reviewer` | none | every key fix has a discriminating test; no fix-removal mutation was re-observed |
| `i18n-auditor` | none | 76 new keys present in all 11 other locales, none copied English |
| `ofm-dedup-reviewer` | none | two small duplications, no release impact |

Their findings are summarised in section 9. None was inflated into a blocker.

## 4. GUI evidence and what is not verified

**Technical integration evidence (done):** the backend journey above; the frontend component and hook tests
(3217); the production build; and a launch of the new release binary.

**Desktop evidence (done, narrow):** the release binary `…/AP Football Manager.app/Contents/MacOS/openfootmanager`
was started twice on an isolated data root. Once through `scripts/gui-validation/run-isolated.sh --binary … --smoke 25`
(exit 0: the game wrote only into the isolated root, no protected folder changed) and once by hand with
`APFM_TEST_DATA_ROOT`. The log shows `AP Football Manager v0.3.0-nightly · 2c00d40`, then the webview calling
`get_settings` and `get_active_game`, with no warning or error. This proves the bundle starts, the frontend loads
and reaches the backend, and that the revision in the binary is the one tested.

**Direct GUI control was not available.** `screencapture` fails with "could not create image from display";
System Events answers `osascript is not allowed assistive access (-25211)`; the app reports 0 windows to it.
Nothing was clicked or looked at. Every step that needs a person is **NOT VERIFIED**:

* every row of [`APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`](APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md) (all four theme/language
  passes), including layout, colour/contrast, focus rings, keyboard use, screen readers, window resizing;
* the screens that no backend test can show: the 2D pitch picture, speed and highlight-mode controls, the
  comparison view, the end-of-season screen, the blocker dialog, the language switch with a match open;
* launching with `open` / Finder (LaunchServices), as the owner will.

## 5. The 15-step acceptance matrix

The steps are those of [`APFM_MANUAL_SMOKE_TEST.md`](APFM_MANUAL_SMOKE_TEST.md). **Technical** = automated, through the
application layer, in `application/playable_journey.rs` (`the_whole_alpha_journey_runs_in_one_career` unless named).
**Desktop** = a person operating the window.

| # | Step | Technical evidence | Desktop |
|---|---|---|---|
| 1 | Start the app, main menu | the release binary starts; frontend loads; `get_settings`, `get_active_game` answered | **NOT VERIFIED** (no window seen) |
| 2 | New game, manager, world, club | `Session::begin` + `install_career` (generated world, club, first save); the form and menus are not exercised | **NOT VERIFIED** |
| 3 | Squad, Tactics, Training; formation change redraws the eleven | preset, formation, eleven, training focus/intensity/schedule set and read back (step 2 of the journey) | **NOT VERIFIED** |
| 4 | Scouting: send a scout | scouts sent as the screen's action does (`send_idle_scouts`) | **NOT VERIFIED** |
| 5 | A report arrives, as ranges not exact numbers | report delivered in the journey; range/"unknown" display pinned by the mission-2 Rust and Vitest suites (`rating_view`, `ReadableRating`) | **NOT VERIFIED** |
| 6 | Shortlist two players, compare | shortlist (journey); compare is frontend-only, Vitest | **NOT VERIFIED** |
| 7 | Youth Academy review, promote | review read, one prospect promoted, senior squad grows (journey; `an_academy_promotion_survives_a_reload`) | **NOT VERIFIED** |
| 8 | Continue to match day, eleven set | days advanced in `live` mode to the next league match, a full eleven set | **NOT VERIFIED** |
| 9 | Manage the match, pitch with players | interactive match started, frames captured | **NOT VERIFIED** (the picture itself) |
| 10 | Speed, highlight mode, pause | pause and resume (journey); speed and highlight are frontend controls (Vitest) | **NOT VERIFIED** |
| 11 | Substitution and tactics change while paused | both orders accepted; at full time the substitute is on the pitch and the starter is in `used` | **NOT VERIFIED** |
| 12 | Save in the match, restart, load, match waiting | save mid-match, reopen the folder, `restore_saved_match`: the match is waiting, dossier unchanged, no result counted yet | **NOT VERIFIED** |
| 13 | Play to full time, finish once | played through the break to full time; finish applies the result once, a second finish is refused; the club's table row moves by exactly 1 | **NOT VERIFIED** |
| 14 | Save, quit, reload; scouting and academy intact | `save_close_and_reopen` compares the whole-world fingerprint; dossier/shortlist equal before and after | **NOT VERIFIED** |
| 15 | Season ends, next season starts | season completed, saved, rolled over, empty table, 30 days played, saved and reloaded identically; also two full seasons and 20 simulated seasons | **NOT VERIFIED** |

At every stage of the journey the test asserts: no fixture id twice, no league pairing duplicated, every table row
equal to the sum of the results naming it, every club's cash equal to its journal, no cash-post id twice, and the
signing's fee booked exactly once. A pre-season friendly is stored under the league's fixture list but is not in
its table; the journey plays those through and makes the interactive match the first league match.

## 6. Save compatibility

All checks used copies; the owner's folder was only read.

| Check | Result |
|---|---|
| Old saves load safely and play on | **all 7** real saves, copied to a scratch folder (the earlier pass covered 3): each loaded and migrated, played 60 days, watched one match finalised once, and a save/reload of the result was identical (`old_saves_load_and_play_on`, 60 s) |
| Owner's save folder untouched | SHA-256 of the 9 files before and after the migration test, the isolated launches and the build: identical |
| New saves reload | journey (fingerprint equality at every reload), `a_reloaded_career_completes_two_seasons` (14.7 s) |
| Mid-match checkpoint survives | journey and `a_watched_match_saved_midway...`; `a_stored_checkpoint_is_compact`, checkpoint tests in `ofm_core` |
| Scouting and academy state persists | dossier JSON equal across a mid-match reload; promotion and academy review equal across a reload; mission 2 and 3 reload tests |
| Invalid save index recovers without deleting data | db tests: truncated and non-UTF-8 index quarantined under a unique name and rebuilt; unusable `.db` files are left in place |
| Failed write keeps the last good save | `a_failed_write_keeps_the_previous_index`, `a_failed_backup_leaves_the_run2_save_unrepaired` |
| Season rollover persists | two-season test, journey |
| No duplicated fixture or cash post | invariants listed in section 5, and 20-season run (section 7) |

Not covered: a save written by an older build that held a scouting dossier or a match in progress (none of the real
saves has either). A real old save opened in the app is migrated in place and an older build cannot read it again,
so copy the folder first (section 10).

## 7. Multi-season simulation results (release build, this HEAD)

| Run | Result |
|---|---|
| `apfm_career_fp 1 gate 20` (80 clubs, 20 seasons) | exit 0; **invariants clean**; 0 clubs in debt in any season; 94.8 s (season 1: 3.8 s, season 20: 5.0 s); mean age 23.1-24.3; total cash grows 1.07e9 to 7.61e9 |
| `apfm_career_fp 1 shipped 3` (440 clubs, 13.9k to 15.8k players) | exit 0; **invariants clean**; 0 clubs in debt; 40.6 s, 50.0 s, 55.8 s per season (146.5 s) |
| `apfm_ff_bench 1 gate 3` | fast-forward and day-by-day give the same fingerprint in every window (7, 30, 90 days, one season with rollover) |
| `a_reloaded_career_completes_two_seasons` | passes: save/reload before the first season, between and after |

## 8. Performance benchmark results

Release build, this machine (Apple silicon), measured serially on an otherwise idle build session. Compared with
the mission-4 table (same commands):

| Measurement | Mission 4 | Now |
|---|---:|---:|
| gate, 20 seasons | 97.3 s | 94.8 s |
| shipped, 3 seasons | 154.4 s | 146.5 s |
| gate day-by-day, 7 / 30 / 90 days | | 190 / 819 / 2250 ms (about 27 / 27 / 25 ms a day) |
| gate fast-forward, 7 / 30 / 90 days | | 148 / 610 / 1553 ms (about 21 / 20 / 17 ms a day) |
| gate season, 304 days | | 6.22 s daily, 3.35 s fast-forward |
| Journey test (whole career to season 2, debug-optimised) | | 10 s |

Not measured here: the webview's transfer of the screen state (about 28 MB per day on the shipped world,
documented in mission 4), memory, and launch time to an interactive window.

## 9. Unresolved issues

### P0
None known.

### P1
None known. This is "none found by the evidence above", which excludes everything only a person at the window
could find; see section 4.

### P2 (accepted for the alpha)
Gameplay and data
* AI clubs do not run scouting; youth potential still predicts development very strongly; the AI reads true ratings
  by design (measured, no exploitation shown in mission 4).
* An interactive match is not stored as a replay; the highlight mode is not remembered; a match saved while
  watching a highlight resumes at the engine's head (up to about 90 simulated seconds ahead of the picture).
* After "Continue anyway" on the blocker dialog, skipping several days confirms again each day while a blocker stands.
* The hidden-rating rule holds on player-facing screens only. Documented leak surfaces
  ([`APFM_SCOUTING_INFORMATION_INTEGRITY.md`](APFM_SCOUTING_INFORMATION_INTEGRITY.md) section 3) are the full `Game` blob sent to the
  webview, youth message contexts, the MCP tools and market value as a proxy. The tests cannot defend those.
* Save and load grow 5-7x over five seasons; shipped careers were run only three seasons.

Accessibility and UI (reviewer findings, unverified in a window)
* `AcademyRowActions`: the `aria-label` does not contain the visible text (WCAG 2.5.3); focus falls to `body` after a promotion.
* `MatchSaveControls`: the "Saved" status is inserted already filled, which some screen readers do not announce.
* `PlayersListTab` and `GlobalMarketResults` sort headers are clickable `<th>` with no keyboard route (predates this
  range); one uses a brand ring instead of the focus token.
* `PlayerProfile`: if the rating request fails the profile logs and shows a dash for the manager's own player.
* `PreMatchSetup` keeps a 10 px text class; `YouthAcademyTab` fetches the review twice and shows dashes silently on failure.
* Contrast of the new rating tiles in both themes was not measured.

Code quality
* `skip_to_match_day_internal` is long; the blocker rule exists in two places; the MCP scouting tool has two copies of
  the range-to-text rule; two near-identical fetch hooks; `InteractiveMatchScreen` and some I7 components exceed the
  style size limits.
* Translations other than English are unreviewed; `pt` and `pt-BR` are identical for 29 of 76 new keys.

Packaging and process
* The app is not signed for distribution, not notarised (section 11).
* The repository's mutation evidence for mission 4 is recorded in a commit message; no fix-removal run was repeated here.

## 10. Exact launch command

Release app, built from `2c00d400`:

```bash
open "$HOME/apfm-migration/apfm-main/src-tauri/target/release/bundle/macos/AP Football Manager.app"
```

**Before the first playtest, back up the real saves**: the app migrates an old save in place and an older build
cannot read it afterwards.

```bash
cp -R "$HOME/Library/Application Support/com.apfootballmanager.app/saves" "$HOME/apfm-saves-backup-20261120"
```

To try the game without touching any real save:

```bash
cd ~/apfm-migration/apfm-main && scripts/gui-validation/run-isolated.sh --binary \
  "$PWD/src-tauri/target/release/bundle/macos/AP Football Manager.app/Contents/MacOS/openfootmanager"
```

(Add `--smoke 25` for a launch-and-close check.) `open` itself was not run in this session (it would use the real
data folder); the binary was launched directly.

Development mode, from source: `npm ci && npm run tauri dev` (first compile 5-15 minutes).

Rebuild the bundle: `npm run tauri build -- --bundles app` (4 min 14 s here). `src-tauri/target/` is git-ignored.

## 11. Mac packaging and signing

| Item | Finding |
|---|---|
| Procedure | the repository's own: `npm run tauri build -- --bundles app`; no `.dmg` was built |
| Location | `src-tauri/target/release/bundle/macos/AP Football Manager.app` (31.09 MiB). The previous release-candidate bundle was copied to `src-tauri/target/release/bundle/previous-alpha-rc/` first; nothing was copied anywhere else |
| Architecture | `arm64` only (`lipo -info`); no Intel slice |
| Executable | `Contents/MacOS/openfootmanager`, SHA-256 `862b8e13…dfbaacdfe2` (`862b8e139c44b40ba00df50d5ab52f767e6d21f25f776134601ec1dfbaacdfe2`) |
| Resources | `icon.icns`, `data/default_names.json`, `data/default_nations.json` |
| Identity and version | bundle identifier `com.apfootballmanager.app`, version 0.3.0, minimum macOS 10.13; the binary logs `v0.3.0-nightly · 2c00d40` |
| Signature | **ad-hoc, linker-signed only** (`flags=0x20002(adhoc,linker-signed)`, no Team ID, `Info.plist=not bound`, `Sealed Resources=none`). **Not** Developer ID signed, **not** notarised, **not** stapled |
| `codesign --verify --deep --strict` | **fails**: "code has no resources but signature indicates they must be present" |
| Gatekeeper | `spctl --assess` does not accept it. A copy re-signed ad-hoc with `codesign --force --deep --sign -` (scratch folder only, the bundle was not modified) verifies as valid but `spctl` still says "rejected" |
| Launch behaviour | the binary starts and works from the folder it was built in (isolated launches above). A copy that is downloaded, AirDropped or otherwise quarantined, and any other Mac, will be blocked by Gatekeeper; that was not tried. The window was never seen |
| To distribute | needs a Developer ID certificate, hardened runtime, notarisation and stapling; none exist here |

## 12. Release decision

**GO — for a local alpha to be played by its owner.** The stated conditions hold: no known P0 or P1, every
mandatory technical gate green, and credible backend evidence that the whole intended journey runs once and
persists (one career from a new game to the second season, 7 of 7 real old saves, 20 simulated seasons with the
invariants clean).

This is not a claim that the game is finished or that the interface works. **Manual desktop acceptance is
outstanding**: the first person to run the 15-step smoke test and the GUI checklist will be the first to see the
window operated. Treat any defect they find there as unknown until then; the bundle is arm64, unsigned and
not distributable to another Mac.
