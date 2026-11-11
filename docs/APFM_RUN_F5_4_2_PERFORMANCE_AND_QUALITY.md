# F5.4.2 performance and quality investigation

## Decision: NO-GO, investigation incomplete

There is not yet evidence that F5.4 builds more competitive squads efficiently. The
production optimization described below is benchmarked but not fully validated.
The prior single-seed comparison and its limitations are documented in
`APFM_RUN_F5_4_FINAL_VALIDATION.md`.

## Reproduction setup

- F5.3: `ae621d1f33816efae0963288519e0a344107f2cd`, existing detached worktree
  `/Users/augustinpatte/apfm-migration/f53-baseline`. It already contained uncommitted
  squad-study instrumentation; that work was preserved.
- F5.4: `4f4fcd5feaeb73fdb98ba307c085397536264685`, detached worktree
  `/tmp/apfm-f542-f54-4f4fcd5f`. This commit follows the F5.4.1 validation commit
  `54be3a6f09864a60ca465abb2cbb669cca93395d` with a documentation-only commit.
- The squad-study source files have the same SHA-1 (`f83083eace8bad05c9153356aab4a19fbef6c57f`);
  the study test differs only in formatting. The engine source and Cargo lockfile have
  no differences between F5.3 and F5.4.1.
- Both original worktrees configure a local M12 override at
  `/Users/augustinpatte/apfm-match-lab`, HEAD `28f34b10d4fe7cadacc0030acaaa0cd1aa9fdf5f`.
  That checkout already has an edited `Cargo.toml` and untracked `LICENSE` and `NOTICE`.
  A comparison must use the same override for both builds and record its exact contents.
- Rust compiler: `rustc 1.95.0 (59807616e 2026-04-14)`.

The F5.3 measurement was started in its worktree with:

```sh
F54_WORLD=gate F54_SEEDS=1,2,3,4,5 F54_SEASONS=2 /usr/bin/time -l cargo test --release -p season-harness --test f54_squad_study -- --ignored --nocapture
```

Output is in `/tmp/apfm-f542-f53-baseline.log`. The equivalent F5.4 command used the
detached F5.4 worktree and `cargo --config 'paths = ["/Users/augustinpatte/apfm-match-lab"]'`;
its output is `/tmp/apfm-f542-f54-baseline.log`. Both completed successfully. The
command-level time and peak memory include compilation, so they are not simulation
measurements. The original harness rounded per-seed simulation time to whole seconds.

| Seed | F5.3 seconds | F5.4 seconds | XI fit F5.3 → F5.4 | Weak-slot clubs % | Unused joiners % | Fees/club-season £m |
| --- | ---: | ---: | --- | --- | --- | --- |
| 1 | 3 | 17 | 67.82 → 67.86 | 20.3 → 21.5 | 16.8 → 19.6 | 0.05 → 0.31 |
| 2 | 3 | 17 | 68.22 → 68.25 | 17.7 → 16.5 | 17.4 → 15.4 | 0.11 → 0.27 |
| 3 | 4 | 17 | 68.25 → 68.30 | 16.5 → 16.5 | 15.8 → 15.1 | 0.12 → 0.30 |
| 4 | 3 | 17 | 68.11 → 68.20 | 13.9 → 15.2 | 14.9 → 16.2 | 0.15 → 0.28 |
| 5 | 4 | 17 | 68.30 → 68.31 | 14.6 → 13.3 | 18.1 → 18.7 | 0.16 → 0.25 |

The ratio of the rounded medians is approximately 5.7; rounding prevents a precise
ratio. The harness timing was changed identically in both comparison worktrees to
print milliseconds for repeat measurements. The sampling run observed 82.9 MiB peak
physical footprint; this is a separate F5.4 process, not a matched memory comparison.

## Findings and remaining work

The listed-loan activation index in `transfers/market.rs` aggregates the best listed
player by position group without checking the prospective borrower or owner. This is
a possible source of false-positive activation, not a measured root cause.

