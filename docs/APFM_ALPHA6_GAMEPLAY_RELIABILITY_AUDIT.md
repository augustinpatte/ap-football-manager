# ALPHA6 — Native gameplay reliability, transfer integrity, UI corrections and alpha audit

Written for: the owner and the independent reviewer of ALPHA6.

**Verdict: CONDITIONAL GO for further owner testing; not "every feature works".** The confirmed defects below are
fixed and covered by tests that fail when the rule is removed. Steps 9 (M12 formation panel) and 15 (calendar tail
delays) are **not done**, parts of steps 10 and 11 are not done, one source-backed test still fails (section 14),
and **no Tauri GUI was operated** (section 18).

## 1. SHAs

* Start: `9c00392241126297c41d21e9625c2117336cb77c` (clean tree).
* Final: see the last line of this file, written at push time. 15+ focused commits on `apfm/main`.
* Source database `~/apfootballmanager/data/world.db`: SHA-256 `36b6908c…27439f7` before **and** after (section 19).

## 2. Defect ledger

Classification: CONFIRMED (source proves it), REPRODUCED (a test or run shows it), UNVERIFIED, DESIGN CHANGE.

| # | Owner observation | Class | Precise root cause | Fix |
|---|---|---|---|---|
| D1 | Career start returns to the menu, ~5 times | **Structure CONFIRMED + REPRODUCED in vitest; the exact "5×" UNVERIFIED (no GUI)** | MainMenu's mount probe and its `game-state-changed` listener each read `get_active_game` and navigated on their own answer (overlapping/reordered answers navigated twice or landed on the older one); TeamSelection called `navigate("/")` **during render**; Dashboard cleared the whole store and went to `/` on *any* failed refresh and redirected when `hasActiveGame` was momentarily false; `start_new_game` leaves a clubless career installed, so backing out of team selection made the menu bounce the player straight back. | `careerSession` (single-flight lookup where a later request forces a fresh read; `none` ≠ `failed`; one create/load at a time; event lookups held back during an operation), `useCareerHydration`, retry panel, back-arrow suppression. |
| D2 | Loan offer showed an error, then the loan was active | **CONFIRMED + REPRODUCED** | (a) Frontend: one `try/catch` wrapped the backend call *and* local bookkeeping, so a local throw after the backend signed the loan showed an error and re-enabled Submit (test red before the fix). (b) Backend: `make_loan_offer_internal` and `exercise_loan_buy_option_internal` were clone→modify→`set_game` and **lost concurrent updates** (stress test red under that code). (c) No protection against a duplicate delivery. | All transfer commands run as one all-or-nothing transaction under the game lock with an optional request id (`StateManager::run_transaction`, `get_transaction_status`); one press = one request; a lost answer is checked against the backend record; nothing after the answer can turn it into an error; Submit stays closed after a lost answer. |
| D3 | Counteroffers "refused systematically" (Bournemouth €35M → €36M for Chevalier) | **CONFIRMED: not systematic** | The buyer *would* accept €36M (ceiling is 1.2× the baseline). `counter_offer` evaluated acceptance first and then returned the hard error `squadFloor.wouldLeaveShort` (PSG with two senior goalkeepers), shown as a raw key. An *excessive* counter on the same player instead **closed the negotiation** although the sale was impossible. | The floor is checked first; the refusal changes nothing and does not spend a round; Decline stays possible; the offers list closes Accept/Counter up front for blocked players (`get_departure_blockers`) and says why. A replacement-dependent agreement was **not** built: registration re-checks the floor at execution, so it would need a new agreement state and a daily watcher; the restriction is explained before submitting instead. |
| D4 | "Senior squad list is full: 31 of 30"; Vitinha sale warning | (a) **CONFIRMED + REPRODUCED**; (b) **REPRODUCED on the real world**; the owner's exact 31/30 screen UNVERIFIED | (a) A sale blocked by an *AI buyer's* full list was reported with the manager's own "Your senior squad list is full". (b) PSG has 23 seniors registered and 7 more out on loan to 2027-06-30; their places are held, so a signing was refused with "23 of 30, free 0" — true to the rule, false as text. | Buyer-named refusal (`buyerSquadFull`, club percent-encoded); `squadFullHeld` with count/max/held/places-to-free. Tested: 31/30 club sells a replaceable midfielder → 30/30; a signing at 31 is still refused; a 22+7 club can sign, 23+7 cannot. |
| D5 | Raw `be.error.squadFloor…?group=common.positionGroups.Goalkeeper` in the counter modal | **CONFIRMED** | `mapTransferNegotiationError` returned `String(err)`; nested-key parameters were never translated. | `describeError` (section 7). |
| D6 | Cannot tell a failed €100M bid / a successful loan from an unchanged row | **DESIGN CHANGE** | Statuses were only visible as small row badges. | Transfer-activity panel (section 4). A fee agreed *before the player's terms* is **not stored by design** (`player_awaits_terms`); the panel says so. |
| D7 | Pre-match pitch: overlapping cards, hidden role selectors | **CONFIRMED + REPRODUCED in Chrome** | Each token was a fixed 96×128–160 px card (avatar, kit icon, name, rating, bar, role `Select`) on a 300–360 px pitch whose rows are only 20 % of its height apart. | Section 9. |
| D8 | M12 formation & substitution panel | feature request | — | **NOT DONE.** |
| D9 | Opponent condition 79–96 % at minute 57 | **investigated: no defect shown** | Section 10. | none |
| D10 | 3-month simulation hidden in the Continue menu | **DESIGN CHANGE** | — | Visible button; section 11. |
| D11 | Schedule is per competition | **DESIGN CHANGE** | — | "My matches — all competitions" is the default (section 12). |
| D12 | `WORLD_REVISION` stayed 9 after Europe | **CONFIRMED** | A revision-9 world (no Europe) was classified current. | Revision 10 + regression test. |
| D13 | `fm24_free_agents` / `fm24_global_market` failures | free agents: **cause established and fixed**; global market: **REPRODUCED, cause not established** | Section 14. | — |
| D14 | Domestic fixture delays 50–66 days | UNVERIFIED | — | **NOT INVESTIGATED.** |

