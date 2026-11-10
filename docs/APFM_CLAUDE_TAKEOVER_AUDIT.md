# APFM takeover audit (Claude), 9 November 2026

Scope: establish a trustworthy technical baseline for the integrated F4–F5.4.5 branch before more
development. This is an audit, not a release decision. **Nothing here claims APFM is beta-ready:
no graphical playthrough has been performed** (see §8).

Evidence is labelled:

* **RUN** — executed by this audit against the HEAD below, log kept.
* **READ** — established by reading source in this audit; not executed.
* **DOC** — taken from an earlier report in `docs/`; not re-measured here.

## 1. Environment

| Check | Result |
|---|---|
| Worktree | `/Users/augustinpatte/apfm-migration/apfm-claude-playable` |
| Branch | `claude/apfm-playable-career` |
| HEAD | `92b2c6169ee192bea83a36b4740ddb254f8c3fea` (`merge: integrate F5.4.2-F5.4.5 sporting AI`) |
| Working tree at start | clean |
| `84d20d89e3b14c25992f20de66e99eb821917a4d` is an ancestor of HEAD | **yes** (`git merge-base --is-ancestor` exit 0) |
| `origin/integration/f545-to-apfm-main` is an ancestor of HEAD | **yes** (exit 0); HEAD *is* that commit, so `git diff --stat origin/integration/f545-to-apfm-main...HEAD` is empty |
| Toolchain | rustc/cargo 1.95.0 (pinned by `rust-toolchain.toml`), node v26.12.1 |
| Other worktrees | seven present (`openfootmanager`, `apfm-main-integration`, `f53-baseline`, `f542-checkpoint`, `f542-deepseek`, `/private/tmp/apfm-f542-f54-4f4fcd5f`); none touched |

### M12 engine dependency

* `src-tauri/crates/ofm_core/Cargo.toml` declares `apfm-match-lab` as a **Git dependency**:
  `git = "https://github.com/augustinpatte/apfm-match-lab", rev = "28f34b10d4fe7cadacc0030acaaa0cd1aa9fdf5f"`.
* `Cargo.lock` records the same revision (`source = "git+…?rev=28f34b10…#28f34b10…"`).
* RUN: `git ls-remote` shows that revision is the tip of the remote branch `f5.3/pressing`; the
  Cargo git cache holds the commit object (`git cat-file -t` → `commit`); the build log shows
  `Checking apfm-match-lab v0.1.0 (https://github.com/augustinpatte/apfm-match-lab?rev=28f34b10…)`.
* There is **no** `src-tauri/.cargo/config.toml` in this worktree, so no local `paths` override
  applies. Every number below was produced against the pinned revision, not a sibling checkout.
  (Earlier F5.x studies used a local override at `~/apfm-match-lab`; the F5.4.2 report notes that
  checkout had uncommitted edits. Those studies are therefore not strictly comparable with this one.)

## 2. Test baseline (all RUN, sequential, `--locked`)

| # | Command | Exit | Result |
|---|---|---:|---|
| 0 | `npm ci` | 0 | installed (node_modules was absent) |
| 1 | `cargo fmt --manifest-path src-tauri/Cargo.toml --all --check` | 0 | clean |
| 2 | `cargo check --locked --manifest-path src-tauri/Cargo.toml --workspace` | 0 | 82 s cold |
| 3 | `cargo clippy --locked … --workspace --all-targets -- -D warnings` | 0 | no warnings |
| 4a | `cargo test … -p ofm_core --test squad_floor_seasons` | 0 | 9 passed |
| 4b | `cargo test … -p ofm_core --test transfers_tests realism::` | 0 | 26 passed, 117 filtered (not a zero-test selection) |
| 4c | `cargo test … -p season-harness --test f3_fast_forward` | 0 | 3 passed |
| 4d | `cargo test … --test contract_single_writer` | 0 | 6 passed |
| 5 | `cargo test --locked … --workspace --no-fail-fast` | 0 | **122 targets: 4,698 passed, 0 failed, 75 ignored** (721 s) |
| 6 | `npm test` | 0 | 332 files passed, 1 skipped; **2,920 tests passed, 4 skipped** (92 s) |
| 7 | `npm run build` (`tsc && vite build`) | 0 | built |