Using the existing production buyer-turn counter and identical measurement-only
instrumentation on seed 1 of the 80-club, two-season study, F5.3 recorded 3,025
buyer evaluations, unmodified F5.4 recorded 9,568, and the current F5.4 worktree
recorded 9,637, over 656 elapsed calendar days in each case: 4.61, 14.59 and 14.69
buyer turns per calendar day respectively. The current version evaluates 3.19 times
as many buyers as F5.3. This is an average over all days, including closed windows,
and does not establish which activations had eligible opportunities. Logs are
`/tmp/apfm-f542-count-f53.log`, `/tmp/apfm-f542-count-f54-baseline.log` and
`/tmp/apfm-f542-count-f54.log`.

In a separate instrumented run of the unmodified F5.4 production sweep, 1,106 of
9,568 buyer turns (11.6%) were selected only because of the listed-loan index: the
counter compared the actual selection with the same selection predicate with that
index removed. This is not a count of ineligible loans or wasted work. The observed
increase over F5.3 is 6,543 turns, so the loan-only activations explain at most 1,106
of that difference for this seed, not the whole increase. The instrumentation lives
only in the isolated worktree; its output is `/tmp/apfm-f542-loan-only-profile.log`.
The counterfactual does not account for downstream changes to transfer caps or later
squads, so it is not an estimate of transactions that can safely be removed. A club
activated this way also runs the user's incoming-loan-offer path, whose candidate
rules are not the same as AI-to-AI loan eligibility.

A second instrumented seed-1 run recorded 908 AI-loan searches among those 1,106
loan-only buyer turns. Twenty candidates passed the existing sporting, ownership,
registration, term and affordability screens; all 20 loan executions succeeded.
Five incoming user-loan offers, three completed permanent transfers and no incoming
user-transfer offers were also recorded on loan-only turns. Some action categories
may overlap on a turn. The remaining
198 activations did not reach an AI-loan search under the sweep's normal ordering and
caps. These counts do not show that the other buyer turns were useless: they could
have eligible opportunities blocked by a cap or affect the day-wide market cap.
Instrumentation and output are isolated in the F5.4 worktree and
`/tmp/apfm-f542-loan-complete-profile.log` respectively.

A further diagnostic on the same F5.4 seed recorded 7,527 buyer turns satisfying
the urgent-need predicate, but only 84 that would not have been selected by the
other predicates. Predicate overlap matters: this is a one-predicate counterfactual,
not an estimate of downstream savings. The run overlapped other work and its
22.990-second simulation time is not a comparable timing benchmark. Its log is
`/tmp/apfm-f542-urgent-profile.log`.

An independent ten-second macOS `sample` run of the F5.4 seed 1 production harness
(`F54_WORLD=gate F54_SEEDS=1 F54_SEASONS=2`, release test binary) captured 7,929
worker-thread samples. The candidate-selection closure accounted for 3,999 samples;
3,661 were in `BuyerView::marginal`, including 2,198 in `ClubSquad::signing`'s
evaluation path. These are stack samples, not invocation counts or exact elapsed
times. The sample is saved at `/tmp/apfm-f542-sample.txt`.

A separate diagnostic build of the F5.4 comparison worktree, using the same seed and
world, counted 36,451 `ClubSquad` constructions and 799,092 calls to the full lineup
assignment. The marginal-signing cache recorded 2,313,340 hits and 1,100,175 misses.
The `SquadFacts` local sale cache recorded 5,862 hits and 84,644 misses; 20,239 of
those misses repeated a previously seen `(squad fingerprint, player ID)` pair in
the process. A matching fingerprint is a candidate for reuse, not proof that every
input is identical: this diagnostic uses the pre-correction fingerprint and the
cache-key collisions described below. The counter also ignores work outside
`SquadFacts::sale_case`. Diagnostic locking and simultaneous tests make the recorded
21.496 seconds unsuitable for a baseline or potential-savings estimate. The log is
`/tmp/apfm-f542-cache-profile.log`.

