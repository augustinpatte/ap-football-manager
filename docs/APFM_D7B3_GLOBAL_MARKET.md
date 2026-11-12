# APFM D7B3 — Global market

**Status:** implemented and locally validated, with a 2026-11-12 completion pass. **Date:** 2026-11-11.
**Branch:** `deepseek/d7b3-world-market`. **Base:** `ea469f3a`.

**Scope:** make the D7B2 external market a realistic, usable global recruitment market:

- fix the search ordering defect D7B2 shipped;
- stop the long-career population decline without inflating the market;
- keep elite talent regenerating while keeping it rare;
- replace the one-click external signing with a club-and-player negotiation;
- expose the market through a global search, profile and recruitment screen.

Companions:
[`APFM_D7B3_NEGOTIATIONS_AND_UI.md`](APFM_D7B3_NEGOTIATIONS_AND_UI.md) — negotiation protocol,
IPC contract and screen flow;
[`APFM_D7B3_LONG_CAREER_VALIDATION.md`](APFM_D7B3_LONG_CAREER_VALIDATION.md) — commands, measured
30-season results and what was not run;
[`APFM_D7B2_EXTERNAL_WORLD_INTELLIGENCE.md`](APFM_D7B2_EXTERNAL_WORLD_INTELLIGENCE.md) — the
projection D7B3 continues;
[`APFM_I1_P5_D7B2_INTEGRATION.md`](APFM_I1_P5_D7B2_INTEGRATION.md) — the integration baseline.

The D7A decision stands. D7B3 still does **not** simulate the 44,032 source clubs. A career's market
is a deterministic projection of the read-only registry plus one SQLite cache per career, exactly as
in D7B2.

## 1. What was wrong in D7B2

D7B2 shipped two confirmed defects and one long-career weakness.

| Id | Defect | Evidence | D7B3 response |
|---|---|---|---|
| K1 | `Ovr` and `Value` sorting were inverted: `DESC` selected the lowest ability/value. | `order_clause` used the age direction for every sort key. | `direction` now drives `Name`, `Ovr`, `Value` and `Club`; `Age` alone uses `age_direction`, because an earlier birth date is an older player. |
| K2 | The searchable market shrank from 61,594 to 14,188 over 30 seasons. | Fixed 900-newgen intake was below the real retirement flow. | Intake is one-for-one with the season's actual retirements, capped at 6,000. |
| K3 | Elite talent did not regenerate once the source cohort aged out. | Newgen potential came only from the shared youth generator. | A small deterministic elite supplement: 8 per 1,000 newgens, at most 24 a season, potential 86–95. |

K3 and K4 from the I1 audit (P5 wage-flow asymmetry and contracted squad-cap asymmetry) are
inherited from P5 and **not** redesigned here. See §6.

### 1.1 The sorting fix and its regression tests

`fm24-adapter/src/registry.rs` now builds the clause from two directions:

```rust
let direction = if query.descending { "DESC" } else { "ASC" };
let age_direction = if query.descending { "ASC" } else { "DESC" };
match query.sort {
    RegistrySort::Name => format!(" ORDER BY search_name {direction}, ofm_id ASC"),
    RegistrySort::Age => format!(" ORDER BY birth_date {age_direction}, ofm_id ASC"),
    // Ovr, Value and Club use `direction`.
}
```

Two committed tests defend the contract against the exact regression:

- `every_sort_direction_orders_the_page_the_screen_asks_for` — for all five sort keys and both
  directions, the returned order matches the requested one, with `ofm_id` as the stable tie-break.
- `every_search_filter_narrows_the_same_page` — every supported filter (text, nation, position,
  age bounds, club, availability, incomplete) narrows the same page and never widens it.

## 2. Population model

### 2.1 One-for-one replacement

The market is a population, not a pipeline. The projection now does this at the end of each season:

```rust
pub fn newgen_intake_for(retirements: usize) -> usize {
    retirements.min(MAX_NEWGEN_INTAKE_PER_SEASON)
}
```