## 3. Production paths changed

Backend: `ofm_core/src/{transactions.rs (new), state.rs, squad_floor.rs, transfers/{bids,execution,registration,consts}.rs, contracts/free_agent_talks/{capacity,consts}.rs, slices/schedule.rs, lib.rs}`, `fm24-adapter/src/install.rs`, `src/commands/{transfers,slices}.rs`, `src/lib.rs`, MCP/journey call sites (pass `None`). Frontend: `services/{careerSession,transactionService,scheduleService,transfersService}.ts`, `hooks/useCareerHydration.ts`, `pages/{MainMenu,TeamSelection,Dashboard}.tsx`, `utils/errorMessage.ts`, `components/{career,transfers,schedule,staff,dashboard,squad,match}/…`. No domain type or SQLite schema changed; **nothing new is persisted** (the ledger is in memory; `UserSchedule`, `TransactionStatus` are IPC-only).

## 4. Transfer and loan status matrix

| Outcome | Where it is stored/read | Shown as |
|---|---|---|
| Loan accepted and active | `Player.active_loan` | Activity panel: Active loans |
| Loan accepted, waiting to register | `LoanOffer.status = PendingRegistration` | Agreed — waiting to register (+ date) |
| Loan rejected / withdrawn | `Rejected`/`Withdrawn` (+`closed_on`) | Rejected or withdrawn (60 days) |
| Permanent offer rejected | `TransferOffer.status = Rejected` | same |
| **Fee agreed, player terms not agreed** | **not stored** (`PlayerTalks` is transient) | explanatory note in the panel; the bid dialog carries it |
| Permanent transfer completed | `Accepted` | Completed transfers |
| Permanent transfer waiting to register | `PendingRegistration` (+`registration_date`) | Agreed — waiting to register |
| Failed with no state change | refusal `be.error.*`, game unchanged (asserted byte-for-byte) | "…Nothing was changed." |
| Result unknown after a lost answer | ledger status | "went through" / "cannot confirm…", Submit closed |

Offers received and the manager's own live offers are separate sections. Tested: classification (8 cases incl. third-party club exclusion, active loan shown once, 60-day window), panel in all 12 languages, save/reload of a pending loan (JSON round trip; **not** the SQLite writer, see 17).

