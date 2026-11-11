# APFM I1: acceptance

Companion to [`APFM_I1_P5_D7B2_INTEGRATION.md`](APFM_I1_P5_D7B2_INTEGRATION.md) (what was merged, conflicts, defects). This document
is the evidence and its limits. Branch `integration/p5-d7b2`; nothing pushed.

Statuses: **PASSED**, **FAILED**, **NOT RUN**, **BLOCKED**. Host: macOS, Apple Silicon, rustc 1.95.0. One heavy run at a time, so
every timing and memory figure was measured with the machine otherwise idle.

## 1. Verdict

**CONDITIONAL GO.** P6 and D7B3 can start in parallel from the integration commit, under the file ownership in the integration
document, section 8.

Why not GO: (a) **no person has played the GUI**, and no GUI path reaches external recruitment at all; (b) the imported world,
which D7B2's product serves, has economy and reload behaviour P5 never validated (K5, K6); (c) D7B2's published market figures
no longer reproduce after integration (K8) and a D7B1 sort-direction defect (K1) must be fixed before a screen exists.
Why not NO-GO: no critical regression was found. The combined simulation is **digit-identical** to P5's tip on every study run,
the one real integration defect (the reservation hole) was found, fixed and pinned, and both source tips are fully merged.

## 2. Commands and results

