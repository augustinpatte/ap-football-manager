# APFM I6 + I7 integration

Target `apfm/main`, baseline `11bbb6d4`. Backup branch of the baseline: `backup/apfm-main-pre-i6-i7-20261119`.
Source branches `claude/i6-match2d` (`aa4e8e33`) and `claude/i7-scouting-youth` (`ba225913`) and their
worktrees are untouched. How to start and test the game: [`APFM_PLAYABLE_BUILD.md`](APFM_PLAYABLE_BUILD.md) and
[`APFM_MANUAL_SMOKE_TEST.md`](APFM_MANUAL_SMOKE_TEST.md).

## 1. Verdict

**Ready for the first real playable test**, with one caveat that cannot be removed from here: **no person has
yet clicked through the merged app.** The window launches and the backend answers; every workflow below is
covered by automated tests through the real backend path and the real frontend code, not by a GUI session.
No unresolved P0 or P1 finding.

## 2. Merge

| Step | Commit |
|---|---|
| I6 (`--no-ff`) | `c31677b6` |
| I7 (`--no-ff`) | `720acd28` |

Both merged **without a textual conflict**, including the 12 locale files, `INTENTIONAL_SAME.json` and
`ARCHITECTURE.md`, which both branches had edited (different regions). Because a clean textual merge of JSON
proves little, it was checked structurally: every locale file parses, and the i18n gate (35 tests: all keys in
all 12 locales, no copied English, every `t("…")` key exists) passes on the merged tree.

The branches share only those 14 files; I6 touched no Rust crate other than `ofm_core::interactive_match` and
the match application/command layer, I7 touched scouting, academy, delegation, the squad floor, `db` and the
command layer. Command registration (`src-tauri/src/lib.rs`) and `interactiveMatchService.ts` were edited by one
branch only.

**Migrations.** I6 added none. I7 owns `v070_scouting_dossier.sql` (`MIGRATION_COUNT` 70, checked by a
compile-time assertion). No renumbering was needed. Old saves: the `run3` tests imitate older builds (v061,
run 1 and run 2 development builds) and load them with the new structures; a dossier and shortlist with
non-default values survive save and load; an old save with no dossier loads with an empty one; an interactive
checkpoint written before I6 restores as before (`InteractiveCheckpoint` is unchanged). **Not run: loading a
real pre-I7 save file from a person's disk.**

## 3. Reviews

I6's three reviewers, which had been cut off by the API limit, were run on the merged tree (read-only; they did
not run cargo).

| Reviewer | Outcome |
|---|---|
| `ofm-architecture-reviewer` | No P0. No crate-boundary, layering, production-panic or save-compatibility failure; finalisation is untouched by I6 and guarded twice. **One P1, fixed:** an order could be accepted while the picture was still behind the engine (a pause pressed while a stretch was in flight, then the reply landed), i.e. at a moment the manager had not seen. |
| `ofm-test-reviewer` | No test that proves nothing in the critical path; frames, order-gating, take-over and wire format are discriminating. Gaps recorded below (P2). |
| `ofm-dedup-reviewer` | No divergent copy that changes behaviour today. Three copies that could diverge are recorded below (P2). |

**The fix** (`liveDirector.ts`): when a reply extends the engine's head while the picture is paused, the
picture runs on to it again, and orders require the picture to be at the head. A test written first fails on
the old code for the predicted reason (orders allowed with the head ahead of the picture) and passes now.
The backend's stale-revision refusal already prevented a *wrong* order; this prevents a *surprising* one.

## 4. Workflow coverage (brief, section 4)

Evidence is automated; "GUI" means a person must still confirm it.

