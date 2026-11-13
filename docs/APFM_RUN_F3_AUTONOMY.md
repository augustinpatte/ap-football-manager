# Run F3 — autonomous clubs, real staff delegation, long-term careers

Branch `claude/run-f3-autonomy`, created from the verified F2 head `e80c60d5` (a clean tree, equal to
`origin/claude/run-f2-gameplay`). Nothing was merged into `integration/gameplay-runs-a-d` or
`apfm/main`; no production save, production world database or other repository was touched. The
revision-6 FM24 world used for the real-data checks is a copy generated in the session scratchpad.

This report says what was built, what was measured, what failed on the way, and what is not done. It
does not claim Football Manager parity.

## 1. Where F3 started

* **F2 was secured.** Clean tree, pushed. The user asked for a complete post-fix workspace run of F2
  that had not been confirmed. It was run on a separate worktree pinned to `e80c60d5` with the real
  revision-6 world: **109 targets, 4,303 passed, 0 failed, 56 ignored**, including `fm24_loans` and
  `fm24_global_market`.
* `integration/gameplay-runs-a-d` is not an ancestor of F2 only because it holds the merge commit of
  F1 (`822215a3`); F2 already contains F1's content. F3 stays on F2's lineage and merges nothing.
* F2's transfer and loan negotiation (delayed registration, player consent, loan registration) and the
  M12 replay interface are **unchanged** by F3.

## 2. Acceptance criteria (written before the work) and where they stand

| # | Criterion | Status |
|---|---|---|
| A1 | Manual, Custom and Full produce different daily play, shown by a test on the real day path | **Met** (`the_modes_differ_where_a_major_decision_is_involved`: four modes, four outcomes) |
| A2 | In Full the assistant controls training, tactical preparation and team selection | **Met** (new Tactics duty; training and the eleven already ran; all traced in the history) |
| A3 | The manager can override any delegated responsibility at any time, and is never overwritten | **Met** for training, the eleven and the tactical plan (mutation-checked); see §4.1 for the scope |
| A4 | Delegated actions respect authorization and money limits | **Met, unchanged**: the existing `authorize` gate; the new duties spend nothing |
| A5 | Every executed action is in the history with why; none is invented | **Met** for the new duties; "result" is not linked to match outcomes (§7) |
| A6 | AI clubs have different, state-derived strategies that change what they buy | **Met** for youth focus and means (measured); **not shown** for ambition (§4.2) |
| A7 | Development reads playing time and injuries without moving the world's average | **Met** (§4.3) |
| A8 | Non-playable leagues are cheap but not frozen; comparable logic | **Met with a recorded divergence** (§4.4) |
| A9 | 30 days at once equal 30 days singly, with F3 features in play; save/reload carries on identically | **Met** (`f3_fast_forward.rs`, 3 tests) |
| A10 | Performance measured before and after | **Met**; cost is +2 to +4 % per day on the 440-club world (§6) |
| A11 | Old saves load | **Met** for the delegation state (new duties completed from the closest old one), the season stats (`serde(default)`) |
| A12 | Rust, MCP, frontend, lint and format checks run and reported honestly | See §5 |

## 3. Audit of what Runs A–F2 left (Phase 0)

* **Delegation** had nine duties with Manual / Approval / Full / Custom derived from them, spending
  ceilings (`authorize`), proposals, a 120-entry history, and actors for the eleven, match
  preparation, scouting, youth, staff hiring and renewals, player renewals and recruitment. Training
  also ran daily, but wrote to the history only when it switched to recovery, and **nothing stopped
  the assistant overwriting what the manager had set**. There was **no tactical duty** (tactics were
  only advice), and no actor for the inbox.
* **AI clubs** buy through one market sweep: appeal and rating gain weighted by geography, with
  reach, need, budget, squad-floor and consent gates. Every club ranked a target the same way.
* **Development** had per-player traits (early or late bloomer, how well a player holds his level), a
  slower curve for goalkeepers, position offsets, and daily training gains scaled by age and staff.
  **Minutes and injuries played no part.**
