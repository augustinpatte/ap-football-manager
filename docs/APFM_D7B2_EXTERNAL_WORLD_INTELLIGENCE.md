# APFM D7B2 — External world intelligence

**Status:** implemented and locally validated. **Date:** 2026-11-11. **Branch:**
`deepseek/d7-database-audit`.
**Scope:** turn the D7B1 global registry from a frozen 2026 snapshot into a persistent,
deterministic, career-specific transfer market — development, retirement, replacement Newgens,
contract evolution, club movement and deterministic club decisions — **without** simulating 44,032
clubs and **without** changing the 240-club active world, the match engine, M12, the 2D viewer, the
AI formation/appointment/squad logic, the competition pyramid, or any frontend screen.

Companions:
[`APFM_D7B2_LONG_CAREER_VALIDATION.md`](APFM_D7B2_LONG_CAREER_VALIDATION.md) — measured
benchmarks and 30-season scenarios;
[`APFM_D7B2_INTEGRATION_CONTRACT.md`](APFM_D7B2_INTEGRATION_CONTRACT.md) — cache layout, command
flow, IPC contract and P5 compatibility;
[`APFM_D7B1_GLOBAL_REGISTRY.md`](APFM_D7B1_GLOBAL_REGISTRY.md) — the registry D7B2 builds on.

The D7A decision still stands: **NO-GO** for simulating the whole source database, **GO** for
staged expansion. D7B2 is the second staged step: the market around the active world now has a
history, but it still has no competitions, no squads and no daily turn.

---

## 1. The problem, and the shape of the answer

D7B1 made 69,301 source players searchable and signable, but every career saw the same 2026
attributes forever. A career that runs five seasons would still meet a 34-year-old at his 2026
ability, with his 2026 contract, at his 2026 club. That is a register, not a world.

D7B2 adds **intelligence without simulation**:

| Question | D7B1 | D7B2 |
|---|---|---|
| Does a young external player improve? | No | Yes, on the game's own development curve |
| Does an old one decline and retire? | Only by the age-37 query cut-off | Yes, probabilistic and monotone |
| Are there new players as the years pass? | No | Yes, a bounded aggregate Newgen intake |
| Do contracts expire, renew or move? | Only by comparing the frozen end date to today | Yes, replayed season by season |
| Does an external club accept, counter or reject an offer? | Accepts the asking price, always | Deterministic accept / counter / reject |
| Is the market the same in two saves? | Yes | No: it is a pure function of the career's seed |
| Is the shared source mutated? | No | No |

The chosen design is **architecture A (pure deterministic projection) with a materialised,
per-career cache database**:

```
world.db (read-only, player's own file)
   │  install-time build (D7B1, unchanged)
   ▼
<app data>/fm24-import/registry.db          ← shared, read-only snapshot
   │  copy-on-first-use, then *replay*
   ▼
<app data>/fm24-import/career-<seed>-<start>-<sha12>.db
   │  one row per surviving player at the career's current season
   ▼
search / detail / preview / recruit commands (D7B1 contract, unchanged)
```

The per-career file is **derived and rebuildable**, not authoritative. Deleting it costs one
rebuild; it never costs a career. The shared snapshot stays byte-identical for every save, so two
careers cannot overwrite each other's history.

### 1.1 Why not the alternatives

| Option | What it would mean | Why not |
|---|---|---|
| **Simulate every external club** | Squads, fixtures, finances for 44,032 clubs | The D7A NO-GO, by a wider margin than the whole-database import |
| **Store one career's world in the shared registry** | Advance the shared rows with one save's clock | The first career to run would corrupt every other save |
| **Re-project on every search** | Replay up to 30 seasons for 69,301 players per request | Seconds to minutes per keystroke; measured at 2.3–20.2 s for a full rebuild, per query |
| **Project on read, one season at a time, no cache** | Compute each row's state for `today` when it is read | A page of 25 rows would replay 25 × seasons; still bounded but wasteful, and retirement monotonicity would depend on read order |
| **Materialised per-career cache (chosen)** | Replay forward once per season into a local SQLite file | One bounded advance per season, then the D7B1 query path untouched |