- `retirements` is the number of external players who actually retired in the season just replayed.
- The cap is `MAX_NEWGEN_INTAKE_PER_SEASON = 6_000`, a safety bound for a corrupt source, not the
  normal operating point.
- A season in which nobody retires adds nobody. A season with 2,400 retirements adds about 2,400
  players.

`career.rs::advance` counts retirements while it rewrites the cache and feeds that count to
`newgen_cohort`. `projected_market` in `external_world.rs` applies the same rule, one season at a
time, so the validation harness and the cache cannot drift apart. The old fixed-900 code path is
gone.

### 2.2 Elite regeneration

Ordinary newgens keep the shared youth generator's ceilings, so an 85+ player remains vanishingly
rare. To keep the very top from draining away, each cohort's **first slots** are elite:

- `ELITE_NEWGENS_PER_THOUSAND = 8`;
- `ELITE_NEWGEN_MAX_PER_SEASON = 24`;
- elite potential is drawn from 86–95.

Slot order is fixed by `(seed, season, index)`, so the same season always names the same elite
prospect. The shared youth generator itself is untouched; D7B3 only marks a prefix of the cohort
and raises that prefix's potential ceiling.

### 2.3 What the real source produced

The cache benchmark against the player's own read-only `world.db`
(`APFM_D7B2_WORLD_DB`, SHA-256 `36b6908c…27439f7`) built the registry once
(81,762 stored / 69,301 eligible / 9,843 active-world rows, 72.2 MB) and then a cold career cache
for seasons 1, 5, 10, 20 and 30.

| Seed | Season | Searchable | Newgens | Avg age | Free | OVR 80+ | OVR 85+ |
|---|---|---:|---:|---:|---:|---:|---:|
| 777 | 1 | 66,710 | 6,000 | 26.0 | 42.5% | 0.39% | 0.02% |
| 777 | 5 | 66,710 | 21,021 | 26.5 | 27.9% | 0.51% | 0.03% |
| 777 | 10 | 66,710 | 42,221 | 26.6 | 27.8% | 0.63% | 0.04% |
| 777 | 20 | 66,710 | 66,634 | 26.4 | 26.5% | 0.48% | 0.03% |
| 777 | 30 | 66,710 | 66,710 | 26.3 | 27.1% | 0.48% | 0.03% |
| 2026 | 1 | 66,678 | 6,000 | 26.0 | 42.3% | 0.38% | 0.02% |
| 2026 | 30 | 66,678 | 66,678 | 26.3 | 26.7% | 0.41% | 0.03% |
| 9001 | 1 | 66,675 | 6,000 | 26.0 | 42.6% | 0.39% | 0.03% |
| 9001 | 30 | 66,675 | 66,675 | 26.3 | 26.9% | 0.47% | 0.02% |

The searchable population is flat to within one player per seed across 30 seasons, against
61,594 → 14,188 in D7B2. The free-agent share settles around 26–28%, close to the D7B2 year-1
share and far above the 27.5% floor that a shrinking market produced. OVR 80+ stays below 0.7% at
every horizon and below 0.5% at year 30; the elite supplement restores the top without inflating
it. Full numbers, latency, cache size and memory are in
[`APFM_D7B3_LONG_CAREER_VALIDATION.md`](APFM_D7B3_LONG_CAREER_VALIDATION.md).

### 2.4 Why the population is stable rather than merely larger

The benchmark shows the mechanism: every measured season has the same eligible count, and the
newgen count rises by roughly the retirement flow each season until the source cohort has been
fully replaced. A one-for-one rule is self-correcting: a season that loses more players generates
more, and a season that loses fewer generates fewer. There is no permanent “newgen layer” growing
on top of the source.

## 3. Search contract

The backend remains the authority for the clock, roster exclusions, availability, eligibility and
paging clamp. D7B3 adds no second filtering path.