An additional instrumented repetition timed the bodies of those 84,644 sale misses
at 1.446 seconds in aggregate and the 1,100,175 marginal cache misses at 5.428
seconds in aggregate (`/tmp/apfm-f542-cost-profile.log`). These are sums of
wall-clock intervals, not independent subsystem elapsed times: they include timing
and counter overhead, can overlap other activity, and omit cache lookup, candidate
construction and other callers. The instrumented simulation took 21.970 seconds
under simultaneous workspace tests; it must not be compared with the clean baseline.

| Measured path | Calls | Aggregate timed interval | Savings supported by evidence |
| --- | ---: | ---: | --- |
| Marginal-signing cache misses | 1,100,175 | 5.428 s | The already-applied cheap-filter reorder lowered the five-seed median; further savings not measured. |
| `SquadFacts` sale cache misses | 84,644 | 1.446 s | 20,239 repeated fingerprint/player pairs suggest reuse to investigate, but collisions and variable costs prevent a safe estimate. |
| Full lineup assignments, all callers | 799,092 | Not isolated | Not measured independently; assignments are included within other timed paths. |

These rows overlap and cannot be added to calculate a subsystem total. The timings
come from a single instrumented run, not the five-seed performance gate.

The candidate filter eagerly computed sporting fit before reputation, age, financial
and positional rejection. The production change defers it until the border rule
actually needs it or a candidate reaches price appraisal. No eligibility rule or
sporting score was changed. The 15 market realism tests passed after this change;
broader behavioral validation is pending.

The optimized active worktree completed the same five-seed release study in 9.595,
9.820, 9.545, 9.481 and 8.585 seconds respectively (median 9.545 seconds). The
pre-change repeat measured 15.647, 15.905, 16.181, 16.362 and 16.657 seconds
(median 16.181), so the median reduction is 41%. All five
reported squad-summary rows match the pre-change F5.4 rows. The command and output
are in `/tmp/apfm-f542-optimized.log`; this measures the uncommitted worktree based on
`4f4fcd5feaeb73fdb98ba307c085397536264685`. The workspace formatting check passed.

The F5.3 repeat with millisecond output measured 3.601, 3.718, 4.065, 3.797 and
3.799 seconds (median 3.797). The optimized median is therefore 2.51 times this
baseline, above the performance gate. The pre-change F5.4 repeat is recorded in
`/tmp/apfm-f542-f54-precise.log`.

After the first optimization, a separate six-second sample captured 4,824 worker
samples. Of these, 722 were in the market candidate closure, 302 under the market's
sale-assessment path, and 248 under a loan-search path, including 169 in
`try_ai_loan`. Within the candidate path, 220 samples were under a sort called by
`year_view_from` while assessing cover; that is an overlapping stack count, not an
independent elapsed time or a demonstrated saving. This is not a call-count profile and these paths are sampled over a
different interval. In `try_ai_loan`, sporting appraisal preceded owner, registration
and affordability checks. A second change deferred appraisal until those checks
succeeded. Its five seed times were 9.515, 9.515, 9.558, 9.395 and 9.277 seconds
(median 9.515), a difference too small to distinguish from noise. It was reverted;
the prior ordering also avoids repeated affordability scans for sporting rejects.

The 15 market realism scenarios pass with the first change. A new independent Total-mode
generated-world test passed for one season: it checks cash, transfer budget, wages and
starting-lineup ownership daily, and requires two distinct recorded arrivals still at
the club. Review identified and prompted stronger move-count and lineup assertions;
full registration consistency, pending-negotiation save/reload and other Total-mode
requirements remain unverified.

