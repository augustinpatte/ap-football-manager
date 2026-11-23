# APFM REALISM3 — market economy, AI transfer realism and domestic fixture calendars

Audit of the REALISM3 mission on the FM 2026/27 import. Everything below was measured on the real
imported world, built from the owner's `world.db` **read-only** into scratch directories; the source
database's SHA-256 (`36b6908c…27439f7`) was identical before and after every step. Numbers are from
the harness examples listed in section K; none is estimated by hand.

Scope reminder: this repository is public. No personal save, source database, installed world or
private export is part of any commit. Player names appear only for the probes the DATA2 audit already
named.

---

## A. Baseline and final commits

| | |
|---|---|
| Baseline (DATA2, pushed) | `79176b0deb1e123b5d0c6ae8897489cca2b22f40` |
| Value continuity | `8b8419c1` fix(economy): carry market values through seasons on the world's own scale |
| Incoming transfer interest | `4b9fc8e5` fix(transfers): approach a star on his ability, not only on his bargain signals |
| Domestic calendars | `185b7642` feat(calendar): league rounds fall across the week by country pattern |
| Calendar serialization, gate fixes | `fix(calendar): an absent weekday pattern serializes as before; gate fixes` |
| Review fixes (tests that could fail, one league-fixture entry point) | `test(calendar): pin window wiring, opener, quotas and appeal field; one league-fixture entry point` (`6b84a0aa`, the final code commit: every gate in K was run on it) |
| This document | the commit that follows `6b84a0aa`; the final SHA is in the report that accompanies the push |

---

## B. Root cause of the missing incoming offers

Reproduced on the real world: a month into a Paris Saint-Germain career, no offer for any player.

For a **user's** player the sweep kept a candidate only if his score reached 35, and the score
(`bids::incoming_interest_score`) was made of *bargain signals*: listed 30, contract under a year
10–40, value over a million 20, low morale 10. A healthy, unlisted star on a long contract could reach
20, so **no buyer ever looked at him**. Of 2,691 PSG player-days scored in 90 days, 98% were dropped
before any buyer weighed them; the five best players (OVR 87–93, 144–161 M) were among them. It was
not a price, budget or interest problem: those checks never ran. (The same sweep's AI-owned targets
were judged on need, price and books; only the user's players were shortlisted by the bargain score
alone.) The market also does not sweep before about 1 July (day 30 of the run), so "a month in" is by
design a month before any club shops.

Fix (`transfers/interest.rs`, `transfers/market.rs`): four questions kept apart.

1. **Appeal** — how visible a player is from what he *is* (ability and credited ceiling, against the
   world's senior players; the top fifth is visible, veterans fade). Owned by `AppealBench`.
2. **Opportunity** — the old bargain score, unchanged.
3. **Buyer interest and affordability** — per buyer, and now for a user's player exactly as for an AI
   club's: a need, a worthwhile gain, a price within the buyer's own ceiling, budget, cash, wage room
   and books.
4. **Seller willingness** — the user's player is the user's to sell (an offer is made, the user
   answers); an AI seller's goes through `willing_to_sell`.

Unchanged protections: one club per player, the daily cap, cooldowns, clubs already in talks, duplicate
pending offers, the inbox volume. A measurement-only **funnel** (21 stages) records where candidates
were excluded.

---

## C. Before / after: the market, by club, seed and window

`realism3_market_bench`: four clubs, three seeds, 90 days through the production day path; the manager
answers nothing, so each offer is counted where it arrives. Permanent offers received, per seed.

| Club (profile) | Day 60 before | Day 60 after | Day 90 before | Day 90 after |
|---|---|---|---|---|
| PSG (flagship, 159 M budget) | 0 / 0 / 2 | **14 / 12 / 15** | 4 / 5 / 3 | **31 / 26 / 31** |
| Nantes (ordinary first division) | 11 / 8 / 6 | 12 / 9 / 7 | 30 / 14 / 12 | 32 / 15 / 19 |
| Cannes (smaller, no budget) | 9 / 9 / 0 | 9 / 9 / 0 | 17 / 22 / 0 | 17 / 22 / 0 |
| Palermo (financially constrained) | 6 / 2 / 2 | 6 / 2 / 2 | 9 / 6 / 2 | 9 / 6 / 2 |

* AI-to-AI permanent transfers by day 90 (PSG 35/29/43 → 34/30/41; Nantes 46/60/49 → 46/60/52; Cannes,
  Palermo identical): the AI market is untouched.
* Distinct PSG players offered for by day 90: 3/3/1 → 7/7/8.
* Offers' fee over value: 0.93–1.0. Invalid offers (free, from his own club, beyond the buyer's means),
  duplicate pending offers, players owned twice: **0** in every run.