Logs: `/private/tmp/claude-501/-Users-augustinpatte-apfm-migration-apfm-claude-playable/24234d76-695e-498a-9792-0e88f70e58ca/scratchpad/baseline/`.

**Not run:** `cargo test --features mcp` and `cargo clippy --features mcp` (CI's second pass);
`npm run lint`, `format:check`, `knip` (the repository's `preflight` script); `cargo deny`,
`cargo machete`; the 75 ignored Rust tests (measurement studies, season-length scenarios) and the 4
skipped frontend tests; `tauri build`; any GUI run. A green count here does not cover them.

The workspace figures differ slightly from the F5.4.2 report (4,678–4,680 passed, one failed): that
report predates the F5.4.3–F5.4.5 commits, which added tests and a fix for the one failure.

## 3. Previously failing items, reproduced against HEAD

| Item | Earlier status (DOC) | At HEAD (RUN) |
|---|---|---|
| `squad_floor_seasons::a_world_with_no_free_agents_and_no_academies_still_finishes_every_day` | failing in `F5.4.2-HANDOFF.md` (newcomer contract start 2027-07-18 vs expected intake date) | **passes** (9/9). Commit `99803764` changed the *test*: it now identifies an intake recruit from the first ledger entry rather than from the current contract start, because a recruit promoted and sold later legitimately gets a new contract. The production behaviour was judged correct, not changed. |
| `f3_fast_forward` thirty days at once == one by one, all delegated | failing (random `Uuid::new_v4()` offer ids) | **passes** (3/3) |
| `contract_single_writer`, `finance_journal` scanners flagging test-only modules | failing | **pass** (6/6; `finance_journal` inside the workspace run) |
| `season_lifecycle::a_career_survives_a_sqlite_reload_at_every_stage_of_four_seasons` | failing at season 4 (missing warning after reload) | **passes** in the workspace run |
| `a_full_back_can_improve_a_team_with_eight_centre_backs`, `a_loan_can_address_a_starting_gap_despite_broad_depth` | failing in the expanded realism run | **pass** within `realism::` (26/26) |

Still open from the F5.4.2 report, not re-tested here: the contract-write scanner stops at an inline
test module and can miss production code after one (`tests/contract_single_writer.rs`
`production_source`; the file-backed `mod tests;` case *is* handled); a dismissed manager's former
club is repaired only on reload.

## 4. Subsystem map and status

Status key: **V** implemented and verified · **U** implemented, not verified by this audit ·
**P** partial · **M** missing · **B** broken. "Verified" means a test I ran passed or the claim
is a RUN result; it never means a person played it.

| # | Subsystem | Status | Evidence |
|---|---|---|---|
| 1 | Tauri commands / React entry | **V** (wiring) | 188 commands registered in `src-tauri/src/lib.rs` (`generate_handler!`); 174 of them have a literal `invoke("…")` caller in `src/` (grep, READ); the other 14 are listed in G6. Routes in `src/App.tsx`: `/`, `/select-team`, `/dashboard`, `/match`, `/settings`, `/sim-lab`, `/world-editor`. |
| 2 | Career creation, world, club choice | **V** (backend), **U** (UI) | `pages/MainMenu.tsx` → `start_new_game` → `/select-team` (`pages/TeamSelection.tsx`, `select_team`). `ofm_core/src/career/`, `generator/`. Tests: `career/tests.rs`, `generated_world_integrity.rs`, `MainMenu.test.tsx`, `TeamSelection.test.tsx` (all in the green runs). |
| 3 | Navigation / page routing | **V** | `components/dashboard/DashboardSidebar.tsx` (permanent, collapsible), `DashboardTabContent.tsx` lazy-loads 18 tabs (Home, Squad, Tactics, Training, Staff, Scouting, Youth, Finances, Transfers, Schedule, Inbox, News, Manager + world tabs). |
| 4 | Calendar advance / day simulation | **V** | `application/time_advancement.rs::advance_time_with_mode` (modes `live`, `spectator`, `delegate`, otherwise simulated); `skip_to_match_day`, `fast_forward_to_date`; UI `hooks/useAdvanceTime.ts`, `FastForwardDialog`. Tests: `fast_forward.rs`, `f3_fast_forward.rs` (3/3, RUN), `useAdvanceTime.test.tsx`. |
| 5 | Match prep, simulation, finalisation | **V** (backend) / **U** (UI) | Three user paths: legacy `engine` live match (`live_match_manager.rs`, `MatchLive`), M12 spatial replay (`spatial_runner.rs`, `M12ReplayViewer`), M12 interactive (`application/interactive_match.rs`, `InteractiveMatchScreen`). All finalise through `turn::post_match::apply_match_report_in`. |
| 6 | Squad selection, formations, tactics, roles | **V** (backend) / **P** (UI) | `set_formation`, `set_starting_xi`, `apply_tactic_preset`, `set_tactics_phase`, `set_player_role` wired in `services/squadService.ts`. UI gap: advanced controls are not separated (§6, G1). |
| 7 | AI managers, tactical adaptation | **V** | `ai_tactics.rs`, `turn/squad.rs` (`ai_select_starting_xi`), `ai_tactical_review_tests.rs`, `tactical_adaptation_probe.rs`. |
| 8 | Squad Intelligence, transfer market | **V** with a documented inconsistency | `squad_intelligence/` (appetite, horizon, impact, model, needs, perception); `transfers/market.rs`, `buyer_view.rs`. See §5 F1. |
| 9 | Contracts, loans, scouting, academy, youth | **V** (backend) / **U** (UI) | `contracts/`, `transfers/loans.rs`, `scouting.rs`, `youth_intake.rs`; UI tabs `TransfersTab`, `ScoutingTab`, `YouthAcademyTab`, `FreeAgentContractModal`, `LoanOfferModal`. |
| 10 | Staff delegation, approvals | **V** (backend, `delegation_scenarios` 19/19 DOC; `delegated_career`, `delegated_recruitment` in the workspace run) / **U** (UI) | `staff_system/delegation*.rs`; UI `DelegationModeCard`, `StaffManagementPanel` (accept/reject proposal → `resolve_delegation_proposal`). |
| 11 | Finances, board, dismissal | **V** | `finances/`, `board_objectives.rs`, `firing.rs`, `ai_finance_guard.rs`; UI `FinancesTab`, `FiredModal`, `JobOpportunitiesCard`; `f53_financial_stability` in workspace run. |
| 12 | Save / reload / migration | **V** | `crates/db`, 68 migrations (`MIGRATION_COUNT` asserted equal to 68), `season_lifecycle` four-season SQLite reload, `legacy_save_migration_tests`; UI save in `DashboardHeader`, load in `MainMenu`/`SavesList`. |
| 13 | Test architecture / perf harness | **V** | 4,698 Rust tests, 2,920 frontend tests; `season-harness` (`f54_squad_study`, `f53_finance_study`, `fast_forward`), `sim-bench`, `mcp-e2e` (43 tests, 16 ignored). No UI-driving test (no Playwright/WebDriver). |

## 5. Findings

### F1. The squad model and the real lineup picker are different algorithms (READ)

`squad_intelligence/model/mod.rs` documents itself (module header, lines 1–10) as using "the lineup
picker's greedy walk". The implementation does not: `best_assignment` solves the optimal
assignment with the Hungarian algorithm ("a manager does not play it so", lines ~768–775). The
game's actual AI picker, `turn::squad::first_choice_eleven`, is the greedy slot-order walk. The
module keeps a measurement-only greedy mirror (`greedy_picks_with`, `Placement::greedy_start`) so
the harness can compare, but **no production decision reads it and no unit test asserts the model's
XI equals the fielded XI**. Consequence: the market can value a signing by a start the AI
manager's picker would not give him. The F5.4.5 commit message quantifies the symptom
(model-never-starts 26/5 → 9/3 on two seeds, DOC); I did not measure how much of the remainder
is this mismatch. The header comment is stale either way.

### F2. Unused-signing metrics improved but were measured only on the harness (DOC)

F5.4.3–F5.4.5 report: unused joiners 26.0/24.1 % → 18.1/15.6 % (F5.3: 16.8/15.3 %); fees per
club-season £0.50 → £0.21m (F5.3 £0.13m); buyer evaluations 53,558 → 15,416 (F5.3 10,026); the
shipped-world succession deals 31/10 → 18/9. These are commit-message figures on two seeds and two
seasons. They were **not** re-measured here on the shipped 440-club world. The earlier
30-season shipped run took 3 h 22 m (F5.4.1) and has not been repeated since the fixes.

### F3. Performance: reproduced against HEAD

See §7. Buyer evaluations fell to 4,184 per seed, but wall time is 10.1 s (about 2.7× the documented
F5.3 figure) and is unexplained. This is the open performance blocker.

### F4. Lightweight engine versus M12 (READ + DOC)

AI-versus-AI fixtures in active competitions are simulated with the in-house `engine`
(`live_match_manager::create_live_match_with` → `engine::LiveMatchState`). Only the user's own
fixtures can use M12 (`spatial_runner`, `interactive_match`), and only in the `live`/`spectator`
modes; `delegate` and fast-forward use the `engine` path for the user too. M12's goals/shots were
tuned in F5.2–F5.3 (2.9–3.0 goals, 24–29 shots, DOC). I found **no test or report comparing the
`engine` path's distributions with M12's on the same inputs** (grep of `docs/` and `tests/` for
that comparison returned nothing). Table positions of AI clubs therefore come from a different
model than the user's matches. Not measured; listed as an open risk.

### F5. High press versus low block (DOC, not re-measured)

F5.3 added M12 press costs: press 99 vs neutral +8 → −6, vs low block +40 → +3, vs counter +50 →
+25. Low line/low block (−21) and counter (−27) remain the weakest tactics. Open per the F5.3 report.

### F6. Guards (RUN)

Contract single-writer and finance-journal guards pass; the inline-test-module blind spot (§3) is
a documented limitation, not a regression.

### F7. Frontend has no machine-run end-to-end test (READ)

Component tests (Testing Library/Vitest) and backend `season-harness`/`mcp-e2e` exist. Nothing
drives the real window. `scripts/gui-validation/run-isolated.sh` only launches an isolated build
for a human; `docs/GUI_VALIDATION.md` states it "cannot drive the UI".

## 6. Gaps against the requested product direction (READ)

| ID | Requirement | State |
|---|---|---|
| G1 | Basic tactics by default, advanced separate | The nine-dial Blueprint panel is **open by default** (`components/tactics/TacticsRightPanel.tsx`: `useState(true)` for `blueprintOpen`). No basic/advanced split. |
| G2 | Customisable table views | Sorting exists (`SquadSortHeader`, `SquadTab`); no column chooser found in squad or transfer tables. |
| G3 | Permanent sidebar | Present (`DashboardSidebar`, collapsible). |
| G4 | English / French | Present: `en.json` and `fr.json` both 5,067 lines; coverage gated by `localeCoverage.test.ts`. 12 locales ship. |
| G5 | Next-action indicators | Present: `DashboardAlerts`, `check_blocking_actions`, `DashboardBlockerModal`, sidebar badges. Quality unverified without a GUI. |
| G6 | Commands with no literal frontend caller (14 of 188) | `advance_time` (the UI uses `advance_time_with_mode`), `get_session_state`, `advance_to_next_event`, `check_season_complete`, `run_single_seeded_match`, `get_player_knowledge`, `set_team_kit_pattern`, `assign_jersey_number`, `create_package_project`, `inspect_world_package`, `validate_world_package`, `validate_competition_definitions`, `read_file_as_data_url`, `write_temp_database`. None is a step of the career journey; `set_team_kit_pattern` and `assign_jersey_number` are small missing UI hooks. (Literal-string grep: a dynamically named call would be missed.) |

## 7. Performance: reproduction against HEAD (RUN)

Command (release, quiet machine, nothing else running, pinned M12 from Git):
`F54_WORLD=gate F54_SEEDS=1,2,3,4,5 F54_SEASONS=2 cargo test --locked --release -p season-harness --test f54_squad_study -- --ignored --nocapture`
(exit 0; log `…/baseline/08_perf_gate.log`). Gate world: 80 clubs, 656 calendar days per seed.

| Seed | Simulation time | Buyer evaluations | XI fit | Weak-slot clubs | Unused joiners | Fees/club-season |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 10.032 s | 4,162 | 67.97 | 17.1 % | 12.7 % | £0.11m |
| 2 | 10.171 s | 4,184 | 68.25 | 17.1 % | 11.3 % | £0.12m |
| 3 | 10.216 s | 4,177 | 68.41 | 15.8 % | 9.4 % | £0.12m |
| 4 | 10.147 s | 4,188 | 68.22 | 14.6 % | 9.1 % | £0.08m |
| 5 | 9.470 s | 4,187 | 68.26 | 15.2 % | 18.4 % | £0.17m |
| **median** | **10.147 s** | **4,184** | | | | |

Reference figures from `APFM_RUN_F5_4_2_PERFORMANCE_AND_QUALITY.md` (DOC, different session, M12 via a
local override with uncommitted edits, other applications running):

| | F5.3 | F5.4.2 (before F5.4.3–5) | HEAD (this run) |
|---|---:|---:|---:|
| Median simulation time | 3.797 s | 8.733 s | **10.147 s** |
| Buyer evaluations / seed | 3,025 | 9,637 | **4,184** |
| XI fit (seeds 1–5) | 67.82 / 68.22 / 68.25 / 68.11 / 68.30 | 67.93 / 68.16 / 68.45 / 68.19 / 68.30 | 67.97 / 68.25 / 68.41 / 68.22 / 68.26 |
| Weak-slot clubs % | 20.3 / 17.7 / 16.5 / 13.9 / 14.6 | 21.5 / 15.8 / 13.9 / 14.6 / 12.0 | 17.1 / 17.1 / 15.8 / 14.6 / 15.2 |
| Unused joiners % | 16.8 / 17.4 / 15.8 / 14.9 / 18.1 | 21.9 / 13.7 / 15.0 / 16.8 / 20.1 | 12.7 / 11.3 / 9.4 / 9.1 / 18.4 |

What this supports and does not support:

* **Supported:** F5.4.3–F5.4.5 cut market work to ~43 % of F5.4.2 (4,184 vs 9,637 buyer
  evaluations) and the squad-quality proxies are at or better than the F5.3 figures on four of five
  seeds, with fewer unused joiners.
* **Not explained, and a confirmed performance blocker:** wall time did **not** fall with the
  evaluation count (10.1 s vs F5.4.2's 8.7 s, vs F5.3's 3.8 s, i.e. about 2.7× F5.3). Buyer
  evaluations are only ~1.4× F5.3, so the remaining cost is per-evaluation or elsewhere (candidate
  appraisal and Squad Intelligence caches were the F5.4.2 hot spots). The comparison crosses
  sessions, machines' load and the M12 source, so the ratio is indicative, not a measurement.
  Attributing it needs F5.3 and HEAD built side by side with the same pinned M12 on a quiet machine.
* Peak resident set of the test process: 2.28 GB as reported by `/usr/bin/time -l` for the whole
  cargo invocation (includes the compiler); the process footprint line reads 101.9 MB. Not a
  game-memory figure.
* The shipped 440-club world and the 30-season run were **not** repeated.

### Model versus picker, measured on the same run

The harness's succession-pathway lines compare what the squad model expects with what the game's
greedy picker would do (`Placement`, F1). Across the 9 pathway reports in the five seeds the model saw
12 starts and the picker 10; **2 were model-only**. On this sample the mismatch is small but not zero;
the sample is small and covers two seasons.

## 8. What this audit did not do

* No graphical session: no window was launched, so **no GUI step of the journey is verified**,
  only the backend commands and component tests behind each step.
* No shipped-world (440-club) or 30-season run.
* No F5.3 timing on this machine; comparisons against F5.3 use DOC figures.
* The three UI tabs `Staff`, `Scouting`, `Youth` were mapped by file listing and service
  wiring only, not by reading each component.

## 9. Decisions needed from the owner

1. Whether the model/picker mismatch (F1) should be fixed by making the model greedy (matches
   the AI) or by making the AI picker optimal (changes every AI lineup and so every balance
   number). This audit did not change either.
2. Whether a column-chooser (G2) is wanted now or after the journey is playable.
3. Whether `delegate` mode should continue to use the `engine` rather than M12 for the user's
   matches.
