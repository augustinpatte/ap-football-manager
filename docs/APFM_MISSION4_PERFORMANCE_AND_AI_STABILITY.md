# Mission 4/5 — performance, career scaling and AI stability (2026-11-20)

Branch `apfm/main`, from `585949d6` (mission 3). Nothing was pushed, reset or deleted. No serialized type
changed; no football rule changed. The historical F5.4/F5.5 timings were **not** treated as measurements of
HEAD: every number below was taken on this tree, in release builds, with the baseline measured first.

## 1. Setup (identical for every before/after number)

| Item | Value |
| --- | --- |
| Machine | Apple M4, 10 cores, 16 GB, macOS 27.0, rustc 1.95.0 |
| Build | `cargo build --release` (`lto = "thin"`, `codegen-units = 1`, `strip = "debuginfo"`) |
| Worlds | **gate**: England, Portugal, Argentina, 80 clubs, 3,742 players. **shipped**: every nation, 440 clubs, 13,865 players. Both start 2033-07-01 |
| Seeds | gate 1, 2, 3 (windows, season, 10 and 20-season careers); shipped 1 (windows, season, 3-season career) |
| Baseline code | `dcaa7512` (production code of `585949d6` plus the benchmark tooling only) |
| Optimized code | `6a6fe7a7` plus the clippy fixes of the final commit |
| Noise | desktop apps (browser, editor) used about two of the ten cores during every run; the benchmarks are single-threaded. Binaries were copied under new names right after each build (a test binary is rebuilt in place under one hash) |

The harness already existed; no second framework was written. Used as is: `apfm_ff_bench` (daily vs
fast-forward windows and a season, with the screen's copy). Added, small: an ignored benchmark
`commands::perf_baseline` for the production command paths the harness cannot reach (`advance_time`,
`advance_to_next_event`, `skip_to_match_day`, `fast_forward_to_date`), the screen's copy and its size by field,
a save and a load and one market sweep; a fingerprint hash on `apfm_ff_bench`'s output;
`apfm_career_fp` (a long career, fingerprinted per season, with the harness invariants watching);
`apfm_ai_info` (phase 4).

## 2. Baseline: where the time was (measured, shipped world, `sample` profiler)

Per simulated day, 440 clubs: **about 500 ms**. Of that, the screen's copy was small:

| Cost per day (shipped, baseline) | ms | share |
| --- | ---: | ---: |
| The day itself (`turn::process_day`) | ~480 | ~94% |
| Clone for the screen (`ui_clone`) | 5.5 | ~1% |
| Serializing it (27.7 MB of JSON) | 20–28 | ~5% |

Inside the day (17.8 k stack samples, baseline binary, inclusive):

| Function | Share of the day | Why |
| --- | ---: | --- |
| `ai_contracts::plan_squads` | 53.9% | below |
| `transfers::registration::signing_place_refusal` | **40.5%** (self + `memcmp`) | four passes over every player per call, and called once per player of every club by `graduate_overage_academy_players` |
| `transfers::strategy::profile_of` | 13.0% | read **every club's** wage bill and profile to answer for one club |
| `chrono` date parsing (`age_on`, `parse_from_str`) | 8.1% | a general format parser, per player, many times a day |
| `evaluate_transfer_market` (squad model: `signing`, `sale`, `year_view_from`) | 39% (overlaps the above) | F5.5 already optimized and caches it |

## 3. Optimizations (each pinned by a test; none changes a decision)

| # | Change | Where |
| --- | --- | --- |
| 1 | The registration gate is one pass over the world, comparing the club once per player and reading nothing else for a player who reserves nothing | `transfers/registration.rs` |
| 2 | Overage-academy graduation asks the gate only when somebody is about to graduate (it asked per player, graduate or not) | `ai_contracts.rs` |
| 3 | `profile_of` reads one club (its squad, league and `calc_wages` bill) instead of the whole world | `transfers/strategy.rs` |
| 4 | A `YYYY-MM-DD` date is read directly (`domain::age::parse_iso_date`, same answer as chrono for every input, anything unusual still goes to chrono) | `domain/src/age.rs`, `contracts/helpers.rs` |
| 5 | `improve_squad` asks "does the club have a senior place?" once for the plan and the four groups, before modelling the squad | `ai_contracts.rs`, `squad_floor.rs` |
| 6 | The Dashboard's Continue, `skip_to_match_day`, `advance_to_next_event` and the one-day command return the screen's copy (`ui_clone`: no per-season history per player, loaded on demand with the profile) like every other time command | `application/time_advancement.rs`, `commands/time.rs` |

A failed attempt was not needed: no experiment was reverted, because every change was measured to help before it
was kept. One idea was **rejected without building it**: replacing `assign_cover`'s full sort (the squad model's
`quicksort`, 12.9% self) with a partial selection. F5.5 already tuned that code, the early-exit depends on slot
conflicts, and a wrong shortcut would change decisions.