| # | Command | Result |
| --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | **PASSED** |
| 2 | `cargo clippy --locked --workspace --all-targets -- -D warnings` | **PASSED** (clean) |
| 3 | `cargo clippy --locked --workspace --all-targets --features mcp -- -D warnings` | **PASSED** (clean) |
| 4 | `cargo test --locked --workspace --no-fail-fast` (final commit `e3c527cf`) | **PASSED**: exit 0, **143 targets, 4,964 passed, 0 failed, 105 ignored** (P5's tip: 142 / 4,905 / 99). 552 s, 2.9 GB peak. |
| 5 | `cargo test --locked --lib --features mcp` | **PASSED**: 667 passed, 0 failed, 10 ignored (P5: 653) |
| 6 | `npm ci` then `npm run lint` | **PASSED**: Biome **2.5.5** from `node_modules` (not the `biome@0.3.3` impostor); 857 files |
| 7 | `npm run format:check` | **PASSED**: 855 files |
| 8 | `npm run build` | **PASSED** (`tsc && vite build`) |
| 9 | `npm test` | **PASSED**: 341 files passed, 1 skipped; **2,997 tests passed**, 4 skipped (P5 2,993 + D7B2's 4 `registryService` tests) |
| 10 | `npm exec --no -- vitest run src/i18n` (the real i18n gate) | **PASSED**: 9 files, 35 tests |
| 11 | `npm run audit:i18n` | Ran, exits 0 by design. Read: the only integration-touched file listed is `fm24-adapter/src/registry.rs` (5 internal exclusion reasons that no UI reads, `exclusionReason` has no consumer). No player-visible string was added; every refusal reuses a key. |
| 12 | `cargo check --locked -p ofm_core -p db` **without** the local M12 override | **PASSED**: the pinned git revision `28f34b10` resolves |

**First workspace run** (before the finance-guard fix): 142 targets, 4,963 passed, **1 failed**, 102 ignored. The failure was
`finance_journal::production_code_does_not_assign_finance_with_plus_equals`, caused by **my** new test file assigning
`team.finance`; fixed in `2aca4712` (cash funded through the ledger) and verified green (4 passed). Row 4 is the run on the final code.

**Test additions (all pass):** 11 in `ofm_core/tests/transfers_tests/external.rs`; 8 in `src/commands/registry/integration_tests.rs`;
3 ignored studies (`i1_combined_career`, `i1_world_overlap`, `i1_fm24_reload`).

**Mutation checks** (remove only the guard, see red, restore, `cmp` the restore byte-exact): dropping the `fm-n-` exclusion fails the
Newgen and reload tests; weakening the elite recruit fails the selection test. The reservation and squad-limit tests were seen
**red on the merged code** before the fix (4 failures, the predicted keys) and green after.

## 3. Required integration scenarios

All run through the production path on both sides (invented source `world.db` → `build_registry` → `ensure_career_cache` →
`search_internal`/`detail_internal` → `recruit_internal` into a generated P5 gate world → `GamePersistenceWriter`/`Reader`).

| # | Scenario | Test | Status |
| --- | --- | --- | --- |
| 1 | External free-agent signing | `a_free_agent_is_signed_once_and_cannot_be_signed_again`, `a_registry_player_is_promoted_once_and_leaves_the_market` | **PASSED** |
| 2 | External contracted-player signing | `a_contracted_player_is_bought_with_one_fee_and_a_named_origin` | **PASSED** |
| 3 | Rejected offer without mutation | `a_rejected_offer_changes_nothing` (the whole serialised game is byte-identical) | **PASSED** |
| 4 | Counteroffer and affordability | `a_counter_is_paid_at_the_floor_and_only_if_the_club_can_afford_it` | **PASSED** |
| 5 | Duplicate promotion prevention (`fm-p-*`, `fm-n-*`) | `a_newgen_identity_is_promoted_once_under_its_own_id`, `a_newgen_is_promoted_once_and_leaves_the_market`, `a_player_the_game_already_has_is_never_offered` (roster and retired archive) | **PASSED** |
| 6 | Pending fee/wage reservations | cash, budget, wage, "registered deal not reserved twice", "a bid cannot spend what an external purchase took" (5 tests) | **PASSED** |
| 7 | Available in the next match | `a_recruit_is_selectable_in_the_next_match` (five-zone team sheet **and** the M12 input) | **PASSED** |
| 8 | Save/reload after recruitment | `a_recruit_survives_a_save_and_reload` (an `fm-p-*` and an `fm-n-*`: same club, wage, contract end; neither offered or recruitable again; seed saved) | **PASSED** |
| 9 | Career-seed isolation | `careers_are_independent_and_the_source_is_never_written` (another seed and a fork of the same seed still see him; source byte-identical; one cache per seed) | **PASSED** |
| 10 | Cache rebuild after loading an older save | `an_older_save_rebuilds_the_cache_and_the_market_is_reproducible` (rebuilt; same market as before; **cache deletion loses nothing**; 4 concurrent readers agree) | **PASSED** |
| + | Squad limit; generated-world namespaces | `an_external_free_agent_respects_the_senior_registration_limit`, `a_generated_world_never_uses_the_registry_namespaces` | **PASSED** |

Financial rules verified (items from the brief): accepted-but-pending transactions reserve the right funds; wages are not reserved
twice; a second transaction cannot spend committed money; a rejected or failed offer leaves finances (and everything else)
untouched; registration does not double-count. **"Delegated transactions do not bypass financial restrictions": the external
route is not reachable by delegation** (no assistant, MCP or AI path calls it), so there is nothing to bypass; P5 notes that
delegation's own *cash* room does not subtract pending fees, and that is unchanged.

## 4. Long-career validation

All **PASSED**. Studies: `season-harness/tests/p45_long_career.rs`, release profile, invariants on, `/usr/bin/time -l`.

| Run | World | Result | Runtime | Peak RSS | vs P5's tip |
| --- | --- | --- | --- | --- | --- |
| 3 seeds × 10 seasons | gate (80 clubs) | invariants **clean ×3**; 0 of 80 clubs in debt at season 10 (all three seeds) | 137.8 s | 191 MB | 137.3 s, 187 MB; **data rows identical (0 diff lines)** |
| 1 seed × 30 seasons | gate | invariants **clean**; 0 of 80 in debt at season 30 | 138.7 s | 659 MB | 139.2 s, 742 MB; **data rows identical (0 diff lines)** |
| Shipped world, 30 seasons | 440 generated clubs | **NOT RUN on the integrated tree.** P5 ran it on its own tip (2,199.7 s, invariants clean, 0 of 13,170 club-seasons in debt). Given the identity proof above, no change is expected, but it is not claimed. | | | |

The baseline is P5's tip built in a detached worktree in the scratch directory with its own target directory (the source worktrees
were not touched). "Identical" compares every season, club, squad, finance and formation row; only the wall-clock "seconds per
season" lines differ. **This is the strongest evidence in I1 that integration changed nothing in the simulation.**

Seniors per club at season 10 / 30: mean 23.2-23.5, range 11-29; one club below 16 (identical on P5's tip, K12).

## 5. The imported FM24 world (240 clubs)

P5's long-career work is on the generated world. These were run **on the imported world** specifically, against a world
regenerated by this tree's `apfm-import` from the player's own `world.db` (SHA-256 `36b6908c…`, matching D7B2's report). The
installed world file in the app data directory was **not** modified.

| Check | Result |
| --- | --- |
| Imported-world integration suites, 7 files, 21 tests (`fm24_career` 2, `fm24_competitions` 2, `fm24_free_agents` 4, `fm24_global_market` 3, `fm24_loans` 4, `fm24_ratings` 3, `fm24_rosters` 3) | **PASSED** on the fresh world. Covers loans, rosters and squad registration, free agents, competitions and cups across save and reload, ratings, the world-wide market. |
| Same suites against the user's *installed* world file | **FAILED** 4 of 21 (`fm24_competitions` 2, `fm24_ratings` 2): the file predates the importer revision. K7: re-import. |
| Registry/roster overlap (`i1_world_overlap`) | 240 clubs, 13,358 players, 13,045 with `fm-p-*` ids, all 13,045 are registry ids; 9,843 `active_world` all on the roster; **3,079 `eligible` registry players are already on the roster and are hidden by the exclusion** (123 `missing_nation`); market offered 64,564 of 67,643. |
| Combined career, 5 seasons, recruiting (`i1_combined_career`, `I1_WORLD=fm24`) | **PASSED**: 32 free agents and **5 contracted players (62.6 M in fees)** signed; refusals 298 `boardWagePolicy`, 216 `squadFull`, 58 `transferWindowClosed`, 10 `transferBudgetTooLow`; **0 refusals changed anything**, **0 ledger mismatches** (cash and budget fell by exactly each fee), **0 duplicate identities**, harness invariants **clean**, 1,030-1,190 MB peak, 171 s. |
| Save/reload after recruitment, then 60 days on | Differing lines by kind (squad-floor journal excluded): at reload **none**; 60 days on **`news` only (5 lines)**; nothing in players, contracts, tables or money; same market on both copies. Identical behaviour without any recruitment, and on P5's tip (K6). |
| Imported-world economy | 4-10 of 240 clubs in debt at season ends, **with or without recruitment** (K5): pre-existing, not an integration effect. |

## 6. Combined scenario in the generated world

Gate world, seed 1, 10 seasons, real registry, recruiting under the world's own rules (`I1_WORLD=gate`): 72 free agents and 5
contracted players (19.6 M in fees); refusals 755 `insufficientFunds`, 621 `boardWagePolicy`, 142 `transferBudgetTooLow`, 108
`transferWindowClosed`; **0 mutated, 0 ledger mismatches, 0 duplicates, invariants clean**; reload 60 days on: no differing line of
any kind. **Finding K3:** the recruiting club's cash reached −1.6 M in season 7 (1 club-season) and its wage bill ~2× budget; the
control without recruitment never goes negative. Debt came from accumulated weekly wages, not from a fee.

This run is a **generated** world with a real-source registry beside it. It says nothing about the imported world, which has its
own run above.

## 7. Determinism and the cache

| Claim | Evidence |
| --- | --- |
| Same career, same clock → same market | `an_older_save_rebuilds_the_cache_and_the_market_is_reproducible` |
| Backward clock → rebuild, same market as then | same test (`rebuilt == true`, ids equal) |
| Cache deletion changes nothing | same test |
| Concurrent queries agree | same test (4 threads); D7B2's `concurrent_callers_see_one_complete_cache` passes |
| Two seeds diverge; a save fork shares one cache; the shared source is never written | `careers_are_independent_and_the_source_is_never_written` |

## 8. Real-source external market (D7B2 benchmark, run on the integrated tree)

`career::tests::benchmark_real_career_cache`, the user's own `world.db` (`36b6908c…`, read-only), seed 777, cold cache per row,
**PASSED**, 68 s, peak RSS 1.45 GB. D7B2's published figures in brackets.

| Year | Season | Searchable | OVR 80+ / 85+ (stored rows) | Newgens | Free | Cache | Cold rebuild | Query |
| ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 2027 | 61,594 (61,069) | 0.42 % / 0.02 % (0.00 / 0.00) | 900 | 43.5 % (45.0) | 59.2 MB | 2.7 s (2.3) | 50 ms |
| 5 | 2031 | 50,189 (49,088) | 0.64 % / 0.04 % (0.01 / 0.00) | 4,500 | 28.9 % (32.3) | 51.1 MB | 7.9 s (7.1) | 42 ms |
| 10 | 2036 | 33,489 (32,021) | 0.90 % / 0.05 % (0.13 / 0.00) | 9,000 | 32.2 % (34.8) | 37.2 MB | 13.1 s (13.0) | 30 ms |
| 20 | 2046 | 14,197 (14,169) | 0.39 % / 0.00 % (0.40 / 0.00) | 14,121 | 26.9 % (26.8) | 21.6 MB | 18.4 s (17.6) | 13 ms |
| 30 | 2056 | 14,188 (14,188) | 0.36 % / 0.01 % (0.36 / 0.01) | 14,188 | 27.5 % (27.5) | 21.6 MB | 22.7 s (19.9) | 13 ms |

The registry's counts are unchanged (81,762 stored, 69,301 eligible, 9,843 active-world), but its **bytes differ** (72,179,712
against 72,048,640) and so do the early years. The early-year market is larger and has an **elite tier D7B2 did not see** (K8).

**Confirmed to be an integration effect, not noise (RUN):** D7B2's own tip, built in a scratch worktree and run on this machine
against the same source, reproduces its published table exactly (73,530 rows, 61,069 searchable, OVR 80+ 0.00 %, registry
72,048,640 bytes, 1.77 GB peak, 67.6 s). The integrated tree differs because the registry build and the projection both run on
`ofm_core` code that P5's D6 rating work changed (`player_rating`, `development`); the exact function was not isolated. The tail
(years 20-30) agrees to within 0.1 %.

Elite availability for a **career** (searchable, best 100 by ability, correct sort): imported world, season 1 → 5: best OVR 93 → 87,
19 → 14 players at 85+; gate world, season 1 → 10: best OVR 89 → 87, 30 → 6 at 85+. The external market's top end decays with the
imported cohort, as D7B2 predicted.

## 9. Not run, blocked, or caveats

- **GUI: BLOCKED / NOT VERIFIED.** This session has no desktop control, so nothing was launched or clicked. A green build is not a
  playable GUI. See section 10.
- **External recruitment has no screen.** No `.tsx` file calls the registry service and no MCP tool references it, so there is
  nothing to click even with desktop control. It is reachable only through the Tauri commands (registered in `lib.rs`) and the
  backend tests above.
- **Shipped-world (440 clubs) 30-season run on the integrated tree: NOT RUN** (see section 4).
- M12's own tests: NOT RUN (the M12 repository is private and was not modified by either branch).
- `cargo deny`, `cargo machete`, `npm run knip`: NOT RUN (P5 did not run them either).
- Only one imported-world seed (1), one club (`fm-c-501`), 5 seasons; the generated combined run is 1 seed × 10 seasons. No
  imported-world 30-season run: at about 40 s a season in the test profile it was not attempted.
- The reviewer agents (`ofm-architecture-reviewer`, `ofm-dedup-reviewer`, `ofm-test-reviewer`) were **not run** on this diff.
- No imported-world run was compared to a P5-tip run for the *simulation* (only for reload behaviour and the generated world); the
  generated-world identity plus "D7B2 touches no simulation file" is the argument, and it is not a measurement for the imported world.

## 10. Manual GUI checklist (for the user)

External recruitment cannot be exercised from the GUI. What can be, and should be, checked by a person:

1. **Re-import the FM24 world** first (K7): run `apfm-import ~/apfootballmanager/data/world.db <out>` and install it as the app
   does, or the old file will lack the cups and the rating revision.
2. Launch (`npm run tauri dev`). New career on the **Random World** (440 clubs): it starts; Continue works.
3. New career on the **imported FM24 world** (240 clubs): it starts and shows the managed club's squad. Confirm the league and the
   six national cups exist (cups missing means the old world file is still installed).
4. Play to the first match and play it in the viewer; play one more with Quick simulation.
5. In the transfer window, make a normal bid for a player and agree the clubs' terms: it works as before (P5's contract hearing
   appears). Withdraw/finish it.
6. **Save, quit, reload the save**: the squad, the saved eleven and the cash are as left. Play a few days.
7. Release or sell a starter, then open the squad screen: the eleven is still eleven, the next slot filled (P5 section 11 of
   `APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`).
8. Make a second bid that the budget cannot cover after an agreed deal waits to register: it is refused for money.
9. **Confirm there is no external-market screen** (expected: nothing in the UI mentions the global registry), and the app shows no
   error at start (the registry commands are registered and idle without a built registry: `get_external_registry_status`
   reports not installed).

Record the result of each step in `APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`; none is claimed here.

## 11. Reproduce

Section 9 of the integration document has the commands. The long runs, from `src-tauri/` in release:

```sh
P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture
P45_WORLD=gate P45_SEEDS=1     P45_SEASONS=30 cargo test --release -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture
APFM_D7B2_WORLD_DB=~/apfootballmanager/data/world.db cargo test -p fm24-adapter --lib career::tests::benchmark_real_career_cache -- --ignored --nocapture
APFM_WORLD_JSON=<fresh apfm-import output> cargo test --release -p season-harness --test fm24_loans -- --include-ignored   # and the other six fm24_* files
I1_WORLD=fm24 I1_SEED=1 I1_SEASONS=5 I1_WORLD_DB=~/apfootballmanager/data/world.db APFM_WORLD_JSON=<fresh> \
  cargo test --lib commands::registry::combined_career::i1_combined_career -- --ignored --nocapture   # I1_SIGNINGS=0 is the control
```

Nothing FM-derived is committed: the generated world, the registry and the caches lived in a scratch directory outside the
repository and were deleted afterwards.