## 5. Concurrency and transaction integrity

* `a_loan_offer_does_not_overwrite_what_another_command_commits`: two threads, 400 concurrent messages + 120 loan offers; **mutation: restoring clone→modify→set fails it** (plus two more tests).
* Duplicate delivery: loan, buy option, accept, counter, bid — one outcome, the repeat replays it. Refusal leaves the game identical (serialized compare). Pending-registration loan survives a reload and registers once.
* Ledger: replay, refused-replay, 16-thread same id runs once, bounded memory, cleared with the career, unreadable replay reported not rerun.
* Cost: a full `Game` clone is **4.4 ms** on the real 16,609-player world (UI projection 4.8 ms; blockers query 0.16 ms), so the all-or-nothing copy has no user-visible cost.

## 6. Squad floor and capacity

Three separate rules, each with its own sentence: buyer willingness; the seller's floor (position cover + 15 seniors); the 30-place list (buyer's, or the manager's own, with held places named). Tests include two senior goalkeepers (blocked, nothing changes, Decline works), three (sale completes), 31 seniors selling a midfielder (→ 30), repeated counter rounds, closed negotiation, blockers equal to the command's own refusal.

## 7. Error localization

One mechanism, `describeError` (`utils/errorMessage.ts`): backend keys, URL-encoded and missing parameters, nested keys as values (unknown ones reduced to a word, never a path), unknown codes, raw Rust prose and JS errors (localized "could not confirm" message with a stable 6-character reference; the raw error goes to the log). Refusals say nothing was changed; an unreadable-outcome key never does. **52 tests across all 12 locales** with a leak deny-list. Two audits run in CI: every `be.error.*` literal in player-facing Rust has an English sentence (it found **7 untranslated keys**, now added), and no source line prints a raw error (fix-fixture proves each can fail; 3 screens fixed). Known gaps: the audits miss keys with inline parameters (`save_load_error.rs`, `spatial_runner.rs` — all currently translated) and `showError(…, String(e))`; `resolveBackendError` remains the display path in 7 other components (`EndOfSeasonScreen`, `InboxTab`, `useMatchSave`, `useLiveMatch`, `SubPanel`, `PlayerRolesPanel`, `PreMatchSetup`), which still show raw prose for non-key failures.

## 8. Career startup / navigation races

Tests deliberately reorder async answers and repeat events: coalescing (mount probe + event share one reading and one navigation), a later request forces a fresh read, events ignored while a career is created/loaded, a deliberate back-out is not bounced, Dashboard keeps the career on a transient failure and leaves only on "no session", TeamSelection never navigates in render. Mutations: coalescing and busy-skip removed → red. **Not covered:** double-click on Create/Load through the real MenuUI (the gate is unit-tested; the wiring is read, not driven), and React StrictMode (dev only) consumes the one-shot suppression flag twice — a dev-only bounce I did not fix.

## 9. Pre-match and M12 interface

* Pre-match (step 8): **measured in Google Chrome** (headless, real `FormationPitch`/`PitchToken`): overlapping token pairs at pitch widths 300/360/520 px — **before 26/22/2, after 0/0/0**, no token outside the pitch. Changes: compact tokens (avatar, shirt number, name, rating, condition) at 16 % of the pitch width, scaled to the row spacing, role control in a selected-player panel (stays reachable in "expand pitch" mode), expand button. Kept: formation/role/set-piece controls, swaps. **Not verified:** the whole pre-match screen in the running app, light/dark contrast, legibility of scaled 12 px text at the smallest pitch (≈0.73× on a 300 px pitch — the reviewer flags this against the 12 px rule), keyboard behaviour. The probe is archived in `docs/appendix/pitch-overlap-probe/`.
* **Step 9 — interactive M12 formation panel: NOT DONE.** No code was written for it.

## 10. M12 fatigue (step 10)

