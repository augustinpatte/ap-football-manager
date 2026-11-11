# APFM I1: integrating P5 and D7B2

Branch `integration/p5-d7b2`, local only: **not pushed, not merged into `apfm/main`**. `origin` is a public repository and the
project carries football-data provenance concerns (`APFM_DATA_PROVENANCE.md`), so nothing was pushed. Evidence, commands and the
verdict are in [`APFM_I1_ACCEPTANCE.md`](APFM_I1_ACCEPTANCE.md).

Evidence labels: **RUN** (executed in this phase), **READ** (read in source), **NOT RUN**, **BLOCKED**.

## 1. Sources

| | Branch | Reported | Actual head (RUN: `git rev-parse`) | Worktree state |
| --- | --- | --- | --- | --- |
| Claude P5 | `claude/apfm-playable-career` | `d379c2c4` | `d379c2c4` | clean, untouched |
| DeepSeek D7B2 | `deepseek/d7-database-audit` | `56bd0a67` | `56bd0a67` | clean, untouched |
| Base | `apfm/main` | | `4e5dd353` | |

Both branches fork **directly from `apfm/main`** (merge base `4e5dd353` for the pair as well): P5 is 217 commits ahead, D7B2 is 4.
Neither contains the other. P5 was merged first (no ancestry argument for the reverse).

## 2. Merge commits and integration commits

| Commit | What |
| --- | --- |
| `d20930af` | `merge(p5)`: `--no-ff` merge of P5. No conflicts. |
| `3bc86d7f` | `merge(d7b2)`: `--no-ff` merge of D7B2. 14 textual conflicts, resolved below, plus one compile fix. |
| `0286cad7` | `fix(external)`: a registry signing is judged against P5's reservations and squad limit (section 4). |
| `72c76e91` | `fix(merge)`: P5's `PlayerRow` test helper completed with D7B2's two new fields. |
| `d919a67b` | `test(registry)`: D7B2's market against a live P5 career, end to end. |
| `2aca4712` | `test(registry)`: funds the test club through the ledger (finance-journal guard); adds the I1 studies. |
| `e3c527cf` | `test(i1)`: imported-world reload study; the combined-career study reports instead of hiding. |
| this documentation commit | the two I1 documents |

(RUN) Both source tips are ancestors of `HEAD`; 0 commits of either are missing.

## 3. Conflicts and exact resolutions

| File | Conflict | Resolution |
| --- | --- | --- |
| `ofm_core/src/lib.rs` | Both sides added `pub mod` lines at one spot (`fast_forward`; `external_registry`, `external_world`). | Kept all three. |
| `ofm_core/src/transfers/mod.rs` | D7B2 widened `transfer_window_is_open` to `pub(crate)`. P5's export list already contains it and is a strict superset. | Took P5's side. |
| `src/i18n/locales/*.json` (12 files) | P5 appended several `be.error.*` blocks and D7B2 a `registry` block at the same position. | Union. **Proven** against both parents: every locale holds exactly `keys(P5) ∪ keys(D7B2)` (3,901 keys each, 7 from D7B2), every value taken from a parent. |

**Not conflicts, but broken by the combination** (Git reported clean merges of the files involved):

| # | Defect | Found by | Fix |
| --- | --- | --- | --- |
| 1 | `external_registry.rs` imports `transfers::fee_block`; P5 had moved it into the private `transfers::affordability` module. Did not compile. | `cargo check` | Re-exported in the merge commit, then replaced in the next commit by the helper in 4. |
| 2 | `fm24_adapter::source::sample_row` (a P5 test helper) builds `PlayerRow` literally; D7B2 added `club_nation` and `club_reputation`. The lib tests did not compile (E0063). | the first workspace clippy | `72c76e91`. |
| 3 | **Semantic**: an external signing completes the moment it is agreed, and was judged with `can_afford_fee` and `BuyerWageFacts::of`, which see only the books. P5 holds the fee, cash and wage of *agreed deals waiting to register* (`pending_fee_commitments`, `BuyerWageFacts::of_committed`). A registry signing could spend money a deferred bid had already promised, and an external free agent skipped P5's 30-senior registration limit. | reading both sides, then 4 failing tests | section 4 |
| 4 | The root finance guard (`tests/finance_journal.rs`) failed on my new test file, which assigned `team.finance`. | the first workspace run | cash is funded through `finances::post_all` (`BoardSupport`). |

## 4. The reservation fix