An additional production-market scenario exposed a sporting defect: eight registered
defenders, including one unavailable for 300 days, triggered the legacy surplus veto
against a needed centre-back. The same club without that injured player signed the
same target. The buyer-interest check now counts available senior defenders, including
borrowed players, rather than counting a long-term injury as usable depth. The new
scenario and all 15 original market realism scenarios pass. In the first five-seed
repeat with the tactical and deterministic-offer changes, the times were 10.753,
10.982, 11.069, 11.549 and 10.700 seconds; all reported squad summaries matched the
previous F5.4 rows. The machine was also running other applications, so a repeat is
needed before attributing the difference from the earlier run to these changes.

Review identified a same-sweep consistency risk: the available-depth count can reflect
a departure while the planned group levels still reflect the start-of-sweep roster.
The current worktree now rebuilds current and planned levels for each buyer from its
current ownership, filters the buyer model the same way, and invalidates seller views
after successful transfers and loans. A focused stale-index buyer-view test passes;
these further changes still need integrated same-sweep tests and performance validation.
The loaned-in player treatment still differs between the surplus gate and the sporting
model. Review also found that shortlist status and prices, consent squads, and the
parent's wage bill after a loan remain snapshots and may be stale during the sweep.

The production-market scenario `a_full_back_fills_a_tactical_gap_despite_centre_back_depth`
reproduced a distinct decision defect: five strong centre-backs caused the broad
defender-group first-cover filter to reject a lower-OVR left-back, even though that
left-back could improve a weak starting slot. Disabling only that filter made the
transfer succeed; restoring it made the test fail again. The correction keeps the
cheap group filter but allows a candidate whose natural position matches a starting
need already identified by the buyer's formation-aware model to proceed to the
ordinary sporting appraisal, consent and financial checks. It does not guarantee a
purchase. The test fails with this exception removed and passes with it restored;
all 17 production market realism scenarios pass (`/tmp/apfm-f542-realism-tactical.log`).
This can increase candidate evaluations and change recruitment outcomes. A five-seed
release measurement of the eager implementation is below; no general sporting gain
is claimed from the single fixture.

Independent architecture and test review found that the exception is still preceded
by the eight-player broad-group surplus gate, matches only the exact natural position,
and does not apply to loans. The existing fixture proves a purchase, but does not
independently assert the starting-slot gain. Three additional production-market tests
for surplus, alternate position and loan cases were run separately: 18 passed and
two failed (`/tmp/apfm-f542-expanded-realism.log`). The eight-centre-back case
failed, while the alternate-position case passed in this fixture. The loan went to
another club (`team-2`), so that failure does not isolate the intended filter and
the fixture needs further investigation. They are not part of the 17-test result
above. Same-sweep loan roster and wage
snapshots, sale-view invalidation, and a deterministic dismissal/rollover SQLite
scenario also need integration coverage.

| Seed | Before tactical change: XI / weak % / unused % / fees £m | After: XI / weak % / unused % / fees £m | Before → after seconds |
| --- | --- | --- | --- |
| 1 | 67.93 / 21.5 / 21.9 / 0.30 | 67.89 / 23.4 / 20.7 / 0.36 | 10.947 → 12.324 |
| 2 | 68.16 / 15.8 / 13.7 / 0.24 | 68.18 / 16.5 / 11.6 / 0.26 | 11.232 → 12.768 |
| 3 | 68.45 / 13.9 / 15.0 / 0.22 | 68.39 / 16.5 / 13.9 / 0.24 | 11.782 → 12.612 |
| 4 | 68.19 / 14.6 / 16.8 / 0.23 | 68.20 / 13.9 / 16.4 / 0.27 | 11.048 → 12.490 |
| 5 | 68.30 / 12.0 / 20.1 / 0.33 | 68.28 / 11.4 / 21.4 / 0.33 | 10.626 → 12.183 |

The median increased from 11.048 to 12.490 seconds, about 13%; the runs were not
interleaved and timing noise is not quantified. Weak-slot prevalence and unused
joiners moved in both directions across seeds, with no demonstrated overall quality
gain. The after log is `/tmp/apfm-f542-tactical-5seed.log`. A local variant that
calculated needs only on the first group-filter rejection measured 12.286, 12.729,
13.095, 12.436 and 12.209 seconds (median 12.436) with identical summary rows
(`/tmp/apfm-f542-tactical-lazy-5seed.log`). This did not establish a meaningful
speedup, so that variant was reverted to the simpler eager implementation.

