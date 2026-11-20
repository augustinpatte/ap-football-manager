# AP Football Manager — alpha release candidate

Date 2026-11-19. Branch `apfm/main`. How to play: [`APFM_PLAYABLE_BUILD.md`](APFM_PLAYABLE_BUILD.md). First manual
pass: [`APFM_MANUAL_SMOKE_TEST.md`](APFM_MANUAL_SMOKE_TEST.md). What the last code change fixed:
[`APFM_FINAL_HARDENING.md`](APFM_FINAL_HARDENING.md).

## 1. Final SHA

All code was validated at **`9e6ba80b`** (hardening merge, ahead of the old remote head `47507b88` by 7 commits).
The commit that adds this file is documentation only; `git log -1 --format=%H` on `apfm/main` is the final SHA.
No production code was changed in this release pass.

## 2. Validation (run on the final code, 2026-11-19)

| Gate | Result |
|---|---|
| `cargo test --workspace --locked` | exit 0; 162 test suites, **5233 passed, 0 failed, 133 ignored** (18 m 47 s) |
| Two-season save/reload (`a_reloaded_career_completes_two_seasons`, ignored by default, run explicitly) | **passed** (26 s) |
| `cargo fmt --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean |
| Same with `--features mcp` | clean |
| `npm test` (vitest) | **358 files passed (1 skipped); 3147 tests passed (4 skipped)**, 0 failed |
| `vitest run src/i18n` (the real i18n gate) | 9 files, **35 tests passed** |
| `npm run build` (tsc + vite) | passes |
| `npm run lint` | passes (3 infos, no warnings or errors) |
| `npm run format:check` | passes (Biome 2.5.5, the pinned version, from `node_modules`) |
| `npm run knip` | passes (two configuration hints, no findings) |

The 133 ignored Rust tests are long-career studies and tests that need external files
(`APFM_OLD_SAVES_COPY`, a fm24 install). They were not run, except the two-season test above.

**Hardening fixes confirmed present and tested** (names in the passing suites):

| Fix | Test |
|---|---|
| A language change does not restart the match | `keeps the match as it is when the language changes` (`useLiveMatch.test.tsx` / `InteractiveMatchScreen.test.tsx`) |
| Command ids are not reused after re-entering a match | `gives an order on a reopened screen an id the engine has not used` |
| Command ids are not reused after a reload | `a_command_id_used_before_a_reload_is_still_remembered_after_it` |
| No order while a request is in flight or the picture is behind the engine | `does not take an order while a stretch of play is still being produced`; `liveDirector.test.ts` |
| Earlier-period goals keep their minute after a reload | `keeps a stopwatch for the first half when it only ever saw the second` |
| Finishing twice applies the result once | `asks the engine to finish once…`, `does not finish again after the result was applied`, `a_managed_match_is_finalised_once_and_survives_a_reload`, `a_match_that_ends_with_goals_is_finalised_with_those_goals` |
| Stale revisions are rejected | `an_order_quoted_against_an_older_revision_is_refused_and_changes_nothing` |
| Old saves migrate without touching the originals | `old_saves_load_and_play_on` (see section 5) |

## 3. Build artifact

A release-mode macOS bundle was produced with the repository's own script
(`npm run tauri build -- --bundles app`, 5 min):

```
~/apfm-migration/apfm-main/src-tauri/target/release/bundle/macos/AP Football Manager.app
```

31 MiB, arm64 only, version 0.3.0, identifier `com.apfootballmanager.app`. It is **ad-hoc signed only**
(no Developer ID, not notarised), and `codesign --verify --deep --strict` reports that the bundle has no sealed
resources. Signing and macOS protections were not changed. Consequences: it opens from the folder it was built
in. A copy that has been downloaded or AirDropped to another Mac will be blocked by Gatekeeper; that case was not
tried. The `.dmg` target was not built (`--bundles app` only). `src-tauri/target/` is git-ignored, so the bundle
is not in the repository; rebuild it with the command above.

**Launch check (done, no window interaction).** The release binary was started with the repository's isolated
harness (`scripts/gui-validation/run-isolated.sh --binary … --smoke 25`). The process stayed up for the 25 seconds,
the webview loaded the bundled frontend, logged `AP Football Manager v0.3.0-nightly · 9e6ba80`, and the backend
answered `get_settings` and `get_active_game`. No panic, error or warning in the log. The harness confirmed that
nothing outside its throw-away folder changed. **Nobody clicked anything in the window**; no GUI workflow is
claimed as tested.

## 4. Startup

Release app (no compile, no terminal):

```bash
open "$HOME/apfm-migration/apfm-main/src-tauri/target/release/bundle/macos/AP Football Manager.app"
```

Development mode (verified in earlier runs; first start compiles for 5-15 minutes):

```bash
cd ~/apfm-migration/apfm-main && npm ci && npm run tauri dev
```

## 5. Save-file safety

* Both launch modes use the same folder: `~/Library/Application Support/com.apfootballmanager.app/saves`
  (the identifier is in `tauri.conf.json`; the folder is `<app data>/saves` unless the environment variable
  `APFM_SAVES_DIR` names an absolute path). An app opened from Finder or `open` does not see variables set in a
  terminal. At the time of this check the folder held 7 careers (written 28 Oct to 2 Nov, schema v59 and v63; the
  current schema is v70) and no `APFM_SAVES_DIR` was set.
* The release launch check ran in an isolated root, so those 7 saves were **not opened or changed** by this pass.
  No save was deleted, moved or migrated.
* Compatibility evidence (from the hardening pass, still valid because every Rust change since is inside
  `#[cfg(test)]`): 3 of the 7 real saves, **copied**, loaded, migrated to v70, played 60 days, played and
  finalised a match once, saved and reloaded identically; the originals' checksums were unchanged. The other 4
  were not opened. Not covered: a real save written by this build with a scouting dossier or a match in progress.
