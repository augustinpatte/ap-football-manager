# Targeted source-audit corrections (2026-11-20)

Branch `claude/audit-corrections-20261120`, from `apfm/main` at 48e07994. Three findings from an
independent source review, each checked against the source before changing anything.

## Finding 1 — mid-match saving had no on-screen control: CONFIRMED

* **Evidence.** `grep` for `saveGame`/`exitToMenu` found the controls only in `src/pages/Dashboard.tsx`.
  `MatchSimulation.tsx` returns `InteractiveMatchScreen` directly while `managing`, and that screen had no
  save or exit control. The Dashboard's window-close guard (`onCloseRequested`) is unmounted on the `/match`
  route, so closing the window mid-match discarded the match silently. Backend support was already there
  (`application/saving.rs`, `live_session::savable_operation`, checkpoint stored by `match_in_progress`).
* **Regression tests (red first, 10 failing before the change).** `InteractiveMatchScreen.test.tsx`,
  describe "save controls": save and confirm; one save at a time with Play/Exit/Finish blocked; save follows
  an accepted order in the call queue; translated failure and retry; fallback sentence for an unknown error;
  Save and return to menu; stay in the match when the exit save fails; close held when progress is unsaved;
  window stays open when Save & Quit fails; no prompt once saved and unchanged.
* **Backend characterisation.** `saving_and_returning_to_the_menu_keeps_the_match_resumable`
  (`application/interactive_match.rs`): exit with a match in progress clears the live state, a reload resumes
  the same match with the accepted order in force, and it finalises exactly once. Mutation: skipping the
  persist in `exit_to_menu` makes it fail; restored.
* **Fix.** `useMatchSave`, `MatchSaveControls`, `useLiveMatch` (`enqueue`, `unsaved`, `markLeaving`),
  `LiveControls`, `InteractiveMatchScreen`, `MatchSimulation` (`onExited`). One save system: the same
  `save_game` / `exit_to_menu` commands. Seven new strings in 12 locales.

## Finding 2 — save index not crash-safe: CONFIRMED

* **Exact failure path.** `save_index::write_index` used `fs::write` (truncate then write). A cut-off write
  leaves truncated JSON. `load_index` returns `be.error.saveIndex.loadFailed` on a parse error,
  `SaveIndexManager::init` propagated it with `?`, `SaveManager::init` did too, and `lib.rs` setup
  (`SaveManager::init(..).map_err(io::Error::other)?`) turned it into a failed launch. Only a *missing* index
  was rebuilt.
* **Regression tests (9 failing before the change)** in `save_index.rs` and `save_manager.rs`: truncated
  index with intact SQLite careers is rebuilt and kept; non-UTF-8 cut-off; unusable `.db` is untouched and
  not listed; recovered index keeps working for new saves; interrupted write leaves the previous index;
  failed replace keeps the previous index; set-aside twice keeps both copies. Mutation: reverting
  `write_index` to `fs::write` fails the two write tests; restored.
* **Fix.** `db::atomic_write::write_atomically` (scratch file, `sync_all`, rename, directory sync); the root
  crate's `atomic_file::write_json` now delegates to it so there is one rule. `load_index_or_quarantine`
  moves a damaged index aside whole as `save_index.json.corrupt-<time>` (never overwrites an earlier one) and
  the existing rebuild reconstructs from the career databases. An unreadable file (permissions/I/O) is still an
  error. Nothing is deleted; invalid `.db` files are logged, not listed.

## Finding 3 — blocker check fails open: CONFIRMED, with a narrower scope than the brief

* **Evidence.** `checkBlockingActions` caught every invoke failure and returned `[]`. Rust boundary:
  `advance_one_day` already enforced blockers (unless `ignore_blockers`). `advance_time_with_mode` (plain
  Continue) had **no** Rust check; `skip_to_match_day` checked only *after* its first processed day.
  `advance_to_next_event` is not registered as a command and is unused by the UI (left unchanged).
* **Not changed on purpose.** A user match day is not refused: the pre-match flow owns lineup decisions and
  the UI never checked there. The daily digest (`advance_one_day`, `useDigestAdvance`) and fast-forward do not
  use `checkBlockingActions` and are untouched. The MCP advance keeps its unchecked behaviour
  (`BlockerPolicy::Skip`): an agent reads `check_blocking_actions` itself.
* **Regression tests.** Rust (`commands/time.rs`): refused while a blocker stands (day not played), refused
  skip before its first day (0 days), acknowledged continue and skip play, nothing blocking plays, match day
  not refused (mutation: removing the exemption fails it). Frontend (9 failing before): the service rejects
  instead of `[]`; failed check on Continue and on Skip advances nothing and shows a translated alert; retry
  works; explicit Continue anyway sends `ignoreBlockers` once with no check loop; a backend `blocked` answer
  opens the blocker dialog.
* **Fix.** `checkBlockingActions` rethrows; `useAdvanceTime` exposes `advanceError` (shown in the Dashboard,
  key `notifications.blockers.checkFailed`, 12 locales) and passes `ignoreBlockers` only after Continue
  anyway; `AdvanceTimeWithModeResponse.blockers` (`serde(default)`); `BlockerPolicy` in
  `application/time_advancement.rs`; entry check in `skip_to_match_day_internal`.