The five-seed study with the current buyer-outlook and seller-view changes measured
10.693, 11.166, 10.911, 11.358 and 10.452 seconds (median 10.911). Its results are:

| Seed | XI fit | Weak-slot clubs | Unused joiners | Fees/club-season £m |
| --- | ---: | ---: | ---: | ---: |
| 1 | 67.93 | 21.5% | 21.9% | 0.30 |
| 2 | 68.16 | 15.8% | 13.7% | 0.24 |
| 3 | 68.45 | 13.9% | 15.0% | 0.22 |
| 4 | 68.19 | 14.6% | 16.8% | 0.23 |
| 5 | 68.30 | 12.0% | 20.1% | 0.33 |

These mixed results are insufficient to establish a general sporting improvement.
After the rollover-warning correction, the same five seeds measured 10.947, 11.232,
11.782, 11.048 and 10.626 seconds (median 11.048), with identical summary rows to
the table. Buyer turns were 9,637, 9,638, 9,624, 9,586 and 9,547 over 656 calendar
days each. Output is `/tmp/apfm-f542-final-2026.log`. The later interleaved comparison
below predates the warning correction and is retained as a matched-load measurement.
An interleaved repeat against F5.3 ran each release test binary directly, one seed at
a time, alternating versions. F5.3 times were 4.727, 4.759, 5.048, 4.880 and 5.011
seconds (median 4.880); F5.4 times were 10.729, 10.981, 11.155, 10.742 and 10.363
seconds (median 10.742, ratio 2.20). All ten runs passed. F5.3 peak RSS ranged from
58.9 to 65.0 MB; F5.4 ranged from 77.8 to 80.1 MB. The median-time processes used
4.68 s user / 0.11 s system and 10.36 s user / 0.20 s system respectively; these
process timings include world generation and reporting, unlike the simulation times.
Logs are `/tmp/apfm-f542-paired-f53-<seed>.log` and
`/tmp/apfm-f542-paired-f54-<seed>.log`. Other applications were active throughout.
More detailed recruitment-value and development metrics have not been collected.

The 440-club, one-season scaling study has started with direct release binaries and
the same instrumentation. For seed 1, F5.3 took 24.283 s and F5.4 took 71.999 s;
peak process RSS was 128.6 MB and 167.9 MB respectively. XI fit was 67.40 → 67.47,
weak-slot clubs 21.9% → 20.7%, unused joiners 13.8% → 30.9%, and fees per club-season
£0.10m → £0.81m. For seed 2, F5.3 took 24.407 s and F5.4 took 69.239 s; peak RSS
was 126.3 MB and 171.2 MB. XI fit was 67.36 → 67.32, weak-slot clubs 21.4% → 23.2%,
unused joiners 12.4% → 29.6%, and fees £0.12m → £0.82m. These figures do not classify
unused players as waste or account for their arrival dates. Logs are
`/tmp/apfm-f542-scale-f53-<seed>.log` and `/tmp/apfm-f542-scale-f54-<seed>.log`.
Process CPU user/system seconds were 23.52/0.46 → 69.23/1.26 for seed 1 and
23.50/0.48 → 66.14/1.22 for seed 2; process time includes generation and reporting.
The performance and sporting gates do not justify the five- or thirty-season scaling
studies yet.

The seed-1 finance log records permanent-transfer joiners rising from 0.08 to 0.53
per club, while free-agent joiners stay near 1.05 per club. This identifies increased
permanent movement as a contributor to the larger joining cohort; it does not prove
which acquisitions were wasteful. The study's unused proxy counts current squad members
who changed club, have at most two season appearances, and are outside the selected XI;
it uses a reconstructed fielded XI rather than the saved selection, and season-wide
appearances rather than minutes since arrival; it does not distinguish late arrivals
and cover.