## 4. Performance table (before → after, release, identical settings)

Fast-forward wall time (`ff_ms`), gate world, **median of seeds 1, 2, 3**:

| Window | Baseline | Optimized | Change | Day-by-day with the screen's copy |
| --- | ---: | ---: | ---: | ---: |
| 7 days | 223 ms | 153 ms | **−31.6%** | 265 → 195 ms (−26.3%) |
| 30 days | 1,012 ms | 626 ms | **−38.1%** | 1,225 → 844 ms (−31.1%) |
| 90 days | 2,889 ms | 1,624 ms | **−43.8%** | 3,591 → 2,342 ms (−34.8%) |
| Full season (304 days, one rollover) | 7,804 ms | 3,544 ms | **−54.6%** | 10,797 → 6,653 ms (−38.4%) |

Shipped world, seed 1 (one round: each window is minutes):

| Window | Baseline | Optimized | Change | Day-by-day |
| --- | ---: | ---: | ---: | ---: |
| 7 days | 2.94 s | 1.46 s | **−50.3%** | 2.78 → 1.27 s (−54.1%) |
| 30 days | 15.56 s | 6.92 s | **−55.5%** | 16.83 → 8.13 s (−51.7%) |
| 90 days | 55.95 s | 27.02 s | **−51.7%** | 59.03 → 29.33 s (−50.3%) |
| **Full season, 304 days** | **145.03 s** | **42.75 s** | **−70.5%** | 157.39 → 53.09 s (−66.3%) |
| Full-season instructions retired | 7.724 T | 2.409 T | −68.8% | |

The percentage grows with the window because the late days of a season are the market-heavy ones: the
quadratic loops cost most when the AI has most to decide.

The production command paths (release test binary, `perf_baseline`, medians):

| Measure | Gate before → after | Shipped before → after |
| --- | ---: | ---: |
| `advance_time` per day | 35.3 → 24.8 ms (−29.5%) | 520.5 → 231.9 ms (−55.4%) |
| `advance_to_next_event` per day | 38.6 → 28.2 ms (−27.0%) | 473.8 → 241.6 ms (−49.0%) |
| `skip_to_match_day` per day | 37.3 → 23.0 ms (−38.3%) | 504.1 → 233.6 ms (−53.7%) |
| 30-day segment, fast-forward | 1,018 → 629 ms (−38.2%) | 16,212 → 7,328 ms (−54.8%) |
| Clone for the screen | 1.15 → 0.95 ms | 5.5 → 5.5 ms |
| Serialize the screen's copy | 5.3 → 5.4 ms | 20.0 → 20.3 ms |
| Save / load (real SQLite file) | 50.8 / 70.1 → 52.1 / 71.8 ms | 132.5 / 207.5 → 135.0 / 213.9 ms |
| Market sweep, one `evaluate_transfer_market` (warm) | 5.9 → 5.4 ms | 42.6 → 38.4 ms |
| Continue / skip response size | 7.87 / 8.01 → 7.43 / 7.55 MB (−5.6%) | 30.16 / 30.55 → 27.92 / 28.23 MB (−7.4%) |

