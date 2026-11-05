# APFM Run F5.1 — Intelligent world, club economy, generational balance, storage

Branches: OFM `claude/run-f5-1-intelligent-world`, M12 `f5.1/intelligent-world` (no M12 change).
Started from the verified F5.0 heads: OFM `f97caf83acb28f3a06f25acfba23240fcea8d2e8`, M12
`d4e6f329d7e48279a9dce5f7f3c1b970f7ff2131` (both read with `git rev-parse` before branching). Nothing is merged
into `apfm/main` or `develop`. Date: 2026-11-04.

## 1. Executive verdict

**GO for the next phase on the football *world*; NO-GO for calling the football final or opening a beta.**

* The world no longer degrades over 30 seasons (3 seeds, 80 clubs): elite players ≥ 80 are 34–39 at season 30 against
  18–33 at season 0; top-10/50/100 means are level or higher; major clubs keep a structural edge (strength percentile
  0.72–0.75, small clubs 0.19–0.26) without being pinned there; clubs in debt fall from 22–25 of 80 to 8–11 and the worst
  debt from −190 M to about −55 M; the median club holds ~27–29 M.
* Root causes found and fixed (all measured, all with regression tests): a **flat generated economy** (reputation meant nothing:
  stadium size was random, no AI club had a sponsor, wages were independent of income), an **academy baseline bug** (every AI
  intake lost 2 potential points, which halved the ≥ 80 tail), a **wage ratchet** at renewals, and **misleading zero
  statistics** for dormant clubs' players.
