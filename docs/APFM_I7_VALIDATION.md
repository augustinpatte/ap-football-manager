# APFM I7 — validation

Design and behaviour: [`APFM_I7_SCOUTING_YOUTH.md`](APFM_I7_SCOUTING_YOUTH.md). Evidence labels: **RUN** (executed on
the final tree), **READ**, **NOT RUN**. Baseline `11bbb6d4`; the gates ran on the tree that contains the review
fixes (the last code commit before this document).

## 1. Mechanical gates (all RUN)

| Gate | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | exit 0 |
| Clippy, default | `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0 |
| Clippy, `mcp` | same with `--features mcp` | exit 0 |
| Rust workspace | `cargo test --locked --workspace --no-fail-fast` | exit 0; 162 result lines; **5,215 passed, 0 failed, 129 ignored** (baseline 5,173 / 128; the extra ignored one is the new study) |
| MCP lib | `cargo test --locked --features mcp --lib mcp_server` | 78 passed |
| Frontend | `npm test` | 352 files passed, 1 skipped; **3,088 passed, 4 skipped, 0 failed** (baseline 3,066) |
| Build | `npm run build` | exit 0 |
| Lint | `npm run lint` (Biome 2.5.5, real package confirmed after `npm ci`) | exit 0, 3 infos (as before) |
| Format check | `npm run format:check` | exit 0 |
| Dead code | `npm run knip` | exit 0 (two configuration hints, as before) |
| i18n | `npm exec --no -- vitest run src/i18n` | 9 files, 35 tests passed |

Two failures were met and fixed on the way, neither by weakening a test: `db/tests/run3_integration_tests.rs` imitates
older builds by dropping each later migration's schema and needed `DROP TABLE scouting_dossier` (its comment now
lists v070); and an earlier Rust run was stopped after the review fixes and repeated in full, so the figures above are
from the final tree.

## 2. Tests added or changed

| Area | Tests |
| --- | --- |
| Estimates (`scouting_dossier.rs`) | the truth is always inside, the centre is not the truth, constant width at the extremes, deterministic, better judging narrower, intersection keeps the truth, confidence follows the ability range |
| Reports and assignments (`tests/scouting_dossier_tests.rs`, 11 + 1) | ranges hold the truth (20 draws, centred on it in few); scout quality sets width and confidence, and a scout below 30 judges no potential; second report narrows strictly in the same season (30 draws) and replaces across seasons; same assignment, same report; travel adds 2 days; cancel and reassign; shortlist refusals (own, duplicate, unknown, full, not listed); shortlist prunes signed, retired and vanished players and keeps free agents; the view separates known from unknown and carries no hidden potential under any field name; stale reports; free agents have no fee |
| Snapshot honesty (`scouting_tests.rs`) | a poor scout's snapshot differs from the truth (1 to 12 points), an excellent one's by at most 2; older reveal-count and unclear-potential tests re-pinned to the new rules (judging below 30 is "unclear") |
| Search (`slices_players_tests.rs`) | scouting status and estimated-potential filters (unscouted and unjudged never pass); contract-expiring filter |
| Academy (`academy_review.rs`) | ready at any age; a child is never a lost cause; ceiling below cover; loan candidate and its exclusions; distant prospect stays |
| Delegation (`delegation.rs`) | ranks on public rating not hidden potential (mutation-checked: failed with the hidden-potential variant after the fixture was sharpened to 64/99 vs 65/65); follows the shortlist first; a report is judged once and a removal stands; the first review does not reach back |
| Persistence (`db`) | dossier and shortlist survive save and load with non-default values; new career clears it; no row loads empty; v070 fingerprint |
| Commands (`commands/scouting.rs`, `squad.rs`) | missing-session keys; shortlist round trip; promotion refused at 30 seniors with the squad-full key and allowed when a place frees |
| Squad floor (`free_agent.rs`, `squad_floor.rs`) | a signed ex-academy free agent is a senior; the top-up restores the floor in the same pass. **Both fail without the one-line fix (RUN, restored).** |
| MCP | result structs render ranges and dashes and round-trip |
| Harness (`season-harness/tests/i7_scouting_youth.rs`, always run) | the dossier is identical across two builds of the same world (120 days, non-empty); it survives a SQLite reload mid-career and the reloaded game plays the same 60 days as the never-saved one, with one report per player; a scripted manager runs scouting and the academy by hand within the senior limit |
| Frontend | dossier section (ranges and unknowns, loading and empty and error states, remove from shortlist and refresh, compare, details dialog, bid closes the report, a report stays open when its player drops off, comparison limit), academy card (verdicts and reasons, promote, blocked promotion, refused promotion, loan and release routing, no review), extra filters, assignment cancel and reassign, shortlist from the search row |

One real defect was found by a test: `onGameUpdate?.(await run())` skips the `await` when the callback is absent, so a
promotion or loan toggle silently did nothing. Fixed in `AcademyReviewCard`.

**Not run:** the mutation of every new test (done for the delegation ranking, the free-agent fix and the promotion
guard's fixture only); reviewer-suggested MCP adapter tests with a live `McpContext`; a same-division control for the
travel days.

## 3. Squad-floor investigation (RUN)

| Run | Result |
| --- | --- |
| Baseline `11bbb6d4` in a separate worktree and target dir, `I9_SEASONS=10 I9_SEEDS=2 I9_ARMS=manual`, strict study assertion | **fails**: "manual training, average staff: the squad floor was broken" (left 1, right 0) |
| Same, instrumented (temporary, not committed) | 2038-04-30: 14 seniors, forward short, keeper reported `signed=[274e…]`, `seniors_after=14`; 2038-05-01: 15 after `promoted=[274e…]`. The signed player was an academy graduate (23, role Youth) |
| Baseline with only `squad_role = Senior` added in `sign_free_agent`, same command | passes; **0 breach days in all four "manual" arms** |
| My tree, same command, before the fix | 0 breach days (the trajectory differs: the delegated scouting duty is part of the arm) |

The temporary worktree was removed afterwards. Distinguishing the two things the brief asked for: this was a **genuine
gameplay failure** (a signing that left the club under the floor for a day), not the documented constraint
(nobody available), which remains best-effort and is reported to the manager.

## 4. Long-career study (RUN, `i7_scouting_youth_study`)

Generated `gate` world, 10 seasons, seeds 1 and 2, the club's administration delegated in both arms. **Delegated**:
the assistant runs scouting and the academy. **Scripted manager**: scouting and youth duties off; each Monday it sends
idle scouts to the best-rated players of 23 or under it has not looked at, shortlists reports that would start or
cover at an affordable price, and acts on the academy review (promote ready players while a place is free, list loan
candidates, release "unlikely" players of 20+). Baseline column: the same study (academy metrics only) on the unmodified
baseline.

| Measure | Seed 1 | Seed 2 |
| --- | --- | --- |
| Days above 30 seniors / below 15 seniors, user club, every arm | 0 / 0 | 0 / 0 |
| World invariants | clean | clean |
| AI club-seasons with negative cash | 0 | 0 |
| Lowest user cash | 7,504,890 | 6,098,019 |
| User academy at season end, delegated | 4 6 6 4 2 0 2 3 4 6 | 3 5 4 3 3 5 4 4 2 4 |
| User academy, scripted manager | 4 1 2 0 0 0 0 0 0 1 | 3 1 1 1 0 1 0 0 0 0 |
| User academy, baseline | 4 6 6 4 2 0 2 2 4 5 | 3 5 4 3 3 6 7 7 6 3 |
| AI academy mean by season (identical in both arms to within 0.2) | 1.8 2.8 2.9 2.9 3.5 4.1 4.0 4.3 4.4 4.5 | 1.8 2.7 2.9 3.0 3.6 4.4 4.3 4.4 4.3 4.4 |
| Largest AI academy | 3 to 8 | 3 to 7 |
| AI intake: mean rating / mean potential | about 49 / 70 each season | about 49 / 70 |
| AI intake given potential 80+ | 0.5 to 3.4 % | 0.5 to 3.8 % |
| User intake (players a season) | 1 to 3, mean rating 41 to 51, potential 68 to 74 | 1 to 3, 41 to 58, 70 to 77 |
| Promotions a season: user (delegated) | 0 0 1 3 4 5 1 2 2 0 | 1 1 1 3 3 0 1 2 4 0 |
| Promotions a season: all AI clubs | 115 to 202 | 124 to 192 |
| Reports delivered a season (delegated / scripted) | 58 to 82 / 43 to 54 | 12 to 93 / 43 to 54 |
| Players ever shortlisted / later at the club | 69 / 7 (delegated), 33 / 0 (scripted) | 44 / 4, 34 / 3 |
| Scripted manager actions over 10 seasons | about 512 scouts sent, 33 shortlisted, 23 promoted, 1 loan listing, 1 release | 513, 34, 27, 1, 0 |
| 24+ cohort: potential at intake / rating now | 70.4 / 69.2 | 70.5 / 69.4 |
| Rating now minus potential at intake | mean -1.2, sd 2.0, p10 -4, p90 +1, worst -8 | mean -1.2, sd 2.2, p10 -4, p90 +1, worst -11 |
| 24+ cohort at or above the world's median senior | 66 % | 62 % |
| Intake potential 75+: rating now (24+) | 76.1 (n=54) | 76.4 (n=46) |
| Cost, 10 seasons, one arm (dev profile) | 134 to 174 s (baseline 134 s) | 135 to 136 s (baseline 135 s) |

Reading: no regression against the baseline in any academy measure; the academy behaves plausibly for AI clubs and
for the user's. Caveats, stated plainly: the user's club in this generated world runs at the 15-senior minimum for
many seasons, so its academy is drained by promotions (the scripted manager promotes eagerly: ready players are
promoted at the first Monday); only 0 to 7 of 33 to 69 shortlisted players were later signed because the study's
recruitment is the delegated one, which does not read the shortlist; potential at intake predicts rating at 24+ very
closely (see design doc, section 4) so this study **does not show that potential is uncertain in development**, only
that scouting estimates are uncertain by design; and two seeds of one generated world are a modest sample. The
scouting and academy features were exercised in every run (hundreds of reports, tens of shortlist and promotion
actions), so these runs do test them.

## 5. Save compatibility (RUN)

`db` tests: dossier round trip with non-default values, empty-load, v070 fingerprint, run3 old-build imitations
(v061, run1, run2 development builds load and gain the structures). Harness test: SQLite reload mid-career replays
identically. **Not run:** loading a real pre-I7 save file from disk.

## 6. Reviewers (RUN, read-only agents, model output)

| Reviewer | Outcome |
| --- | --- |
| `ofm-architecture-reviewer` | No boundary, layering or save-compatibility failure. Findings: size ceilings (`ScoutingTab`, search card, `build_scout_report`: noted, not extracted), the players-page hot path (**fixed**: early return when both filters are off), pre-I7 snapshots not converted (documented), the first delegate run re-reviewing every report (**fixed**), an unused parameter and a direct call instead of the wrapper (**fixed**) |
| `i18n-auditor` | 114 keys present with matching placeholders in 12 locales. Quality findings **fixed**: "cover" as defensive cover in es, it, pt, pt-BR; three Czech errors; Russian fee wording; concatenated aria-labels now keys; `defaultValue` dropped. Not actioned: male-player wording, Czech "odstupné", Russian "скаут" next to the older "разведчик" |
| `ui-accessibility-reviewer` | Fixed: stacked modals, ambiguous row buttons, reason in a `title`, `aria-pressed` with a changing label, silent comparison limit, uppercase error text, raw colours in new search cells, dialog vanishing when its player leaves the list. Not actioned: focus return after a row disappears, live-region wording, unverified contrast of `text-fg-subtle` |
| `ofm-dedup-reviewer` | Fixed: one `current_report_on`, one `EXPIRING_WITHIN_DAYS`, shared `as_senior`. Not actioned: table style constants shared across three components, judging-tier breakpoints, `ensure_assistant` copied into a fifth harness file |
| `ofm-test-reviewer` | Fixed: narrowing now discriminating, snapshot noise tested, hidden-value leak check, shortlist pruning edges, promotion key pinned. Not actioned: MCP adapter tests with a live context, wider command-adapter tests, same-division travel control, frontend keyboard-activation tests |

`/preflight` was not run as one skill; its components were run as in section 1.

## 7. The real application

* **Launch smoke (RUN):** `npm run tauri dev` built the desktop crate with the new commands, the webview loaded and
  logged `AP Football Manager v0.3.0-dev`, the backend answered `get_settings`, and the log has no panic, error or
  warning line. The process was then stopped.
* **Automated frontend workflows (RUN):** the component tests in section 2 drive the dossier, shortlist, comparison,
  assignments, academy review and filters against a mocked backend.
* **Manual GUI verification: NOT DONE.** Nobody opened a career, sent a scout, read a report, shortlisted, compared,
  promoted, delegated or saved and reloaded in the running window; this session has no GUI automation. The backend
  half of those workflows is covered by the Rust and harness tests above. A manual pass of the Scouting and Youth
  Academy tabs, the Staff duty owners and a save and reload is still owed.