Cache audit: a synthetic regression demonstrated that identical candidate slot fits
and projections with different natural-goalkeeper status produced the same key even
though goalkeeper outfield coverage is capped. The named test failed at the key
assertion before the correction and passed after it. Both the candidate and squad fingerprints now
include natural-goalkeeper status. This prevents reuse across those different model
inputs; it does not establish that a collision occurred in the benchmark. Full
regression validation remains incomplete.

The subsequent `cargo test --manifest-path src-tauri/Cargo.toml --workspace
--no-fail-fast` completed with 4,678 passed, one failed, and 75 ignored tests
across 122 test targets and doc-test groups (log:
`/tmp/apfm-f542-workspace-latest.log`). The only failing target was
`squad_floor_seasons`, for the pre-existing youth contract-date assertion described
below. The four-season SQLite lifecycle and the ten-season, two-seed financial
stability scenario both passed. Two additional exploratory regression tests were
added after this run started and are not covered by its result.

The subsequent complete workspace run on the tactical implementation finished
with 4,680 passed, one failed and 75 ignored (`/tmp/apfm-f542-workspace-tactical.log`).
Its sole failed target was again `squad_floor_seasons`. The three newer market
scenarios were added after its compilation and are covered only by the separate
20-scenario run noted above.

The architecture reviewer also identified that the old one-byte shift encoding used
255 for both `None` and a projected change of -1. A second test reproduced that
candidate-key collision before correction. The candidate and squad encodings now use
signed 16-bit values with a distinct absence sentinel. Both focused cache-key tests
pass after correction. A cache-facing signing test also passes, comparing two formerly
colliding veteran projections to fresh calculations. A full cache audit and sales-cache
integration test remain open.

The five-seed release repeat after both cache corrections measured 8.745, 8.877,
8.733, 8.687 and 8.574 seconds (median 8.733, 2.30 times the F5.3 median). The
reported squad-summary rows again match pre-change F5.4; output is
`/tmp/apfm-f542-shift-fixed-bench.log`. The performance gate remains failed.

With the goalkeeper-key correction and first market optimization, the five release
seed times were 8.663, 8.992, 8.967, 8.748 and 8.611 seconds (median 8.748, or 2.30
times the F5.3 median). All reported squad summaries remained identical to the
pre-change F5.4 summaries. This is not evidence of improved sporting quality. The
log is `/tmp/apfm-f542-final-bench.log`; those timings precede the shift correction.

Frontend validation: `npm test -- --run` passed 2,920 tests in 332 files; four tests
in one file were skipped. `npm run build` and the full `npm run preflight` passed;
the latter also reported the same four skipped tests. On the implementation with the
tactical correction, `cargo check --manifest-path src-tauri/Cargo.toml --workspace`
and `cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets --
-D warnings` passed (`/tmp/apfm-f542-final-check.log` and
`/tmp/apfm-f542-final-clippy.log`). The formatting check also passed. A fresh
`npm test -- --run` passed 2,920 tests with four skipped and `npm run build`
passed (`/tmp/apfm-f542-final-frontend-test.log` and
`/tmp/apfm-f542-final-frontend-build.log`). The final workspace repeat is in progress.
The first complete Rust workspace
attempt stopped in `ofm_core` with 2,039 passed, one failed and four ignored. The
failure was pre-existing at `4f4fcd5f`: F5.4.1 raised the cross-border minimum gain
to seven, but the old unit test still expected four to pass. The unit test now checks
that four and six fail and seven succeeds; the production rule is unchanged. A full
workspace rerun is needed.

