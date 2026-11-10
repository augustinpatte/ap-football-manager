# F5.5 squad intelligence and transfer market performance

## Outcome

The F5.4 squads are now **31.5% off the instruction count they were costing** on the same
workload, measured in one binary so that no build-to-build code generation difference can
account for it, with the harness's sporting output byte-identical.

| Measurement | F5.3 | F5.4 `92b2c616` | This branch |
| --- | ---: | ---: | ---: |
| Instructions retired, 5 seeds × 2 seasons | 389.4B | 957.6B | **699.4B** |
| Ratio to F5.3 | 1.00× | 2.46× | **1.80×** |
| Per-seed simulation seconds | 4 s (rounded) | 11.5–18.0 s (load-bound) | **6.84–7.23 s, median 7.16 s** |

The rows above compare separate builds of separate trees and carry the caveat in
"Comparing two builds" below. The figure that does not depend on that caveat is the
same-binary one:

| One binary, 5 seeds × 2 seasons | Instructions | Real | User |
| --- | ---: | ---: | ---: |
| All five optimizations restored to their old code | 1024.0B | 52.1 s | 51.8 s |
| This branch | **701.7B** | **36.0 s** | **35.5 s** |
| Change | **−31.5%** | −30.9% | −31.6% |

Per seed the same binary reports 10.36/10.54/10.58/10.49/9.82 s before and
7.11/7.13/7.18/7.12/6.82 s after. That binary also carries the five measurement
switches, hence its 701.7B against the released binary's 699.4B; the −31.5% is computed
inside it because it is the only binary that can run both sides.

## Reproduction setup

- Branch `deepseek/f55-market-performance`, five commits on top of `92b2c616`
  (the merged F5.4.2–F5.4.5 sporting AI).
- `src-tauri/Cargo.toml` `[profile.release]` as committed: `lto = "thin"`,
  `codegen-units = 1`, `strip = "debuginfo"`.
- rustc 1.95.0 (59807616e 2026-04-14), macOS arm64.
- From `src-tauri/`:

```sh
F54_WORLD=gate F54_SEEDS=1,2,3,4,5 F54_SEASONS=2 \
  /usr/bin/time -l <binary> --ignored --nocapture
```

- The study writes its report to **stderr**, so the report and the `/usr/bin/time -l`
  resource block land in the same captured file. Reports are compared with the
  `… in X.XXXs` lines removed.
- The machine was shared throughout (load average 8–18). Wall and user time move with
  that load; instructions retired does not, so it is the primary metric and wall time
  is quoted as a cross-check.

## Comparing two builds: why every attribution here is one binary

The first attempt compared separate release builds. It produced results that could not
all be true: the same source measured 770.4B one afternoon and 911.4B after a rebuild
(18% apart, from a working tree whose `git diff` against the first build's commit was
empty), and builds of nearly identical trees ranged from 701B to 1024B depending on
which optimizations were present.

That is thin LTO with `codegen-units = 1`: a one-function edit changes inlining
decisions across the whole program, so retired instructions — while stable to ~0.06%
across runs of one binary — are not comparable across binaries.

Every number in "Optimizations" below therefore comes from **one binary carrying all
five changes plus an environment switch per change**, so the only difference between
two runs is the branch the switch selects. The switches (`F55_OLD_PAIR_SORT`,
`F55_OLD_LOAN_SCAN`, `F55_NO_BASE_COVER`, `F55_CLONE_CANDIDATE`, `F55_OLD_FITS`) were
measurement scaffolding and were removed before the final commit; the released code
contains none of them. The final binary measures 699.2B/699.5B, consistent with the
701.7B of the toggle binary once the switches' own branches are gone.

## Profiling evidence

`sample <pid> 7` on the release benchmark, seed 1, 2 seasons (1 ms sampling). The main
thread spends the run waiting on the harness's semaphore, so the figures are from the
study thread: 5639 samples, of which 4251 are inside `advance_one_day`.

| Top-of-stack frame | Samples |
| --- | ---: |
| `squad_intelligence::model::best_assignment` (Hungarian lineup) | 801 |
| `_platform_memcmp` + `DYLD-STUB$$memcmp` | 639 + 174 |
| `core::slice::sort::unstable::quicksort` | 400 |
| `squad_intelligence::model::year_view_from` | 189 |
| `squad_intelligence::model::SquadModel::evaluate_with` | 146 |
| `sip::Hasher::write` | 141 + 37 |
| `squad_intelligence::model::fits` | 28 |
| `squad_intelligence::model::SquadModel::fingerprint` | 43 |