* Day 30 has no sweep for any club (the window opens about 1 July).
* Funnel at day 90, PSG (seed 1): of 33,406 buyer–player pairs weighed, 19,789 had no sporting reason,
  8,976 priced above what the buyer would pay, 3,899 hit the inbox throttle, 329 had no starting need,
  156 failed the books test, 113 could not carry the wage, and 31 became offers. Before, 1,866 pairs
  were weighed (almost all of PSG's players never reached a buyer).
* Unlisted flagship stars and no-realistic-buyer cases: the funnel shows both. PSG's top five are
  reached by buyers on appeal alone; a player no buyer can afford or needs ends at `rejected_price` /
  `rejected_no_interest` and produces no offer, as it should.

3-season economy run (`apfm_economy`, PSG, seed 1): events (AI transfers 36/86/142 → 35/98/131),
extensions' wage change (median +18% both), free-agent signings' bill-over-budget (median 100% both),
fee over value (median 0.83–0.87 both), integrity violations 0, season time 29–44 s both. Negative-cash
clubs at a season end: 1 → 2 in two seasons (noted, not chased).

---

## D. Value, fee and wage over several seasons

### D.1 What happened before (reproduced)

The active world's players keep their value for the whole career (**0.0%** of 4,807 club players change
value in five seasons — see F.1). The discontinuity was at the **boundary of the external registry**
(the ~81,000-player pool that feeds promotions, free agents and newgens). A player who left the
registry for the active world, or whose registry state was projected to the next season, was
**re-priced from the native formula**: stars lost four fifths of their value, ordinary players gained
a hundredfold.

Registry value on the first projected season (`realism3_value_continuity`, 69,301 players):

| Rating band | Value after ÷ value before, median (p10–p90) |
|---|---|
| 40–49 | ×198 (×52–×298) |
| 50–59 | ×144 (×53–×325) |
| 60–69 | ×52 (×17–×121) |
| 70–79 | ×7.3 (×1.8–×24) |
| 80–89 | ×0.53 (×0.18–×1.3) |
| 90–99 | ×0.31 |

Wages did the same (median ×35 at 50–59, ×15 at 60–69, ×4.4 at 70–79). Over the registry as a whole,
5 seasons: p50 value 48.5 k → 4.14 M, **max 78 M → 15 M** (the best player in the registry lost 80%);
share of players whose value more than doubled in the first season **97%**; elite (≥ 20 M) value ratio
p50 **0.27**.

### D.2 After

Value is **carried**, not recomputed: it keeps its place on the world's own scale and moves by that
scale's ratio between the state the player was in and the one he is in (`value_scale.rs`,
`ValueScale::carry`). The scale is fitted from the DATA2-valued players of the loaded perimeter, written
into the registry (`meta value_scale`) and read identically by the world build and the career cache
(a real-source test proves the two give the same value for the same player). A native world carries no
scale and keeps the native formula.

| Registry, 5 seasons, 66,650 players | Before | After |
|---|---|---|
| p50 value, season 1 / 3 / 5 | 4.14 M / 4.28 M / 4.27 M | **60 k / 73 k / 86 k** (source 48.5 k) |
| p90 value, season 1 / 5 | 6.9 M / 7.2 M | 677 k / 849 k (source 616 k) |
| max value, season 1 / 2 / 5 | 15.1 M / 15.2 M / 14.6 M | **92.6 M / 142 M / 137 M** (source 78 M) |
| share more than doubled, season 1 | 97% | **1.2%** |
| share more than halved, season 1 | 0.4% | 7.9% |
| elite (≥ 20 M) value ratio p50 | 0.27 | **0.99** |
| Median value ratio, rating 60–69 | ×42 | **×1.00** |
| Median value ratio, rating 70–79 / 80–89 | ×5.6 / ×0.40 | **×0.92 / ×0.88** |

Newgens take `ValueScale::value`: a prospect enters at the price of players like him in that world.
Probe players (Mbappé, Rashford, Mainoo, Tielemans, Godts, Moscardo) keep their DATA2 values through
projection and activation (`data2_named_players`, 10 real-source tests, green under revision 9).

### D.3 What is not flat

* **Wages of ordinary registry players** now start from the source wage and move by the pay scale's own
  ratio, but they are clamped at a 500 floor and a 2 M ceiling: p50 586 → 500 (the floor), and p90/p99
  drift **down** 22–52% / 5–30% over seasons 1–3 as contracts are renewed at the formula's wage. Before:
  p90 ×4 in one season. This is a large improvement and not a flat line (F.3).
* **Value is not a function of a club's money** (requirement D): a valuation no longer reads any
  budget; the affordability ceiling that bent values (DATA2) is removed. A buyer's capacity is judged
  per buyer in the sweep (`rejected_budget`, `rejected_wage`, `rejected_books`).

---

## E. Resolution of the FM-scale / native-scale discrepancy

The imported FM world is on a scale about a tenth of the native formula's at the top (a 90-OVR player is
~110 M, the formula says ~12 M) and more than a hundred times *below* it for ordinary players (a 50-59 OVR is ~18 k,
the formula says ~2.7 M at 55). The generated players, newgens, free-agent renewals and the native fixtures use
the formula. Two scales in one game were the bug; the choices were a global multiplier or a world-owned
scale. A multiplier cannot work: the ratio is ×0.3 at the top and ×200 at the bottom. So:

