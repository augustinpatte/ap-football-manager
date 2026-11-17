# APFM I4 integration

Date: 2026-11-16. Target branch: `apfm/main`. Shared base: `0dc3582dc8a4c0a7b7a87d6746377c7618f90d36`.

This records the integration of the two I4 development branches and the validation of the combined
result. It is an integration record only; the design of each branch lives in
[`APFM_I4A_STRATEGIC_AI.md`](APFM_I4A_STRATEGIC_AI.md) and
[`APFM_I4B_WORLD_COMPETITIONS.md`](APFM_I4B_WORLD_COMPETITIONS.md).

## 1. What was integrated

| Branch | Head | Commits | Scope |
|---|---|---|---|
| I4A `deepseek/i4-strategic-ai` | `47da6fba` | `f33058f8`, `44497b15`, `eba96658`, `47da6fba` | Strategic club plan, plan-driven recruitment, renewals and sales; the 30-senior hold on every promotion and loan route |
| I4B `deepseek/i4-world-competitions` | `a7ef4fa9` | `25f71b2a`, `711aea40`, `a7ef4fa9` | Foreign countries renew on their own authored calendars; archive-once editions; rollover refactor |

Both descend directly from the shared base (`git merge-base` returned `0dc3582d` for each).

| Step | Commit |
|---|---|
| Backup of the pre-integration `apfm/main` | ref `backup/apfm-main-pre-i4-20261116` → `0dc3582d` |
| Merge I4A (`--no-ff`) | `9b80b1f4` |
| Merge I4B (`--no-ff`) | `ab7e2321` |

Both merges applied with no textual conflicts, and no integration-fix commit was needed. The two
branches share no changed file (`comm` of their file lists is empty), but they still interact
through behaviour, so the gates below were run on the merged tree rather than assumed from the
standalone results.

The working tree was clean at the start; no user work existed to preserve. Neither source worktree
nor its branch was touched.

## 2. Where the branches meet

The rollover and country-renewal paths were read for any route that registers a senior player
without going through I4A's `signing_place_refusal`:

* `turn/country_renewal.rs` (I4B) touches competitions only: archiving, prize money, promotion and
  relegation, cup re-seeding. It never moves a player or registers a squad member.
* `end_of_season/mod.rs` (I4B) changes the rollover's competition handling. The squad side of a
  season end is still `apply_season_end_squad_turnover`, which calls `ai_contracts::plan_every_ai_squad`
  (I4A, guarded) before the youth intake.
* Every other senior-registering route already calls `signing_place_refusal`: `squad_floor`,
  `youth_pathway`, `ai_contracts`, `external_registry`, `transfers::market` (loans),
  `transfers::execution`, free-agent talks and the assistant.

So I4B's lifecycle does not bypass I4A's protection. This was established by reading the call
sites and then confirmed by the studies in §4, not by the reading alone.

## 3. Static and unit-level gates

Run from the repository root on the merged tree (`ab7e2321`).

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | exit 0, no diff |
| Clippy, CI scope | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings` | exit 0 |
| Clippy, MCP | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets --features mcp -- -D warnings` | exit 0, 0 warnings |
| Rust workspace | `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace` | exit 0; 154 result lines; **5,103 passed, 0 failed, 124 ignored** |
| Frontend suite | `npm test` | exit 0; 345 files passed, 1 skipped; **3,053 passed, 4 skipped** |
| Frontend build | `npm run build` (`tsc && vite build`) | exit 0 |

The ignored tests are the long studies, which are run explicitly in §4. Of the 124 ignored, the
imported-world study needs a local FM24 world file and was **not run** (see §6).

Tests passing in the workspace run, by area the integration touches:

| Area | Passing | Failing |
|---|---|---|
| `ai_contracts::` (renewals, planning) | 54 | 0 |
| `ai_squad_sales::` | 13 | 0 |
| `squad_floor::` | 59 | 0 |
| `squad_intelligence::plan::` | 13 | 0 |
| `transfers::` | 201 | 0 |
| `youth_pathway::` (academy promotion) | 12 | 0 |
| `staff_system::assistant::` | 12 | 0 |
| `turn::country_renewal::` (foreign renewal) | 9 | 0 |
| `end_of_season::` (lib) | 66 | 0 |

Biome was confirmed as the real package (`2.5.5`) before the frontend run, not the `biome@0.3.3`
impostor.

## 4. Combined long-career studies

Both use existing harnesses and the same `gate` world (80 clubs, foreign countries included). Both
were run with `--release` and a 40-minute watchdog, and nothing else ran concurrently.

### 4.1 Ten-season strategic AI study (I4A harness, on merged code)