The call graph localised the two biggest avoidable costs:

- `evaluate_transfer_market` (699 samples) → `try_ai_loan` (304), of which **245 + 59
  samples are inside `memcmp`**: the loan search compared the id of every player in
  the world against each loan target (see optimization 5).
- `evaluate_with` → `year_view_from` → `quicksort` (400 samples across the run): the
  cover sorted `(f32, u8, u16)` triples with a comparator that called `f32::total_cmp`
  for every comparison (see optimization 3).

`best_assignment`, `year_view_from` itself and `memcmp` outside the loan path remain the
largest frames after this work; nothing was changed there.

## Optimizations

Each row is measured in one binary over five seeds and two seasons. "Cost of the old
code" is what the run costs with that one switch flipped back, everything else
unchanged.

| Commit | Change | Old code costs | Share of 701.7B |
| --- | --- | ---: | ---: |
| `14197500` | `Member::candidate` reads the `&Player` instead of cloning it | +29.1B | +4.1% |
| `14197500` | `fits` rates each distinct formation slot once | +7.2B | +1.0% |
| `f18566d1` | Reuse the squad's own year view when a signing cannot cover | +27.7B | +3.9% |
| `8187142a` | Sort the cover's pairings as packed `u64` keys | +129.6B | +18.5% |
| `d4ece824` | Fingerprint in pieces; reuse the stored squad hash | (in the total) | — |
| `fff29ee0` | Read a loan target in place instead of scanning the squad | +69.1B | +9.8% |

The five switches together cost 322.3B (1024.0B against 701.7B); the individual
figures do not add up to that because the cheap-cover and cheap-loan paths change how
much of `year_view_from` the market calls at all.

### 1. Read a signing without cloning the player (`14197500`)

`Member::candidate` cloned the whole `Player` — a struct with several `Vec`s and
`Option`s — only to clear `active_loan`, because `Member::of` refuses loaned players,
then dropped the clone. A market sweep builds a candidate for every listed player and
every interested club, so this was a heap allocation per candidate. The function now
reads the `&Player` directly and builds the same member from the same fields in the same
order; loaned players are still accepted (a loan is what the club could end by signing
him) and every other player is rejected exactly as before.

`fits` also rates a slot once per distinct formation position: a formation names two
centre backs and two strikers, and a slot's rating does not depend on where it sits.

Covered by a test that a loaned candidate and the same player without a loan produce
the same member, and a test that repeated slots share their fit.

### 2. Keep the squad's own view when a signing cannot cover (`f18566d1`)

`evaluate_with` rebuilt the year view, including the cover, for every candidate and
every horizon year. Most listed players are no better than the cover the club already
has. When the improving chain is `Unchanged`, every slot is already covered, and the
newcomer loses every pairing, the year *is* the squad's own view already computed at
build time: the view is copied instead of rebuilt, and the newcomer's squad vector is
only built when a year needs a full solve.

Covered by an oracle test (`evaluate_with_full`) that compares both paths bit for bit
over 80 random squads and candidates, and by a counter test that the shortcut is taken.

### 3. Sort the cover's pairings as packed keys (`8187142a`)

The cover picks the best bench-to-slot pairings first, so it built a
`Vec<(f32, u8, u16)>` of `bench × 11` entries and sorted it with
`b.0.total_cmp(&a.0).then(…)`. `total_cmp` is branchy and ran on both operands for
every comparison of every sort of every year of every candidate.

The pairings are now one `u64` each: the level's `f32` bits mapped to the integer order
`total_cmp` defines, inverted so a higher level sorts first, with the slot above the
bench position in the low bits. The tie-break (level descending, then slot, then bench
position) is therefore the plain `Ord` of the key, and the level is read back from the
member rather than carried in the pair.

This is the largest single win: **18.5%** of the run. It is checked against the
pick-the-best-again oracle over 60 squads whose levels are thirds, so both fractional
levels and exactly equal levels occur.

### 4. Fingerprint without a buffer (`d4ece824`)

`SquadModel::fingerprint` and `fingerprint_of` copied every field into a `Vec<u8>`
before hashing it, one allocation per call, and `ClubSquad::sales` recomputed the squad
fingerprint even though `ClubSquad` already stores it.

`StableHasher` feeds the same bytes to FNV-1a in pieces, so no buffer is built; the byte
order is unchanged, which is why every fingerprint keeps its value and every cached
answer stays valid. The pinned "a save file can rely on this number" test passes
untouched. `sales` reads the stored fingerprint.