* The scale belongs to the **world** (anchors by rating at prime age, age factors, credited ceiling by
  age), fitted from its own DATA2-valued players (≥ 5 per anchor, ≥ 20 per age; the same fitter in the
  world build and the registry), written into the registry's `meta`, and applied to generated
  fillers too (`reprice_generated_players`).
* It is **versioned** (`VALUE_SCALE_VERSION`); a stored scale of another version, or none, falls back
  to the native formula, so old saves and native worlds are unchanged.
* `WORLD_REVISION` is 9 (8 for values; 9 for calendars). An installed world of an earlier revision is
  reported outdated and left alone; nothing replaces it but an explicit install.
* No OVR manipulation, no hardcoded price, no second competing formula: `native_value` is the same
  function it always was and is the scale's fallback.

---

## F. Remaining anomalies (confirmed unless marked)

1. **P2, confirmed, pre-existing, all worlds: values of the active world never change.** No production
   code revalues a player after he is created or projected. Five seasons on the real world: 0.0% of
   4,807 club players changed value; a 33-year-old whose rating fell from 87 to 77 is still worth
   121 M. Fees therefore depend on a frozen number. A correct fix needs a persisted valuation basis per
   player (rating, ceiling and age at his last valuation) and a migration so that `carry` can be applied
   every season; that is a new mechanic and not part of this mission. The projection boundary no
   longer loses or inflates value (section D); the frozen value does not.
2. **P2, confirmed: the market does not sweep before about 1 July.** Day 30 of a 1 June start has no
   offers for anyone.
3. **P2, confirmed: wage tail drift** described in D.3.
4. **P2, confirmed: 2,123 contracts (21% of 10,157; 26% of the weekly bill) start after the 1 June
   career start** (1,162 on 13 July, 542 on 1 July; lengths 0.5–8 years, median 3). The save is dated
   mid-July and the adapter's start date is a configured choice; a contract's start date is kept as FM
   has it. Effect: nothing in the game reads the start date to gate wages, so those players are paid
   from day 1; before their dates that is about **3.0% of one year's wage bill** in the world over the
   six weeks. The movement ledger records no signing for them. Not fixed: moving the career start would
   move the whole preseason and window calendar and invalidate every benchmark.
5. **P2, confirmed: rest under three days in the national-team qualifiers** (`International/LeagueTable`,
   from the second season: 434 and 325 pairs at seasons 2 and 3). Identical in the baseline world
   (434 / 325), **zero among clubs**, so not a club-calendar defect and not introduced here.
6. **P2, confirmed: the effective Monday/Tuesday share is below its authored weight** (2% for Serie A
   against 10%). The planner guarantees the minimum rest *per round*, so a day within three days of the
   previous round's latest day is not offered; after a Sunday the next Monday and Tuesday are lost.
7. **P2, confirmed: a league fixture is sometimes moved by the existing booking reconciliation** to
   clear a cup tie (Ligue 1: Monday 0% authored and 12% by the fourth season). The
   reconciliation keeps the rest guarantee; the pattern is not enforced on moved fixtures.
8. **P2, confirmed: the season span.** A pattern season ends earlier than the real one (Ligue 1 on
   25 April against a real mid-May finish) because at most six pauses (international weekends) are
   inserted; the window end is an upper bound.
9. **Unverified: a graphical check.** No actual GUI claim is made; the Tauri window was not operated in
   this environment. The calendar screens read fixture dates from the same data the audit reads.

10. **P2, confirmed (architecture review): a career cache advanced under the old formula is not
    repaired** when the world is reinstalled at the new revision (the cache is keyed on seed, start
    season and source SHA). A career already in progress keeps the values it was given, as DATA2
    states; new careers are unaffected.
11. **P2, confirmed (architecture review): no validation of a league's window.** A `season_end_*` that
    leaves too few weeks makes the planner place the last rounds past it; all twelve shipped windows
    were measured to end on or before their date, and nothing checks a modder's.
12. **P2, confirmed: `CalendarMetadata.matchday_gap_days` still says 7 for a patterned league**, which
    is no longer how it is dated. Nothing reads it for scheduling.
13. **P2, confirmed: size.** `evaluate_transfer_market` is ~948 lines and `market.rs` is 1,458 of its
    1,500-line ceiling (both pre-existing and slightly worse); `project_from` is at 100 lines.

No P0 or P1 defect is open on the scope of this mission.

---

## G. Calendar distributions for the 12 divisions

Measured on the real world after the pattern (`realism3_calendar_audit`, seed 1, three rollovers,
every division of the world, league fixtures only; friendlies appended to the league's list are
excluded). The "before" run is the legacy code path on the same world (no pattern), the "after" run is
the world rebuilt with the generated patterns. **The dates are generated; they are not the official
fixture lists** (which are licensed data not used here).