* **A real old save opened in the app is migrated in place** (it is rewritten as v70 on the next save, and an
  older build cannot read it afterwards). Before the first playtest:
  `cp -R "$HOME/Library/Application Support/com.apfootballmanager.app/saves" "$HOME/apfm-saves-backup-20261119"`
  (about 380 MB for the whole app-data folder; the saves are most of it).

## 6. Known limitations (accepted for the alpha)

* P2: an interactive match is not stored as a replay; the highlight mode is not remembered when the screen is
  reopened; a match saved while watching a highlight resumes at the engine's head (up to about 90 simulated
  seconds ahead of the picture).
* P2: AI clubs do not run scouting; youth potential still predicts development very strongly; translations other
  than English are unreviewed.
* P2: `InteractiveMatchScreen` and some I7 components exceed the style size limits.
* The four repository reviewers (`ofm-test-reviewer`, `ofm-architecture-reviewer`, `ofm-dedup-reviewer`,
  `ui-accessibility-reviewer`) were not run on the hardening diff.
* Not run here: the 133 ignored Rust tests (other than the two-season one) and any long-career study.

**Known P0 / P1: none.**

## 7. Manual tests still required

All of [`APFM_MANUAL_SMOKE_TEST.md`](APFM_MANUAL_SMOKE_TEST.md) (15 checks, 20-30 minutes): this is the first time a
person clicks through the app. Checks 10-13 (pause during play, orders after the pause settles, save and reload
mid-match, finishing once) and the language switch with a match open are where the last hardening changes land.
Also: open one of the existing old saves from the backup-protected folder and play a day.

## 8. Release readiness

**Ready for the first manual playtest.** Every automated gate above is green on the shipped code, a release
bundle exists and starts, and the frontend reaches the backend. It is an alpha: unsigned, unverified by any person
in the GUI, with the P2 limits above.