## 2. The determinism contract

Every projected value is a pure function of four inputs:

1. the **frozen source row** (`registry.db`, read-only);
2. the **career seed** — `Game::seed`, already random per career in `world::build`, already saved
   in `game_meta.seed` and covered by the existing round-trip test
   `a_games_seed_survives_a_save_and_load`;
3. the **career season**, derived from `Game.clock` (`projection_season`: July–June, matching the
   rest of the game), never from wall-clock time;
4. the **absolute season being replayed** — the season number itself, not "the third season since
   the save".

Because the replay order is fixed (snapshot → season 1 → season 2 → … → target), advancing an
existing cache one season at a time produces exactly the same rows as rebuilding it from scratch
for the target season. The cache builder relies on that in both directions:

* a cache already at the target season is reused as is (`seasons_advanced = 0`);
* a cache behind the clock is copied and advanced one absolute season at a time;
* a cache **ahead** of the clock (an older save loaded after a newer one) is rebuilt from the shared
  snapshot, because retirement cannot be reversed.

Consequences that the tests pin:

* `a_projection_is_identical_when_repeated` — same input, same output;
* `two_careers_with_different_seeds_diverge` — two seeds, different markets;
* `five_seasons_are_repeatable_and_seed_specific` — five seasons, identical for one seed and
  different for another;
* `retirement_never_reverses` — once gone, gone for every later season;
* `the_same_career_and_season_reuse_the_same_rows` (cache) — a reload does not re-roll the market;
* `advancing_never_resurrects_a_retired_player` (cache) — an advanced cache never brings a retired
  player back;
* `a_clock_moved_backwards_rebuilds_from_the_source` (cache) — loading an older save shows the
  world as it was, not players who had already retired.

No new `Game` field, no save-format migration, no custom RNG state. The seed is the only new
career input, and it already existed.

### 2.1 Save-specific independence

Two careers opened from the same source share **nothing mutable**:

* the same `registry.db` (opened read-only, `PRAGMA query_only = ON`);
* different cache files, named from the career seed, the source start season and the source
  SHA-256.

A promoted player leaves the market by the D7B1 identity exclusion, which is derived from the live
`Game` (roster plus retired archive) on every search — not from a shared "signed" flag. Save A
signing `fm-p-1234` removes him from save A's search; save B still sees and can sign him.

## 3. Development

External players develop through `ofm_core::development::develop_one_season`, the **same** curve the
active world uses. D7B2 adds no second progression model. That gives, by construction:

* **young players improve**, with variable growth driven by the player's traits and a seeded
  stream;
* **potential is a ceiling**: `refresh_player_derived` and the development curve keep
  `ovr <= potential`;
* **not everyone reaches his ceiling** — growth points scale with the gap and the age curve, and
  ceiling drift is the game's own;
* **some players stagnate**, because growth is probabilistic and age-weighted;
* **physical decline precedes and outpaces technical decline**, because that split is inside
  `develop_one_season`, not re-implemented here;
* **attributes, OVR and potential stay consistent**, because OVR is recomputed from the attributes
  (`natural_ovr` + `refresh_player_derived`) rather than moved independently.

Market value and wage are **re-projected every season** from the projected ability, ceiling and
age (`projected_market_value`, `projected_wage`). A player cannot be valued once at 20 and then age
for a decade without revaluation. Wages scale with the employing club's reputation, and are clamped
to `[500, 400_000]` per week.

The ignored 30-season test `an_old_player_declines_and_retires` and the long-career benchmark
(`thirty_season_markets_stay_populated_and_credible`) assert the aggregate shape: mean OVR stays
stable around 63 in the synthetic fixture, no player exceeds his potential, and age/quality mixes
do not run away. Numbers are in the validation document.

## 4. Retirement and replacement

### 4.1 Retirement

At the end of every replayed season a player may retire. The chance is age-based, then raised for a
player with no club and for a player with no market:

| Age | Base chance |
|---:|---:|
| < 32 | 0% |
| 32 | 6% |
| 33 | 14% |
| 34 | 26% |
| 35 | 44% |
| 36 | 62% |
| 37 | 80% |
| 38+ | 100% |