Weekday columns are Monday / Tuesday / Wednesday / Thursday / Friday / Saturday / Sunday, as a
percentage of the division's fixtures. "One weekday only" is the number of clubs that play every league
match on the same weekday.

#### Before: legacy cadence, 2026/27

| Division | Clubs | Rounds | Mon / Tue / Wed / Thu / Fri / Sat / Sun (% of fixtures) | Midweek (Tue-Thu) | Mean share of a club's season on its commonest weekday | Clubs on one weekday only | First round to last fixture |
|---|---|---|---|---|---|---|---|
| Premier League | 20 | 38 | 0 / 0 / 0 / 0 / 100 / 0 / 0 | 0% | 1.00 | 20 | 08-21 → 2027-05-07 |
| Championship | 24 | 46 | 0 / 0 / 0 / 0 / 100 / 0 / 0 | 0% | 1.00 | 24 | 08-14 → 2027-06-25 |
| League One | 24 | 46 | 0 / 1 / 0 / 0 / 0 / 99 / 0 | 1% | 0.99 | 10 | 08-15 → 2027-06-26 |
| Ligue 1 | 18 | 34 | 0 / 0 / 0 / 0 / 97 / 0 / 3 | 0% | 0.97 | 2 | 08-21 → 2027-04-09 |
| Ligue 2 | 18 | 34 | 0 / 0 / 0 / 0 / 0 / 97 / 3 | 0% | 0.97 | 0 | 08-08 → 2027-03-27 |
| National (Ligue 3) | 18 | 34 | 0 / 0 / 0 / 0 / 0 / 97 / 3 | 0% | 0.97 | 0 | 08-01 → 2027-03-20 |
| Bundesliga | 18 | 34 | 0 / 0 / 0 / 0 / 100 / 0 / 0 | 0% | 1.00 | 18 | 08-28 → 2027-04-16 |
| 2. Bundesliga | 18 | 34 | 2 / 0 / 0 / 0 / 0 / 98 / 0 | 0% | 0.98 | 6 | 08-01 → 2027-03-20 |
| Serie A | 20 | 38 | 0 / 0 / 0 / 0 / 0 / 100 / 0 | 0% | 1.00 | 20 | 08-22 → 2027-05-08 |
| Serie B | 20 | 38 | 0 / 0 / 0 / 0 / 100 / 0 / 0 | 0% | 1.00 | 20 | 08-21 → 2027-05-07 |
| LaLiga | 20 | 38 | 0 / 0 / 0 / 0 / 0 / 100 / 0 | 0% | 1.00 | 20 | 08-15 → 2027-05-01 |
| LaLiga 2 | 22 | 42 | 0 / 0 / 0 / 0 / 98 / 2 / 0 | 0% | 0.98 | 4 | 08-14 → 2027-05-28 |

#### After: generated patterns, 2026/27

| Division | Clubs | Rounds | Mon / Tue / Wed / Thu / Fri / Sat / Sun (% of fixtures) | Midweek (Tue-Thu) | Mean share of a club's season on its commonest weekday | Clubs on one weekday only | First round to last fixture |
|---|---|---|---|---|---|---|---|
| Premier League | 20 | 38 | 2 / 0 / 8 / 3 / 7 / 53 / 29 | 11% | 0.53 | 0 | 08-21 → 2027-05-23 |
| Championship | 24 | 46 | 2 / 0 / 13 / 2 / 12 / 63 / 8 | 15% | 0.63 | 0 | 08-14 → 2027-05-02 |
| League One | 24 | 46 | 2 / 10 / 7 / 1 / 10 / 69 / 2 | 18% | 0.69 | 0 | 08-15 → 2027-05-01 |
| Ligue 1 | 18 | 34 | 0 / 0 / 9 / 1 / 18 / 30 / 42 | 10% | 0.44 | 0 | 08-21 → 2027-04-25 |
| Ligue 2 | 18 | 34 | 2 / 0 / 12 / 0 / 25 / 48 / 14 | 12% | 0.48 | 0 | 08-08 → 2027-04-11 |
| National (Ligue 3) | 18 | 34 | 0 / 0 / 9 / 0 / 18 / 61 / 13 | 9% | 0.61 | 0 | 08-01 → 2027-04-11 |
| Bundesliga | 18 | 34 | 0 / 0 / 15 / 0 / 8 / 58 / 19 | 15% | 0.58 | 0 | 08-28 → 2027-04-25 |
| 2. Bundesliga | 18 | 34 | 2 / 0 / 12 / 3 / 23 / 39 / 22 | 15% | 0.39 | 0 | 08-01 → 2027-03-28 |
| Serie A | 20 | 38 | 2 / 0 / 8 / 3 / 8 / 27 / 53 | 11% | 0.53 | 0 | 08-22 → 2027-05-23 |
| Serie B | 20 | 38 | 2 / 0 / 11 / 3 / 15 / 43 / 27 | 13% | 0.44 | 0 | 08-21 → 2027-05-16 |
| LaLiga | 20 | 38 | 2 / 0 / 11 / 0 / 8 / 34 / 46 | 11% | 0.48 | 0 | 08-15 → 2027-05-16 |
| LaLiga 2 | 22 | 42 | 1 / 0 / 10 / 2 / 14 / 40 / 33 | 12% | 0.42 | 0 | 08-14 → 2027-06-06 |