This one was not separately switched: it is part of the 1024.0B → 701.7B total and its
own share is below the resolution of the benchmark.

### 5. Read a loan target in place (`fff29ee0`)

An AI club with nothing to buy walks the loan targets and, for each, searched the whole
player list by id. The targets are collected once anyway, from the same list, so each
target now also records the place its player holds in `Game::players`. The sweep appends
players but never reorders them, so the place still names him; the id is checked anyway
so a reordering could never land a loan on the wrong player.

Covered by a test that every target names both the place and the id of its player and
that the best target comes first.

## Sporting equivalence

Not "no obvious change": the study's entire report is identical.

- **Before and after, same binary:** the 507-line report of the five-seed, two-season
  study is byte-identical between all five switches on and all five restored, and
  between each single switch and the optimized path.
- **Before and after, across the work:** the final branch's report is byte-identical to
  the report of the pre-change F5.4 binary, saved before any edit, line for line
  (everything except the `… in X.XXXs` timings).
- Market participation is unchanged, e.g. seed 1:
  `teams 9638 | positional need 377 | squad need 7450 | loan need 9170 | shopped 4162 |
  review day 1368 | closing need 1243 | whole market 474` — the same counts before and
  after, so the same clubs entered the market for the same reasons and the same loans
  and transfers were made.

The report covers squad quality, XI fit, weak slots, keeper shortages, permanent
transfers, loans, unused recruits, expenditure and financial health for each of the five
seeds at the points the study records.

## Regression tests added

Every optimization ships with a test that fails if the equivalence it relies on stops
holding, in the file it changes:

- `a_loaned_player_is_read_as_the_signing_he_would_be` — a loaned candidate equals the
  same player without the loan.
- `repeated_slots_share_their_fit_without_changing_it` — two centre backs are rated
  once with the same result.
- `the_unchanged_cover_shortcut_answers_exactly_as_the_full_evaluation` — 80 random
  squads, bit for bit against the full path.
- `a_signing_below_the_whole_cover_reuses_the_squads_own_view` — the shortcut is taken.
- `fractional_and_equal_cover_ratings_keep_the_pairing_order` — 60 squads with thirds,
  against the pick-the-best-again oracle.
- `writing_bytes_in_pieces_hashes_them_as_one_slice` — the hasher equals
  `stable_hash`; `the_answer_is_pinned_to_a_number_a_save_file_can_rely_on` still pins
  the value.
- `a_loan_target_names_the_place_and_the_id_of_its_player` — the place and id agree and
  the best target is first.

## Checks run

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check          # clean
cargo clippy --manifest-path src-tauri/Cargo.toml -p ofm_core --all-targets   # clean
cargo test  --manifest-path src-tauri/Cargo.toml -p ofm_core             # lib 2063 passed, 4 ignored;
                                                                         # all 44 integration targets ok
cargo test  --manifest-path src-tauri/Cargo.toml -p ofm_core --test transfers_tests  # 143 passed
cargo build --release -p season-harness --tests                          # benchmark binary
```

Not run: the full workspace suite, the frontend suite, `npm run build` and the MCP
feature pass. This branch changes no frontend, IPC, MCP, schema or save format, so the
Tauri command and UI gates were out of scope.

## Remaining limitations

- **The cross-build comparison to F5.3 is indicative, not exact.** The same source
  rebuilt measured 18% apart, so "2.46× → 1.80×" could move by several points in either
  direction. The −31.5% is the figure that holds, because it is one binary.
- `evaluate_transfer_market` is ~900 lines and remains the hot function: the next
  candidates are `best_assignment` (the Hungarian solve, 801 of 5639 samples) and
  `year_view_from` itself. Both were left alone deliberately — the assignment is what
  makes the lineup a manager's rather than a picker's, and changing it is a sporting
  change, not an optimization.
- `MARGINAL_CACHE` is one process-global `Mutex<HashMap>` capped at 400,000 entries and
  is cleared wholesale when full. It was not touched; a per-club budget would be a
  behaviour change.
- The loan target keeps a per-target id check. It costs one string comparison per target
  against the whole-list scan it replaced, and it is what makes the stored place safe.
- The `fits` dedupe assumes a slot's rating depends only on its position, not on its
  index. That is true of `positional_fit_for_assignment` today and is asserted by the
  repeated-slot test, but it is an assumption a future slot-dependent rating would have
  to revisit.