| | Workflow | Covered by |
|---|---|---|
| A | Create and load a career | `playable_journey`: new career from a generated world, save, reopen, fingerprint equal |
| B, C | Club, squad, tactics, training | `squad_and_tactic_choices_are_kept_and_survive_a_reload` and the existing squad/tactics/training tests |
| D, E | Interactive M12 match with the real 2D data | I6 `frames_tests` (frames equal the engine's own recording to the centimetre), `liveMatch.real.test.ts` on real payloads, `InteractiveMatchScreen.real.test.tsx` through the real service module |
| F | Pause, resume, speed, highlight modes | `liveDirector.test.ts` (14), `livePlayback.test.ts` (21), screen tests |
| G | Substitution and tactics at the engine's position | `an_order_never_rewrites_frames_already_handed_out`, order-gating tests incl. the new one |
| H | Full time, finalised exactly once | `a_match_watched_on_the_2d_screen_is_finalised_once…`, `a_live_match_cannot_be_advanced_past_or_finalised_twice`, and the new boundary test |
| I | Fixture, table, statistics, career updated | the same tests assert the club's completed fixtures `+1` and the standings |
| J, K | Search, assign scouts, uncertain reports | `scouting_dossier_tests` (ranges contain the truth, quality sets width, second report narrows), `slices_players_tests`, dossier component tests |
| L | Shortlist and compare | `commands/scouting.rs` tests, `ScoutingDossierSection.test.tsx` |
| M | Academy review and promotion | `academy_review.rs`, `AcademyReviewCard.test.tsx`, promotion command tests |
| N | 15-senior best-effort floor, 30-senior maximum | `free_agent.rs`, `squad_floor.rs`, `squad_floor_seasons`, promotion refused at 30 with the squad-full key |
| O | Save and reload without losing scouting data or corrupting a match | `db` round trips, `i7_scouting_youth.rs` reload test, **new**: `a_watched_match_saved_midway_keeps_the_dossier_and_is_finalised_once` |
| P | Continue into the next season | `a_reloaded_career_completes_two_seasons` (ignored by default; **run explicitly on the merged tree, passes, 25 s**) and `season_lifecycle` |

**The one new integration test** (`application/playable_journey.rs`) is the only seam that neither branch could
test alone: a career with a delivered scouting report and a shortlisted player; the user's match is watched
with frame capture; the career is saved **in the middle of the match** and opened the way "Load Game" does,
including the interactive-match restore; the dossier and shortlist are identical after the reload and after
full time; the match is finalised once and a second finish is refused with the club's result count unchanged.
Its first run failed because the *screen's view* of the dossier includes a live market flag (`transfer_listed`)
that the load path legitimately refreshes; the assertion was moved to the stored dossier, which is the data
that must survive. That was a test-design error, not a persistence bug.

## 4b. A bounded combined career study was not run

The existing harness tests that exercise both features over time (`i7_scouting_youth` always-run tests, the
`i5_i9_integration` and `i9_staff_training` suites, `squad_floor_seasons`) ran inside the full workspace run
and pass. The long studies were not repeated: I6 does not touch the simulation (frames are read-only, proven
identical with and without capture) and the merge did not change any file both branches share in Rust.

## 5. Gates on the merged tree

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean |
| `cargo test --locked --workspace --no-fail-fast` | **5,228 passed, 0 failed, 131 ignored** (162 suites), run on the merge result **before** the director fix and the new test were added (baseline 5,173; I6 alone 5,186; I7 alone 5,215) |
| `cargo test --features mcp --lib` | **682 passed**, 0 failed, 11 ignored |
| `cargo test --lib` (default), with the new test | 596 passed, 0 failed, 11 ignored |
| `npm test` | **3,134 passed**, 0 failed, 4 skipped (356 files) |
| `npm run build` (tsc + vite) | clean |
| `npm run lint`, `format:check` | clean (3 pre-existing infos) |
| `npm run knip` | clean (2 pre-existing configuration hints) |
| `vitest run src/i18n` | 35 passed |

The workspace run was made on the merge result before the director fix and the new test; after them the
affected gates (fmt, both clippy runs, both lib test runs, the full frontend suite, build, knip, i18n) were
repeated. The Rust integration-test suites outside `--lib` were not repeated because neither change touches
them.

## 6. The desktop application

`npm run tauri dev` compiled the desktop crate from this tree, the window's webview loaded the merged frontend
(build `720acd2`) and the backend answered `get_settings` and `get_active_game`. No panic, error or warning
line in the output. The instance was then stopped (an unrelated `openfootmanager` process belonging to another
worktree was left alone).

**Not done: any GUI interaction.** This session has no UI automation. Nothing here claims that a career was
opened, a match was watched, or a button was pressed in the window. The 15-check pass in
[`APFM_MANUAL_SMOKE_TEST.md`](APFM_MANUAL_SMOKE_TEST.md) is the first real exercise.

## 7. Findings

**P0 / P1 introduced by the integration: none unresolved.** (One P1 found in I6 by review and fixed: section 3.)

**P2, recorded, not fixed**

* Test gaps (reviewer): the fake engine in `InteractiveMatchScreen.test.tsx` accepts any order revision (the real
  one refuses a stale revision) and bumps it by 1 per advance (the real one by about 2 a second); no hook-level
  test of a double `finish()` on a gated promise (the double-click test is absorbed by the disabled button);
  the real-payload fixture covers only the first three minutes (no half time, substitution or order) and names
  players by id; `play_capturing` in `frames_tests.rs` has no iteration guard; the application finalisation test
  does not assert a non-0-0 score.
* Duplication: "which way does home attack" is written three times (`livePlayback.ts`, `LivePitch.tsx`,
  `InteractiveMatchScreen.tsx`); the goal-line expression twice (`M12ReplayViewer.tsx`,
  `InteractiveMatchScreen.tsx`); "key event" means a different set in Rust `is_key_event` and TypeScript
  `KEY_EVENTS`. `matchMinute` may now be used only by its own test.
* Behaviour: a language change while the match screen is open re-runs start and can show the "rejected frames"
  warning; after a reload only the current period's clock is known, so a goal banner for an earlier period shows
  no stopwatch; a match saved in highlight mode resumes at the engine's head (up to about 90 simulated seconds
  ahead of what was shown), and that gap appears only in the feed.
* Size: `InteractiveMatchScreen` is about 240 lines in one component (limit 200); I7's `ScoutingTab`, search card
  and `build_scout_report` are over the ceilings (recorded by I7's reviewer).
* The accessibility advisories of I6 (hue-only team distinction backed by numbers and a legend, unmeasured
  contrast on the away colour) and I7 (focus return after a row disappears).
* Translations in all non-English locales are unreviewed by native speakers.

**Known limitations (not blockers, from the brief):** no stored replay for an interactive match; no spectator
live 2D mode; no per-player phase overlay; AI clubs do not run scouting; youth potential remains strongly
predictive; the delegated-tactics fitness difference is not causally explained.

**Public remote.** `origin` (`augustinpatte/ap-football-manager`) reports `visibility: PUBLIC`. The earlier
integration (`11bbb6d4`) is already on it. This push follows the brief's explicit instruction; if the project
should be private, that needs changing on GitHub.