#### Before: legacy cadence, fourth season 2029/30

| Division | Clubs | Rounds | Mon / Tue / Wed / Thu / Fri / Sat / Sun (% of fixtures) | Midweek (Tue-Thu) | Mean share of a club's season on its commonest weekday | Clubs on one weekday only | First round to last fixture |
|---|---|---|---|---|---|---|---|
| Premier League | 20 | 38 | 13 / 76 / 0 / 3 / 8 / 0 / 0 | 79% | 0.76 | 0 | 08-21 → 2030-05-07 |
| Championship | 24 | 46 | 11 / 78 / 0 / 2 / 9 / 0 / 0 | 80% | 0.78 | 0 | 08-14 → 2030-06-25 |
| League One | 24 | 46 | 11 / 4 / 73 / 2 / 9 / 1 / 0 | 79% | 0.73 | 0 | 08-15 → 2030-06-26 |
| Ligue 1 | 18 | 34 | 16 / 71 / 0 / 3 / 10 / 0 / 0 | 74% | 0.71 | 0 | 08-21 → 2030-04-09 |
| Ligue 2 | 18 | 34 | 18 / 0 / 71 / 3 / 9 / 0 / 0 | 74% | 0.71 | 0 | 08-08 → 2030-03-29 |
| National (Ligue 3) | 18 | 34 | 18 / 0 / 74 / 3 / 6 / 0 / 0 | 76% | 0.74 | 0 | 08-01 → 2030-03-20 |
| Bundesliga | 18 | 34 | 6 / 82 / 0 / 3 / 9 / 0 / 0 | 85% | 0.82 | 0 | 08-28 → 2030-04-16 |
| 2. Bundesliga | 18 | 34 | 6 / 0 / 83 / 3 / 8 / 0 / 0 | 86% | 0.83 | 0 | 08-01 → 2030-03-20 |
| Serie A | 20 | 38 | 3 / 0 / 87 / 3 / 8 / 0 / 0 | 89% | 0.87 | 0 | 08-22 → 2030-05-08 |
| Serie B | 20 | 38 | 3 / 87 / 0 / 3 / 8 / 0 / 0 | 89% | 0.87 | 0 | 08-21 → 2030-05-07 |
| LaLiga | 20 | 38 | 0 / 0 / 89 / 0 / 11 / 0 / 0 | 89% | 0.89 | 0 | 08-15 → 2030-05-01 |
| LaLiga 2 | 22 | 42 | 0 / 76 / 14 / 0 / 7 / 3 / 0 | 90% | 0.76 | 0 | 08-14 → 2030-05-28 |

#### After: generated patterns, fourth season 2029/30

| Division | Clubs | Rounds | Mon / Tue / Wed / Thu / Fri / Sat / Sun (% of fixtures) | Midweek (Tue-Thu) | Mean share of a club's season on its commonest weekday | Clubs on one weekday only | First round to last fixture |
|---|---|---|---|---|---|---|---|
| Premier League | 20 | 38 | 14 / 1 / 6 / 4 / 11 / 43 / 21 | 11% | 0.43 | 0 | 08-24 → 2030-05-24 |
| Championship | 24 | 46 | 5 / 4 / 12 / 2 / 10 / 60 / 7 | 18% | 0.60 | 0 | 08-17 → 2030-05-03 |
| League One | 24 | 46 | 12 / 8 / 6 / 2 / 9 / 60 / 2 | 16% | 0.60 | 0 | 08-17 → 2030-05-03 |
| Ligue 1 | 18 | 34 | 12 / 0 / 9 / 2 / 22 / 26 / 29 | 11% | 0.35 | 0 | 08-24 → 2030-04-28 |
| Ligue 2 | 18 | 34 | 10 / 0 / 11 / 1 / 26 / 43 / 9 | 12% | 0.44 | 0 | 08-10 → 2030-04-14 |
| National (Ligue 3) | 18 | 34 | 10 / 0 / 8 / 1 / 18 / 52 / 10 | 9% | 0.52 | 0 | 08-03 → 2030-04-14 |
| Bundesliga | 18 | 34 | 6 / 0 / 15 / 0 / 10 / 55 / 15 | 15% | 0.55 | 0 | 08-31 → 2030-04-28 |
| 2. Bundesliga | 18 | 34 | 7 / 0 / 10 / 6 / 29 / 33 / 16 | 15% | 0.38 | 0 | 08-03 → 2030-04-01 |
| Serie A | 20 | 38 | 10 / 0 / 8 / 4 / 15 / 24 / 39 | 12% | 0.39 | 0 | 08-24 → 2030-05-24 |
| Serie B | 20 | 38 | 7 / 0 / 11 / 2 / 19 / 41 / 22 | 12% | 0.41 | 0 | 08-24 → 2030-05-19 |
| LaLiga | 20 | 38 | 4 / 0 / 8 / 3 / 13 / 33 / 39 | 11% | 0.43 | 0 | 08-17 → 2030-05-19 |
| LaLiga 2 | 22 | 42 | 6 / 0 / 16 / 0 / 17 / 36 / 24 | 16% | 0.37 | 0 | 08-17 → 2030-06-07 |

