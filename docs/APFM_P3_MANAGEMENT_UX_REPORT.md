# APFM P3 management UX report

Branch `claude/apfm-playable-career`, starting from `bdab6019` (P2.5). Not merged into `apfm/main`. M12, the external 2D viewer and DeepSeek's worktree were not touched.

## Verdict

**Automated evidence is green. The desktop application has still not been played by anyone.**
No window was opened or clicked in this phase. Everything below was checked with component tests
(jsdom), Rust unit and integration tests, the type checker, the linters and the production build.
None of it says how a screen looks or feels. The GUI acceptance checklist stays mandatory before any
public release; this phase added rows to it (see §7).

## 1. Commits (oldest first)

| Commit | Change | Kind |
|---|---|---|
| `92e2459f` | Staff tab: `aria-pressed` on view and role toggles, named search box and icon buttons, project focus ring everywhere, design tokens (the folder joins `FINISHED_FOLDERS`); alert list in a persistent polite status region; focus moves to the opened screen after an alert click | fix (the six P2.5 defects) |
| `f213d8e6` | 14 tests through the real `Dashboard.handleNavigate` for every alert and every inbox route the Rust side emits | test |
| `8daf3763` | Squad: a player's profile opens from the keyboard (the name is a real button) | fix |
| `3b17bb60` | Squad: search, position and status filters survive a visit to a profile | fix |
| `e7eacf46` | Staff Management: Loading / failure with Retry instead of a blank panel; Inbox: refused actions, mark-all-read, clear-old and delete show the backend's reason | fix |
| `ae81e7ba` | Tactics: what a preset leaves alone (Rust); defensive-shape command, one-dial payload, reopen (React) | test only |
| `b842b231` | Squad column chooser, saved in the settings file | feature |
| `7b0eae15` | Delegation: a refused proposal answer resyncs the game; proposals show their duty and date; repeated buttons are described by what they act on | fix |
| `0a842d63` | Fixes from the accessibility and i18n reviewers (§4) | fix |

## 2. Phase by phase

### Phase 1: Staff accessibility (the six P2.5 defects)

| P2.5 defect | Status |
|---|---|
| Missing `aria-pressed` on view toggles | Fixed, also on the role filter chips. Test: `StaffTab.accessibility.test.tsx` |
| No project focus ring | Fixed on every control; `components/staff/` is now in `FINISHED_FOLDERS`, so `designSystem.test.ts` keeps it that way |
| Raw palette colours | Fixed (tokens, `Badge`). Pre-existing value-by-threshold painting remains, see §6 |
| Unlabelled search input | Fixed (`type="search"`, `aria-label`) |
| Focus lost after navigation from an alert | Fixed in `DashboardWorkspaceContent`: a labelled region takes focus once per alert click, never on load |
| No live region for alert changes | Fixed: `DashboardAlerts` keeps a polite status region on the page even when empty |

### Phase 2: Dashboard navigation

Tests mount the real `Dashboard`, the real alert model, alert list, workspace and tab router (tab
bodies, sidebar and header stubbed). Covered and passing: pending proposals to Staff; injured
starter to Squad; match-day lineup warning to Squad; urgent message to Inbox; exhausted players to
Training; finance crisis to Finances; and every `NavigateTo` route the Rust code emits
(`?tab=Transfers|Finances|Training|Tactics|Schedule|Staff|Squad` and `/team/<id>`) through the real
`InboxTab`. Mutation-checked (§3).

**What was deliberately not added.** The alert model has no alert for transfers, contracts or the
board, and none was invented. Those reach the manager as inbox messages: transfer offers route to
Transfers, the contract review routes to Squad, and the board directive carries only an
Acknowledge action, so there is no destination to navigate to.

### Phase 3: three usability defects, from the code

1. **No keyboard way into a player** (squad rows were `tr onClick` with a `span` name and a
   mouse-only context menu). Fixed.
2. **Filters lost on every profile round trip** (the sort was already held by the dashboard, the three
   filters were not). Held the same way, with a local fallback when no one holds them.
3. **Silent failures**: Staff Management drew nothing while loading and forever after a failure; Inbox
   swallowed every refused action (`catch {}`). Both now say what happened. Two new strings in all
   12 locales.

### Phase 4: tactics

The simple/advanced split was not touched. Added tests only:

* Rust: applying a preset leaves the starting XI, a chosen defensive shape and the training plan
  untouched.
* React: choosing the out-of-possession formation sends one `set_out_of_possession_formation`
  and nothing else; a dial change sends only that dial; reopening the screen shows the saved dial and
  sends no writing command.