```
I4_SEEDS=1,2,3 I4_SEASONS=10 cargo test --locked --release --manifest-path src-tauri/Cargo.toml \
  -p season-harness --test i4_strategic_ai -- --ignored --nocapture
```

Exit 0; `1 passed`, 322.76 s. Seeds 1, 2 and 3 each completed all 10 seasons (about 107.5 s per
seed, 8.3 to 11.7 s per season on seed 3).

This study is measurement-only: it prints rather than asserts, and its exit code alone proves
little. The log was therefore checked for its failure markers explicitly:

| Check | Result |
|---|---|
| `!!` breach lines | 0 |
| "stopped early" / "holds N seniors" / panics | none |
| Season rows read from the `seniors max / clubs >30` table (3 seeds × 10 seasons, 79 clubs each) | 30 |
| Largest senior group on any club in any season | 30 |
| Clubs above 30 seniors, summed over all 30 season rows | 0 |
| Permanent AI deals per seed | 460, 538, 512 |
| Mean per-signing gain | +0.32, +0.29, +0.31 |
| Mean squad XI fit, seed 3 | 69.4 in season 1 rising to 71.2 in season 10 |

### 4.2 Foreign-calendar studies (I4B harness, on merged code)

```
cargo test --locked --release --manifest-path src-tauri/Cargo.toml -p season-harness \
  --test foreign_calendar -- --nocapture --include-ignored --skip imported_world
```

Exit 0; `3 passed, 0 failed, 1 filtered out`, 352.44 s.

| Test | Result |
|---|---|
| `foreign_leagues_keep_their_authored_calendar_and_archive_every_edition_once` | pass |
| `a_reloaded_career_keeps_the_renewed_calendar` | pass |
| `thirty_seasons_of_foreign_calendars` | pass; 30 seasons, 352.3 s; 278 editions archived across 9 competitions |

Per-season cost was flat: 9.4 s (season 1), 11.2 s (10), 11.8 s (20), 11.8 s (30), maximum 12.8 s.
The test asserts, every season, that the authored opener month and year hold and that no edition is
archived twice.

### 4.3 What the two studies do and do not prove together

Both ran on the same merged binary and world, and each property held. They were **not asserted in a
single run**: `season_harness::Invariants` does not check the senior limit, and the I4A study does
not check calendars. The combined claim is therefore two separate green runs of one binary, plus the
§2 reading of the call sites, rather than one run asserting both. A single shared invariant would
close this (see §6).

## 5. Coexistence of I4A and I4B

| Capability | Evidence on the merged tree |
|---|---|
| Strategic recruitment | 460, 538 and 512 AI deals over the three 10-season seeds; plan tests pass |
| Renewals | `ai_contracts::` 54 passed |
| Sales | `ai_squad_sales::` 13 passed |
| Academy promotion | `youth_pathway::` 12 passed; no club above 30 seniors in 3 seeds × 10 seasons |
| Loans and senior-place limit | `transfers::` 201 passed; 0 club-seasons above 30 |
| Foreign competition renewal | `turn::country_renewal::` 9 passed; 30-season study green, 278 editions archived once each |
| Season rollover | `end_of_season::` 66 passed in lib; calendar findings empty for 30 seasons |
| Save and IPC compatibility | the merge adds no `Serialize`/`Deserialize` derive and no `#[serde]` field; `domain/`, `crates/db/`, migrations and `src/` are untouched |

## 6. Remaining risks and limitations

**Release blockers:** none found.

**Not run, and why:**

* `imported_world_keeps_foreign_calendars_for_five_seasons` needs `APFM_WORLD_JSON` pointing at a
  local FM24 import and was skipped. I4B recorded it as passing standalone (212.9 s); it has **not**
  been re-run on the merged tree.
* The `shipped` and `lower` worlds were not studied here; only the `gate` world.
* The Tauri app was not launched; no GUI check was done.

**Later improvements:**

* Add the 30-senior limit to `season_harness::Invariants` so the foreign-calendar study asserts it
  in the same run as the calendars (§4.3).
* The I4A study reports breaches instead of failing on them. Making a `!!` line fail the test would
  stop its exit code from being misleading.
* The 30-season cost is 9 to 13 s per season. I4B recorded 226.1 s for the same study under a
  different test profile, so the two figures are not like for like and no regression is claimed,
  but it is worth re-measuring in one profile when performance next matters.

## 7. Commits

| Commit | What |
|---|---|
| `9b80b1f4` | merge I4A |
| `ab7e2321` | merge I4B |
| this commit | `docs/APFM_I4_INTEGRATION.md` |