Monthly density (league fixtures per month, season 2026/27):

- Ligue 1, before, fixtures per month: 26-08: 18, 26-09: 36, 26-10: 45, 26-11: 36, 26-12: 36, 27-01: 45, 27-02: 36, 27-03: 36, 27-04: 18
- Ligue 1, after, fixtures per month: 26-08: 18, 26-09: 36, 26-10: 32, 26-11: 40, 26-12: 36, 27-01: 45, 27-02: 27, 27-03: 36, 27-04: 36
- Premier League, before, fixtures per month: 26-08: 20, 26-09: 40, 26-10: 50, 26-11: 40, 26-12: 40, 27-01: 50, 27-02: 40, 27-03: 40, 27-04: 50, 27-05: 10
- Premier League, after, fixtures per month: 26-08: 20, 26-09: 30, 26-10: 47, 26-11: 43, 26-12: 40, 27-01: 50, 27-02: 30, 27-03: 50, 27-04: 31, 27-05: 39
- Championship, before, fixtures per month: 26-08: 36, 26-09: 48, 26-10: 60, 26-11: 48, 26-12: 48, 27-01: 60, 27-02: 48, 27-03: 48, 27-04: 60, 27-05: 48, 27-06: 48
- Championship, after, fixtures per month: 26-08: 36, 26-09: 60, 26-10: 71, 26-11: 61, 26-12: 72, 27-01: 72, 27-02: 60, 27-03: 60, 27-04: 49, 27-05: 11

Reading the tables:

* **Before**, the legacy cadence keeps the weekday of the opener: every club of the Premier League,
  the Championship, Serie A, Serie B, LaLiga and the Bundesliga plays every match on one weekday. It
  also **drifts with the calendar**: the opener is a fixed month-and-day, so by the fourth season
  Ligue 1 and the Premier League play 71–76% of their matches on a Tuesday, Serie A on a Wednesday
  (the weekday of 21 August moves one day a year).
* **After**, no club plays one weekday all season, in any season; the commonest weekday carries 35–69%
  of a club's matches on average (no club uses fewer than four weekdays in the first season); midweek rounds are 9–18% (the 46-game divisions need the most) and **are added
  only as the season window requires, above the pattern's minimum**.
* Round-robin pairing is intact: every ordered pair meets exactly once, home and away differ by at
  most one, each round has every club once (checked on all 12 divisions in every snapshot).

### G.1 Calendar rules

| League | Pattern (weight Mon…Sun) | Midweek | Season window end |
|---|---|---|---|
| England, 1st | 1·0·0·0·1·12·6 | Tue/Wed, ≥ 4 | 24 May |
| England, 2nd and 3rd | 0·0·0·0·2·14·1 / 0·0·0·0·1·15·0 | Tue/Wed, ≥ 8 | 3 May |
| France, 1st | 0·0·0·0·2·3·4 | Wed, ≥ 4 | 17 May |
| France, 2nd / 3rd | 1·0·0·0·3·4·1 / 0·0·0·0·3·8·1 | Tue–Wed / Wed, ≥ 4 / ≥ 3 | 17 May / 24 May |
| Germany, 1st / 2nd | 0·0·0·0·1·6·2 / 0·0·0·0·3·4·2 | Tue/Wed, ≥ 5 | 17 May / 24 May |
| Italy, 1st / 2nd | 1·0·0·0·1·3·5 / 1·0·0·0·2·4·3 | Wed, ≥ 4 / ≥ 5 | 24 May / 31 May |
| Spain, 1st / 2nd | 1·0·0·0·1·3·4 / 1·0·0·0·2·4·3 | Wed, ≥ 4 / ≥ 5 | 24 May / 7 June |

These live in `config/adapter.toml` (`[[calendar.league]]`), per country and tier; a league the
configuration does not name keeps the legacy cadence. Nothing about the scheduler is specific to a club
or a league: the planner is `ofm_core::schedule_pattern`, and its input is the `MatchdayPattern` stored
on the league's calendar (`SeasonCalendar.matchday_pattern`, `serde(default)`).

### G.2 How the scheduler works

* The pairings are unchanged (`schedule::round_pairings`, the circle method, legs reversing home and
  away). Only the dates change.