* **Dormant leagues** (outside the followed scope) are resolved by scoreline only. They keep standings,
  the weekly market, contracts, youth intake and ageing, but **recorded no appearances or minutes**:
  in the generated 440-club world only 8 % of senior players outside the followed league had any
  minutes, and the mean was 1.3 appearances a season.
* **Player career decisions** already include letting a contract expire, an age-dependent expected
  contract length, and a move-openness score. Audited, not changed.

## 4. What was built

### 4.1 Delegation that acts, yields and records (commit `97c324d3`, `361e1387`)

* **Override without hooks.** The assistant remembers what each duty last wrote (training plan, the
  eleven, the tactical plan) as a saved fingerprint. If the club's setting no longer matches, the
  manager changed it, through any door (screens, MCP, a reload): the duty stands back for a few days
  (7 for training, 3 otherwise) and the history says so, marked "left to you". Handing the duty over
  again takes it back. For the eleven, a player who left the club is not mistaken for a manager's
  decision. Files: `domain/src/delegation.rs`, `ofm_core/src/staff_system/delegation_control.rs`.
* **Tactics duty** (new). The day before a match the assistant sets the plan the club's style, the
  squad's legs and pace, and recent results call for, **the same function the weekly review gives AI
  clubs** (`ai_tactics::plan_for_club`). An assistant rated under 45 does not read the squad, so he
  may ask a tired side to press; the same assistant always plans the same way.
* **Training** now records intensity changes and recovery orders in the history (not the daily
  rotation of the focus).
* **Offers duty** (new, reject-only). A capable assistant turns down an offer for a player who is not
  on the transfer list when it is under three quarters of the club's own fair price, and an offer for
  a first-eleven player unless it is at least 30 % above that price. A poor assistant only
  recognises an insult (under half) and does not protect starters. He **never accepts, counters or
  lists**. Offers for listed players stay with the manager.
* **Old saves.** A state saved before a duty existed gets its setting from the Lineup duty when it
  loads, so a "Full" save is still Full and a "Manual" one delegates nothing.
* UI: the two duties appear in the delegation panel; history strings for override, tactics, training
  intensity and both offer answers are translated in all 12 locales, in each file's register.

### 4.2 Club strategy (commits `01017093`, fix `1ec43109`, see §8)