Measured through the interactive-match controller (6 seeds, fit players, balanced tactics), mean on-pitch condition (1 = fresh), home v away: 0′ 1.000/1.000, 30′ 0.851/0.859, **57′ 0.915/0.918**, 75′ 0.836/0.847, 90′ 0.759/0.789. Findings: both sides fall alike; starting condition shows through (start 70 → 0.654 at 57′); the **half-time break restores ≈0.13** (0.744 → 0.874 between 44′ and 48′), which is why 57′ reads higher than 30′; the UI shows `condition × 100`, the same unit. The owner's 79–96 % is inside this distribution. **No defect shown; nothing changed; the M12 repository was not touched.** Whether a +13-point half-time restore is *realistic* is a calibration question for the owner (the M12 report quotes an end condition of 0.63, mine is 0.76–0.79 with synthetic near-uniform players). **Not done:** variation of endurance, physical fitness, pressing, tempo, substitutions. Pinned as three tests (no `--nocapture` needed to pass).

## 11. Delegation and 90-day simulation (step 11)

Done: visible **Simulate 3 months** button beside Continue (opens the existing dialog at 90 days and the play-to-date policy; stop-at-next-match and stop-for-decisions remain choices in it); a warning before full delegation (focus on Cancel, Escape closes, focus returns); sale protection in the Squad and Transfers context menus. **Not done:** (a) a readable *post-run report* of transfers, loans, fees, contract and financial changes, injuries, inbox events and skipped decisions (the dialog still shows only progress, matches and "passed decisions"); (b) deterministic 90-day careers proving one automatic signing and one automatic sale with a protected player staying — the backend delegation engine was **not** re-verified here beyond the existing suite. The protection toggle's squad-menu path is tested; the transfers-menu path is not.

## 12. Unified calendar

`get_user_schedule` projects the manager's fixtures from every competition in the game (league, cups, European, friendlies) in date order, each with its competition id, fixture id, home/away, status/score, next-match mark, original date if moved. Tested in Rust (merge order, next match across competitions, postponed match moves and keeps its original date, results, clubless manager) and 18 component tests (PSG-style: Champions League visible without choosing it; filter; replay ids; moved note; 12 locales). A clubless manager gets an empty list, not an error. Not tested: competition renewal and save/reload of the combined view, a fixture dated exactly today in two competitions, friendlies/international rows (the projection is generic over `game.competitions`).

## 13. Wider gameplay audit (step 14)

Covered by the existing suites plus this mission's: permanent transfers, loans and buy options, free agents (source-backed), squad floor, finances (ledger/posting not re-audited), calendar projection, save/reload of pending deals. **Not verified by this mission:** scouting visibility/eligibility, youth academy actions, staff hiring, training, contract renewal, European competitions beyond the existing INTEGRATION5 evidence, classic vs M12 match simulation (only the condition measurement), season rollover, dismissal. They are not claimed as tested here.

## 14. Real-world tests (`fm24_free_agents`, `fm24_global_market`)

Built to a scratch directory from the real source (`apfm-import`, 31 MB, read-only on `world.db`), run with `APFM_WORLD_JSON`/`APFM_WORLD_DB` **set explicitly** (without them these tests print nothing and pass — they are not run by the default suite).

* `the_player_can_sign_an_fm24_free_agent_at_the_wage_he_asks` — **REPRODUCED, root cause established**: PSG has 23 registered + 7 on loan until 2027-06-30; their places are held (7 free ≤ 7 held). The rule is deliberate; the refusal text was the defect. Fixed (D4b); the test now asserts the held-places refusal, makes room as a manager would (moves a senior to the youth squad) and signs. 4/4 pass.
* `clubs_outside_the_players_league_buy_players` — **REPRODUCED, cause NOT established, left failing.** Measured: 72 AI↔AI transfers a season (test wants ≥120), 51 (71 %) bought by out-of-scope clubs, 46 (64 %) crossing a border (test wants <55 %). It passed at I2 (2026-11-13) and fails on the control world without Europe too (48 transfers). Earlier audits document the volume falling from ≈500 (Run F2) to 36–72 through the economy-realism work and call it defensible, but I could not show whether 72 is right, and the brief forbids editing the expectation to get green. **P2, owner/balance decision.**

## 15. Multi-season tests