* `plan_rounds` gives each round the weekdays it may use. An ordinary round uses its week's weighted
  days; a **midweek** round (spread evenly, never in the first two or last two rounds, never next to
  another) shares its week with the round after it; **spare weeks** in the window (up to six) become
  international pauses; a round's earliest day is never less than three days after the previous round's
  latest day, so **no club can play twice inside the world's minimum rest by construction**.
* `allocate_dates` shares a round's fixtures over its days by exact largest-remainder quotas; which
  fixture gets which day is decided by a hash of its own key, so the same club list always gives the
  same calendar and a club's weekday varies round to round.
* The window end is the first occurrence of its month-and-day on or after the opener (an end before the
  opener's month and day belongs to the following year). The plan keeps every fixture on or before it.
* No pattern (or an all-zero one) is the legacy calendar exactly (a test compares every id, date and
  pairing).
* The pattern is stored with the league, so **renewal keeps it**: `regenerate_league_for_season` reads
  it from the league's calendar. A save written before the field existed has no pattern and renews on
  the legacy cadence.
* Continental competitions: the planner dates one league table only; a continental layer books its own
  dates through the same minimum-rest rules (`calendar_booking`), which this change does not touch.

---

## H. Example: the Paris Saint-Germain league calendar, no licensed data

Ligue 1, seed 1, season 2026/27, generated (the opener is 21 August).

Before: 34 league matches, **Friday 33 times and Sunday once**: `Fri 21-08, Fri 28-08, Fri 04-09,
Fri 11-09, Fri 18-09, Fri 25-09, Fri 02-10, Fri 09-10, Fri 16-10, Fri 23-10, Fri 30-10, Fri 06-11…`

After: Friday 3, Saturday 9, **Sunday 19**, Wednesday 3 of 34 matches:
`Sun 23-08, Sun 30-08, Sun 06-09, Sun 20-09, Sun 27-09, Wed 30-09, Sat 03-10, Sun 11-10, Sun 25-10,
Sat 31-10, Sat 07-11, Sat 14-11…`

The weekday sequence is a property of the league's pattern and the club's pairings, not of the club:
nothing in the code names it, and every other club of the division has a different sequence over the
same days.

---

## I. Multi-season, fixture integrity and performance

* Three full seasons through the production day and rollover path, seeds 1 and 2: every division keeps
  every pairing exactly once (**12/12 in all snapshots**), home/away imbalance ≤ 1, **0 double
  bookings, 0 club fixtures with rest under three days** (the shortest club gap is 3), and no more than
  3 matches in any club's 7-day window (the legacy cadence reaches 2 in the first season and 3 later;
  the midweek rounds are what makes 3 possible early). No fixture duplicated or skipped across a
  rollover; a postponed fixture count of 0 (none occurs in the run).
* The first season's calendar is **identical across seeds** (12/12 divisions); later ones differ only
  because promotion and relegation put different clubs in the league (9/12, then 7/12 identical).
* Ids: a fixture's id is derived from its competition, matchday, date and pairing and is stored with
  the fixture, not recomputed on load.
* Performance: the three-season, all-division audit takes 96 s on the world with the patterns and 96 s
  on the same world without them (288 clubs in scope). The 440-club generated-world benchmark was **not
  re-run**: a league with no pattern takes the legacy path (the circle method moved into a shared
  function that produces the same fixtures; a test compares every id), so no change is expected there,
  and this is listed as unmeasured in L.
* Completed fixtures are never moved: the scheduler builds a new season, and the reconciliation is
  unchanged.

---

## J. Save compatibility, source integrity, world revision

* **Source:** the owner's `world.db` has SHA-256 `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7`
  before and after the whole mission; it is only read.
* **World revision 9.** The installed world is not touched; a world of an earlier revision is reported
  outdated and replaced only by an explicit install (tested).
* **Old careers:** `SeasonCalendar.matchday_pattern` is `Option` with `serde(default)`; a save
  without it loads and renews on the legacy cadence. Values carry no new serialized field on `Player`
  or `Game`; the scale lives in the registry's `meta` (an older registry without it reads as the native
  scale).
* **DATA2 guarantees under revision 9:** `data2_named_players` (10 real-source tests: provenance, no
  invented histories, loan owners and estimated terms, Godts as an academy player, scouting masks,
  no duplicate players), `realism3_value_scale` (3 real-source tests: world/registry scale equality),
  and `synthetic_world` all pass.
* **Future-dated contracts:** investigated in F.4.

---

## K. Tests, mutation checks and reviewers

All gates run sequentially on the final code, one workspace build at a time, in the existing
`src-tauri/target` (no second worktree, no concurrent compilation).

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (default) | pass |
| the same with `--features mcp` | pass |
| `cargo test --locked --workspace --no-fail-fast` (default) | **5,445 passed, 0 failed**, 148 ignored |
| the same with `--features mcp` | **5,532 passed, 0 failed**, 148 ignored |
| `npm run format:check`, `npm run lint` (Biome) | pass |
| `npm run knip` | pass |
| `npm run build` (tsc + vite) | pass |
| `npm test` (Vitest, includes `src/i18n` coverage) | 374 files passed, 1 skipped; 3,284 tests passed, 4 skipped |
| `npm run audit:i18n` | advisory, exits 0; its candidates are the existing MCP-markdown strings (out of scope by the root rule); no player-facing string was added in this mission |
| fm24-adapter real-source tests (`APFM_WORLD_DB`, `--include-ignored`) | 144 lib + 10 `data2_named_players` + 3 `realism3_value_scale` + 14 `synthetic_world`: all pass |