`+18` points for a free agent, `+12` for OVR < 55 and another `+10` for OVR < 45, capped at 100%.
The roll is seeded by `external/retirement/<absolute season>/<player id>`, so it is stable for a
player-season regardless of when or how often the cache is rebuilt.

This is deliberately **not** a universal age cut-off. A 34-year-old regular at a club usually plays
on; a 34-year-old free agent with a low ability usually stops. The registry's query still applies
the existing `MAX_EXTERNAL_AGE = 37` availability filter, so a player who has not yet rolled
retirement at 37 disappears from search and from recruitment at the same moment. Retired players
are dropped from the cache and are never re-created, because the identity stream is keyed by the
absolute season and the source id.

### 4.2 Newgens

`NEWGEN_INTAKE_PER_SEASON = 900` is an **aggregate** intake for the whole external world, not a
per-club academy and not a per-league quota. Each intake:

* generates identity `fm-n-<season>-<index:04>`, so it is unique by construction and cannot collide
  with a source `fm-p-*` id;
* draws position, nationality, name, attributes and potential from the game's existing youth
  generator (`generate_youth_academy_recruit_with_nationality`), so Newgens obey the same ceilings
  and distributions as the active world's own youth intake;
* places ~70% at a club whose reputation suits the player's level, and leaves ~30% free agents —
  the intake is a supply of people, not 900 guaranteed contracts;
* gives them 1–3 year contracts at the projected wage;
* is generated from `external/newgen/<season>/<index>` alone, so a cache rebuild invents the same
  class;
* is projected forward like any other player from the season it enters.

The intake is **smaller than the source's retirement flow** on purpose: the market thins over a long
career instead of inflating with generated players. The real-source consequence is measured and
reported honestly in the validation document (searchable market ≈ 61k at season 1, ≈ 14.2k from
season 20 onward). The synthetic fixture stabilises around 14.1k.

Newgens cannot duplicate active-world academy players: they live only in the external cache until
promoted, and the existing identity exclusion removes any `fm-p-*` or promoted `fm-n-*` id from
search.

## 5. Contracts and club movement

Contract evolution is intentionally lightweight — one decision per player per season, no agent, no
wage negotiation loop:

| State | Rule |
|---|---|
| **Free agent** | Signs for a club with probability 55% (≤ 28), 38% (29–31), 18% (32–34), 0% (35+ or OVR < 40); picks a club whose reputation suits his ability |
| **Contract expiring this season** | Renews with probability 62% / 48% / 32% depending on his importance to the club (OVR vs the club's expected OVR), minus 12 points at 31–32 and 25 at 33+; a renewal is 1–4 years at the projected wage |
| **Not renewed** | Becomes a free agent; he is not silently kept |
| **Contracted, not expiring** | May move to a better-matched club with probability 12% (≤ 25), 8% (26–29), 4% (30–32), 0% (33+ or OVR < 45); 2–4 year contract at the new club |
| **Everything** | Wages and values are recomputed from the projected ability and the club's reputation; club selection is reputation-banded, never "any club" |

The clause that matters for correctness is the expiry test: it compares the contract end to the
**end of the season being replayed**, not to itself. The first implementation of this check
compared an end date to the same end date and therefore never expired a contract; the 30-season
free-agent share was ~53% instead of ~26% before the fix. The fixed version is what the validation
document measures. This is why the free-agent share is a regression test, not a cosmetic number.

Move targets are clubs **outside the active perimeter**: a projected player is never placed at a
simulated club's id unless he has joined its `Game` roster. This keeps the club sheet a screen reads
and the market's club field from contradicting each other. The pool is still the tens of thousands
of source clubs that employ at least one player.

The rules produce **neither universal renewal nor universal free agency**: renewal is
importance-weighted and age-penalised, and the measured free-agent share is 23–28% across seeds and
seasons. Club changes are justified by the player's level, so a 60-OVR player is not moved to a
1000-reputation club.

## 6. External club decisions on offers

D7B1's external clubs accepted the asking price and nothing else. D7B2 replaces that with a pure,
reusable decision:

```
minimum_fee =
  asking_price × clamp(
      0.90
    + 0.012 × clamp(ovr − expected_ovr_for_reputation(seller), −20, 35)
    + youth bonus (0.10 at ≤23, 0.05 at ≤27, −0.08 at ≥32)
    + 0.025 × min(contract years left, 5)
    + clamp((buyer reputation − seller reputation) / 2500, −0.08, 0.08)
    + stable jitter ∈ [−0.05, 0.05],
    0.80, 1.30)
  rounded, minimum 1,000
```

The jitter is seeded by `career seed + "external/club-valuation/<player id>" + season`, **not by
the call**, so opening the same preview repeatedly cannot re-roll the club's mood. The decision is:

| Offer | Outcome |
|---|---|
| free agent / asking price 0 | `Accepted { fee: 0 }` |
| `< 75% of the minimum` | `Rejected { minimum }` |
| `≥ 75% and < minimum` | `Countered { fee: minimum }` |
| `≥ minimum` | `Accepted { fee: offer }` |

The **one-click recruitment path** offers the club's own request, `max(asking_price, minimum_fee)`.
That is the key design choice for availability: an important player is *expensive*, never
*impossible*. The explicit-offer path is implemented and unit-tested in
`ofm_core::external_registry` (`preview_external_recruitment_with_offer`,
`recruit_external_player_at_offer`): a lowball is refused with
`be.error.registry.offerRejected` **before the first mutation**, and a counter is deterministic and
payable.

**Current IPC surface:** the Tauri commands still expose the one-click preview/recruit by
`player_id` only; there is no offer parameter on the wire yet. So the rule is live for the club's
request and fully tested for explicit offers, but a user-facing "make an offer" screen is
**DEFERRED** to the transfer-negotiation phase (P5). This is stated again in §8.

## 7. Reconciliation: 9,843 vs 13,045

D7B1 reports `active_world = 9,843`, while the shipped world imports **13,045** source identities.
Both numbers are correct; they count different things. D7B2 did not change either, but the market's
exclusion set depends on understanding them, so the reconciliation is recorded here.

### 7.1 From 81,762 source people to the active world

| Step | Count | Note |
|---|---:|---|
| Source players | 81,762 | every row in `world.db` |
| Contracted players in the source | 61,314 | 20,448 unattached |
| Imported perimeter / free-agent pool | 10,045 / 3,000 | chosen by the world import |
| **Imported identities in the shipped world** | **13,045** | `fm-p-*` players in `Game.players` |
| Generated players added by world build | 112 | `fm-p-*`-like generated ids, not source people |
| **Total active-world players** | **13,157** | 13,045 + 112 |

The 10,045 imported perimeter players are placed as:

| Placement | Count |
|---|---:|
| Proper club squad | 8,063 |
| Loaned out | 694 |
| Internal loan (same club group) | 557 |
| B-team academy | 464 |
| B-team, playing there | 186 |
| Loaned in from outside the perimeter | 81 |
| **Total** | **10,045** |

Registration and contract membership overlap but are not identical:

| Set | Count |
|---|---:|
| Registered in the perimeter and on a contract | 9,084 |
| Contract in the perimeter, not registered | 880 |
| Registered in the perimeter, contract elsewhere | 81 (the loan-ins) |
| Contract in the perimeter (any registration) | 9,964 |
| Registered in the perimeter (any contract) | 9,165 |
| Union | 10,045 |

### 7.2 Why the registry says 9,843

The registry classifies a source player `active_world` when **his club is in the active
perimeter**. It then applies its own stricter identity rules before labelling him usable: a name, a
birth date, a mapped OFM nation and a usable position are required. 121 of the 9,964 contract-in
players fail those rules.

```
9,964 contract-in
− 121 excluded by the registry's stricter identity rules
= 9,843  registry `active_world`
```

And the full identity equation:

```
9,843  contract-in and registry-usable
+ 3,000 free agents imported by the world build
+    81 loaned in from outside the perimeter (registered, not contract-in)
+   121 contract-in but registry-excluded
= 13,045 imported source identities
```

The 112 generated world players are not source people, so they never appear in the registry at all.

### 7.3 What D7B2 excludes from search

The exclusion set grew in D7B2: it now hides both `fm-p-*` and `fm-n-*` identities already on a
roster or in the retired archive. Before, only `fm-p-*` was hidden, so a promoted Newgen could be
offered for signing a second time. The test
`the_context_excludes_roster_and_retired_source_identities` pins both prefixes.

## 8. Feature status

| Feature | Status | Evidence |
|---|---|---|
| Deterministic per-career projection | **IMPLEMENTED AND VALIDATED** | 11 unit tests in `external_world`, 4 cache tests, seed-divergence tests |
| Materialised per-career cache, rebuildable | **IMPLEMENTED AND VALIDATED** | `career.rs` tests + real-source benchmark |
| Shared source remains read-only | **IMPLEMENTED AND VALIDATED** | `open_registry` opens `READ_ONLY` + `PRAGMA query_only`; source SHA-256 in cache name/meta |
| Credible development (growth, stagnation, decline, ceiling) | **IMPLEMENTED AND VALIDATED** | Uses the game curve; ceiling and decline tests |
| Probabilistic retirement, monotone, reload-stable | **IMPLEMENTED AND VALIDATED** | Retirement table + monotonicity tests + cache tests |
| Aggregate Newgen supply | **IMPLEMENTED AND VALIDATED** | Uniqueness, repeatability, population and quality invariants over 30 seasons |
| Lightweight contract renewal / expiry / free agency | **IMPLEMENTED AND VALIDATED** | Expiry bug fixed; free share 23–28% measured |
| Deterministic club accept / counter / reject | **IMPLEMENTED AND VALIDATED** (rule) | `club_decisions_are_ordered_and_deterministic`, lowball and counter tests |
| Explicit offer on the IPC surface | **DEFERRED** | Rule exists; commands expose one-click only |
| External-club squads, competitions, finances | **NOT IMPLEMENTED (by design)** | Would be the D7A NO-GO |
| Reconciliation 9,843 vs 13,045 | **IMPLEMENTED AND VALIDATED** | Counts reproduced from the real source; equations above |
| 30-season multi-seed validation | **IMPLEMENTED AND VALIDATED** (synthetic + real source) | See validation document |
| Worker-thread offload for cache rebuild | **IMPLEMENTED AND VALIDATED** | All reads run in `spawn_blocking` |
| Concurrent cache maintenance | **IMPLEMENTED AND VALIDATED** (smoke test + review) | Process-wide mutex; `concurrent_callers_see_one_complete_cache` |

## 9. Limitations

1. **No external-club simulation.** External clubs are reputation + name. They do not have squads,
   fixtures, finances or a bidding AI. A player can move to a club that the game never plays.
2. **The market contracts over a long career on the real source.** Intake is 900/season while the
   real 69k eligible pool retires faster; the searchable market falls from ≈ 61k (season 1) to
   ≈ 14.2k (season 20+) and then stabilises. It does not collapse, but it is not a permanent 69k
   worldwide market. The intake constant is the knob.
3. **Elite regeneration is weak.** The game's generated-player ceiling distribution yields almost
   no 85+ OVR players. After the imported source rows retire (≈ season 20), the external market has
   essentially no elite tier. This mirrors the active world's own generator, so it is a pre-existing
   game-wide property rather than a D7B2-only defect, but it is real.
4. **One cache per (seed, start season, source hash), not per save file.** Two forks of one career
   share a cache file; if their clocks differ they will rebuild/thrash but always return the correct
   season. The file is derived, so this is a performance wrinkle, not a correctness one.
5. **No daily external events.** The market advances at season boundaries only. A contract cannot
   expire mid-season in the external world; the projection test compares to the season end.
6. **Attributes are still converted once from the source.** Development moves the converted body;
   it does not re-derive FM CA/PA semantics.
7. **No user-facing offer screen.** See §8.
8. **No save-size claim re-measured after D7B2** — the cache lives outside the save, so the save
   format and save/load path are unchanged; this is structural, and the validation document says
   what was and was not timed.