**Finding, not changed.** Picking a different preset deliberately replaces the phase dials with that
preset's (#365, documented in `squad.rs`). The screen shows an "Unsaved changes" marker beforehand but
asks no confirmation, so a manager who tuned dials and then picks a preset loses them. Whether that
needs a confirmation is a product decision. A related observation: after the tactics screen is left and
reopened, tuned dials are shown as the current setup ("Custom tactic", Synced) because no preset
matches them any more; the dials are kept, but the unsaved marker is not.

### Phase 5: column chooser (implemented)

Optional columns: jersey, position, familiarity, traits, age, condition, morale, OVR, contract. The
name and the actions menu are never hideable. A new setting `squad_hidden_columns` holds the *hidden*
ids, so an old settings file (field missing, `serde(default)`) and any column added later both show.
Rust bounds it to 32 ids of at most 32 characters, deduplicated; an id it does not know is kept for
newer builds. Sorting follows what is visible (hiding the sorted column falls back to the default
order). Keyboard: real button with `aria-expanded`, native checkboxes with visible labels, Escape
closes and returns focus. English and French are tested with the real translations. Squad table only;
the transfer tables were not touched.

### Phase 6: delegated management

The screen already presented responsibilities, proposals with the backend's reasons, accept/decline,
spending limits, and take-back per duty and as the Manual mode. Nothing was rewritten. Changes:

* **Defect found by reading the code**: the backend drops a proposal it cannot carry out
  (`resolve_proposal`, no rollback in `mutate_active_game`), but the panel only showed the error and
  kept listing the dead proposal, with the dashboard count stale. A refused answer now reads the game
  again. Tested at the panel and through the dashboard alert flow.
* Each proposal shows its duty and its date in the player's locale (both are backend data).
* Repeated Accept / Decline / Delegate / Take back / Apply buttons carry an accessible description
  naming what they act on, without changing their names.

No authorization was changed. Accepting still runs through the ordinary rules
(`market::hire`, `negotiate_renewal`, and so on), so a proposal can still fail on the budget or the
rules. Save and reload of delegation state is covered by the existing Rust tests listed in §3.

## 3. Verification (counts are as printed)

| Check | Result |
|---|---|
| `npm test` | **340 files passed, 1 skipped; 2,993 passed, 4 skipped** (P2.5: 333 files, 2,930) |
| `npm run build` (`tsc && vite build`) | pass |
| `npm run lint`, `npm run format:check` | clean |
| `npm run knip` | exit 0 (one configuration hint) |
| `npm exec --no -- vitest run src/i18n` | 9 files, 35 tests passed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean |
| `cargo test --locked --lib` (root) | 567 passed, 0 failed, 8 ignored (P2.5: 563) |
| `cargo test --locked --lib --features mcp` | 653 passed, 0 failed, 8 ignored |
| `cargo test -p db --lib` | 441 passed, 1 ignored |
| `cargo test -p ofm_core --lib delegation` | 80 passed |
| `cargo test -p season-harness --test delegated_career`, `--test delegated_recruitment` | 1 and 2 passed |
| `cargo test --lib playable_journey` | 9 passed, 1 ignored (the release-mode two-season test was not re-run) |

**Not run:** the whole-workspace `cargo test` (about 12 minutes; only `settings.rs` and a test in
`squad.rs` changed in Rust); the ignored two-season journey; `cargo deny`; `cargo machete`;
`/preflight` as one skill invocation; any run of the app.

### Mutation checks (a named test must fail without the fix)

| Fix | Mutation | Result |
|---|---|---|
| Alert routing | `handleNavigate` ignores the tab | all 14 navigation tests fail |
| Alert routing | the proposals alert points at Squad | the Staff test fails |
| Inbox routing | the `?tab=` parser disabled | the 7 `?tab=` route tests fail |
| Squad filters | the roster ignores the held filters | the round-trip test fails |
| Tactics payload | the service defaults another dial | the one-dial test fails |
| Preset leaves lineup alone (Rust) | preset also resets the defensive shape | the test fails (`5-4-1` vs `3-4-3`) |
| Column chooser | no save to the settings | three tests fail |
| Column chooser | no sort fallback on hide | the sort test fails |
| Proposal resync | no resync after a refused answer | both new tests fail |

Every new fix test was seen red for its predicted reason before the fix, with these exceptions, which
were **not** red first:

* the 14 navigation tests (`Dashboard.navigation.test.tsx`) passed on their first run; they were then
  proven able to fail by the three mutations above;
* the tactics tests (Rust preset, React payloads and reopen) describe existing behaviour and pass by
  design; they were mutation-checked above;
* the second squad-name test (row click still opens a profile), "toggles a column from its visible
  name" and the `squadColumns.ts` model tests were written together with, or after, the code they
  cover (the model tests only because the module did not exist yet).

## 4. Reviewer agents (read-only, run on `bdab6019..HEAD`)

| Agent | Outcome |
|---|---|
| `ui-accessibility-reviewer` | Five reachable findings, all fixed in `0a842d63`: Retry stranded focus and gave no feedback; "Show all columns" disabled itself under the keyboard; the Escape handler was document-wide; status regions that mount with their content; raw palette on the new name button and the moved header row. Contrast of `text-warning` on `bg-sunken` in light mode and of the chips in high contrast is **a manual check**. |
| `i18n-auditor` | No missing locale, placeholder or gender problem. A raw ISO date in the proposal and log lines: fixed (locale date). Advisory, not changed: ru "штаб" vs "персонал" and es "staff" vs "personal" in `staff.mgmt.loadFailed` match `myStaff` but not `mgmt.budget`; zh-CN "列" is terse as a button label and as the panel legend (a native speaker should decide). |
| `ofm-test-reviewer`, `ofm-architecture-reviewer`, `ofm-dedup-reviewer` | **Not run** in this phase. Regression effectiveness was checked by the mutation table above instead. |

## 5. Accessibility defects fixed (summary)

Staff: pressed state, names, focus ring, tokens, search label. Dashboard: live region, focus
hand-off. Squad: keyboard path to a profile; column chooser reachable and operable by keyboard.
Staff Management and Inbox: errors and loading are announced (`role="alert"` / persistent status),
Retry is busy-aware and returns focus. Delegation: repeated buttons are described.

## 6. Remaining defects and gaps

* **Squad scroll position is not kept** after a profile round trip (checklist 2.3 now says so).
* **Tactics**: a preset replaces tuned dials without confirmation (§2, Phase 4); the unsaved marker is
  not kept after reopening; formation, play-style and starting-XI write failures are only logged
  (`console.error`), not shown to the player (`useTacticsLineup.ts`).
* **Inbox**: raw palette and the 1,505-line test file `InboxTab.test.tsx` (over the 1,000-line ceiling,
  pre-existing); failing to mark a message read is only logged.
* **Squad table** (pre-existing, from the reviewer): search input focus ring is a 30% brand colour, the
  Clear button has no focus-visible style, the out-of-position icon is amber with a hover title only,
  and the row itself has no keyboard activation (the name button now covers the main action).
* **Column chooser** closes on Escape or its button, not on a click elsewhere. It covers the squad
  table only; no column order or width.
* **Staff**: the `Badge` and `ProgressBar` variants still depend on thresholds computed in TypeScript
  (for example OVR 65), against the "a rated value is painted by the backend" rule. Staff has no
  backend tone yet.
* **Dashboard**: no alert exists for transfers, contracts or board notices (by design, see Phase 2).
* The wording differences from P2.5 (alert versus tab name in es/pt/de/cs/tr) still need a native
  speaker.
* All P2.5 open items about the engine, the gate world versus the shipped world, and `cargo deny` /
  `cargo machete` stand.

## 7. Manual checks still required

Everything in `APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`, four passes (light/dark, English/French).
Added or changed in this phase: 2.3, 2.7 to 2.10 (column chooser, keyboard on the roster), 4.6 (alert
focus and announcement), 4.8 (Inbox failure notice), 7.5, 7.7 and 7.8 (proposal context, refused
answer, Management loading and failure), 10.2. Contrast in both themes and in high contrast, a real
screen reader on the alert region and the Retry flow, and whether the column panel feels right over
a narrow window cannot be judged by these tests.

## 8. Can P4 begin safely?

**For work that does not depend on how the screens behave in a window: yes, on the same terms as
P3.** The backend contract changed in one place only (the optional `squad_hidden_columns` setting,
backward compatible and tested at the file level), and every gate above is green.

**For work that builds on these screens: not on the strength of automation alone.** P2.5 recommended
that one person complete checklist sections 1, 4, 5 and 8 in one theme and one language before the
next UX phase; that has still not happened, and this phase added screens (the chooser, the
focus hand-off, the failure notices) that nobody has seen working. This is a recommendation; the
decision is the owner's.
