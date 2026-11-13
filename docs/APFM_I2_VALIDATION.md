# APFM I2: validation

Evidence and its limits for [`APFM_I2_INTEGRATION.md`](APFM_I2_INTEGRATION.md). Branch `integration/i2-unified`; nothing pushed or merged.
Host: macOS, Apple Silicon. Labels: **PASSED**, **FAILED**, **NOT RUN**. A local pass is not a claim about CI.

## 1. Verdict

**CONDITIONAL GO.**

For: the integrated simulation is **identical to P6.1** on the three-seed, ten-season study (every table, cell for cell, 720 lines),
at +0.003 % instructions; three ten-season careers with the global market active finish with no refusal that mutated anything, no
ledger mismatch, no duplicate identity, clean invariants and an identical reload twin; the imported 240-club world plays and
reloads; the 30-senior limit held at exactly 30 through 216 refusals; the full Rust workspace, the MCP-feature library tests and the
full frontend suite pass; clippy is clean at `-D warnings` with and without `--features mcp`.

Against a plain GO: **no human or automated driver has used the desktop interface**, and a static review found accessibility gaps
in the new Global Market screen (integration §6 item 2); one design gap (a free agent can take a place promised to an agreed
purchase) is documented and pinned, not fixed (item 1); two in-game routes lack a boundary test (item 3).

Why not NO-GO: no regression was found against P6.1, and every defect the integration exposed (six, integration §5) was fixed with a
test that was seen to fail first.

## 2. Gates run on the final tree (`801461fe`)