Payload facts (not timings): the screen's copy is 7.35 MB (gate) / 27.74 MB (shipped) at the start of a career,
of which `players` is 80% (5.87 / 21.88 MB), `competitions` 12%, everything else under 3%. After **five seasons**
(gate) the full game is 11.36 MB and the screen's copy 9.25 MB: player history is 19% of the full state (5.5%
at the start), so item 6 saves more the longer the career. Parsing the shipped copy with `JSON.parse` in Node
takes 48 ms (7.4 MB: 12 ms), a proxy; the webview's own transfer was not measurable here.

### Long careers (identical seed, build and settings; `apfm_career_fp`)

| Career | Baseline | Optimized | Change | Every season's world |
| --- | ---: | ---: | ---: | --- |
| gate seed 1, 20 seasons | 225.9 s | 97.3 s | **−56.9%** | identical |
| gate seed 2, 10 seasons | 108.3 s | 46.5 s | −57.1% | identical |
| gate seed 3, 10 seasons | 108.6 s | 47.9 s | −55.9% | identical |
| **shipped seed 1, 3 seasons** | **586.7 s** | **154.4 s** | **−73.7%** | identical |

Scaling over a career is bounded: on the 20-season gate career a season costs 3.9 s (optimized) in season 1,
about 5.0 s from season 5, and 5.2 s in season 20 (baseline 8.2 s → ~11.5 s → 12.4 s). The shipped world's
three seasons were 42.6, 52.7 and 59.0 s (baseline 153, 204, 230 s): the same early rise, not measured long
enough to see whether it flattens as the gate world does.

## 5. Hotspots after the work (shipped, `sample`)

`signing_place_refusal` fell from 40.5% of the day to 2.9%; `plan_squads` from 53.9% to 9.0%; `profile_of`
from 13.0% to 2.7%; chrono's date parsing from 8.1% to 0.8%. What is left is the squad model the market asks about candidates:
`evaluate_transfer_market` 65.7%, of which `ClubSquad::signing` 29.5%, `ClubSquad::sale` 18.5%,
`year_view_from` 20.3% (the sort inside `assign_cover` is 13.7% inclusive) and `Solver::solve` 13.7%.

## 6. Simulation integrity (same seed, baseline against optimized)

The comparison is the world's **fingerprint** (`season_harness::fingerprint`: every fixture and result, every
table, every player's club, role, wage and contract end, club money, staff, managers and the inbox, one line per
fact) and the harness invariants, not a final score.

| Check | Result |
| --- | --- |
| 15 windows (gate seeds 1–3: 7, 30, 90 days and a full season; shipped 7, 30, 90) | fingerprint **identical** in every one (6,688 to 28,512 lines) |
| Shipped full season with rollover | identical (`74e967b5f1211f19`, 28,962 lines) |
| Long careers (gate 20, 10, 10 seasons; shipped 3) | the fingerprint **and** these aggregates are identical at the end of every season: clubs in debt, total cash, mean wage bill per club, number of players, mean age |
| Harness invariants (fixtures complete and not duplicated, tables equal results, registration and squad limits, contracts, progression, finances, suspensions…) | clean on both builds in every career; the report is empty |
| Fast-forward against day-by-day | identical in every window (asserted inside `apfm_ff_bench`) |

Because the optimizations change how often the world is read and not what is decided, the economic
distributions are the same by construction and the runs confirm it. The end-of-season rollover, promotion
and relegation, youth intake, ageing, contract expiry and renewals, transfers and delegated duties all run in
these careers (the gate career plays 20 rollovers).

## 7. Regressions prevented (tests; each shown to fail when the fix is removed)