A club's profile is **derived from its state, never stored**: where its reputation puts it in its own
league (moved one step by last season's finish), what it can spend against what its squad is worth
(or an overstretched wage bill), and how young and well housed its squad is. The market reads it in
two places only: the buyer's ranking of options it already considered, and a cap on how much of its
cash a short-of-money club puts into one player unless it has a hole to fill. Who may be signed, the
player's consent, the price and F2's registration are untouched.

Measured on the FM24 world over a season with every league active (`measure_club_strategies.rs`):

| Group | Clubs | Buys/club | Loans/club | Mean age | Signings aged ≤ 22 |
|---|---|---|---|---|---|
| Wealthy | 25 | 2.64 | 5.20 | 24.6 | 38 % |
| Comfortable | 151 | 1.74 | 3.62 | 24.7 | 39 % |
| Constrained | 64 | 1.16 | 1.78 | 24.8 | 32 % |
| Least youth-built third | 81 | 1.80 | 3.90 | 25.3 | **29 %** |
| Most youth-built third | 81 | 1.35 | 2.58 | 24.4 | **43 %** |
| Smallest third by reputation | 81 | 2.49 | 4.06 | 24.6 | 37 % |
| Biggest third by reputation | 80 | 1.18 | 1.10 | 25.1 | 33 % |

Against a run with the profile switched off, youth-built clubs' share of young signings goes from 34 %
to 43 % and constrained clubs buy about a third fewer players (1.53 → 1.16) and borrow more
(1.36 → 1.78). No club ends below the squad floor. Small clubs already borrowed four times as much as
big ones before F3.

**Tried and dropped because the numbers did not support them:** shopping for loans before buying
(no effect: loans are limited by wage room, not by order), and the claim that contenders sign fewer
prospects (40 % against 37 %: not distinguishable). The contender weight stays in the code as a
bounded tilt, covered by a unit test, **but a season of the real world does not show it**.

### 4.3 Development with minutes and injuries (commit `b46e3f64`)

* A season's development reads the player's minutes and his injured days (a new `injured_days` on the
  season stats, `serde(default)`, counted daily where recovery already ticks). Growth moves with
  playing time, most for the young (a teenager who never plays grows 18 % less than the table, a
  starter 27 % more); a season lost to injury costs growth and speeds decline.
* **Centred on the typical senior**: on the FM24 world a senior plays 40 % of a starter's minutes on
  average (median 379, a bimodal spread). A first, one-sided version pulled the world's mean rating
  down 0.3 to 0.4 points in eight seasons; the two-sided one moves it by 0.1.
* Only senior squad members are judged on minutes, and only where their club's matches were
  recorded. Academy players and clubs with no record develop as a typical season would.
* In a full simulated season, among players of the same age band and room to grow, those who played
  gain **+0.59 rating points** more than those who barely played (+1.2 for 16–18, +0.7 for 19–21,
  +0.1 for 22–23). The raw difference is only 0.11 because players who play are nearer their ceiling;
  the test compares inside bands.
* Existing per-player variety (early or late bloomer, resilience, goalkeeper and position offsets) is
  unchanged, so identical players still diverge.

### 4.4 Non-playable leagues (commits `b46e3f64`, `60145bca`)

Dormant leagues stay scoreline-only, as the product decision asks (cheap, not detailed), with one
extension: each club's likely eleven is credited with an appearance and ninety minutes per fixture.
Without it the rule in §4.3 would have treated a whole world as benched. The result of every fixture
is unchanged (tested), goals, ratings, condition and injuries are still not simulated there.

Over five seasons of the gate world (`world_evolution.rs`) the dormant and followed halves move
together in squad age (−2.5 against −2.8 years), keep every squad above the floor and trade alike
(19.0 against 17.6 arrivals per club). **They differ in rating improvement by 1.5 points** (the
dormant half improves more; 1.8 before the playing-time effect existed): the cheap tier has no
injuries or fatigue, and the followed league includes the passive user club. This is a recorded
divergence, bounded by the test at 2.5, not a solved one.

## 5. Tests and verification

New tests (all were seen failing for the predicted reason first unless noted):

| Area | Tests |
|---|---|
| Delegation modes and history | `delegation_control` (7), `delegation_offers` (8), domain (5), db (2), `measure_delegation_modes` (+1) |
| Override | the eleven and training overrides, **mutation-checked** (fail with the rule disabled) |
| Strategy | `strategy` unit tests (9), `measure_club_strategies` (real FM24 world) |
| Development | `development` (5 new, mutation-checked), `aging` (3), injured-days counter, `world_development` (2) |
| Dormant tier | `dormant` (4 new): crediting, injured players skipped, results unchanged, and a two-competition day equal to two separate calls |
| Equivalence and reload | `f3_fast_forward` (3), `world_evolution` (1) |
| Frontend | `StaffManagementPanel` (+1), the i18n gates |

Run results:

| Check | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean, both |
| `cargo test --workspace --no-fail-fast`, with the real revision-6 FM24 world | **113 targets: 4,357 passed, 0 failed**, 56 ignored |
| `cargo test --workspace --features mcp --no-fail-fast`, same world set | **113 targets: 4,443 passed, 0 failed**, 56 ignored (F2's MCP run had no world, so its real-data tests were vacuous; this one's were not) |
| Frontend `npm test` | **327 files / 2,888 tests passed**, 1 file (4 tests) skipped, as in F2 |
| `npm run lint`, `format:check`, `knip`, `build` | clean |
| i18n gates (`localeCoverage`, `frontendKeyCoverage`) | pass, 12 locales |

Not run: the desktop application, `scripts/apfm/run-m12-tests.sh` (F3 does not touch the M12 integration;
F2's run of it stands), and the five project reviewer agents.


## 6. Performance (fast-forward benchmark, 440-club world, mean of three runs, ms per game day)

| Window | F2 head | F3 | Change |
|---|---|---|---|
| 7 days | 53.3 | 55.5 | +4.1 % |
| 30 days | 55.5 | 57.6 | +3.8 % |
| 90 days | 55.1 | 56.2 | +2.0 % |

Each run asserts the fast-forward ends in the same world as the days played singly.
Delegation itself costs about 0.1 ms/day (6.2 → 6.3 on the 80-club gate world, Manual → Full).

The first version of the dormant playing record cost +3.7 ms/day: it scanned every player once per
dormant competition per day. It was found by switching each addition off in turn, and restructured so
the day is the unit (one grouping pass, credits by position); that is why the table above is +2 to
+4 % and not +7 %. `club_strength` used to scan every player twice per dormant fixture; that is gone
from the dormant path.

## 7. Limitations and what is not done

* **The history says what and why, not always "with what result".** Outcomes are executed / approved
  / declined / left to the manager. A decision is not linked to the match it prepared.
* **Overrides cover three duties** (training, the eleven, the tactical plan). The other duties act
  on information the assistant derives each time, or through proposals the manager answers.
* **Offers duty is reject-only**, and in 120 days of the gate world no offer for the manager's players
  arrived, so real play shows no executed entries; unit tests cover it. Loan offers (15 waiting in
  that run) and rival-interest notices (7) stay manual.
* **Ambition has no measured effect** on a season of buying (§4.2). Free-agent signings by AI clubs
  were not given a strategy.
* **Dormant leagues** have no goals, ratings, injuries, fatigue or morale, so they have no top
  scorers and run 1.5 rating points ahead of the followed league over five seasons (§4.4).
* **Contract promises, injuries decisions and player conversations** remain manager-only; the
  delegation audit's other gaps (live in-match decisions in M12) are unchanged.
* The player-career items (refusing extensions, leaving on a free, age-dependent contract length) were
  audited as already present and not extended.
* No desktop session was run; the visual check of the two new duty rows is the component test only.

## 8. Corrections to earlier reports and to this branch

* **The F2 delegation audit said the 22 to 27 waiting "Transfer" items were incoming offers for the
  manager's players. They are 15 loan offers and 7 rival-interest notices.** No permanent offers
  arrived in those 120 days.
* **Commit `01017093` (club strategy) went out with four failing `ofm_core` integration tests**
  (`transfers_tests`): its profile treated a zero wage budget as over budget and a squad with no
  value as having no cash. Only the library tests had been run before committing. The fix is its own
  commit `1ec43109` (it now reuses the market's `is_overstretched_at`), and a unit test pins the second case.
* The global-market test's border-share bar moved from one half to 55 %: six seeds give 45.2 to 50.4 %
  in F3 and five give 47.0 to 49.3 % in F2 (mean 48.2 % both). The old bar failed on a seed at the top
  of normal spread. This is a threshold change backed by measurement, not a behaviour change.
* `start_the_agreed_loan` (F2) moves the clock without playing the days between; two loan tests still
  use it. Unchanged here.

## 9. Recommended next run

1. Link a decision to its consequence in the history (the match a lineup or tactical plan prepared).
2. Give dormant clubs a minimal injury and fatigue model so their rating improvement matches the
   followed league's, and attribute goals to the likely eleven for top-scorer tables.
3. Loan offers and rival-interest notices for the manager's players as delegated decisions, with a
   proposal flow for the ones that spend or sell.
4. Extend overrides to the remaining duties, and an explicit "take back control" action.
5. Look at free-agent and loan strategy for constrained clubs, where the profile has the least effect.
6. Compress or thin the M12 replay (78 MB a match, from F2) and integrate live M12 decisions, the two
   largest open items for the match screen.
