# APFM P4.6: D6 integration and job-offer persistence

Branch `claude/apfm-playable-career`, from `eb1d9d79` (P4.5). Not merged into `apfm/main`, not pushed. M12 and the
external 2D viewer were not touched. No P4.6 strategic AI work was started.

Evidence labels: **RUN** (executed this phase), **READ** (read in source), **REVIEW** (a reviewer agent's finding,
checked here).

## 1. Summary

| | Result |
| --- | --- |
| Part A, D6 integration | D6 merged with `--no-ff`; all 3 D6 commits and all 7 P4.5 commits preserved. **No textual conflict, no semantic conflict** (the merged tree compiled first time). The opening-rating specification is live and **mutation-checked**. |
| Rating correction | Opening ratings now agree with the attributes for **every** player (was 41.4 % beyond rounding in the gate game). Ceilings and ids are untouched; the correction raises the top of the opening world (players rated 80+ : 18→24, 27→45, 33→47 on gate seeds 1-3). |
| Stale saves | New, **non-destructive, idempotent** load-time repair for saves written by a pre-D6 build (granular position, old rating). Derived fields only; written back; no save-format bump. |
| Part B, job-offer clock | Persisted in `game_meta` (migration **v069**). The ten-season reload twin now passes on three seeds **without copying the timer by hand**; red when the loader drops it. Offer frequency and eligibility are unchanged. |
| Regression found and fixed | `new_game_from_save` would have inherited the old career's job-offer clock; fixed, test red first. |
| Long career / balance | Ten-season gate comparison against the pre-merge tree: market metrics within three-seed noise; no club in debt, none over 110 % of wage budget, invariants clean (3 seeds). |
| Shipped world | 3 seasons and a 2-season reload twin on the 440-club world: invariants clean, no club in debt, reload identical (section 7.3). |
| Performance | **+1.4 % instructions** against the pre-merge tree, same study; the F5.5 gain is preserved. |
| Reviewers | Architecture: no blocking. Dedup: no blocking. Test: 1 blocking (a write-back assertion that could not fail), fixed and mutation-checked. |
| Verdict | **GO** for the strategic AI completion phase, with the conditions in section 12. |

## 2. Commits

Starting points verified before any change: P4.5 `eb1d9d79` is `aec1af70` plus seven commits; D6 `456eba86` is `e5282b67`
plus three; `e5282b67` is the merge base and an ancestor of both; both working trees were clean.

| Commit | Change |
| --- | --- |
| `da495676` (D6) | `fix(world)`: re-derive a player's rating when his position is refined |
| `71f0ff70` (D6) | `feat(audit)`: check contracts, club ids and competition references |
| `456eba86` (D6) | `docs(world-data)`: the D6 audit and fixes |
| `cff2253f` | `merge(d6)`: the `--no-ff` merge of the three above onto P4.5 |
| `efdaad5d` | `feat(ratings)`: activate the opening-rating spec; repair stale ratings on load |
| `b5c300e0` | `fix(save)`: persist the job-offer clock (v069) |
| `5d3e66cb` | `refactor(ratings)`: one drift rule; tests that pin the repair's write-back and old saves; reviewer fixes |
| (this report) | `docs(p46)`: this document |

## 3. Integration (Part A)

### 3.1 Conflicts

The only file changed on both sides since `e5282b67` was `db/src/save_manager.rs`, and the hunks did not overlap (D6 edited
a test fixture, P4.5 edited other code). `git merge` reported no conflict. The semantic check was `cargo check --workspace
--all-targets`: it passed unchanged, so P4/P4.5 had added **no** caller of the changed
`upgrade_player_identity(player, slot, on)` signature. One test (below) asserted the bug it was meant to track and failed, as
its own message predicted.

### 3.2 The eight checks the brief names

| # | Check | Result |
| --- | --- | --- |
| 1 | Opening OVR matches the attribute-derived rating | **0 players** beyond rounding after `begin_career` (**RUN**: `opening_ratings`, 2 nations; D6: shipped world 3,786 audit errors → 0). Without D6's refresh call, two tests go red (mutation, **RUN**) |
| 2 | Potential remains valid | `potential >= ovr` for every player; a ceiling moves only where the corrected rating overtakes it (test asserts `ceiling.max(ovr)` exactly, and that the fixture lifts at least one) |
| 3 | Ids and identities unchanged | Test compares id, name, birth date and nationality across `begin_career` |
| 4 | Save/load compatibility | Sections 5 and 7.2 |
| 5 | Existing saves not silently corrupted | Section 5.1: derived fields only, idempotent, written back, narrow trigger |
| 6 | Market values and wages remain coherent | Orderings inside a position are intact; a **cross-position price gap** remains by design (section 4.3) |
| 7 | AI recruitment respects budgets | 0 clubs in debt and 0 over 110 % of wage budget in every season of 3 gate seeds; `delegated_recruitment` passes (**RUN**). One latent gap D6 found stays open (section 11) |
| 8 | Squad selection and match eligibility | Invariants observer clean for 10 seasons on 3 gate seeds (includes `banned-player-never-plays`); min squad 18, no club without a keeper |