| Test | Mutation that makes it fail |
| --- | --- |
| `the_one_pass_registration_gate_agrees_with_the_four_pass_original` (random worlds, every club on the edge of the limit, every player excepted in turn, three counts asserted to occur) | retired check dropped; the incoming player no longer excepted; pending loans not counted |
| `graduating_with_the_place_check_only_at_graduations_agrees_with_checking_every_player` | the place check removed |
| `a_clubs_profile_read_alone_is_the_one_the_whole_world_reading_gives` | the club's wage bill read as zero or at 9/10 (each club's budget sits one point either side of the overstretched line); another club's squad read |
| `the_direct_date_reader_agrees_with_chronos_parser_on_every_shape` | month and day swapped |
| `continuing_a_day_sends_the_screens_copy_without_player_history` | the whole game sent again |
| `a_club_whose_places_are_spoken_for_signs_no_one_to_improve_itself` (+ its control) | the place check before the improvement pass removed (also moved after the plan, or placed only before the covers loop) |
| `the_skip_and_continue_responses_send_the_screens_copy_without_player_history` | `skip_to_match_day` or `advance_to_next_event` arriving with the whole game |

Three mutations **survived** at first and exposed weak tests, which were then repaired: the registration
differential (its sampled `incoming` ids rarely held a reservation), the profile differential (no club was
overstretched, then none sat near the line, so a bill read at 9/10 passed) and the skip/continue response test
(its fixture ended in a blocker, which already sent the screen's copy). The old four-pass counts stay in the code under `#[cfg(test)]`
as the reference the fast gate is checked against.

### Reviewers (architecture, test, dedup; run on `585949d6..HEAD`)

| Reviewer | Result | Action |
| --- | --- | --- |
| `ofm-architecture-reviewer` | proved by reading the code that a failed free-agent attempt writes nothing (both `Err` returns precede every mutation), that graduation and `profile_of` are equivalent to what they replaced, and that the `ui_clone` copy drops only `career` and the `movement_history` entries; no boundary or save-format change | added a `debug_assert!` of the precondition of `sign_free_agent_for_with_a_place` (the full workspace passed with it active), `SquadCapacity::seniors_only`, and moved `perf_baseline` to `commands/` (application must not import commands). **Not done:** `perf_baseline()` is ~230 lines (test-only, ignored) |
| `ofm-test-reviewer` | the gate, graduation, improvement-guard and date tests bite (9 gate mutations, 4 guard placements and 3 date mutations all killed; a 3 M-input fuzz of the date reader found no difference); gaps: wage-bill value, three untested `ui_clone` edits, no positive control in the graduation test | all three fixed (table above). **Not done:** a test that a failed attempt leaves `Game` unchanged (argued from code only), the `profile_of` branch for a club in no league, a known-vector test of `fingerprint_hash`; the fingerprint comparison between builds is run by hand, not pinned by the suite |
| `ofm-dedup-reviewer` | `parse_iso_date` is the right canonical date reader but only two of about 130 `%Y-%m-%d` sites use it; the FNV-1a helper was copied into two examples | the helper now lives once in `season-harness/src/fingerprint.rs`. **Not done:** routing the other date sites through `parse_iso_date` (chrono's parser is 0.8% of a day after this work, so it is not worth the churn; recorded for epic #589) |

## 8. Phase 4 — do the AI clubs use hidden values in recruitment?

**Source-proven:** yes, by documented design (`squad_intelligence`: "the AI clubs read the world as it is").
`Member::candidate(player, …, 0.0)` reads the true `ovr`, the attribute-derived fits and the true `potential`
(through `projected_shifts` and `potential_gap` in the market's targeting) of any player in the world. Only the
manager's assistant reads through `perception::blur_points`.

**Measured (does it show?)** `apfm_ai_info` records every club-to-club move of a player aged 23 or under to an AI
club and compares his potential gap (`potential − ovr`) with the average gap of every senior of the same age and a
rating within three points, in the same world on the same day:

| World | Seasons | Moves compared | Mean gap vs the pool | Share above the pool |
| --- | ---: | ---: | ---: | ---: |
| gate seed 1 | 3 | 22 | −1.55 (se 1.01) | 31.8% |
| gate seed 2 | 3 | 41 | −1.28 (se 0.77) | 41.5% |
| gate seed 3 | 3 | 26 | −1.22 (se 0.78) | 38.5% |
| shipped seed 1 | 2 | 22 | −3.22 (se 0.81) | 27.3% |

If recruiting followed the hidden ceiling the difference would be positive. It is zero to negative in every
world: the AI buys young players on rating and age, and what it buys is, if anything, below the pool's ceiling.
There is **no demonstrated exploitation and no demonstrated harm**, so **no perception rule was added**: it would
change every AI decision for no evidenced benefit and would cost a hash per candidate evaluation in the code that
is the whole of the remaining runtime. Reopen it with a measurement that shows a bias, not a principle.

## 9. Unresolved performance risks

1. **The market's squad model is two thirds of a shipped day** (section 5). It is already cached (F5.5); further
   gains need a change to the model itself (an incremental or partial solve), which can change decisions and
   needs its own study.
2. **The screen is sent ~28 MB of JSON a day in the shipped world, 80% of it `players`.** Serializing it costs
   20–28 ms; the webview's receive, parse and state update were **not measured** (no GUI here). A lighter
   projection or a delta would be a contract change across the frontend.
3. **Save and load grow faster than the career.** After five gate seasons the database is 26.1 MB (6.4 MB at the
   start) and a save takes 341 ms (51 ms), a load 231 ms (70 ms). Saves are manual, not on the daily path, so this
   was only measured; 30-season behaviour is unmeasured.
4. **Per-season cost rises about 40% over the first five seasons** (gate) before flattening; the shipped world was
   run three seasons only. A 30-season shipped run (hours) was not attempted: three seasons already give strong
   comparative evidence.
5. **Memory.** `/usr/bin/time` `maximum resident set size` for the shipped world varied between runs of the same
   binary (1.74 to 2.24 GB over a season; baseline 1.74 GB) and between builds, while polled RSS peaks were equal
   (1.24 GB baseline, 1.21 GB optimized over 127 days) and the gate world's were identical to 0.2%
   (590.6 against 590.9 MB). No change adds a retained allocation, and the last build measured the baseline's
   figure; the earlier excess is attributed to when the OS compresses cold pages, **not proven**.
6. **Single machine, shared load.** Gate figures are medians of three seeds; shipped windows are one round each.
7. The `measure` examples read process-wide caches (`MARGINAL_CACHE`, `SALES_CACHE`); they are pure functions of
   their keys and were untouched.

## 10. Reproduce

From `src-tauri/`, release builds, on an otherwise idle machine:

```text
cargo build --release -p season-harness --example apfm_ff_bench --example apfm_career_fp --example apfm_ai_info
./target/release/examples/apfm_ff_bench 1 gate 3              # windows 7/30/90 and a season, daily vs fast-forward
./target/release/examples/apfm_ff_bench 1 shipped 1 season    # the 440-club season (minutes)
./target/release/examples/apfm_career_fp 1 gate 20            # per-season fingerprints, aggregates, invariants
./target/release/examples/apfm_ai_info 1 gate 3
PERF_WORLD=shipped PERF_SEED=1 PERF_ROUNDS=2 PERF_SEASONS=0 \
  cargo test --release --lib perf_baseline -- --ignored --nocapture   # command paths, payload, save, market
```

Compare two builds by their `fingerprint` fields; copy each binary under its own name right after building it,
and take a baseline from a `git worktree` with its own `CARGO_TARGET_DIR`.

## 11. Validation and final state

| Check | Result |
| --- | --- |
| `cargo test --workspace` (code state `3240b436`) | 5319 passed, 0 failed (135 ignored: displays, the nightly, the long studies) |
| After the last test additions: `-p domain -p ofm_core --lib`, root `--lib`, root `--lib --features mcp` | 130 + 2408, 620, 707 passed, 0 failed |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean |
| `cargo fmt --check` | clean |
| Frontend | not touched by this mission (no `src/` change), so not re-run |
| Benchmarks | section 4, one quiet-machine set per build; fingerprints of the final build equal to the baseline's on the gate windows and season |

**Not verified:** the desktop app was not run, so the webview's cost of ~28 MB a day is unmeasured; the shipped
world was run for three seasons only; the machine was shared with desktop applications; the shipped-world memory
figures are inconclusive (section 9).