* Storage after 30 seasons is **123 MB** (F5.0: 168 MB; 184 MB before this run's journal fold).
* **Release blocker, unchanged: M12 scoring.** The production engine scores **4.85 goals a match** on held-out career inputs
  (football ≈ 2.6). A bounded attempt found no single structural fix; it is documented with a failing gate script (§14, §22).
* **Not done:** no GUI playthrough was possible (§18), succession-aware squad planning, release clauses, new delegation
  depth, load time did not improve (§15).

## 2. Starting branches and verified commits

| Repo | Branch | Verified head at start |
|---|---|---|
| OFM | `claude/run-f5-0-universal-simulation` → new `claude/run-f5-1-intelligent-world` | `f97caf83` |
| M12 | `f5.0/universal-simulation` → new `f5.1/intelligent-world` | `d4e6f329` (uncommitted licence change in `Cargo.toml`, untracked `LICENSE`/`NOTICE`: left as found) |
| Viewer | `main` | `88f2ce61`, not touched |

## 3. Existing systems audit

**V** verified · **D** defective · **I** incomplete · **M** missing. "After" is this run's state.

| Subsystem | Where | Before | After |
|---|---|---|---|
| AI transfer strategy / needs | `transfers/strategy.rs`, `market.rs` (`target_fills_a_need`) | V (need-based, profiles) · I: no age/succession planning | V · I unchanged |
| Valuation, offers, counter-offers, negotiation rounds | `transfers/bids.rs`, `realism.rs` | V (rounds, suggested counter, break-off) | V |
| Contracts, renewals | `contracts/`, `ai_contracts.rs` | D: wage ratchet (+15–30 % a renewal, never down) | **fixed for AI** (freeze when over budget) |
| Release clauses | — | M | M |
| Wage budgets | `ai_budget_review.rs`, generator | D: set from the generated squad, not from income; cuts capped at 5 %/yr | fitted to income at generation; 15 %/yr cut in debt |
| Loans | `transfers/loans.rs` | V (existing harness tests) | V |
| Squad planning / floor | `squad_floor.rs`, `ai_contracts::plan_squad_depth` | V · I: no ageing-starter anticipation | V · I |
| Roles, playing-time promises | `ai_roles.rs`, `post_match.rs` | V | V |
| AI managers, tactics | `ai_manager_appointments.rs`, `ai_tactics.rs` | V | V |
| Board objectives | `board_objectives.rs` | V | V |
| Reputation | `reputation.rs` | D: no economic meaning in a generated world | drives income, squad quality, academy |
| Revenue / expenses / cash / debt | `finances/`, generator | D: flat; AI clubs had **no sponsor**; ground size random | see §8 |
| Youth intake | `youth_intake.rs` | D: −2 potential for every AI intake | baseline restored; ±1 by standing |
| Development / decline | `development.rs` | V (calibrated, minutes/injury-aware) | V |
| Retirement | `aging.rs` | V age rule; free-agent drift-out removes players at ~28 in this closed 80-club world | unchanged, documented |
| Scouting, staff, facilities | `scouting.rs`, `staff_system/` | V · I: AI never staffs a youth department | unchanged (compensated by the baseline) |
| AI lineup selection | `turn/squad.rs` | D: flat rest tolerance, 3.7 % of starters < 50 % | graded tolerance (§12) |
| Injuries, suspensions | F5.0 | V | V |
| Career statistics | `post_match.rs`, `commands/stats` | D: dormant minutes produced per-90 zeros | `detail_minutes_played` (§13) |
| Season rollover, save/load | `end_of_season`, `db` | V · storage grew | §15, §17 |
| Delegation | `staff_system/delegation*.rs`, harness `delegated_*` tests | V (existing tests pass in the full suite) | **not extended** (P2) |

## 4. Implemented code changes (commits in §23)

`club_economy` (new), `generator/club_shape` (new), AI sponsorships, wage freeze, debt-aware budget cut, academy shift,
graded rest tolerance, `detail_minutes_played`, `CashJournal::fold_before`, v068, `world_health` instrument and the f51 studies.

## 5. AI club management

What exists and was measured, not rebuilt: clubs plan depth by position group against a floor and a planning margin, buy only
where a target improves cover (`target_fills_a_need`), list surplus/expiring/over-budget players, renew by need and budget.
New: clubs now **earn** according to who they are (§8), so the finance guard and the budget review constrain recruitment
differently for a large and a small club. Over 30 seasons (3 seeds) the market made 1,640–1,690 paid transfers (~55 a year,
~2.0 bn in total fees) and ~8,300 free signings; squads stay at a mean of 20.6–20.9 seniors with no club below the floor except the
harness's passive user club.

## 6. Squad planning

Unchanged. Open item: an ageing starter (a goalkeeper at 34) is not anticipated; planning reacts when the contract or the floor
requires it. Position-cover "thin line" counts (clubs below 2 GK / 5 DF / 5 MF / 3 FW natural players) are 7–12 of 80 at
season 30 and 0–4 at season 10 — a watch item, not a failure (formations need fewer).

## 7. Transfers and contracts

* AI renewals: a content player of a club **over its wage budget** is frozen at his current wage; an unhappy one still asks the full
  wage and leaves if the board refuses (`settled_renewal_wage`). The wage bill no longer ratchets without a matching income.
* The user's negotiation path is unchanged (rounds, counter-offers, break-off round).
* Not implemented: release clauses (M), per-window strategic planning beyond the weekly review.

## 8. Financial model

The one dial is `club_economy::income_scale(reputation)`: 0.4 at the bottom of a pyramid to 1.6 at the top (4×). It sets the
generated stadium (`45 000 × scale ± 15 %`), ticket price (×0.8–1.2), the AI sponsor (65 000 × scale a week, signed for a year
on Mondays, never for the user's club), the opening wage bill (90 % of the board's own plan, fitted by one factor per squad)
and the opening squad quality (−3…+3 rating points). Nothing names a club.

Baseline (F5.0 code): by reputation tercile the stadium was ~46–52 k seats, wage budget 0.27 M a week and wages 12 M a year for
**every** tercile; small-ground clubs could not cover their bill. After 30 seasons 22–25 of 80 clubs were in debt, worst −164…−201 M.

After (3 seeds, season 30, final code): clubs in debt **8 / 9 / 11** of 80, worst balance −30…−55 M, median cash 29.0 / 27.2 / 27.3 M,
90th percentile 100 M+. Operating margin by tercile (seed 1, season 15–25): top +3.4 to +3.8 M, middle about −1 to 0, bottom about
−1.1 to −1.6 M a year: **mid and small clubs still drift into a small structural deficit** (wages creep ~30 % while revenue is flat).
Debt counts still rise slowly after season 20 (seed 3: 1 → 10 → 11). Not solved; a growth index for revenue was rejected because
it would hand every existing save a one-off windfall.

## 9. Youth development

Root cause of the thinning tail: the academy shift was centred on a youth-department rating of 50 and `VACANT_RATING` is 20, and
**no AI club ever hires youth staff**, so every AI intake was drawn 2 potential points low (arrivals' mean 68.0 against 69.6).
Now an empty department is the baseline, staff add up to +3, and a club's standing adds a symmetric ±1 (`academy_potential_shift`):
arrivals' mean 69.3–70.2, p90 75–77, and 1–3 arrivals a year with potential ≥ 80 at every tier. Arrivals ≈ departures ≈ 190 a year.

## 10. Generational balance (3 seeds, club players only)

National-pool fillers (World Cup benches, `NATIONAL_POOL_ID_PREFIX`) are excluded: they entered ~1,000 players every fourth season
and had swung the earlier numbers. Baseline before the academy fix (seed 1, clean instrument): players ≥ 80 18 → 13 (-28 %), prospects
(≥ 80 potential, age ≤ 21) 25 → 6.

| Season 0 → 30 | seed 1 | seed 2 | seed 3 |
|---|---|---|---|
| top 10 mean | 82.3 → 82.4 | 83.5 → 83.3 | 83.9 → 83.1 |
| top 50 mean | 79.2 → 80.5 | 80.3 → 80.6 | 80.5 → 80.5 |
| top 100 mean | 77.8 → 79.4 | 78.5 → 79.5 | 78.8 → 79.4 |
| players ≥ 80 | 18 → 39 | 27 → 34 | 33 → 34 |
| best player | 84 → 85 | 87 → 85 | 87 → 86 |
| prospects ≥ 80 pot. | 25 → 19 | 18 → 16 | 34 → 14 |
| population | 2 160 → 2 514 | 2 160 → 2 539 | 2 160 → 2 518 |

Honest caveats: this world's scale tops out at 84–88 (no player ≥ 90 at any point; ≥ 85 is 0–2 in steady state); the opening
world's prospect crop is richer than the steady state (prospects fall to 11–22); club players over 30 are ~12 % of the squad
(football ≈ 25 %) because players who find no club drift out of this closed world at ~28; the population grows 17 % in the first
ten seasons then holds.

## 11. Major-club competitiveness

Cohorts are the top and bottom quarter by reputation *within each country at season 0*. Mean within-country strength percentile
(squad = best 14 seniors): big **0.70–0.80 → 0.72–0.75**, small **0.23–0.27 → 0.19–0.26**; the correlation of reputation with
strength is 0.66–0.75 at season 0 and settles at 0.51–0.57. Baseline: big 0.38–0.53, i.e. **no advantage at all**, even at season 0.
Big clubs sit in the top strength quarter 50–60 % of the time at season 30: an edge, not a monopoly.

## 12. AI squad rotation

`ai_select_starting_xi` tolerated a flat 6–12 point drop to rest a tired player below the "exhausted" line (45 %). The tolerance now
grows 0.4 points per point of condition under the manager's rest threshold, capped at the exhausted figure (20).
Measured with the F5.0 study (2 seasons, same instrument): AI starters under 50 % at kick-off **3.7 % → 2.5 %** with every club
played in full (86 k starters), **0.8 %** in the default followed scope; the user's club 0.0 %. Position groups are uniform (GK 1.4 %,
DEF 2.3 %, MID 2.8 %, FWD 2.7 %), so it is not a deputy-availability problem; the residual is unexplained and attributed, unproven, to congested
cup/league weeks. Match importance and injury risk are not inputs.

## 13. Historical statistics

Contract (kept from F5.0, now explicit): permanent results, standings, champions and records; permanent per-player totals (season
statistics and folded totals); recent match lines for five seasons where the full engine played; none for dormant fixtures.
Defect found: the Advanced Stats card derived shots/passes/tackles per 90 from season totals, so a dormant club's player showed
0.00 over 2,700 minutes. Now `PlayerSeasonStats.detail_minutes_played` counts only minutes whose counters were recorded
(engine matches add them, the lightweight tier adds none, legacy saves are backfilled on load), the overview reports
`detailRecorded`, and the card shows a dash and a translated note (12 locales). Nothing is fabricated. The recent-matches card
already renders nothing when there are no lines. No compact per-match record for dormant fixtures was added (storage first).

## 14. M12 realism (release blocker)

Reproduced with the real integration: `dump_adapted_career_inputs` on the new-economy world → M12 example, **production preset `f44`**,
held-out inputs (40 inputs × 12 seeds = 480 matches):

| variant | goals | shots | on target | offsides | dismissals | penalties |
|---|---|---|---|---|---|---|
| production (`base`) | **4.85** (±0.20) | 41.6 | 19.1 | 3.7 | 0.27 | 0.75 |
| neutral tactics | 4.12 | 39.0 | 17.9 | 1.6 | 0.22 | 0.54 |
| identical teams (control) | 3.39 | 37.4 | 16.8 | 1.5 | 0.18 | 0.50 |

Football ≈ 2.6 goals, ≈ 27 shots, ≈ 0.06 dismissals. Layers: extreme tactics ≈ +0.7, team heterogeneity ≈ +0.4, base engine
shot volume +10 shots. A first run used `f43` (the example's default) and read 4.92: **the example must be run with `F44_FLAGS=f44`**.
The `finish` quality floor (0.35, 0.45) lowered goals only from 4.75 to 4.46 (inside noise): not the lever. No engine change was
made: the remaining corrections are the F4.2–F4.4 research line (passing/recovery over a high line, shot generation, movement), a
versioned preset and new golden digests, and were not safe to attempt blind. `scripts/f51-m12-realism-gate.sh` reproduces the
measurement and **fails by design** (ceilings 3.3 goals, 32 shots).

## 15. Storage

30 seasons, 80 clubs, every competition played, release build (`f50_storage_growth`, `F50_SEASONS=30`):

| | F5.0 | F5.1 before fold | **F5.1 final** |
|---|---|---|---|
| file | 168 MB | 184 MB | **122.6 MB** |
| match history | 40 MB | 39 MB | 39 MB |
| cash journal (table + 3 indexes) | 58 MB | 98.5 MB | ~40 MB |
| retired players / managers | 35 / 10 MB | 15 / 6.4 MB | 15 / 6.4 MB |
| save / load at 30 seasons | 1.5 s / 1.0 s | 2.5 s / 1.8 s | **1.9 s / 1.7 s** |

The journal grew because solvent clubs now pay upkeep and sponsors post weekly. The fold replaces posts older than two years by one
per club, kind and month (balances exact; opening balances and folds are never folded); v068 deflates the retiree archives. Remaining
growth: ~3 000 fold rows a year, `competition_edition_archive` 24 MB, `retired_players` 15 MB, messages 7 MB. **Performance did not
improve:** load is 1.7 s against F5.0's 1.0 s (population +17 %, 15 k retirees to inflate, 109 k journal rows; a 20-season profile
is in `f51_load_profile`, retirees 134 ms and editions 129 ms of 864 ms, the rest unattributed). Process RSS 1.6 GB (benchmark
holds copies). The app's own RSS was not measured.

## 16. Thirty-season benchmarks

Three seeds × 30 seasons × gate world (England 2 tiers, Portugal, Argentina; 80 clubs), `f51_world_health`, ~100 s a run. One
world shape only: the shipped 440-club world and package worlds were **not** run for 30 seasons.

## 17. Save/load validation

`f51_journal_fold`: five seasons, saved each season, reloaded: 34,180 posts (9,212 folds), every club's cash and the post count equal
the live game. Unit tests: fold sums, opening balance kept, idempotence, DB deletion of exactly the replaced rows, the save that
carries a fold clears the pending marker, v068 rows beside legacy JSON rows, damaged blobs fail with a translated error, the v065-style
"released build" imitation tests drop the new columns. A loaded legacy save is compacted at its first rollover (not run on a real
user save: saves were not touched).

## 18. Desktop validation

**Not performed.** This session cannot drive or capture a window. `scripts/gui-validation/run-isolated.sh --smoke` (isolated data root)
exists and was not run. What was verified instead: `npm run build` (tsc + vite) succeeds, the backend commands behind the screens are
covered by tests. Manual checklist to run: create a career; pick a club; open the squad; set tactics; advance days; play or simulate
a match and read the report; open a dormant-league player's profile and check the **Advanced Stats card shows dashes and the note, not
zeros**; check injuries/suspensions and the league table; negotiate a transfer; sign a player; open Finances (an AI club's
sponsor and gate are not visible there, the user's are); pass a transfer window; finish a season; save and quit; reload; start the
next season; open a pre-F5.1 save and confirm it loads (v068 migrates) and rolls over.

## 19. Test results

* Rust, default features, `cargo test --workspace --no-fail-fast`: final run on the last code: **4,547 passed, 0 failed, 72 ignored**. With `--features mcp`: **4,633 passed, 0 failed** on the run before
  two non-semantic edits (a type alias and an import move for clippy), not repeated. F5.0 had 4,498 / 635 (a different count basis: its
  figure listed default features and MCP separately).
  **One unstable test:** `generator::world_io::tests::a_manifest_without_a_teams_shard_is_an_error_not_an_empty_world` failed once in a parallel
  full run and passed three times alone and in every other full run; it is outside the code this run changed and was not diagnosed.
* Clippy `-D warnings --all-targets`, default and `--features mcp`: clean. `cargo fmt --check`: clean.
* Frontend: 2,916 tests pass (F5.0: 2,913), `npm run build` (tsc + vite) passes, Biome lint and format clean, knip clean.
  `npm run audit:i18n` is advisory and flagged nothing in this diff.
* New regression tests, by area: club economy, generator shaping (reputation predicts squad strength and ground, no opening bill above model
  income), sponsorship renewal, wage freeze (pure and through `renew()`), debt-aware budget cut, academy shift, graded rest tolerance (with
  controls and a cap case), detail minutes (domain, post-match, dormant, db round trip, stats overview, card, helper), journal fold
  (domain, ofm_core, db, end to end through `persist_active_game`), v068 deflate (both archives, legacy JSON rows, damaged blobs).
* **Mutation testing, by hand, one mutant at a time:** 27 mutants of the new logic, 26 killed, 1 equivalent (a damaged retiree blob
  decoded to an empty string fails JSON parsing with the same translated error). Three were first mis-written (an equivalent mutant, a
  line `cargo fmt` had reflowed, and a test whose cap case could not bind) and were corrected until each was killed for the right reason.
* Studies (not gates): `f51_world_health`, `f51_finance_flows`, `f51_journal_fold`, `f51_load_profile`, the F5.0 storage and
  consequence studies.

## 20. Reviewer findings

Five read-only reviewers ran on `git diff f97caf83..HEAD` (the UI reviewer's brief was folded into the i18n auditor's).

| Reviewer | Finding | Disposition |
|---|---|---|
| architecture | `fold_before` could drop a month's posts on i64 overflow, contradicting its comment | **fixed**, test `a_month_that_cannot_be_summed_is_kept_whole` (mutant killed) |
| | `renew()` scanned every player twice | **fixed** (bill computed once, `wage_policy_verdict_at_bill`) |
| | doc comment on the wrong function in `journal_repo` | **fixed** |
| | `world_health` is ~245 lines (limit 100) | **not changed**: harness instrument; split named for epic #589 |
| | `season-harness` not classified in the crate matrix; fold ids use the Rust `Debug` name of a `CashKind` | not changed, named |
| dedup | the ticket-price factor reached the weekly gate and the AI estimate but not two older estimates (snapshot, upkeep) | **fixed** |
| | the gate estimate constants (0.76, 20.0, 19) exist in five places; two definitions of "stature" (400–780 vs 250–900) | not changed, named for #589 |
| | `world_health` re-implements position grouping, squad thresholds, mean/median helpers | not changed, named |
| | JSON-or-deflated row read written four times in `db`; `fold:` prefix repeated in SQL | not changed, named |
| test | no test of the gate price wiring, the debt wiring in `review_wage_budgets`, the academy shift wiring, fold after a reload of an already-folded or legacy save, `fold_old_posts` being called at rollover | wage-freeze wiring, month alignment and fold idempotence **added**; **the rest are open** |
| | three tests pass on seeded judgement noise at mid manager quality (48 %/58 % cases); `world_health` has no tests | open |
| i18n / accessibility | key complete in 12 locales, no Rust prose; note had no semantic role and the dashes no tie to it | **fixed** (`role="note"`, `aria-describedby`); dash glyph and raw palette classes left, as in the rest of the card |

## 21. Unresolved defects

* M12 scoring/shots/movement/discipline (blocker). Numerical superiority, pressing, referee: not re-validated.
* Mid and small clubs run a small structural deficit; debt counts rise slowly after season 20.
* Young age profile (club players over 30: ~12 %), extreme tail (≥ 85) is 0–2 players in steady state.
* AI starters under 50 %: 2.5 % when everything is played in full.
* Load time 1.7 s at 30 seasons (+0.7 s against F5.0); journal still grows ~3 000 rows a year; messages and archives unfolded.
* AI academies are never staffed; facilities do not affect intake.
* No succession-aware planning, no release clauses, delegation unchanged.
* `APFM_ENGINE_READ_TOKEN` is still not a repository secret (F5.0 §22).

## 22. Release blockers

1. **M12 realism** — `scripts/f51-m12-realism-gate.sh` must pass (≤ 3.3 goals and ≤ 32 shots on held-out career inputs, `f44` or its successor).
   Follow-up plan: versioned `full_match_f51` preset; bisect order from §14 (tactic extremes at the OFM adapter, shot generation, then
   settled possession over a high line); new digests; replays of old matches stay on their stored revision.
2. **No GUI playthrough** of any F5.x change, including a migrated v65–v67 save.

## 23. Exact commits

OFM `claude/run-f5-1-intelligent-world`, from `f97caf83`:

| SHA | What |
|---|---|
| `83a68b8c` | club economy, academy baseline, wage freeze, debt-aware budget cut, graded rotation, `world_health` instrument |
| `82a2b4d4` | statistics contract: detail minutes, "not recorded" card |
| `59a5bd6e` | journal fold, v068 deflated retiree archives, studies, M12 gate script |
| `6f9061ae` | review fixes and this report |
| the commit that adds this table | SHAs and push status (it cannot name itself; `git log -1` on the branch) | M12 `f5.1/intelligent-world`: no change, `d4e6f329`, the pin is unmodified.

## 24. Push status

Both branches are **pushed**, nothing merged into `apfm/main`, `develop` or any other branch:

* OFM `origin/claude/run-f5-1-intelligent-world`, code head `6f9061ae`;
* M12 `origin/f5.1/intelligent-world` at `d4e6f329` (no change, so the OFM pin in `ofm_core/Cargo.toml` and `Cargo.lock` is unmodified and was not re-verified; M12's uncommitted licence change and untracked `LICENSE`/`NOTICE` were left as found; the viewer was not touched).

No credential was read or written. User saves and original datasets were not touched.

## 25. GO / NO-GO

**GO** to the next phase for: world health, club economy, generational balance, storage. **NO-GO** for "football final" and for a public
macOS beta until §22 is closed.