The workspace rerun passed the prior `ofm_core` unit-test failure, but stopped later
in `squad_floor_seasons`: seven tests passed and
`a_world_with_no_free_agents_and_no_academies_still_finishes_every_day` failed because
a newcomer did not satisfy its intake assertion. The same named test failed against
unmodified F5.4 at `4f4fcd5f`, on the same player ID. A diagnostic run found that
player attached to a club as a senior with contract start 2027-07-18, while the test
expects every newcomer to retain the intake date 2027-07-01 after 30 simulated days.
The reason for that later contract has not been established. The assertion is
unchanged. Later workspace tests have not yet run in this attempt.

A subsequent `--workspace --no-fail-fast` run completed with four failing targets
and 75 reported ignored tests. In addition to the
above failure, it reported `contract_single_writer::only_world_generation_stages_a_contract`
and `finance_journal::production_code_does_not_assign_finance_with_plus_equals`: both
scanners flagged pre-existing test-only modules as production. The fixture files now
declare their test-only gate at file level, and the contract guard recognises this
explicit first-line gate; a regression verifies that a later comment cannot hide live
code. The new guard test failed before the change, and both complete guard test files
pass both with `rustc --test` and with Cargo (6/6 contract guard, 4/4 finance guard).
Read-only review found no new production-write bypass in the file-level gate, but
identified a pre-existing limitation: the contract guard stops scanning after an inline
test module and can miss production code later in that file. It also recognises the new
file-level gate only in the exact leading form used here. These remain open.
The workspace run also reported
`f3_fast_forward::thirty_days_at_once_equal_thirty_days_one_by_one_with_everything_delegated`.
The same named fast-forward test also fails in the isolated unmodified F5.4 worktree;
a diagnostic comparison shows different `sales.offers[].offer_id` values for the same
action. `create_incoming_user_offer` generated those IDs with `Uuid::new_v4()`, so the
two replayed paths obtained different identifiers. A focused replay-and-uniqueness test
failed for that reason. A replacement derived ID checks existing offers and sales memory
for collisions; both replay-and-uniqueness and retained-sales-memory tests pass. All
three `f3_fast_forward` tests pass with this correction, including the previously failing
comparison. A dedicated pending-negotiation save/reload case for these IDs remains open;
other offer-creation paths still use random IDs. The other failures remain unresolved.

The next full workspace run, started before the guard fix, passed the three
fast-forward scenarios but also failed `season_lifecycle::a_career_survives_a_sqlite_reload_at_every_stage_of_four_seasons`
at season 4, comparing a news item to a message after reload. Five other lifecycle
tests passed. The same named test passed in one isolated run against unmodified F5.4
and failed again at the same stage in an isolated current-worktree run. A diagnostic
showed 1,043 unique messages before save and 1,044 after load: the load-time squad
floor repair emitted a missing warning after rollover. The current worktree now emits
the same user warning after rollover, without signing players; the four-season SQLite
test now passes in isolation. This can notify the manager earlier than the next
simulated day. Broader regression and benchmark validation after this change remain
in progress. Review identified an unresolved related case: if a manager is dismissed
at rollover, the formerly user-controlled club can still be repaired only on reload.
The shared match-day warning guard also checked fixture dates without distinguishing
scheduled from completed fixtures. A focused regression failed before the guard was
changed to consider only matches not yet completed; all 46 squad-floor unit tests
then passed (`/tmp/apfm-f542-finished-match-before.log` and
`/tmp/apfm-f542-floor-after-match.log`). The dismissed-manager edge case remains open.

Subsystem time attribution, a complete cache invalidation audit, and full delegation
integration validation remain incomplete. The two-seed one-season
scaling comparison above was completed; longer scaling studies have not been run. The required performance,
sporting, scaling, long-career and full regression gates remain open; the long-career
run should wait for the preceding gates. No final implementation commit exists.

The shared read-only architecture, test and deduplication reviewers were consulted.
They found no demonstrated behavioral regression from the filter reorder. They noted
that the pre-existing market function exceeds the configured size ceiling, that the
lazy border guard duplicates conditions in `worth_the_border` (epic #589), and that
the tests do not yet prove both reordered paths or pending-negotiation persistence.
No UI or localization surface was changed, so those specialist reviews were not run.