- `RegistryQuery` adds no field; sorting and filtering are the existing contract.
- The page still carries `items`, `total`, `page`, `pageSize` and `counts`.
- `includeIncomplete` remains available for diagnostics but defaults to `false` on the screen.
- The frontend sends one `pageSize = 25` request per page; it never loads the whole registry.

## 4. Valuation and affordability authority

`ExternalOfferContext` remains the sole authority for what an external club wants for a player.
D7B3 consumes it; it does not add a parallel valuation formula.

The negotiation reuses the P5 financial rules unchanged:

- fees are checked with `fee_block_after_pending`;
- wages with `BuyerWageFacts::of_committed(...).purchase_verdict(...)`;
- signing bonuses with the P5 `signing_bonus_check` / `ensure_bonus_affordable` path;
- the transfer window with `transfer_window_is_open`;
- the buyer's squad limits with the existing `squad_blocker`.

There is no second affordability system in the frontend. The screen displays the backend's
`canAffordFee`, `wagePolicyAllows`, `canAffordBonus`, `windowOpen` and `blocker`.

## 5. Files and ownership

| Surface | Files |
|---|---|
| External registry and ordering | `src-tauri/crates/fm24-adapter/src/registry.rs` |
| Career cache and intake | `src-tauri/crates/fm24-adapter/src/career.rs` |
| Projection, population, elite generation, `ExternalOfferContext` | `src-tauri/crates/ofm_core/src/external_world.rs` |
| Negotiation and completion | `src-tauri/crates/ofm_core/src/external_registry.rs` |
| Pending places a purchase must count | `src-tauri/crates/ofm_core/src/transfers/registration.rs` |
| Commands and IPC | `src-tauri/src/commands/registry.rs`, `src-tauri/src/lib.rs` |
| Service wrapper | `src/services/registryService.ts` |
| Global market screen | `src/components/transfers/GlobalMarket*.tsx`, `globalMarketModel.ts`, `useGlobalMarket.ts` |
| Tab wiring | `src/components/dashboard/DashboardTabContent.tsx`, `DashboardSidebar.tsx`, `src/pages/Dashboard.tsx` |
| Locales | `src/i18n/locales/*.json` (`globalMarket` block), `src/i18n/INTENTIONAL_SAME.json` |

D7B3 does not touch the match engine, M12, tactical AI, formation logic, match-day decisions, the
active-club strategic AI, `develop_one_season`, `refresh_player_derived`, the shared youth generator
or the P6 worktree.

## 6. Inherited limitations

These are P5 behaviours observed during the I1 audit. D7B3 neither introduces nor fixes them.

- **K3 — wage-flow asymmetry.** A purchase's committed wage can be judged against a wage policy
  that does not yet see every pending outgoing contract in the same window. The negotiation shows
  the same verdict the P5 free-agent path shows.
- **K4 — contracted squad-cap asymmetry.** D7B3 closes this on the external route: a contracted
  purchase now meets the same 30-senior registration limit as a free agent, and counts the places
  promised to deals already agreed and waiting to register. That is the rule P6 applies to an
  in-game purchase (`ff28efe9`), and D7B3 shares its `pending_incoming_purchases` helper rather
  than adding a second count. P5's other in-game purchase routes are fixed by P6, not here.

A fix to K3 would change P5's shared financial reservation code, which is outside this work's
ownership. It is recorded here rather than patched locally.

## 7. Definition of done

- K1 fixed with committed regression tests for every sort key, direction and filter.
- Searchable population flat across 30 seasons and three seeds.
- OVR 80+ below 0.7% and OVR 85+ at or below 0.05% at every measured horizon.
- Elite prospects emerge every season without a superclub.
- Cache incremental and cold builds produce identical rows at the same season.
- External signing is a negotiation; no counter, rejection or poor preview mutates the game.
- A contracted external purchase meets the 30-senior limit, including places promised to agreed
  deals, as P6 applies it to an in-game purchase.
- The global market is usable on desktop through the existing design system and all 12 locales.