| Gate | Command | Result |
| --- | --- | --- |
| Rust format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | **PASSED** |
| Clippy | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings` | **PASSED** |
| Clippy, MCP | same with `--features mcp` | **PASSED** |
| Workspace tests | `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace --no-fail-fast` | **PASSED**: 152 results, **5,032 passed, 0 failed, 121 ignored** |
| Library tests, MCP | `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --features mcp` | **PASSED**: 670 passed, 0 failed, 10 ignored |
| Frontend suite | `npm test` | **PASSED**: 343 files passed, 1 skipped; **3,012 passed**, 4 skipped |
| Frontend format / lint | `npm run format:check`, `npm run lint` | **PASSED** (format failed once mid-run on my own unformatted line; fixed, `801461fe`) |
| Dead code | `npm run knip` | **PASSED** |
| Type check + build | `npm run build` | **PASSED** |
| i18n | `npm run audit:i18n` (reporter, exits 0 always) and `vitest run src/i18n` (the gate) | report read; gate **PASSED** (9 files, 35 tests) |

Counts, for comparison: P6.1 reported 5,013 / 0 / 121 (the workspace) and 667 (MCP library). The merge commit alone ran
5,028 / 0 / 121; the integration fixes added 4 Rust tests (5,032) and 1 frontend test (3,011 on the merge, 3,012 final).
`--locked` was used throughout; this tree has no local dependency override, so the pinned M12 revision was what built.

### Not run
`cargo deny`, `cargo machete`; `/preflight` as one invocation (its components were run individually, above); CI; a thirty-season
career; an imported-world career longer than five seasons; any desktop GUI step.

## 3. Reviewer agents

| Agent | Result |
| --- | --- |
| `ofm-architecture-reviewer` | One medium-high finding, **confirmed and fixed** (fee and bonus overdraw). Medium: free-agent places not reserved (documented, pinned). Low: mutation ordering, cache rule stamp. Crate boundaries, serialization, size limits and the 30-limit at agreement, registration and market: clean. |
| `ofm-dedup-reviewer` | Squad limit and pending purchases: no third copy. Duplication inside D7B3 recorded (integration §6 item 4). Its two key findings were **confirmed and fixed**: an unregistered error key and the hard-coded wage unit. |
| `ofm-test-reviewer` | Static only (no test was run by it). Medium: no cross-path squad test through the real writers, now **added** and mutation-checked; reservation untested on `respond_to_offer`/`counter_offer` (open); the free-agent behaviour was unpinned, now **pinned**. |
| `i18n-auditor` | Two blocking findings, **confirmed and fixed** (three keys with no translation; parameterised blocker not resolved). Wage unit **fixed**. Translation-quality notes (pt-BR, cs, plurals) **open, unverified by me**. |
| `ui-accessibility-reviewer` | Static only. No keyboard trap, div-as-button, hex colour or unlabelled icon button. Medium findings **open** (integration §6 item 2); contrast and the sticky-card height need a human and a real window. |

Reviewer statements of the form "would fail without X" were treated as hypotheses. Where I acted on one, I reproduced it.

## 4. Fixes: red-then-green evidence

| Fix | Red (before) | Green (after) |
| --- | --- | --- |
| Fee and bonus tested together | `a_fee_and_bonus_together_cannot_overdraw...` and `a_signing_bonus_cannot_spend_cash_an_agreed_purchase_is_holding` both failed on the status assertion (the move read `agreed`) | pass |
| Parameterised blocker resolved | component test failed with the component reverted to `t(outcome.blocker)` | pass |
| Cross-path squad place | `an_agreed_in_game_purchase_holds_its_place_against_a_global_market_purchase` failed with the reservation mutated out (`promised = 0`) | pass |
| Free-agent gap | characterization test; documents current behaviour, not a desired rule | pass |

## 5. Long-career validation

### 5.1 Integrated simulation against the P6.1 baseline (RUN)

`P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10`, invariants on, the same release test binary built from each tree (P6.1 from
`git archive adb44b4a` in an isolated directory with its own target; no other worktree touched):

| Check | Result |
| --- | --- |
| Full stdout of the two runs, diffed | **720 lines each; the only differing lines are three "seconds per season" wall-clock rows.** Formations, top share, changes, finances, squads, loans, prospects: identical to the cell. |
| Invariants | `clean` on all three seeds, on both trees |
| Ten-season career | about 45 s with invariants on |

This is the strongest regression evidence: D7B3's merge did not alter a single simulated outcome of the P6.1 world.

### 5.2 Cost (RUN)

Bare release test binary under `/usr/bin/time -l`, invariants off, three seeds × ten seasons:

| | P6.1 baseline (this machine) | Integrated | Change |
| --- | --- | --- | --- |
| Instructions retired | 2,566,062,767,158 | 2,566,135,290,971 | **+0.003 %** |
| Wall time | 130.5 s | 130.4 s | flat |
| Max RSS / peak footprint | 188.1 MB / 171.2 MB | 186.7 MB / 158.6 MB | flat (footprint differences are run-to-run noise) |

The baseline reproduces P6.1's own recorded 2,566.0 B instructions, which validates the method. Instructions retired are the figure
to trust; wall time on a shared machine is not.

### 5.3 Careers with the global market active (RUN)

`commands::registry::combined_career::i1_combined_career`: a generated career played through the production day and rollover; the
manager shops in the real registry (built from the player's own `world.db`, SHA-256 prefix `36b6908c`, never committed) before each
season and monthly for free agents, under the world's own money and wage rules, then the career is saved, reloaded as a twin and
compared. Generated world, 10 seasons, 3 seeds:

| Seed | Free agents / contracted signed | Fees | Refusals that mutated | Ledger mismatches | Duplicates | Invariants | Reload twin | Time / peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 61 / 8 | 38.8 M | 0 | 0 | 0 | clean | identical, same market | 112 s / 1.55 GB |
| 2 | 33 / 8 | 55.9 M | 0 | 0 | 0 | clean | identical, same market | 102 s / 0.99 GB |
| 3 | 34 / 8 | 27.3 M | 0 | 0 | 0 | clean | identical, same market | 101 s / 1.10 GB |

Every season: 0 clubs in debt, 0 duplicates, market 66.6 k (17.7-18.9 k free agents), newgens per 100 best growing from 8-16 to 78-81, best OVR in
the market 87-95. Refusals by key were boardWagePolicy ~1.1-1.3 k, transferBudgetTooLow ~0.2-0.3 k, insufficientFunds ~0.1 k,
transferWindowClosed ~0.1 k, negotiationNotAgreed 3-9, playerRefusesMove 0-2; **no squadFull** (the study's club keeps 10-20 seniors).

Imported FM24 world (240 clubs, regenerated by this tree's `apfm-import` into the scratchpad; the installed world was not touched),
5 seasons, 2 seeds: 26 + 29 free agents and 5 + 5 contracted signed (30.3 M and 45.0 M in fees); **216 `squadFull` refusals with seniors
capped at exactly 30**; 0 mutating refusals, 0 ledger mismatches, 0 duplicates, invariants clean, reload twin identical (the two known
reload differences, 5 `news` items 60 days on, also hold on P5's tip per I1). Control (`I1_SIGNINGS=0`): 0-2 clubs in debt per season
without any recruiting, so that is world behaviour, not the market. Each seed's study took 171 s (wall time including the build was longer); peak RSS 1.2-1.3 GB.

### 5.4 Imported-world suites (RUN, non-vacuous)

`APFM_WORLD_JSON` set to the fresh world, `--include-ignored`, release: `fm24_career` 2, `fm24_competitions` 2, `fm24_free_agents` 4,
`fm24_global_market` 3, `fm24_loans` 4, `fm24_ratings` 3, `fm24_rosters` 3, `i1_fm24_reload` 1 = **22 passed, 0 failed**. A first
attempt matched zero tests (zsh did not split an argument string) and was discarded, not counted.

## 6. End-to-end flow against the brief

| # | Step | Automated evidence |
| --- | --- | --- |
| 1-2 | Create a career, select a club | `i1_combined_career` (generated world); `fm24_career` and `build_fm24_career` (imported world) |
| 3 | Load squad, finances, tactics | The harness invariants read squads, cash, wages each day; AI tactics in 5.1 |
| 4 | Select formation and roles | **AI** formation selection: 5.1 (identical to P6.1). Role support: engine tests in the workspace suite. A *human* choosing a formation and roles through the screen: **not exercised** |
| 5-7 | Advance the calendar, simulate, finalize; standings, stats, finances | `play_day` / `roll_season_over` in 5.1, 5.3, 5.4; `fm24_competitions` |
| 8-10 | Search the market, negotiate, complete or reject | `i1_combined_career` calls search → negotiate → finalize; counts above include counters, refusals and player refusals; command tests `commands::registry::tests` |
| 11 | Squad-size, wage, budget constraints | 216 `squadFull`, `boardWagePolicy`, `transferBudgetTooLow`, `insufficientFunds` refusals, all non-mutating; new overdraw tests |
| 12-14 | Save, reload, verify | reload twin in 5.3; `i1_fm24_reload`; `a_recruit_survives_a_save_and_reload` |
| 15 | Multiple seasons with the systems active | 5.1 (10 seasons × 3), 5.3 (10 × 3 and 5 × 2) |

## 7. GUI testing status

**NOT PERFORMED.** The desktop application was not launched and no desktop-control tool was used. The Global Market screen is
covered by component tests (25 files / 208 tests under `src/components/transfers`), the production build and a static accessibility
review, none of which is a manual pass. The manual acceptance checklist of I1 still applies, with Global Market steps added: open
the tab, filter and sort, page, open a profile, check terms, receive a counter, put it back, sign, confirm the roster and cash, save,
quit, reload; then try a signing into a 30-player squad and one whose bonus exceeds cash.

## 8. Known failures and untested areas

* **Failures in the final tree:** none.
* **Failures met and resolved during the phase:** the combined-career study aborted twice (stale sort and refusal assumptions,
  integration §5 items 5-6); a frontend format check failed once on my own edit.
* **Untested:** the desktop GUI; keyboard and screen-reader behaviour of the new screen; contrast; zh-CN layout of the filter row;
  `respond_to_offer`/`counter_offer` at the 30 boundary; pending loan-ins against the limit; a cache built under the D7B2 intake
  rule; thirty-season careers; the imported world beyond five seasons; CI.
* **Not a regression test of P6 itself:** role support intentionally changes match outcomes, so P6 and P6.1 results are not expected
  to match. The comparison in 5.1 is P6.1 against the integrated tree, which must match, and does.

## 9. Reproduction

```bash
npm ci && npm test && npm run build && npm run lint && npm run format:check && npm run knip
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets --features mcp -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace --no-fail-fast
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --features mcp

# Integrated simulation (compare against a P6.1 build of the same test; use arrays in zsh, not strings)
P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release --locked --manifest-path src-tauri/Cargo.toml \
  -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture --test-threads=1

# Careers with the market (the real registry; never committed)
I1_WORLD=gate I1_SEED=1 I1_SEASONS=10 I1_WORLD_DB=$HOME/apfootballmanager/data/world.db \
  cargo test --locked --manifest-path src-tauri/Cargo.toml --lib \
  commands::registry::combined_career::i1_combined_career -- --ignored --nocapture      # I1_SIGNINGS=0 is the control

# Imported world
cargo run --release --locked --manifest-path src-tauri/Cargo.toml -p fm24-adapter --bin apfm-import -- \
  $HOME/apfootballmanager/data/world.db <out-dir>
APFM_WORLD_JSON=<out-dir>/apfm-fm24-2026.json cargo test --release --locked --manifest-path src-tauri/Cargo.toml \
  -p season-harness --test fm24_career --test fm24_competitions --test fm24_free_agents --test fm24_global_market \
  --test fm24_loans --test fm24_ratings --test fm24_rosters --test i1_fm24_reload -- --include-ignored
```

A zero-test selection exits 0; always read the `test result` line for a non-zero count.