### 3.3 The opening-rating specification

`ofm_core/tests/opening_ratings.rs`: the ignored P4.5 specification `a_new_career_opens_with_every_rating_agreeing_with_the_attributes`
is **live**. The characterisation test that asserted the mismatch exists was retired, as its own message instructed ("the
mismatch this test characterises is gone: retire the spec below"). Added: identities and ceilings preserved across
`begin_career`, and idempotence of the identity upgrade. Mutation: removing the `refresh_player_derived` call from
`upgrade_player_identity` turns the specification and the ceiling test red; restored.

## 4. What the rating correction changes

### 4.1 Opening world (gate, three seeds, `p45_long_career` season 0)

| Seed | Rated 80+ | Rated 75+ | Top-50 mean |
| --- | --- | --- | --- |
| 1 | 18 → 24 | 136 → 198 | 79.2 → 80.0 |
| 2 | 27 → 45 | 151 → 225 | 80.3 → 81.5 |
| 3 | 33 → 47 | 185 → 247 | 80.5 → 81.4 |

The shift is **expected**: wingers, wing backs, wide and defensive/attacking midfielders had been under-rated by 5 to 10
points since the opening (P4.5 section 4.4). It is the correction, not a regression.

### 4.2 By position (generated world, 2 nations, **RUN** probe `economy_priced_before_the_rating_correction`)

| Position | n | Rating before → after | Value paid / value the corrected rating prices |
| --- | --- | --- | --- |
| Right winger | 48 | 60.1 → 68.5 | 0.76 |
| Left winger | 62 | 57.5 → 66.6 | 0.72 |
| Defensive midfielder | 110 | 60.1 → 66.1 | 0.81 |
| Attacking midfielder | 75 | 60.9 → 65.8 | 0.83 |
| Full backs | 432 | 63.7 → 64.7 | 0.94-0.95 |
| Centre back, centre mid, striker, keeper | 1,095 | unchanged | 0.96-0.99 |

(The 0.96-0.99 on unchanged positions is the probe's own rounding and age-band approximation.)

### 4.3 Economy: value and wage are not re-priced

Value is sized at generation from the coarse-position rating (`500 * ovr^2 * age factor`; wage is value/200, which the
world then scales to about three quarters on average, 0.74-0.81 for the unchanged positions) and the correction was made afterwards. So a winger is now **rated about 9 points higher
than the rating his price was set from**: his value is about 0.72-0.76 of what his rating prices and his wage about 0.57-0.65
of value/200 against 0.74-0.81 for the unchanged positions, i.e. roughly a fifth to a quarter cheaper relative to them; a
defensive or attacking midfielder by a little less. Within a position the order of prices is intact. This is the deliberate residual D6 documented and this
phase kept: re-pricing is an economy change and was not asked for. It is **the** owner decision in section 12. Effects seen in the
ten-season runs: none on budgets (section 7.1), because wage budgets are normalised to each club's own bill.

### 4.4 Market behaviour, ten seasons, gate seeds 1-3, same `f54_squad_study` and seeds (**RUN**, mean of three seeds)

| Metric | Pre-merge | Merged |
| --- | --- | --- |
| XI fit | 69.36 | 69.45 |
| Weak-slot clubs | 8.97 % | 9.17 % |
| Out-of-group starters / club | 1.096 | 1.117 |
| Usable bench | 11.34 | 11.40 |
| Unused joiners | 16.87 % | 16.67 % |
| Permanent transfers / club-season | 0.823 | 0.807 |
| Free-agent signings / club-season | 2.087 | 2.100 |
| Fees / club-season (M) | 1.240 | 1.217 |

Every difference is inside what the three seeds themselves disagree on (seed-to-seed spread: weak-slot clubs 7.8 to 11.1 %).
**No unintended regression is visible; the expected change (a stronger top of the world) shows in section 4.1, not in the
market.** What this does not show: a deterministic seed-by-seed trajectory comparison is not meaningful, because the corrected
ratings change the opening state and every later random draw follows from it.

## 5. Save behaviour

### 5.1 Stale ratings (new: `player_rating::repair_stale_ratings`)

| Case | Behaviour |
| --- | --- |
| New career | Rated correctly by D6's refresh inside the identity upgrade |
| Pre-D6 save with **coarse** positions | D6: the identity upgrade now also refreshes the rating |
| Pre-D6 save with **granular** positions and the old rating (the case D6 deferred) | **Repaired on load** |
| Healthy save | No-op, no resave |

The repair runs on every load, after the identity upgrade, and re-derives a player only when `|ovr - natural_ovr|` exceeds
`MAX_OVR_ATTRIBUTE_DRIFT` (2.0), the audit's own definition of a corrupt rating (one predicate,
`ovr_drifts_from_attributes`, shared with the audit). It changes only what `refresh_player_derived` changes: `ovr`, the traits
derived from attributes (as daily training already does for every trained player) and a ceiling the new rating overtakes.
Identity, attributes, contract, wage and market value are not touched (a test compares the whole player apart from those
three fields). A player with no rating is left to the existing backfill. It is **idempotent** (a repaired player does not
drift) and **not destructive**; the load writes it back (`needs_resave`), proven by settling the file, making the stored rating
stale with SQL, loading, and reading the file through a second connection (mutation: removing `needs_resave` turns it red).

Why unconditional and not gated on the save-format version: it is an invariant like the existing stranded-fixture repair, it
needs no marker that cannot be read from the data, and it avoids a format bump that would stop older builds opening the file.
Why 2.0 and not 0.5: it repairs what the audit calls corrupt and leaves rounding-close and mildly drifted ratings to heal as
the player trains. The tolerance test pins the band 1.16 to 2.16, not exactly 2.0 (section 11).

### 5.2 Job-offer clock (Part B)

| | |
| --- | --- |
| Defect | `Game::days_since_last_job_offer` was never written; a load restarted the clock (P4.5 section 4.5) |
| Change | Migration **v069**: `ALTER TABLE game_meta ADD COLUMN days_since_last_job_offer INTEGER` (nullable). `GameMeta.days_since_last_job_offer: Option<u32>` with `#[serde(default)]`; written and read in `meta_repo` (INSERT list, `VALUES ?24`, `params!`, SELECT index 23) and `game_persistence` |
| Old saves | NULL reads as `None`, which is exactly what every older save held in memory after loading: they behave as they always did |
| Damaged value | A stored value outside `u32` reads as `None` rather than refusing the load |
| Conventions | Appended and registered; `EXPECTED_MIGRATION_COUNT` 68 → 69; fingerprint pinned in `SHIPPED` (FNV-1a, the algorithm first checked against v068's pin); the older-build imitation in `run3_integration_tests` drops the column |
| Not changed | `job_offers.rs`: frequency, thresholds and eligibility |
| Compatibility cost | A save opened by this build is at `user_version` 69, which a 68-migration build refuses, as for every migration. The rating repair needs no bump |
| Also fixed | `new_game_from_save` now clears the clock: before v069 a load always restarted it, so starting a new career from a save was clean; with the column it would have inherited the old schedule |

Tests (**RUN**, all red first): round trip of absent, 0, 1, 10, 11 and `u32::MAX` (and "not started" stays distinct from
"zero days"); five repeated save/load/advance cycles; NULL storage read through a fresh connection; a damaged value; meta JSON
without the field; a real v068 save migrated twice with its row intact; a released v061 file loaded through
`SaveManager::load_game` (no clock, column added, clock kept afterwards); the new-career reset.

## 6. Reload equality after a long career

`p45_reload_twin_after_a_long_career`, 3 gate seeds, 10 seasons, release (**RUN**): saved through SQLite, reloaded through a
fresh manager, both copies played 60 more days. **Identical before and after on all three seeds, with the timer no longer
carried across by hand**; the clocks at the save were 0, 3 and 1 days, and the test now asserts the clock directly so a
regression names itself. Invariants clean. With the loader made to drop the clock, the test fails on that assertion
(mutation, **RUN**). The cosmetic reload differences P4.5 listed (national-team list order, a re-inferred weak foot, staff
nationality fields, a float's last digit) are unchanged and none reaches play within 60 days.

## 7. Long-career and shipped-world validation

### 7.1 Gate world, ten seasons, three seeds, `p45_long_career` (**RUN**, pre-merge and merged)

| Check | Pre-merge | Merged |
| --- | --- | --- |
| Invariants | clean ×3 | clean ×3 |
| Clubs in debt, any season (rows read: 1, 5, 10) | 0 | 0 |
| Clubs over 110 % of wage budget | 0 | 0 |
| Median wage / budget | 1.00 | 1.00 |
| Squad sizes (min / mean / max) at season 10 | 19-20 / 23.0-23.3 / 28-30 | 18-20 / 23.0-23.5 / 29-31 |
| Clubs without a keeper, with one keeper | 0, 0 | 0, 0 |
| Formations | 1 (100 %) | 1 (100 %) |

The one-formation result is the P4.5 debt, unchanged.

### 7.2 Save/reload

Section 5 and 6, plus the existing `season_lifecycle` four-season SQLite reload, which passes in the workspace run.

### 7.3 Shipped world (440 clubs)

Targeted, not a 30-season run (P4.5 already ran that on the pre-merge world): the shipped 440-club, 13,865-player world,
seed 1, **3 seasons with the invariants observer on**, and the reload twin on the same world for 2 seasons, release profile
(**RUN**). The runs shared the machine with the full workspace test suite, so wall time is indicative only (147 s for 3 seasons,
about 49 s a season; P4.5 recorded 36 s rising to 54 s on an idle machine).

| Check | Result |
| --- | --- |
| Invariants, 3 seasons | **clean** |
| Squads (min / mean / max seniors) | season 1: 19 / 22.0 / 25; season 2: 20 / 23.8 / 29; season 3: 19 / 24.6 / 30. No thin squad, no club without a keeper or with one keeper |
| Clubs in debt | 0 in every season |
| Clubs over 110 % of wage budget | 0, 1, 0 (the one in season 2 is the same episode count P4.5 recorded for this world; no rule changed) |
| Median wage / budget | 1.00 |
| Pyramid | 23 leagues, size never changes, 24 promoted and 24 relegated from season 2, no unfinished league |
| Formations | 1 (100 %), no change: the P4.5 debt, unchanged |
| Opening world | rated 80+ : 188 players, 75+ : 1,128; the world holds 11,880 players at season 0 |
| Reload twin, 2 seasons | **identical after reload and identical 60 days on**; job-offer clock `Some(2)` carried by the save; invariants clean |
| Opening audit | `generated_world_integrity` (the 440-club world, after identification) passes in the workspace run: 0 findings, where D6 measured 3,786 errors without the fix |

Not measured here: instructions or memory for the shipped run (the timing wrapper also covered the compile, so those figures
would be meaningless); the P4.5 shipped figures (1,612 s and 1,177 MB for 30 seasons) are the reference.

## 8. Performance (**RUN**, instructions retired is the primary figure)

Same study (`f54_squad_study`, gate, 3 seeds × 10 seasons), release profile, `/usr/bin/time -l`. The baseline was built from
`eb1d9d79` in a **separate worktree with its own target directory** (a binary copied out of a shared target directory is
rebuilt in place and would silently be the new code).

| Tree | Instructions | Peak RSS |
| --- | --- | --- |
| `eb1d9d79` (pre-merge) | 2,400.0 B | 195.8 MB |
| merged (`b5c300e0`) | 2,433.1 B (**+1.4 %**) | 204.5 MB (+4.4 %) |

The pre-merge figure reproduces the 2,400.2 B P4.5 recorded, so the machine and method agree. Wall time (126 s against 147 s)
is **not** a result: the merged run overlapped three reviewer agents. The F5.5 gain (−26.9 % against F5.4) is preserved; the
+1.4 % is the corrected world's own dynamics plus the load-time repair (a load-path cost only). The commits after `b5c300e0`
touch the load path and tests, not the simulation.

## 9. Tests

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --workspace --all-targets --features mcp -- -D warnings` | clean |
| `cargo test --locked --workspace --no-fail-fast` | **132 targets, 4,810 passed, 0 failed, 86 ignored** (P4.5: 130 / 4,784 / 86; D6 on its own base: 124 / 4,733 / 77) |
| `cargo test --locked --lib --features mcp` | 653 passed, 0 failed, 8 ignored |
| `npm test` | 340 files passed, 1 skipped; **2,993 tests passed**, 4 skipped (unchanged: no frontend file was touched) |
| `npm run build`, `npm run lint`, `npm run format:check` | all exit 0 |

Not run: `cargo deny`, `cargo machete`, `npm run knip`, `npm run audit:i18n`, and the `/preflight` skill as one invocation
(its component commands above were each run). No player-visible string was added, so the i18n reviewer was not required; the
one error path changed (a damaged clock) degrades to `None` and returns no string.

Mutations run (each: remove only the fix, see red, restore, see green): D6's refresh call (2 tests red); the repair call in
`load_game` (1 red); the `needs_resave` line (the write-back test red, the in-memory test green); the loader's clock (the reload
twin red).

## 10. Reviewer findings

| Reviewer | Blocking | Outcome |
| --- | --- | --- |
| `ofm-architecture-reviewer` | none | **Fixed:** the module cycle `world_integrity` ↔ `player_rating` (constant and a shared predicate now live beside `natural_ovr`); `load_game` growth (the repair is a helper); `upgrade_player_identity` narrowed to `pub(crate)`; `new_game_from_save` inheriting the clock (a real regression, test red first); the docs now say v069 is a schema migration. **Listed:** `load_game` is about 255 lines (pre-existing, over the 100-line ceiling); D6's `audit_competitions` nests to depth 5 with an estimated cognitive complexity near 30 (not measured; the clippy lint is not enabled yet) |
| `ofm-dedup-reviewer` | none | **Fixed:** the drift rule written twice; my test inlining `fresh_reader`. **Listed for #589:** `attrs_at` duplicates `attrs` in `market_fixtures`; `d6_world_measure` hand-counts what `audit_competitions` reports and hard-codes 2.0; two copies of the world builder in the D6 tests; eight `GameMeta` struct literals in tests and no builder; two "refresh where needed" loops on the load path (kept separate: different log lines) |
| `ofm-test-reviewer` | **1** | **Fixed:** the write-back assertion passed with the `needs_resave` line deleted (`sample_game()` already forces a resave); split into an in-memory test and a write-back test using settle, SQL-corrupt, load, read, and mutation-checked. Also fixed: the unit test compared four fields, now the whole player; no test loaded a pre-v069 file through `load_game` (added); the competition audit had no positive control (added). **Listed:** the tolerance test pins 1.16 to 2.16 and not 2.0 exactly; no keeper, loaned or starting-XI case for the repair (it reads only attributes); `legacy_migration` does not call the repair (the later `load_game` does) |

## 11. Remaining defects and debt

1. **Value and wage are not re-priced after the rating correction** (section 4.3): wingers and midfielders are about 20-25 %
   cheaper than their corrected rating implies. A balance decision, not a data-integrity defect.
2. **A latent delegation wage-budget gap (D6 section 5.3, not mine and not fixed):** the assistant authorises each signing
   against the wage bill as it stands, not against wages of deals already agreed and awaiting registration (+1.8 % over budget in
   D6's probe). Not triggered in any run here (0 clubs over 110 %). The transfer policy was not to be changed in this phase.
3. The repair's tolerance test does not sit at exactly 2.0; an integer-valued fixture is needed to pin it.
4. Pre-existing size debt touched or worsened: `load_game` (about 255 lines), D6's `audit_competitions` / `audit_membership`.
5. D6 duplication listed in section 10.
6. A save opened by this build cannot be opened by an older one (v069), as for every migration.
7. Carried over from P4.5 and unchanged: one formation for ever, the youth pipeline thinning over a career, the user's club
   selecting greedily, the cosmetic reload differences, and the standing condition that **no person has played the window** in
   the GUI.

## 12. Reproduce

```sh
# from src-tauri/, release profile
P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_reload_twin -- --ignored --nocapture
P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture
F54_WORLD=gate F54_SEEDS=1,2,3 F54_SEASONS=10 cargo test --release -p season-harness --test f54_squad_study -- --ignored --nocapture
cargo test -p ofm_core --test opening_ratings economy -- --ignored --nocapture      # the value and wage gap
# instructions: run a built test binary under /usr/bin/time -l; build the baseline commit in its own worktree and target dir
```

## 13. Decision

### GO for the strategic AI completion phase

Why: D6 integrated without a conflict and the opening-rating gap is closed with a repair that is idempotent, derived-fields-only
and written back; the job-offer clock is persisted and a ten-season career reloads and plays on identically without any manual
bridging; the ten-season market and finance results are unchanged beyond the expected stronger top of the world; invariants are
clean; the F5.5 performance is preserved (+1.4 %); all three reviewers' blocking findings are fixed and mutation-checked.

Conditions, in order:

1. **Owner decision (affects balance, not correctness):** whether to re-price value and wage from the corrected rating, in one
   dedicated change calibrated against the economy report (section 4.3). Not blocking AI work, but any balance study run
   before it measures wingers and midfielders as bargains.
2. **Use this tree as the new ten-season baseline.** Baselines recorded against `e5282b67` or `eb1d9d79` start from a different
   opening world (section 4.1) and must not be compared with new runs.
3. Close the latent delegation wage-budget gap (section 11.2) before any phase that extends delegated recruitment.
4. The standing GUI condition from P2.5 and P3 is unchanged.

The strategic AI completion phase was not begun in this session.
