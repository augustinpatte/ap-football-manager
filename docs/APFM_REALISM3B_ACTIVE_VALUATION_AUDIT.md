# APFM REALISM3B — active-player valuation continuity

Corrective mission after the independent audit of REALISM3. Everything below was measured on the real
FM 2026/27 import (built from the owner's `world.db`, **read-only**, into scratch directories) and on the
shipped 440-club generated world. Numbers come from the harness examples listed in section K; none is
estimated by hand. The repository is public: no personal save, source database, installed world or
private export is part of any commit, and player names are not written.

---

## A. Baseline and final commits

| | |
|---|---|
| Baseline (REALISM3, pushed) | `aba4386bd7f18f0c79b1dfd596b12c54f82f170e` |
| Implementation | `20096aeb` fix(economy): revalue active players after a season's development (every gate in K ran on this code) |
| Study tooling | `bd43d4d1` test(realism3b): five-season valuation, offer-repetition and cost studies |
| This document | the commit that follows them; the final SHA is in the report that accompanies the push |

---

## B. Exact root cause

REALISM3 left one defect open (its F.1): **no production code revalues a player after he is created or
projected.** Traced over every writer:

| Writer | What it changes | Revalues? |
|---|---|---|
| `generator/generation.rs`, `youth.rs` | creates a player with a native-formula value | sets it once |
| `external_world.rs` (`project_player`, `newgen_cohort`), registry | the registry's ~81 k players, carried on the world scale | yes, but only the registry |
| `aging::apply_seasonal_aging` → `development::develop_season`, `refresh_player_derived` | rating, ceiling, traits, once a season; age follows the date | **no** |
| `training.rs` (daily) | attributes, hence rating | **no** |
| `external_registry::to_player` | copies the registry value at activation | sets it once |
| `fm24-adapter` (`pipeline`, `reprice_generated_players`) | prices the world at build | build only |

So an active player's `market_value` is the number he was created with, for the whole career. Reproduced
with the harness on the baseline: **0.0 % of club players change value in any of five seasons, on every
seed and every club** (table E.1), and the REALISM3 example is still there — a player whose rating fell
86 → 71 over five seasons is still worth €75.2 M, age 29 → 34 (and, in the audit's wording, 87 → 77 at
€121 M). Red tests reproduced it before any code changed
(`aging::a_veteran_whose_rating_falls_is_worth_less_after_the_season` and
`…a_prospect_whose_rating_rises_is_worth_more_after_the_season`: cohorts of 200 and 80 players that
demonstrably decline/grow, values unchanged).

A second, related defect was found while tracing the populations: the **in-career academy intake**
(`youth_intake`) prices recruits with the generator's native formula. On an imported world that is ten to
a hundred times the going price: the median value of a ≤ 19-year-old club player jumped €76 k → €1.45 M at
the first rollover in the baseline (table E.4).

---

## C. Design: a persistent valuation basis

REALISM3's `ValueScale::carry` already moves a value by the world scale's ratio between two states. What
was missing was the *from* state and the scale itself, both in the save.

**`domain::player::ValueBasis`** (on `Player.value_basis: Option<_>`, `serde(default)`, skipped when
`None`): the market value, rating, ceiling and age at the last valuation, the calendar year of the season
end that last revalued him, and the model (the scale's version, 0 for the native formula).
24 bytes in memory, ~81 bytes of JSON per player (1.08 MB for 13,358 players).

**`Game.value_scale`** (`serde(default)`; native when absent): the scale the career's values sit on. It is
read from the world's metadata (`WorldDataMetadata.value_scale`, written by the adapter pipeline, skipped
when native so generated worlds and old `.ofm` files are byte-identical) when the career is built, and
stored in the save. A career never reads a scale from an installed file, and reinstalling the world does
not change how an existing career is read.

**`ofm_core::revaluation`** (the one rule, no new formula):

* `anchor_value` / `anchor_missing_bases` — a basis equal to the stored value and the state he is in now;
  nothing is repriced;
* `revalue_after_season(player, scale, on, year)` — value = `scale.carry(value, basis, now)`, then the
  basis is replaced. No basis → anchored where he stands. Already revalued this year → nothing. A basis of
  another model, or one his value no longer matches (something priced him since) → re-anchored without a
  ratio. No value, or retired → left alone;
* `enter_the_market` — a recruit from a generator that prices natively takes the price of players like him
  on a calibrated world scale (`ValueScale::value`), then is anchored.

**Lifecycle point.** One bounded pass inside the existing per-player loop of `apply_seasonal_aging`: a
player with no basis is anchored *before* development, then development and `refresh_player_derived` run,
then the revaluation. The marker is the **calendar year of the season end**, not the user's league label
(an architecture review showed labels can repeat after a change of league; a test pins it).

**Where it is anchored.**

| Population | Anchored |
|---|---|
| First-team players of a new career | `begin_career`, last step (after the identity upgrade settles ratings) |
| Legacy save | on load, after the rating repair and backfill (so the anchor reads the real stored state) |
| Registry player signed | at the door, at the value the registry carried to this season |
| Academy intake recruit | the day he joins, after repricing on the world scale (before the wage policy judges him) |
| Free agents, loaned players, anything created elsewhere | `anchor_missing_bases` safety net at the next season end, before development |

---

## D. Persistence and migration

* **v072** (`v072_value_basis.sql`): `players.value_basis TEXT` (JSON, NULL = none) and
  `game_meta.value_scale TEXT` (JSON, NULL/empty = native). Registered, `EXPECTED_MIGRATION_COUNT` 72, its
  fingerprint pinned in `SHIPPED`; the old-build imitation helper in `run3_integration_tests.rs` drops both
  columns so the released-career fixtures exercise a file without them.
* All five positional sites of `player_repo.rs` and every site of `meta_repo.rs` are updated.
* **Legacy policy (explicit).** A save with no basis loads with every player anchored to the value, rating,
  ceiling and age already stored: no value changes at load. A save with no scale loads on the **native**
  formula and never adopts a newer world's scale (nothing silently imports DATA2/REALISM3 values). A
  damaged basis or scale reads as absent and never stops a career opening; a damaged scale is logged.
  Consequence, stated plainly: a career begun before this change from an imported world carries its values
  by the native formula, which is steeper at age steps (≈ −40 % at 30, −58 % at 33) than the FM scale.
  Careers begun after it carry the FM scale.
* The installed world was not rebuilt or touched.
* The wire copy (`ui_clone`) omits the basis (about 1 MB per payload that no screen reads); a test pins
  that the game keeps it.
* Tests: non-default round trip written twice (update path included), NULL and damaged JSON, legacy load
  anchors without repricing and writes the anchor back, save → close → reload → rollover → save →
  reload, a repeated season's end after the rating moved further changes nothing.

---

## E. Results

### E.1 Share of players whose value changes (5 seasons, per season)

| run | baseline | after |
|---|---|---|
| PSG seed 1 / 2 / 3 | 0.0 % in every season | 99.6–100 % |
| Nantes (ordinary first division) | 0.0 % | 99.7–99.9 % |
| Palermo (constrained) | 0.0 % | 99.6–99.9 % |
| Generated 440-club world | 0.0 % | 95.5 → 72.0 % (the native formula has an age plateau 25–29 and flat players do not move) |

Correlation between a player's rating change and the log of his value ratio, per season: 0.72–0.77 on all
runs (baseline 0). Wages of the same player, season to season: median ratio 1.00 (contracts are not
rewritten).

### E.2 Proportionality (PSG seed 1, one season, FM scale)

| Rating change | n | value ratio p10 / **p50** / p90 |
|---|---|---|
| drop ≥ 5 | 4 | 0.03 / **0.19** / 0.26 |
| drop 1–4 | 730 | 0.35 / **0.52** / 0.74 |
| flat | 1,877 | 0.67 / **0.92** / 1.12 |
| gain 1–4 | 4,617 | 1.01 / **1.38** / 1.89 |
| gain ≥ 5 | 1,811 | 1.37 / **1.87** / 2.63 |

| Age (start of season) | n | p10 / **p50** / p90 |
|---|---|---|
| ≤ 20 | 2,865 | 1.11 / **1.68** / 2.40 |
| 21–23 | 2,083 | 0.88 / **1.32** / 1.82 |
| 24–26 | 1,692 | 0.88 / **1.27** / 1.76 |
| 27–29 | 1,368 | 0.77 / **1.05** / 1.43 |
| 30–32 | 830 | 0.43 / **0.66** / 0.86 |
| 33+ | 201 | 0.25 / **0.43** / 0.67 |

The swings are the world's own: the FM scale is steep (about ×1.3 per rating point near the top) and its
age factors are fitted from the source's own values — the source's 76-rated players are worth €3.5 M at
24–28, €0.75 M at 33–35 and €0.22 M at 36+, so a 34-year-old 76 at €0.41 M is where DATA2 puts him. On the
generated world (native formula) the same table is gentler: flat players p50 1.00, drop 1–4 p50 0.94,
gain ≥ 5 p50 1.32, 33+ p50 0.94.

### E.3 The original example

Baseline: rating 86 → 71 (age 29 → 34), value €75.2 M → €75.2 M (ratio 1.00). After, same player, same
seed: €75.2 M → **€0.136 M** over the five seasons; no player and no percentage is hardcoded — it is
`ScaleValue(now)/ScaleValue(then)` applied to his own value, once a season. One-season examples of the
largest declines among players rated 80+ (rating, age, value): 82→77 (34→35) €4.68 M→€0.94 M; 81→77
(33→34) €6.27 M→€0.87 M; 82→78 (30→31, ceiling 91) €21.5 M→€5.18 M. Growing: players rated 90/92 at age
20 reach the top of the table at €363–407 M (their stored values were above the scale, and carry keeps
that premium, see F.2).

### E.4 Distributions and stationarity (PSG seed 1)

Value of club players, p10 / p50 / p90 / p99 / max:

| season | baseline | after |
|---|---|---|
| s0 | 30 k / 364 k / 10.7 M / 91.6 M / 268 M | same |
| s1 | 31 k / 413 k / 9.8 M / 91.0 M / 268 M | 41 k / 404 k / 12.9 M / 113 M / 407 M |
| s3 | 65 k / 1.21 M / 12.5 M / 98.6 M / 268 M | 73 k / 871 k / 21.5 M / 150 M / 446 M |
| s5 | 172 k / 1.59 M / 12.5 M / 103 M / 268 M | 76 k / 1.10 M / 26.4 M / 166 M / 481 M |

The movement of the percentiles is **not** inflation of the model; it is the simulation's own development
being priced. The check that separates the two is the value at a fixed rating (median, players aged
24–29):

| rating band | baseline s0 → s5 | after s0 → s5 |
|---|---|---|
| 60s | 191 k → 99 k | 191 k → 223 k |
| 70s | 2.30 M → 0.65 M | 2.30 M → 2.58 M |
| 80s | 28.4 M → 8.5 M | 28.4 M → 29.3 M |
| 90s | 139 M → 104 M | 139 M → 197 M |

After: stable. Baseline: collapses, because values are frozen while ratings climb. Mean rating of club
players rises 65.0 → 71.0 in the baseline and 65.0 → 70.7 after (identical): that is the pre-existing
development economy (F.1). Median value of ≤ 19-year-old club players: baseline €76 k → €1.45 M at s2 (native
newgens); after €94 k → €128 k.

Top valued players: s0 €268 M / 265 M / 255 M; after, s5 €481 M / 352 M / 337 M (baseline s5: €268 M /
265 M / 255 M, held by players whose ratings had fallen 94 → 81).

### E.5 Offers, fees, wages, budgets

* Fee over value of completed AI-to-AI transfers: p50 0.83 (baseline 0.84) on PSG seed 1; 0.81–0.83 on the
  others (baseline 0.84–0.86): fees still track value.
* Paid transfers over five seasons fell (PSG 598 → 407; generated 954 → 833; total fees 1,075 M → 710 M).
  The split by player age shows where: players ≤ 23 account for 276 → 179 of them, and deals of €1–10 M
  238 → 166. The baseline's extra deals are overwhelmingly young players priced at the native formula (B).
  Deals of players aged 27+ also fell (the aged are cheaper). This is a behaviour change worth watching
  (F.3), not a loss of the market: world cash, wages and debt are within noise.
* **Wages:** same-player weekly wage ratio median 1.00; world weekly wage bill at s5 +1.4 % (153.6 M vs
  151.5 M); renewals now price a player on his current value (the intended consequence).
* **Cash and debt (world):** median club cash at s5 €40.25 M vs €40.02 M; clubs in debt at s5 0 vs 0 on PSG
  runs (Nantes 0 vs 1, Palermo 1 vs 0); total debt €0. No finance, budget or sponsor code reads
  `market_value`; the transfer envelope is 15 % of cash. A rising value creates no cash.
* **User club cash** (the manager decides nothing, so it follows the world's chance paths): PSG seeds 1/2/3
  €1,044 M / 1,062 M / 1,065 M vs €1,052 M / 1,062 M / 1,058 M. Nantes €20.2 M vs €27.9 M, Palermo −€0.55 M
  vs +€7.4 M: single-club paths diverge once the world differs (the baseline Palermo already swings
  €1.6–7.0 M between seasons) and are not systematic across the PSG seeds; see F.4.
* **Offers (Step 7, below)** are identical before and after for the first 120 days (a revaluation happens
  at the season's end).

### E.6 Market volume (Step 7): are the incoming offers repeated approaches?

`realism3b_offer_audit`: PSG and Nantes, seeds 1–3, 120 days from 1 June, with a manager who either ignores
every offer or rejects each on the day it arrives (24 runs, baseline and after identical).

* Ignoring: 34–45 offers for 3–7 players (PSG 41–45 for 5–7 players from 23–24 clubs); up to 12 offers for
  one player from at most 10 distinct clubs (median 6). Rejecting each offer: 66–69 PSG offers, up to 20
  for one player from 11 clubs; Nantes 80–144.
* Repeated approaches (same club, same player): 11–12 per PSG run when ignored (26 %), 30–33 when rejecting.
* Cooldown: 96 repeats arrived inside 30 days of the earlier offer's closing, and **all 96 are inside the
  documented closing-window shortening** (`WindowPhase::rebid_cooldown_days`: 30 → 15 in the last week →
  7 in the last 72 h → 3 on the closing day). **0 break even the phase rule.** (An earlier cut of the tool
  binned deadline-day offers as "no window" and reported 9 "breaks"; the binning was fixed and the counts
  recomputed.)
* Over the window: new offers per 10 days (PSG seed 1, ignoring) 13, 6, 6, 4, 3, 11 from day 51 on; 3–6
  offers across the whole run were first seen on a closed-window day (the closing day itself). No duplicate
  pending offer, no invalid offer.

The behaviour is defensible and respects every restriction, so **nothing was changed** (AI-to-AI transfers
included).

---

## F. Remaining anomalies

1. **P2 — rating inflation in the development economy (pre-existing, now visible).** Mean club-player rating
   rises 65 → 71 in five seasons and players rated ≥ 90 go 19 → 41, on baseline and after alike. With values
   now following ratings, the top of the market drifts up (max €268 M → €481 M). Not touched: the mission
   forbids altering rating or potential to produce financial results.
2. **P2 — premium carry.** A player stored above the scale keeps his proportional premium, so absolute gains
   of the very top are larger (the €407 M 20-year-old). That is `ValueScale::carry`'s design (REALISM3).
3. **P2 — fewer paid transfers (−32 % PSG, −13 % generated).** Explained in E.5 as the end of native-priced
   kid deals plus cheaper veterans; not investigated further for AI behaviour.
4. **P2 — one-club cash paths** (Nantes/Palermo) differ in sign from the baseline on single runs; PSG and
   the world aggregates do not. Not shown to be systematic; more seeds on constrained clubs would settle it.
5. **P2 — native-world age steps** (value × 0.6 at 30, × 0.25 at 33 in `native_value`): flat players of 29–32
   lose up to 40 % in a year on a generated world and legacy imported careers. Pre-existing formula.
6. **P2 — stepping-stone promises** read the live `market_value` for their fee floor (`transfers/promises.rs`);
   an open promise's floor moves with a revaluation. Named in `GAME_SYSTEMS.md`; no cash moves.
7. **P2 — `load_game` is ~258 lines** (limit 100; 245 before, this change adds a 13-line anchor block). The
   block could move into the load-time repair group.
8. **P2 — a damaged stored scale** is logged but the next save writes native over it.
9. **P2 — `Standing` from a `Player` is written inline in five places** (`revaluation.rs`, `pipeline.rs`,
   `external_world.rs`, two tests) and `model_of` could live on `ValueScale`; the study examples repeat
   `flag`/percentile/`pearson` helpers (`world_health` already has them). Named for epic #589.
10. **P2 — `new_game_from_save`** (test-only today) resets the clock but keeps bases; re-anchor if it ever gets
    a production caller.
11. **Carried over from REALISM3, untouched:** F.2–F.8, F.10–F.13 of that report.

No P0 or P1 defect is open on this mission's scope.

---

## G. Runtime and memory

* One revaluation pass over every player: **1.44 ms** (13,358 players, FM world), **0.54 ms** (13,865,
  generated world); once a season, inside the existing loop.
* Anchoring when nothing is missing: 8 µs. Memory: 24 bytes per basis (≈ 0.3 MB); save: ≈ 1.1 MB of JSON
  per 13 k players.
* Five-season run, PSG seed 1, two jobs in parallel: 142.1 s baseline vs 140.5 s after. Generated 440-club
  world: 328 s vs 330 s. All twelve study runs finished within ±3 % of each other.

---

## H. Source-data integrity

The owner's `~/apfootballmanager/data/world.db` has SHA-256
`36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` **before and after** every import, study
and test of this mission (checked at the start, after each import, and at the end). It was only read; the
worlds built from it live in the session's scratch directory and are not committed. The installed world and
every real save were not touched.

---

## I. Examples of the lifecycle (unit and integration tests)

* 87-rated 32-year-old at €90 M, anchored, falls to 77 at 33 → €9.1 M under the sample imported scale (the
  test recomputes the expectation from the scale and the basis).
* 19-year-old 60/80 at €1 M gains 8 points → worth more by the scale's own ratio.
* A basis taken under the calibrated model, revalued under native (and the reverse): value kept,
  re-anchored on the model now in force.
* A recruit with a calibrated scale enters at `scale.value(standing)`, at least 3× below the native price.

---

## J. Contracts, transfers and loans

A revaluation takes one player; it writes `market_value` and `value_basis` and nothing else. Tests with the
real code paths: pending offers keep fee, wage and status and the contract stays; a transfer completes at
the agreed fee, the player arrives with the value and basis he had and the next season's end carries on
from them; a loan across a season's end revalues him once, and the return neither resets nor revalues him;
a registry recruit keeps the registry's value and is anchored on it. Promotion from the academy and
registry activation have no value code at all (nothing calls a pricing function).

---

## K. Tests, mutation checks and reviewers

All gates were run sequentially on the exact final code, in the existing `src-tauri/target` (one workspace
build at a time; the baseline was built once in a scratch worktree sharing it, then removed).

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (default) | pass |
| the same with `--features mcp` | pass |
| `cargo test --locked --workspace --no-fail-fast` (default) | **5,472 passed, 0 failed**, 148 ignored (REALISM3: 5,445) |
| the same with `--features mcp` | **5,559 passed, 0 failed**, 148 ignored (REALISM3: 5,532) |
| fm24-adapter real-source tests (`APFM_WORLD_DB` set, `--include-ignored`) | 144 lib + 10 `data2_named_players` + 3 `realism3_value_scale` + 14 `synthetic_world`: all pass |
| `npm run format:check`, `npm run lint` (Biome 2.5.5), `npm run knip` | pass |
| `npm run build` (tsc + vite) | pass |
| `npm test` (Vitest, includes `src/i18n` coverage) | 374 files passed, 1 skipped; 3,284 tests passed, 4 skipped |
| `npm run audit:i18n` | advisory, exits 0; no player-facing string was added |

The Rust test count rose by 27 (default 5,445 → 5,472; MCP 5,532 → 5,559): the `revaluation` unit tests, the
`transfers::revaluation_tests` protections, the aging, career, world-build, registry, youth-intake and wire-copy
tests, and the persistence and legacy-load tests in `db`. Two of them were first seen red against the
baseline for the predicted reason (value unchanged after a season, with non-vacuous cohorts). A test run
with a zero-test selection, a compile error or a timeout was never counted as evidence. The real-source
tests return early (pass vacuously) without `APFM_WORLD_DB`; the line above is with it set.

Study binaries: the baseline and the final build were copied out under separate names right after each
build (a shared target directory rebuilds a binary in place), and the baseline world was built by the
baseline adapter, so before and after differ only in the code.

**Mutation checks** (disposable edit of the one line, the named tests must fail, then restore from a
backup and `touch`): 18 runs, **all killed**: revaluation call removed; pre-development anchor removed;
carry from the current state instead of the basis; season-marker check removed (strengthened after review
so the test moves the rating between the two calls); model-mismatch check removed; stale-value check
removed; `begin_career` anchor removed; load anchor removed; revaluation rewrites the wage; revaluation
rescales pending offers; transfer resets the basis; loan return resets the basis; registry door not
anchored; recruit not priced on the scale; recruit not anchored; world scale not copied into the game;
basis column not written; scale column not written. The marker-check mutation was re-run against the
strengthened save-manager test and is killed there too.

**Reviewers** (read-only agents on the uncommitted diff; they were told not to compile):

* `ofm-architecture-reviewer`: no P0, no P1. Six P2: season marker keyed on a league label (**fixed**: the
  calendar year of the season end, with a test); damaged scale silently overwritten (**warning added**);
  promises read the live value (**documented**); `load_game` size (named, F.7); youth recruits judged on
  the native price (**fixed**: repriced before the wage policy); visibility (**narrowed**, `anchor_value`
  and `enter_the_market` are `pub(crate)`).
* `ofm-test-reviewer`: no P0. P1: the basis age never asserted independently (**fixed**: age and model
  asserted from the fixture's birth date and in the career test); a repeated-season check that could not
  fail (**fixed**: ratings move first, scale read from the game). P2 fixed: the unreachable club-money
  assertions (removed, the comment says why), the transfer test's missing next-season carry (added), the
  loan bound (now an exact carried value), the update path of the repo test (second save changes the basis),
  `anchor_missing_bases` (keeps an existing basis, counts, idempotent), both model directions. Not fixed:
  one conditional assertion for a veteran who may retire, native-world youth price equality, test bundles.
* `ofm-dedup-reviewer`: no P0/P1; no competing formula; P2 helpers named in F.9; a stray `.bak` file
  (deleted).
* `i18n-auditor`, `ui-accessibility-reviewer`: not run — no player-facing string, component or style
  changed (the frontend gates pass unchanged). An economic-realism reviewer is not available; E and F are
  that analysis.

---

## L. Verdict

**GO for EUROPE4**, with the P2 items in F carried forward.

* The confirmed defect is fixed and shown on the real world, the shipped generated world, five clubs and
  three seeds: values follow what players become (E.1–E.4), proportionately (E.2), without moving a
  contract, an offer, a fee already paid or a club's cash (J, E.5).
* Old careers load without any repricing and keep working (D), the world scale travels with the career, and
  a replayed or retried season's end cannot pay twice.
* Cost is negligible (G); the AI transfer market was examined (E.6) and deliberately left alone.
* **Not verified:** no GUI claim (the Tauri window was not operated); careers begun *before* this change
  from an imported world carry values by the native formula (D); more constrained-club seeds would settle
  F.4; economic realism of the *absolute* top-end value (F.1) is a development-economy question that this
  mission was told not to touch.
* Source-data SHA-256 unchanged: `36b6908c…27439f7`.