With `APFM_WORLD_DB`/`APFM_WORLD_JSON` set, `cargo test --release -p fm24-adapter -p season-harness --no-fail-fast` ran against the real source-backed 310-club world: **352 passed, 1 failed** (the documented `clubs_outside_the_players_league_buy_players`, section 14), 97 ignored, **no SKIPPED outputs**. That includes the multi-season, save/reload, European and domestic invariant harnesses (no duplicated players, lost fixtures, double postings or corrupted state were reported). I did not add new multi-season tests of my own, and the player's real saves were not opened.

## 16. Gates and mutation checks

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (default and `--features mcp`) | clean (after fixing 5 findings in my own tests) |
| Rust workspace tests, default | 5,662 passed, 1 failed on the first run (`the_one_pass_registration_gate_agrees_with_the_four_pass_original`: its *reference* still produced the old misleading message — updated and re-run green) |
| Rust workspace tests, `--features mcp`, final code | **5,755 passed, 0 failed**, 156 ignored |
| Source-backed (`APFM_*` set) adapter + season-harness | 352 passed, 1 failed (known, open), 97 ignored |
| Vitest (full), after the last edit | **3,456 passed**, 4 skipped (pre-existing) |
| `tsc` + `vite build`, Biome format and lint, Knip | clean (3 pre-existing Biome infos) |
| i18n suites (12 locales), architecture/design-system tests | green |
| **Mutation checks** (rule removed → named test red → file restored byte-for-byte) | **23 killed, 0 survived**: Rust 11 (ledger replay, ledger atomicity, loan clone→set, floor-first counter, buyer-list wording, held-places wording, blockers, schedule filter, world revision, request id ignored, ledger recording) and frontend 12 (coalescing, busy skip, loan local-step conflation, double press, nested keys, unknown-code fallback, activity sections, delegation warning, pitch scale, blocked accept, schedule filter, refusal detection) |
| Reviewers (read-only agents): architecture, i18n, accessibility, dedup, test quality | no P0. **Fixed:** clubless Schedule error, `outcomeUnreadable` + "nothing changed", Submit open after a lost answer, 4 accessibility P1s, Czech mistranslation and cs/ru/tr/zh wording, plural-verb string, ScheduleTab size, tautological assertions. **Reported, not fixed:** items in section 17 |

The `--features mcp` workspace run (a superset of the default one) was made on the final Rust code; the default-feature run was made one commit earlier and only its one failure was re-run.

## 17. Remaining issues

**P1:** none known open. **P2:** step 9 not built; step 11 report/90-day verification; step 15 not investigated; `clubs_outside…` real-world test red; unverified GUI; StrictMode dev bounce; `resolveBackendError` still shows raw prose in 7 components; audits' blind spots (section 7); error-text/percent-encode/param-resolver duplication named for epic #589 (`percent_encode` is the 5th Rust copy; three TypeScript error-text extractors); the frontend and backend define "next match" separately; four near-identical submit handlers; `ScheduleTab` is now 692 lines but `PreMatchSetup`, `Dashboard`, `SquadRosterView`, `MainMenu` components are still over the 200-line function limit; `.lock().unwrap()` in `with_ledger` is now poison-tolerant but not annotated; the transfers panel's "selling" test cannot distinguish a sale by the manager from an AI→AI move to a bidder (offers carry no seller). Persistence tests use JSON round trips, not the SQLite writer + fresh reader.

## 18. Native vs browser vs component vs static

* **Native Rust execution:** all Rust tests, the source-backed runs, the condition measurement, mutation checks.
* **Real browser (headless Chrome):** the pitch-overlap probe only (synthetic players, real components).
* **Component/hook tests (jsdom, mocked IPC):** everything else in the UI, including races. Not equivalent to operating Tauri.
* **Static inspection:** reviewer reads (architecture, i18n, accessibility, dedup, test quality — all read-only agents; none re-ran my mutations).
* **No display / GUI not operated** (as in earlier missions); `scripts/gui-validation/run-isolated.sh` was not used.

## 19. Source database SHA-256

Before: `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7`. After the source-backed runs and the importer: **identical** `36b6908c…27439f7`. The only world built was written to `~/apfm-scratch/alpha6-world` (72 MB, removable).

## 20. Disk and cleanup

`src-tauri/target` 9.1 GB (reused, no second worktree), `~/apfm-scratch` 72 MB (safe to delete), 44 GB free. The headless-Chrome profile and logs are in the session scratchpad.