- `transfers::fee_block_after_pending(game, team, fee)` is one helper over P5's own `fund_block_after` with the club's `pending_fee_commitments`. **No second affordability rule.**
- `external_registry` now reads the committed figures: the fee check through that helper, the wage verdict through `BuyerWageFacts::of_committed`. A registry signing is "a purchase being agreed now", the case `of_committed` documents, not a registration, so the wage is counted once.
- An external **free agent** meets `contracts::squad_capacity` (30 seniors), as every other free agent does. A contracted purchase follows the transfer rules, which have no such limit (as on P5's tip).
- `ExternalOfferContext` and the offer decision are untouched: `external_world.rs` is **byte-identical** to D7B2's (RUN: 0 diff lines) and the valuation formula exists in one place (`external_world.rs:272`).
- Red before, green after: cash, budget, wage and squad-limit tests (`transfers_tests/external.rs`) failed on the merged code for the predicted reason and pass now.

## 5. Feature-preservation matrix

| Feature | Source | Status | Evidence |
| --- | --- | --- | --- |
| Shot accuracy relative to the defence (3.9 → 2.7-3.4 goals) | P5 | **Preserved** | `engine` is untouched by the merge; the 3-seed and 30-season studies are **digit-identical** to P5's tip (section 7) |
| Saved starting-eleven repair on every departure | P5 | **Preserved** | P5's tests pass in the workspace run; the sim studies are identical |
| Pending transfer fee and wage reservations | P5 | **Preserved and extended** to the external route | section 4; P5's `bid_money` tests still pass |
| Manager identity at the touchline; injury replacement | P5 | **Preserved** | as above |
| Formation persistence; cross-engine tests | P5 | **Preserved** | `p5_formation_persistence` and the spatial tests pass |
| Global registry, source identity mapping | D7B1 | **Preserved** | `build_registry` runs on the real source: 81,762 stored, 69,301 eligible, 9,843 active-world (D7B2's figures) |
| Deterministic per-career projection; rebuildable SQLite cache | D7B2 | **Preserved** | integration tests: cache rebuilt after an older clock, deletion loses nothing, concurrent readers agree |
| Retirement and Newgen generation | D7B2 | **Preserved** | Newgens appear, are promoted once, survive reload |
| External contracts, club movement | D7B2 | **Preserved** | real-source benchmark (section 8 of the acceptance document) |
| Offer acceptance, counteroffer, rejection | D7B2 | **Preserved** | a rejected offer leaves the whole game byte-identical; a counter is paid at the floor |
| Recruitment into normal `Game.players` rows | D7B2 | **Preserved** | `fm-p-*` and `fm-n-*` promoted exactly once, selectable in the next match, survive save/reload |
| Save compatibility | both | **Preserved** | no `Game` field, table or migration added by either; `Cargo.lock` gains one dependency line (a root dev-dependency) |
| Source registry read-only | D7B2 | **Preserved** | byte-identical before and after two careers recruiting |

## 6. The two worlds

The "240 versus 440" difference is **a difference in scope, not a mismatch** (READ):

| | Imported FM24 world | Generated default world ("Random World", "shipped") |
| --- | --- | --- |
| Clubs | **240** (`fm24_adapter::pipeline::EXPECTED_CLUBS`) | **440** (`ofm_core::generator::STANDARD_WORLD_TEAM_COUNT`) |
| Players | 13,358 (13,045 with `fm-p-*` ids, 112 generated fillers and others) | 11,880, all fictional |
| Built from | the player's own `world.db` via `apfm-import` | the procedural generator |
| Validated by P5 | **No** | Yes (30 seasons, 440 clubs, seed 1; 3 and 30 seasons on the 80-club gate world) |
| Validated by D7B2 | the registry's exclusion logic and the 9,843/13,045 reconciliation | no |

P5's validation is of the generated world and **must not be read as validating the imported world.** In I1 the imported world was
exercised separately (the acceptance document, sections 5 and 6). Measured on the fresh import (RUN, `i1_world_overlap`):

- All 13,045 `fm-p-*` roster ids are registry ids; all 9,843 `active_world` registry rows are on the roster.
- **3,079 players the registry marks `eligible` are already on the roster** (the free-agent pool and loan-ins; 123 more are `missing_nation`). Without the exclusion set the market would offer them a second time. The exclusion is load-bearing, and it is derived from the live roster and retired archive on every search.
- In a generated world no player id uses the `fm-p-`/`fm-n-` namespaces (tested), so a registry player cannot collide with a generated one.

The world size was **not** changed to reconcile the numbers.

## 7. Defects and limitations found (none critical)

| # | Where | Description | Evidence | Status |
| --- | --- | --- | --- | --- |
| K1 | D7B1 `registry::order_clause` | `descending: true` sorts **ascending** for `Ovr` and `Value` (it reuses `Age`'s reversal, which is right only for age). A "best first" request returns the *lowest*-rated players. My first study signed 42 such players before I noticed "best ovr 24". | READ + RUN | Not changed (D7B1 behaviour; no UI exists yet). **D7B3 must fix before a screen exists.** |
| K2 | external route | No player consent. P5's manual route puts a contract to the player, who can refuse; the one-click external route (D7B1 design) never asks. Also no instalments, bonus or sell-on terms. | READ | Not built (the brief forbids expanding negotiation). |
| K3 | P5 wage policy, applied to the external route | The squad-floor waiver lets a short squad exceed its wage budget, and **future wage cash flow is not reserved**. A bulk recruiter in the gate world (72 free agents and 5 contracted in 10 seasons) drove its own club to cash −1.6 M in season 7 (1 club-season of 800) and a wage bill of ~2× budget. Fees never overdrew cash; no refusal changed anything. | RUN (`gate_combined_10`) | P5's rule, not integration's. P6/finance. |
| K4 | `contracts::squad_capacity` | Caps free-agent signings only; a contracted purchase has no senior cap (also P5's rule). The managed club reached 34 seniors in the imported-world run. | RUN | Documented asymmetry. |
| K5 | imported world economy | Clubs in debt: **4 → 9 → 10 → 4 → 2 of 240** across five seasons, and 4 → 7 → 8 with **no recruitment at all** (control). P5's "0 clubs in debt" is from generated worlds. | RUN | Pre-existing; imported world not covered by P5's economy work. |
| K6 | imported-world reload | A reloaded career lists competitions in another order, lacks the unpersisted squad-floor journal, and 60 days on yields a few extra `TransferRoundup` news articles. **Nothing in players, contracts, tables or money.** | RUN, **identical output on P5's tip and on this tree** | Pre-existing, cosmetic as far as measured. |
| K7 | the user's *installed* FM24 world file | Dated Oct 28; two imported-world suites fail against it (`fm24_competitions` 0 of 2, `fm24_ratings` 1 of 3: "installed world older than revision 6", no national cups). All 21 imported-world tests pass on a world regenerated by this tree's importer. | RUN | **The user should re-import** for P5/D6's rating revision and the cups to apply. |
| K8 | D7B2 documents | The registry build and the projection run on `ofm_core` code (`player_rating`, `development`) that P5's D6 work changed. Integrated figures differ from D7B2's published tables (OVR 80+ at year 1: 0.42 % against 0.00 %; registry 72,179,712 against 72,048,640 bytes). **D7B2's own tip reproduces its tables exactly on this machine**, so this is an integration effect (acceptance document, section 8). | RUN | D7B3 must re-baseline its numbers. |
| K9 | `recruit_external_player_at_offer` | 115 lines, over the 100-line ceiling (D7B2's original; not touched). | READ | D7B3. |
| K10 | `tests/transfers_tests.rs` | 3,121 lines, over the 3,000-line cap (3,119 on P5's tip; +2 for the module line). | READ | P5's; the new tests are in their own file. |
| K11 | tests | The `fm24_*` season-harness tests **pass vacuously** without `APFM_WORLD_JSON`, so a green workspace run says nothing about the imported world. | RUN (they ran for real in I1) | Documented; consider a loud skip. |
| K12 | thin squads | One club of 80 sits below 16 seniors at seasons 5, 10 and 30 in the gate world. | RUN, **identical on P5's tip** | P5's. |

**Not defects, confirmed:** `ExternalOfferContext` is the only valuation; external recruitment is **backend-accessible only** (no
`.tsx` file calls `searchExternalPlayers`, `previewExternalRecruitment` or `recruitExternalPlayer`; no MCP tool references the
registry). No external-market UI was created.

## 8. What P6 and D7B3 must not change concurrently

P6 owns match realism, the match engine, M12 and tactical AI. D7B3 owns the external world.

**P6 owns:** `src-tauri/crates/engine/`, `ofm_core/src/turn/`, `live_match_manager.rs`, `spatial_*`, `interactive_match/`, `ai*.rs`,
`matchday*.rs`, the M12 pin in `ofm_core/Cargo.toml`.

**D7B3 owns:** `ofm_core/src/external_registry.rs`, `external_world.rs`, `fm24-adapter/src/{registry,career}.rs`,
`src/commands/registry.rs` (and `registry/`), `src/services/registryService.ts`, the `be.error.registry.*` keys.

**Shared interfaces: change by agreement, one owner at a time.**

| Interface | Why |
| --- | --- |
| `transfers::fee_block_after_pending`, `transfers::pending_fee_commitments`, `BuyerWageFacts::of_committed`, `contracts::squad_capacity` | The reservation seam. D7B3 consumes it; a P6 change to P5's transfer rules moves it under D7B3. |
| `ExternalOfferContext`, `ExternalOfferDecision`, `ExternalRecruitPreview` (Rust and `registryService.ts`) | One valuation rule; the IPC shape. |
| `ofm_core::development::develop_one_season`, `player_rating::refresh_player_derived`, the youth generator | D7B2's projection runs on them. A P6 or rating change **changes the external market** (K8). |
| `Game::seed`, `projection_season` | The cache key and clock. |
| `src/i18n/locales/*.json` (12 files) | Both phases add keys in the same region and **will conflict textually**, as they did here. Use one owner, or append at distinct anchors. |
| `Cargo.lock`, `ofm_core/Cargo.toml`, `src-tauri/Cargo.toml` | Dependency edits. |

## 9. Reproducing

```sh
git switch integration/p5-d7b2
printf 'paths = ["<path to apfm-match-lab at 28f34b10>"]\n' > src-tauri/.cargo/config.toml   # git-ignored; optional, the pin also resolves
cd src-tauri && cargo test --locked --workspace --no-fail-fast
cd .. && npm ci && npm test && npm run build && npm run lint && npm run format:check
# the long studies and the imported world need the player's own files, never committed:
I1_WORLD=fm24 I1_WORLD_DB=~/apfootballmanager/data/world.db APFM_WORLD_JSON=<fresh apfm-import output> \
  cargo test --lib commands::registry::combined_career -- --ignored --nocapture
```