The 148 ignored tests are the real-source and measurement studies that need the owner's database or
take minutes; the real-source ones were run by hand above. **Without `APFM_WORLD_DB` the real-source
tests return early and pass vacuously**; their result counts only when the variable is set.

**Mutation checks (disposable edit of the one fix, test must fail, then restore):** thirteen runs on the
calendar work, all killed: rest guarantee removed, window filter removed, hash ordering removed,
legacy path bypassed, renewal hook bypassed, build hook bypassed, the opener day removed, the break logic disabled, the break cap
lowered, the window end dropped at the shared builder (killed in both the renewal and the build test),
the quota remainder order reversed, the 29 February clamp changed; and three on the new tests of the
value work (wage ratio skipped, appeal field filter removed, the unchanged-state guard removed). Two
tests were rewritten when a mutation survived (the carried-wage test and the opener test): the first
version could not fail.

**Reviewers** (read-only agents on `79176b0d..HEAD`; they were told not to compile):

* `ofm-architecture-reviewer`: **no P0, no P1**. P2: `evaluate_transfer_market` is ~948 lines (894
  before this mission, +53, in a file now at 1,458 of 1,500 production lines); `project_from` is exactly
  100 lines; an impossible window silently overshoots (nothing validates `season_end_*`);
  `median_ln` panics on an empty slice if a config sets `min_per_anchor = 0`; a career cache advanced
  under the old formula is not repaired on reinstall (keyed on seed, start and source SHA only);
  `carry`'s doc disagreed with the code (**fixed**); `native_value` could be `pub(crate)`;
  `matchday_gap_days` still reads 7 for a patterned league; `AppealBench::new` per sweep not measured.
* `ofm-test-reviewer`: six P1 findings (tests that could not fail). **Fixed:** the opener test, the
  window wiring at the build and renewal sites, the adapter pattern assertions (whole pattern, and a
  populated config as the negative control), the carried-wage test, the appeal-field filter. P2 fixed:
  remainder quotas, the midweek count, the break count, club-count assertions before the loops, a
  four-figure value for the unchanged-state test and a post-plateau age transition. **Not fixed:** the
  expiring-contract wage paths under a calibrated scale and the unpriced-newgen branch are covered only
  by the real-source tests; the `career.rs` scale read has no non-ignored test; no golden fixture id
  pins the legacy calendar to its pre-change literal (the test compares the legacy and patterned entry
  points to each other); odd club counts and the all-zero midweek fallback have no case.
* `ofm-dedup-reviewer`: **no P1**. Fixed: the pattern/window resolution written twice (now
  `schedule::build_league_fixtures`), the hard-coded two legs on a patterned renewal (now the stored leg
  count), the audit's literal 3 (now `calendar_booking::MIN_GAP_DAYS`), the shortlist bar's literal 35
  (now `VISIBLE_APPEAL`), a 29 February window end that vanished instead of clamping, and the editor's
  calendar type and schema reference (which did not list `matchdayPattern`). **Left, named for the
  consolidation epic:** `round_significant` exists three times (the adapter's two copies should call
  the `ofm_core` one); the six-player probe table is hand-copied in three examples; percentile,
  Spearman and argv-flag helpers are repeated across the study examples instead of living in
  `season_harness::world_health`.
* `i18n-auditor` and `ui-accessibility-reviewer`: not run. No player-facing string, component or
  style changed (a TypeScript type gained an optional field; the adapter's Rust and TOML are out of
  scope by the root rule).

**Disk, at the end:** `src-tauri/target` 7.7 GB (debug 7.1 GB, release 661 MB), one directory, no
duplicate worktree built; the session's scratch directory 503 MB, which holds three 57 MB scratch
worlds and the study binaries and is outside the repository.

---

## L. Remaining defects, separated

**Confirmed, not fixed (all P2):** F.1 (active-world values are never revalued, pre-existing),
F.2, F.3, F.4, F.5 (national-team qualifiers' rest, pre-existing and identical at baseline), F.6, F.7,
F.8, F.10–F.13, and the test and consolidation items listed under the reviewers in K.

**Unverified:** a graphical check of the calendar and inbox screens (no display or assistive access
here; **no GUI claim is made**); a three-seed run of every club (the benchmark covers four contrasting
clubs and three seeds, the calendar audit two seeds); the 440-club generated-world performance benchmark.

**Not done on purpose:** no release; no reinstall of the owner's world; no push of a history rewrite.
