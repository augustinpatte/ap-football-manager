# APFM EUROPE4 — Modern European club competition engine (2026/27 league-phase architecture)

**Verdict: GO for the independent audit, with the limits in section M.** The league-phase engine is
implemented, integrated with the existing competition, match, calendar, persistence, rollover, archive
and UI paths, and proven over multiple seasons and seeds in a synthetic world. It is **not** wired
into the shipped world (real participants are INTEGRATION5), the GUI was not operated, and three
regulation components are stated approximations (C1, M). It does not claim full UEFA compliance.

## A. Baseline and final commits

* Baseline `1a44a90bb53b7623fe4c9bbaa4a7211c9d517c49` (`apfm/main`, local == origin, clean tree at the start).
* Commits on top (oldest first):
  * `a3742d6d` feat(engine): decide extra time and penalties on the aggregate of a two-legged tie
  * `eace368d` feat(league-phase): UEFA-style league phase and two-legged knockout format
  * `ce0445e5` feat(ui): league-phase standings, knockout bracket and overview
  * `12b52e93` test(season-harness): synthetic European season with three 36-club competitions
  * this document (the final commit, whose SHA is the one reported with the push)

The sections below are the mission's A–N. Sections B0–C0 were written before any engine code (the
Step 1 / Step 2 gate); the rest record evidence.

---

## B0. Step 1 — the architecture map (what exists today)

One competition type is `domain::league::League` (alias `CompetitionState`), carried in
`Game.competitions`. `Game.league` is a **legacy mirror and working buffer** (the turn loop moves a
competition into it for the day, then moves it back); it is never authoritative.

| Concern | Where it lives today | What a new format must plug into |
|---|---|---|
| Competition definition | `ofm_core::generator::competition_def` (`CompetitionDefinition`, `FormatDef`, `validate_definitions`, `build_explicit_competition`); `domain::league::{CompetitionRules, CompetitionFormat}` | a new `CompetitionFormat` variant + an authored `leaguePhase` block, validated explicitly (no silent fallback) |
| Creation / participants | `build_explicit_competition`, `world::plan` / `world::foundations` (first edition), `end_of_season::berths::continental` (rollover entrants), `cup_entrants` | entrants are re-read at rollover; field size is kept from `participant_ids.len()` |
| Fixture generation | `schedule::{build_round_robin_fixtures_with, seed_knockout_round, ...}`, `group_stage` (snake-seeded **round-robin groups**) | the league phase is neither: needs its own generator (draw → matchday colouring → dates) |
| Match processing | three result writers: `turn::post_match::apply_match_report_in` (live, watched, unwatched), `catchup::apply_simulated_result` (dormant / scoreline), `world_cup` (national teams, separate). Exactly-once guard: `post_match::fixture_is_open` | one dispatch function called by the first two |
| Standings | `StandingEntry::record_result` + `League::sorted_standings` (points, GD, GF, authored tiebreakers incl. head-to-head) — **does not model** away goals, opponent aggregates, discipline or coefficient | a dedicated table derived from the fixtures |
| Stage transitions | groups → knockout in `group_stage::process_completed_fixture`; knockout rounds in `schedule::advance_knockout_round_with` (**single-leg only**: winner = `Fixture::advancing_team_id`, which is wrong for a leg of a two-legged tie) | a new flow module; the old advance function already returns early for formats it does not own |
| Extra time / shootout | `matchday::fixture_allows_extra_time` → `League::is_knockout_fixture` → `engine::LiveMatchState.allows_extra_time` (extra time when level **in this match**) | the engine must know the first-leg score so a second leg is level on **aggregate** |
| Calendar | `calendar_booking` (3-day rest, `Layer::Continental` outranks league and domestic cup), `calendar_reconcile` (daily sweep, forward-only moves, round gating by `knockout_rounds`), `national_team` blackout windows, `knockout_dates` | continental fixtures are already the highest club layer; the sweep needs a leg-order constraint |
| Completion / archive | `competition_completion::verify_completed_edition` (strict proofs per format), `edition_archive`, `domain::edition_archive::CompletedEdition` | a proof for the new format; archive the final ranking |
| Rollover | `end_of_season::{is_regenerated_at_rollover, rollover}`; `turn::cup_renewal` (domestic cups only, deliberately) | new arm: a league-phase edition is regenerated only when it is `Completed` (or empty); an unfinished edition is retained |
| Persistence | `db::repositories::competition_repo` — one JSON column per `League` field (positional SQL, migrations `vNNN`, `MIGRATION_COUNT`) | one new nullable JSON column + migration |
| Scope | `Game::competition_in_active_scope`, `refresh_user_competition_scope`; out-of-scope competitions are resolved by the dormant tier (`turn::dormant` → `catchup`) | both paths must settle league-phase results through the same dispatch |
| Frontend | `src/components/tournaments/*` (standings, group tables, `KnockoutBracket`, fixtures, history); types in `src/store/types.ts` | new views keyed on `rules.format === "LeaguePhaseAndKnockout"` |

### Decision: the smallest maintainable extension

* **No second engine.** `League` stays the one competition container. One new `CompetitionFormat`
  variant (`LeaguePhaseAndKnockout`), one new optional rules block (`CompetitionRules.league_phase`)
  and one new optional state block (`League.league_phase`) with an explicit ownership rule: the
  block owns *only* what the existing fields cannot express (pots, draw, ranking inputs, stage,
  two-legged ties). Fixtures stay in `League.fixtures`; rounds stay in `League.knockout_rounds`
  (so the calendar sweep, the UI and the completion proof see the same rounds).
* Everything that decides an outcome lives in `ofm_core::league_phase` (draw, schedule, table,
  knockout, flow), a `mod.rs` shell plus submodules. `domain` holds data only.
* Result writers call **one** dispatch (`competition_flow::after_result`) instead of two
  hand-copied calls; formats it does not own keep the exact previous behaviour.
* `engine` never imports `domain`: the first-leg score reaches it as two plain integers through the
  existing bridge (`live_match_manager`).

## B1. Changed domain models

| Type | Change | Compatibility |
|---|---|---|
| `domain::league::CompetitionFormat` | new variant `LeaguePhaseAndKnockout` | every existing `match` over the format gained an arm (the compiler listed all six); no stored value changes |
| `CompetitionRules.league_phase: Option<LeaguePhaseRules>` | pots, opponents per pot, home/away grouping, advancement (8 + 16), association caps, `allow_same_association` (explicit relaxation, default off), `max_clubs_per_association` (game-balance stand-in for access lists), the date template, prize amounts | `serde(default)` + `skip_serializing_if`: absent and not written for every other format, so old saves serialize byte-identically |
| `League.league_phase: Option<LeaguePhaseState>` | **the only owner of league-phase state**: persisted `stage`, `calendar_year`, `draw_seed`, `seeding`, `pots`, club `associations`, optional `coefficients`, the derived `table`, the final `outcome` (ranking, direct, play-off, eliminated), the knockout `ties`, `champion_id`, `draw_error`, `next_entrants`, `payments_posted` | same; SQLite column `competitions.league_phase_json` (migration **v073**), NULL for all other formats; a stored-but-unreadable value **fails the load** instead of resetting |
| `CompletedEdition.league_phase` | frozen copy of the state in the archive | `serde(default)`, `skip_serializing_if` (archive rows are deflated JSON, no migration) |
| `League::sorted_standings` | returns the league-phase table (Article 18 order) when state exists | unchanged for every other competition |

Ownership rule: `League.fixtures` and `League.knockout_rounds` stay the one list of matches and
rounds (so the calendar sweep, the UI and the completion proof read what they always read);
`LeaguePhaseState` owns only what those cannot say. `League.standings` is kept in sync from the
table (rank order) so code that reads it (the active-scope check) sees the field. `Game::league`
remains the legacy working buffer; nothing league-phase is authoritative there.

Stage machine (persisted, never inferred from fixture counts):
`Created → LeaguePhaseScheduled → LeaguePhaseInProgress → LeaguePhaseCompleted → KnockoutPlayoffs →
RoundOf16 → QuarterFinals → SemiFinals → Final → Completed`. `LeaguePhaseCompleted` is the one
stage a failed play-off dating can rest in until the retry (it is left within the same call
otherwise).

New module layout (`ofm_core::league_phase`, a `mod.rs` shell plus submodules): `draw`,
`matchdays`, `calendar`, `profiles`, `table`, `bracket`, `flow`, `verify`, `exclusivity`, `prize`.
Cross-cutting hooks: `competition_flow` (the single dispatch every result writer calls),
`matchday::fixture_allows_extra_time`, `live_match_manager` (first-leg score into the engine),
`calendar_reconcile` (second-leg ordering), `competition_completion` (strict proof),
`end_of_season` (rollover guard and regeneration), `turn::cup_renewal` (renewal after a late
final), `world::foundations` (first edition), `generator::competition_def` (authored format and
validation), `engine::LiveMatchState` (aggregate-aware extra time).

---

## C0. Step 2 — official regulation references

Normative source: UEFA documents site, 2026/27 regulations (enforcement date 29 July 2026). Read
through the page text (article pages are plain HTML; the landing page is a table of contents only).
**Annex B (bracket) and Annex C (calendar) are images with no text.** What could not be read is
listed explicitly below; nothing was inferred from a previous season without being labelled.

URLs read:

* CL: Articles 16, 17, 18, 19, 20, 21, 22, 23 — `documents.uefa.com/r/Regulations-of-the-UEFA-Champions-League-2026/27/...`
* ECL: Articles 16, 17, 18, 19 — `.../Regulations-of-the-UEFA-Conference-League-2026/27/...`
* EL: Article 16 only. Articles 17–19 were **not fetched directly**; the landing page links the same
  content ids as the CL articles, so identity with the CL text is *inferred*, not verified.
* Calendar dates (CL only): UEFA news page "2026/27 Champions League: Teams, dates, draws, format,
  final" (page states "dates are subject to change"). EL/ECL dates are not published there.

### Source-supported requirements

| # | Requirement | Source | Strength |
|---|---|---|---|
| R1 | 36 clubs, single league ranking; win 3 / draw 1 / loss 0 | CL/ECL 17.03 | mandatory |
| R2 | CL, EL: 4 pots of 9; each club plays 2 opponents from each pot, one home one away per pot → 8 single-leg matches, 4 home 4 away | 16.01, 16.02, 17.01 | mandatory (pot sizes, count); UEFA may adapt draw conditions (16.03) |
| R3 | ECL: 6 pots of 6; one opponent from each pot; pots paired 1–2, 3–4, 5–6, one at home and one away per pair → 6 matches, 3 home 3 away | ECL 16.01, 16.02, 17.01 | as above |
| R4 | Same association cannot be drawn against each other | 16.02 | **"in principle"** (soft) |
| R5 | At most two opponents from any one other association | 16.02 | hard text, "subject to 16.03" |
| R6 | No more than two consecutive home or away matches; one home + one away across matchdays 1–2 and 7–8 (6-match: first two / last two) | 17.02 | **"normally"; UEFA may derogate** (soft) |
| R7 | Ranks 1–8 → round of 16; 9–24 → knockout play-offs; 25–36 eliminated | 17.04 | mandatory |
| R8 | Tiebreakers after points: (1) goal difference, (2) goals scored, (3) away goals scored, (4) wins, (5) away wins, (6) points of league-phase opponents collectively, (7) their collective goal difference, (8) their collective goals scored, (9) lower disciplinary points (yellow 1, red 3, two-yellow expulsion 3), (10) higher club coefficient | 18.01 (identical in ECL) | mandatory order |
| R9 | 24 clubs enter the knockout phase in predetermined positions by league rank | 19.01 | mandatory |
| R10 | Play-off pairs: seeded 9/10, 11/12, 13/14, 15/16 vs unseeded 23/24, 21/22, 19/20, 17/18 respectively; seeded club hosts the return leg "in principle" | 19.02 | pairing hard; home leg soft |
| R11 | R16: seeded pairs 1/2, 3/4, 5/6, 7/8, each drawn into one of two positions against a play-off winner; seeded club hosts the return leg | 19.03 | as above |
| R12 | QF/SF follow Annex B; seeds 1–4 host the QF return leg, seeds 1–2 the SF return leg; "the team that eliminates them takes over their seeding position in the bracket path" | 19.04 | mandatory |
| R13 | All knockout ties except the final are two-legged; aggregate decides | 20.01, 20.03 | mandatory (UEFA may make a tie single-leg, 20.02) |
| R14 | Level on aggregate after two legs: two 15-minute extra-time periods at the end of the second leg, then penalties. **No away-goals rule** | 21.01 | mandatory |
| R15 | Final: one match at a neutral venue; level after 90 minutes → extra time → penalties | 22.01 | mandatory |
| R16 | Matches Tue/Wed (first league matchday Tue–Thu; final Saturday); in each knockout round a club is in principle scheduled once on a Tuesday and once on a Wednesday | 23.01 | UEFA may make exceptions |

### 2026/27 Champions League match dates (UEFA news page, subject to change)

MD1 8–10 Sep 2026 · MD2 13/14 Oct · MD3 20/21 Oct · MD4 3/4 Nov · MD5 24/25 Nov · MD6 8/9 Dec ·
MD7 19/20 Jan 2027 · MD8 27 Jan 2027 · play-offs 16/17 & 23/24 Feb · R16 9/10 & 16/17 Mar ·
QF 6/7 & 13/14 Apr · SF 27/28 Apr & 4/5 May · final Sat 5 Jun 2027 (Estadio Metropolitano, Madrid).

### What is NOT source-supported (stated up front)

* **Annex B** (bracket) is an image. The R16 pairing structure and the "seeds host the return leg"
  rules come from the Article 19 text; the **QF/SF path layout** (QF A = paths 2+7, B = 3+5,
  C = 4+6, D = 1+8; SF = A+C and B+D) is the published layout as the implementer knows it, and is
  consistent with R12 (seeds 1–4 in different quarter-finals, seeds 1 and 2 in different
  semi-finals). **It must be checked against the Annex B image.** Which semi-final winner is the
  "silver side" nominal home team in the final (19.05) is likewise not readable and is configurable.
* **EL and ECL matchday dates** are not published on the pages read. They are *derived* (the CL
  week's Thursday) and labelled as such.
* **EL Articles 17–19** were not read (see above).
* Article 24 (cross-referenced by 23.01) and Annex D (coefficients) were not read. **Club
  coefficients are not available to the engine and are never invented**: see the ranking section.


---

## C1. Compliance matrix (implemented · tested · approximated · not implemented)

Legend: **I+T** implemented and covered by tests that fail when it is broken (see L2 for the
mutation evidence); **A** implemented as a stated approximation; **N** not implemented.

| Req | Status | Where, and what is approximate |
|---|---|---|
| R1 single 36-club table, 3/1/0 | I+T | `league_phase::table`; rebuilt from fixtures on every result (never incremented) |
| R2 CL/EL: 4×9 pots, 2 opponents per pot, one home one away, 8 matches | I+T | `draw` (block-structured repair + independent `verify`), `matchdays`; 144 fixtures, 18 per matchday |
| R3 ECL: 6×6 pots, 1 per pot, paired-pot home/away, 6 matches | I+T | same engine, `HomeAwayRule::PairedPots`; 108 fixtures |
| R4 same association not drawn together | I+T | on by default; relaxation is only `allow_same_association` in the authored rules, tested separately; never used silently |
| R5 ≤ 2 opponents from one other association | I+T | enforced in the draw and re-checked by `verify`; a field that cannot meet it is a typed `Infeasible` error (per-club, whole-association and pot-level conditions) |
| R6 home/away pattern (no 3 in a row; 1H+1A in first two and last two) | **A** | exact 4/4 (3/3) balance is guaranteed; the *pattern* is optimised by annealing and is **not always perfect**: mean ≈ 2 club-level violations per schedule (0–4 over 3,600 sampled draws). The regulation itself says UEFA may derogate |
| R7 ranks 1–8 / 9–24 / 25–36 | I+T | `table::outcome_of`, fixed once the last league-phase match is recorded |
| R8 tiebreakers 1–10 | I+T / **A** | criteria 1–8 exact. 9 (discipline) is read from match reports (yellow 1, second yellow 3 in all, red 3): team officials are not modelled and matches settled by scoreline carry no cards. 10 (club coefficient): **not available**; the seeding order stands in and the outcome says so (`coefficient_fallback_used`) |
| R9 24 clubs enter in fixed positions | I+T | `bracket` |
| R10 play-off pairings 9/10–23/24 … 15/16–17/18 | I+T | the draw within each pairing prefers different associations when the alternative allows it (Article 19 as read states no knockout association rule: a preference, not a rule) |
| R11 R16 seeded pairs drawn against their play-off group's winners | I+T | |
| R12 path inheritance; seeds 1–4 host QF returns, 1–2 SF returns | I+T / **A** | inheritance and host rules tested; the QF/SF **layout** is from the published bracket and must be checked against Annex B (image only) |
| R13 two-legged ties by aggregate; no away goals | I+T | tested with an away-goals-favoured tie decided the other way; the abolished rule was also re-introduced as a mutation (killed). UEFA's 20.02 single-leg derogation: **N** |
| R14 extra time at the end of the second leg on aggregate, then penalties | I+T | the engine's extra time and shootout are decided on aggregate (`LiveMatchState::set_first_leg_goals`); scoreline-settled ties get a shootout by the same rule |
| R15 final: one match, extra time, penalties | I+T / **A** | neutral venue not modelled; nominal home is the winner of the lower bracket path |
| R16 Tue/Wed dates; once on each day per round | A | CL dates are the published 2026/27 ones; EL/ECL derived (Thursday of the CL week, placeholder finals); other seasons shift by whole weeks. Each knockout round puts a club on both days where the window has two; league-phase matchday dates are split across the window's days without a per-club alternation guarantee. UEFA exceptions: **N** |

Not read (see C0): Annex B, Annex C (images), Article 24, Annex D, and the text of EL Articles
17–19 (identity with the CL text is inferred).


---

## D. Deterministic draw validation

**Method.** Each pairing block (a pair of pots, or one pot with itself) starts as an exact structure
(a cycle for two opponents per pot, a perfect matching for one), so every club has exactly the
demanded opponents per pot by construction. A seeded min-conflicts repair then swaps partners
*inside a block* (which cannot change that count) until the association rules hold; a typed error is
returned for a field no draw can satisfy. A first version used a depth-first search with
most-constrained-slot ordering: it thrashed on end-game parity dead ends and failed every seed, which
is why it was replaced. Every draw is re-checked by an independent `draw::verify` (self/repeated
pairs, same association, per-pot counts, per-association cap) before it is returned.

**Infeasibility is named, not searched for.** Four necessary conditions are checked first, each with
its own typed reason: too few eligible opponents in a pot; too few eligible opponents across
associations; an association too large for the caps (whole-association capacity); and the pot-level
condition (one association's clubs in one pot need opponents from the rest of that pot). The last
one was added after the synthetic harness found a reputation-ordered field that clustered one
association in a pot. To make every admitted field drawable, `LeaguePhaseRules::validate` bounds an
association's share of an edition at half a pot (`max_clubs_per_association`: 4 for 4×9 pots, 3 for
6×6), and the rollover's field builder enforces it. **This limit is a game-balance stand-in for UEFA's
association access lists, which the game does not have.**

**Study** (`season-harness/examples/europe4_study.rs`, `cargo run -p season-harness --example
europe4_study -- 300`): three profiles × four association distributions (a realistic skew of 15
associations, 18 balanced, 12 of three, a top-heavy 9) × 300 seeds = **3,600 draws**.

| | Result |
|---|---|
| Drawn | **3,600 / 3,600 (100 %)** |
| Restarts needed | mean 0.00, max 0 (the first attempt always succeeded) |
| Time per draw + schedule | mean ≈ 55 ms, worst 1.49 s (the home/away annealing dominates; the draw alone is sub-millisecond) |
| Home/away balance | exact for every club in every schedule (asserted) |
| Home/away *pattern* (R6, soft) | clean in about 1 schedule in 6; mean 2.0 club-level violations, max 4 (sample of 12 seeds per cell) |

**Acceptance counts verified by tests** (`league_phase::draw`, `matchdays`, `tests`): 144 / 144 / 108
league-phase fixtures; 18 fixtures per matchday; every club exactly once per matchday; no repeated
unordered pair; no self-pair; two opponents from each pot (one opponent for the six-pot format); at
most two from any other association; same-seed reproducibility (and a different seed differs); stable
unique fixture ids; infeasible fields (3 associations, an 8-club association, a clustered pot) refused
with typed errors; malformed fields (no association, repeated club, wrong pot size) refused.

## E. Standings and tiebreakers

`league_phase::table::ranked_rows` rebuilds the 36-row table from the completed league-phase fixtures
on every result (so a result recorded twice cannot count twice), in the Article 18 order. One test
per criterion where the fixture is built so that the *seeding* fallback would give the wrong answer
(goal difference over goals scored; away goals; away wins; opponents' collective points with the
better-seeded club having the weaker opponents; disciplinary points with the dirtier club better
seeded). Verified: discipline reads yellow = 1, second yellow = 3 in all, red = 3 from the match
report's events, and a match with no report contributes nothing (no data is invented); the coefficient
is never invented (seeding order stands in, flagged in the outcome and shown in the UI); the final
split is 8 / 16 / 12 with every club on exactly one route. The live table is *provisional* (flagged
in the UI) until the last match, because the opponent-result criteria move until then. The league
phase cannot complete while any scheduled league-phase fixture is unresolved and requires exactly
the expected fixture count, so a half-played or corrupt phase never completes.

## F. Knockout bracket and aggregate results

Play-offs 9/10–23/24, 11/12–21/22, 13/14–19/20, 15/16–17/18 under every draw option (40 seeds); the
round of 16 draws seeds against the winners of the play-off group that feeds them; a club that beats a
seed inherits its bracket path (so the next round's pairing and return-leg host follow the *path*, not
the club); an unresolved feeder builds nothing. Ties resolve on aggregate; a level aggregate needs a
recorded, decisive shootout in the deciding match and is **never** resolved by default to the home
side (a level result with no shootout leaves the round open, and the completion proof refuses it);
the away-goals rule is not applied (a tie the old rule would have given to the side with more away
goals was decided by the shootout in the test, and re-introducing the rule was a mutation that was
killed). The engine decides extra time and penalties on the aggregate: a second leg level on aggregate
goes to extra time though the leg alone is not level, and one level on its own but not on aggregate
ends at 90 minutes (`engine::LiveMatchState::set_first_leg_goals`; tests with asymmetric first-leg
scores). Matches settled by scoreline (the dormant tier) use the same rule through
`competition_flow::shootout_required`.


## G. Calendar integration with the domestic competitions

* **Dates** come from a versioned template (`LeaguePhaseCalendar`) authored for a reference season;
  later seasons shift every date by the whole number of weeks nearest the elapsed years, so weekdays
  hold and the drift never exceeds four days (tested to 2039). The CL template is the dates UEFA
  published for 2026/27 (read from UEFA's own page, "subject to change"); EL/ECL are **derived** (the
  Thursday of the CL week) with **placeholder** finals; both are labelled in the data and in C0.
  A knockout round is moved forward in whole weeks only if the round before it ran late.
* **Collisions.** Continental fixtures are `Layer::Continental`, the highest club layer, so the
  existing daily sweep (`calendar_reconcile`) moves *domestic* fixtures around them, never the
  reverse; the three-day rest rule and the international blackout apply as for everything else, and
  completed fixtures are never moved. The sweep gained one rule: a second leg is never dated before
  its first leg plus the rest window.
* **Evidence from the synthetic harness (D1 below):** after every day of every run the harness checks
  that no club has two unplayed matches inside the rest window, that no European match is left
  unplayed in the past, and that the harness's own invariants hold (rest between matches, no stranded
  fixture, standings and results consistent).
* **A defect the harness found, and what it was.** With Argentina (a southern-hemisphere league on a
  January calendar) among the twelve associations, its league was displaced by up to 76 days by
  European fixtures and held the whole world's rollover back to July, August and October 2034–36,
  which in turn stranded World Cup qualifiers. This is not an engine bug (a southern league is not in
  UEFA's competitions) but a poor harness world; the harness now uses twelve European nations and every
  rollover lands in May. **Real limitation recorded in M:** the domestic calendars are generated
  *without knowing* the European dates, so a league with many European clubs is settled by the sweep
  after the fact; the sweep moved up to 48 days for ENG fixtures in the harness and the league still
  finished within three days of the control world.
* No calendar is "impossible" in the harness; the sweep reports a failure (`SearchCapExceeded`) rather
  than hanging, and a competition whose dates cannot be generated (missing template, wrong matchday
  count) is refused at authoring time and at the draw with a typed error.

## H. Match finalization and exactly-once effects

* **One dispatch.** Both result writers (`turn::post_match::apply_match_report_in` for the engine's
  matches, watched or not, and `catchup::apply_simulated_result` for matches settled by scoreline) now
  call `competition_flow::after_result`, which routes league-phase competitions to the new stage
  machine and every other format to exactly the two calls it made before. Quick simulation, managed
  matches and the unwatched fixtures of the day all arrive at `apply_match_report_in`, whose existing
  `fixture_is_open` guard makes a second finalisation a no-op (re-verified by mutation: removing the
  guard fails `a_fixture_is_finalised_once`). The 2D view is a projection of the same engine report,
  not a second engine; the standalone spatial adapter (a diagnostic) is not used for league play.
* **Derived, not incremented.** The league-phase table, the stage and every tie resolution are
  functions of the recorded fixtures: settling twice, in any order, after a reload, changes nothing
  (tested, including a persisted stage rolled back to a stale value, which cannot draw a round twice).
* **Prize money posts once.** Every payment has a stable key (`participation:club`, `win:fixture`,
  `draw:fixture:club`, `stage:round:club`, `champion:club`) recorded in the competition's state; a day
  that runs twice, a reload and a repeated result derive the same owed set and find it posted.
  Posting is per club, so a club the world no longer has cannot hold back anyone else's money. The
  amounts are **game-balance assumptions** (`ContinentalPrize`), not UEFA's distribution; a competition
  with none authored pays none. Gate revenue is the existing weekly matchday income, which counts a
  club's home matches in any competition.
* **Scope.** A competition the manager follows is played by the engine, the rest by scoreline (the
  dormant tier). The harness found and fixed one integration bug here: when the daily renewal drew a
  new edition containing the manager's club, the competition was not added to the manager's scope, so
  their own European matches were settled by scoreline all season. `renew_finished_league_phases` now
  refreshes the scope (a test fails without it).


## I. Save, reload and multi-season rollover evidence

* **SQLite.** `competitions.league_phase_json` (migration v073, hash pinned, `MIGRATION_COUNT` 73,
  the older-build helper updated). Saved with the same positional-SQL discipline as every column; a
  stored value that cannot be read **fails the load** rather than resetting the competition (test).
  Other formats store NULL (test). The save-whole test writes a drawn competition with a played
  league phase and its first knockout round, reads it back and compares state, rules, rounds and
  fixtures; settling the loaded copy changes nothing.
* **Between every stage.** `a_save_and_reload_at_every_stage_loses_nothing` (harness) saves and
  reloads the whole game each time any European competition moves to a new stage (more than twelve
  reloads across three competitions), asserts every European competition is byte-identical before and
  after each reload, plays on from the reloaded state, and checks that every edition ends with a
  champion the strict completion proof accepts. (This reload is the game's JSON round trip, not the
  SQLite file; the SQLite side is the db test above.)
* **Rollover.** An edition still being played at the rollover is **kept**, never regenerated, deleted
  or counted as complete (`is_regenerated_at_rollover` is true only for a champion-holding edition or
  one that never started); the next field is set aside in its state and used by the daily renewal once
  it finishes (its final falls in June, after most domestic seasons end). A played edition that is not
  in the archive is not renewed (the rollover and the renewal agree). An edition that never started
  (its draw was refused) is retried.
* **Multi-season (harness D1).** See L: two seasons × seed 1 as a standing test; three seasons × seeds
  2, 3 and 4 as a by-hand test. Each edition: 144 / 144 / 108 league-phase fixtures + 45 knockout
  fixtures (189 / 189 / 153), 8 / 8 / 4 / 2 / 1 ties, one champion, one archive record per edition
  (never two), a fresh draw each season, the manager's club in at most one of the three.

## J. Archives and qualification interfaces

* **Archive.** `CompletedEdition` freezes the final ranking (`standings` in Article 18 order), the
  fixtures, the rounds and the whole league-phase state (draw, table, outcome, ties, champion). An
  edition is archived only if `verify_completed` accepts it: it recomputes the table and every tie
  from the fixtures and rejects a stale or hand-edited state (tampered winner, wrong champion, stale
  table, dropped fixture, an outsider in a round: each tested). It is archived once; the archive is
  insert-only as before.
* **Field resolution and exclusivity.** Where an edition's field is *inferred* from the previous
  season's domestic results (top four of each first division plus cup winners, strongest first), the
  three fields are made disjoint highest competition first, and an association is held to its access
  limit; the shortfall is refilled strongest-first from the clubs the world has in its regions. A
  field the berth machinery resolves (authored berths, cascade and cross-target exclusivity) is left
  to it. A field of the wrong size is refused and the edition keeps its clubs.
* **Not done here (contract for INTEGRATION5).** The game has no UEFA access lists, titleholder
  entitlements or club coefficients, and EUROPE4 does not invent them. The contract: the world
  supplies, per edition, a seeding-ordered field (`Field.seeding`, strongest first, which is how
  coefficients enter the pots), each club's association (`Field.associations`), optionally club
  coefficients (`Field.coefficients`, which the ranking's last criterion then uses), and a salt.
  `league_phase::regenerate_for_season` / `renew_from_teams` are the entry points.

## K. Frontend support, accessibility and i18n

* **Screens** (`src/components/tournaments/LeaguePhase*.tsx`, wired into `TournamentsTab`): the
  36-club table in the backend's order with three zones (direct / play-offs / eliminated, ranges from
  the rules), a legend, a provisional-table note, a coefficient-fallback note; the stage; the champion;
  the manager's next European match; a failed-draw alert on both Overview and Standings; the bracket
  of two-legged ties with each leg, the aggregate, the winner and the shootout. A competition whose
  draw has not been made says so. No UEFA marks or assets.
* **i18n.** 61 new keys, translated into all 11 non-English locales (not copied): the player-facing
  `tournaments.leaguePhase.*` set and the `be.error.leaguePhase.*` / `be.error.competitionDef.leaguePhase*`
  errors (draw, calendar, rules, authoring). Only "Final" is an intentional same-as-English term
  (es, id, pt, pt-BR, tr). The backend round name "Knockout Play-offs" is localised through
  `localizedRoundName` (the i18n auditor found it would otherwise reach players in English).
  Register fixes applied for cs and tr after the audit.
* **Accessibility** (reviewed by `ui-accessibility-reviewer`, fixes applied): the zone is never
  colour-only (named in text on every row and in a legend, plus a compact legend on Overview); the
  rank number is neutral and only the 4 px edge carries colour; row headers (`th scope=row`);
  sr-only labels for each leg, the aggregate, penalties and "advances"; one focusable, named scroll
  region for the bracket (with a documented lint exception); `focus-visible` rings with the design
  tokens; no nested landmarks. **Not verified:** no browser or screen reader was available (see M);
  contrast was not measured.


## L. Performance, regression gates and reviewer results

### L1. Performance and determinism

* Draw + matchday schedule: mean ≈ 55 ms, worst 1.49 s per competition (3,600-seed study, release-grade
  dev profile). A full edition is built once a season.
* Harness: two European seasons of a 12-nation, ~280-club world (three 36-club European competitions
  beside the domestic leagues and cups, the manager's own on the engine, the rest by scoreline) take
  about 100 s; three seasons about 205 s; peak resident memory about 220–380 MB (`/usr/bin/time -l`).
  The same world without European competitions plays three seasons in about 110 s.
* Determinism: the draw, the matchday plan, the dates and the whole built edition are identical for the
  same field and salt (tested), different for a different salt; the whole competition is a function of
  its recorded results (tested by settling twice and by reloading between every result).

### L2. Deliberate regression checks (mutation)

Each row breaks one thing in production code, runs the named tests, expects red, and restores the
file (`mutate.py`; byte-exact restore, mtime touched). **19 of 19 mutations were killed.**

| Mutation | Killed by |
|---|---|
| draw drops the second opponent per pot (pot allocation) | `league_phase::draw` (5 tests) |
| repeated opponents allowed in the repair | `draw` (3) |
| home/away orientation ignores the trails (balance) | `league_phase::` (9) |
| wrong qualifier count (7 direct places) | `league_phase::` (16) |
| wrong seeded pairings in the play-offs (this was also a real bug caught while writing the test) | `bracket` |
| league phase completes one match early | `league_phase::` (6) |
| next round drawn twice from a stale stage | `a_stale_stage_cannot_draw_a_round_twice` |
| penalty winner inverted | `level_on_aggregate_goes_to_penalties_without_an_away_goals_rule` |
| level tie resolved by default instead of left open | `a_level_tie_without_a_shootout_is_never_resolved_by_default` |
| away-goals rule revived | same, plus the away-goals test |
| archiving accepts an unfinished edition | `an_unfinished_edition_is_never_archived` |
| prize posting forgets what it posted | `league_phase::prize` (4) |
| a club in two competitions' fields | `league_phase::exclusivity` (2) |
| a match finalised twice | `turn::post_match::a_fixture_is_finalised_once` |
| competition lost after reload (column not written) | `competition_repo::a_league_phase_competition_survives_a_save_whole` |
| competition regenerated mid-edition at the rollover | `lifecycle_tests::the_rollover_regenerates_only_a_finished_or_empty_edition` |
| continental fixtures lose their calendar priority | `calendar_reconcile` (4) |
| engine ignores the first leg | `engine::aggregate_tests` (3) |
| renewal forgets the manager's scope | `a_renewed_edition_joins_the_scope_of_the_manager_whose_club_is_in_it` |

(The reviewer's findings that five tests could not tell a right implementation from a wrong one were
fixed *before* this sweep: fixtures now put the better-seeded club on the wrong side of each criterion.)

### L3. Gates (final tree)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (default features) | clean |
| same with `--features mcp` | clean |
| `cargo test --locked --workspace --no-fail-fast` (default) | **5,566 passed, 0 failed**, 153 ignored (long studies) — was 5,472 at the baseline |
| same with `--features mcp` | **5,653 passed, 0 failed**, 153 ignored |
| Vitest (whole frontend suite) | **3,302 passed**, 0 failed, 4 skipped — was 3,284 |
| `tsc` + `vite build` (`npm run build`) | passes |
| Biome format / lint (Biome 2.5.5, installed, not `npx`) | clean (3 pre-existing infos) |
| Knip | clean |
| i18n gates (`localeCoverage`, `frontendKeyCoverage`, placeholder parity, duplicates) | 35 / 35 |
| Harness, standing tests | two seasons of three European competitions (seed 1) and a save/reload at every stage: pass, ≈ 100 s |
| Harness, by hand | seeds 2, 3 and 4, **three seasons each**: pass, 558 s in all, 213 MB peak; control world without Europe: pass |
| Mutations | 19 of 19 killed (L2) |

Not run: the adapter tests that need the owner's private source (`APFM_WORLD_DB`, `APFM_WORLD_JSON`):
EUROPE4 does not need them and the owner's world was not touched.

### L4. Reviewers (all read-only, run on the uncommitted tree)

| Reviewer | Findings | Disposition |
|---|---|---|
| `ofm-architecture-reviewer` | no P0. P1: a competition whose draw is refused was dead for the career (no retry). P1/P2: exclusivity overrode authored berths. P2: rollover could renew an unarchived edition; one unknown club blocked every prize; size limits; production `expect`; stale `draw_error`; public modules | **all fixed** except the size limits of `end_of_season/mod.rs`, which was already past them (1,533 production lines at the baseline against the 1,500 cap; now 1,553) |
| `i18n-auditor` | P1: "Knockout Play-offs" would reach players in English; cs/tr register; one tr term; smaller wording | **fixed** (round name localised; cs/tr/de/es/it/zh wording corrected) |
| `ui-accessibility-reviewer` | P1: failed-draw alert missing on Overview; zone colour contrast; colour-only cue and no compact legend; bracket scores unlabeled. P2: nested landmarks, focus rings, row headers | **fixed** (see K); contrast measurement and raw-palette → role-token migration of the new components **not done** (M) |
| `ofm-dedup-reviewer` | P1: `nextMatchOfClub` duplicated `findNextFixture`; round-name mapping; knockout-fixture-id set built three times; duplicate stage-key helpers. P1 not done: `LeaguePhaseTable` is a near copy of `StandingsTable`; renewal prologue copied from `renew_finished_cups` | the first four **fixed**; the last two **named in M** |
| `ofm-test-reviewer` | five tests that could not discriminate, one circular oracle, one vacuous assertion, result-writer wiring only covered by the minutes-long harness | **fixed** (see L2 note, hand-computed prize amounts, unit tests for the dispatch, the extra-time predicate and the scoreline shootout); remaining P2s are in M |

### L5. Defects the harness found (all fixed, each with a regression test)

1. A reputation-ordered field can cluster one association in a pot, for which no draw exists; the
   solver reported it as an exhausted search. Now a typed pot-level infeasibility, and the field builder
   bounds an association's share so such a field cannot be built.
2. The daily renewal drew a new edition containing the manager's club without adding it to the
   manager's scope: their European matches were settled by scoreline.
3. (Harness world) A southern-hemisphere league inside the European fields delayed the world's
   rollover by months; the harness now uses European nations only.


## M. Remaining defects and limits

No known P0 or P1. Everything below is P2 unless marked, with its evidence.

| # | Item | Evidence / consequence |
|---|---|---|
| 1 | **QF/SF layout and the final's nominal home are not verified against Annex B** (image only). Implemented from the published layout and checked against Article 19.04 | `bracket::QUARTER_FINAL_PATHS`, `SEMI_FINAL_QUARTERS`; one data edit if the image differs |
| 2 | **EL and ECL dates are derived, finals are placeholders**; EL Articles 17–19 were not read (identity with CL inferred) | `profiles.rs` provenance strings; the CL dates are UEFA's |
| 3 | **Home/away pattern (R6) is optimised, not guaranteed** | mean ≈ 2 club-level violations per schedule, max 4 over 3,600 draws (D); the regulation allows derogation |
| 4 | **Ranking criterion 9 is partial and 10 is a stand-in**: no team officials; scoreline-settled matches carry no cards; no club coefficients exist in the game | `coefficient_fallback_used` is set and shown whenever the stand-in decides |
| 5 | **Fields are inferred, not UEFA access lists** (top four of each first division plus cup winners, strongest first, an association held to 4 (3) clubs, refilled by reputation) | the contract for real fields is in J; INTEGRATION5 |
| 6 | **Domestic calendars are generated without knowing the European dates**; the daily sweep reconciles them afterwards | in the harness the ENG league had 177 of 420 fixtures moved, at most 48 days, and still finished within three days of the control world; not a defect of the engine but the next calendar improvement |
| 7 | **UEFA's discretionary rules are not modelled**: single-leg ties (20.02), calendar exceptions (23.01), neutral final venue | a tie is always two legs; the final's venue is the nominal home |
| 8 | **Prize amounts are game-balance assumptions** | `balance_prize`, labelled in the data and the docs; no UEFA table was invented |
| 9 | **No GUI was operated and no browser or screen reader was available** | the React views are tested by Vitest and reviewed against the accessibility checklist; contrast was not measured; `tsc`/`vite build` passing is not graphical acceptance |
| 10 | New React components use the raw palette classes the neighbouring tournaments files use, not the role tokens; the folder is not in `designSystem.test.ts`'s finished list | works in both themes (every colour has its `dark:` partner); a migration, not a defect |
| 11 | Duplication named, not consolidated (dedup reviewer, P1/P2): `LeaguePhaseTable` is a near copy of `StandingsTable` (goal-difference tone ternary now in four places); the zone legend dots; the renewal prologue shared with `renew_finished_cups`; a `parse_date` and a fixture-literal copy; test-helper overlaps | for epic #589 |
| 12 | `end_of_season/mod.rs` is past its size caps (1,553 production lines against 1,500; `regenerate_competitions_for_new_season` over 100 lines). Both were already past them at the baseline (1,533); this change adds 20 lines | the league-phase rollover logic itself lives in `league_phase::rollover` |
| 13 | Unit coverage of the *live* result path for a second leg (`apply_match_report_in` with a real engine report, and `create_live_match` handing the first-leg score to the engine) is end-to-end only: the harness plays real engine matches for the manager's competition, while unit tests cover the dispatch, the extra-time predicate, the `KnockoutLeg` value and the engine's aggregate logic separately | a mutation of each seam is killed (L2) |
| 14 | Pre-existing, unrelated: `be.error.competitionDef.invalidSeasonMonth` and `invalidSeasonDay` have no locale entries | noticed by the i18n auditor |
| 15 | A shipped world is unchanged: none of its definitions use the new format. The feature is reachable through authored definitions and the harness | by design (INTEGRATION5) |

## N. Reserved for INTEGRATION5

1. Real participants: association access lists, titleholder entitlements, coefficient-based pots, the
   qualification path of each association (including the Europa/Conference cascades UEFA applies),
   replacing the inferred field and the access-limit stand-in.
2. Club coefficients (and a source for them) so ranking criterion 10 is real, and pot membership by
   coefficient.
3. Authoring the three European competitions into the shipped world (definitions, regions, names) and
   a decision on how an existing career's continental cup migrates.
4. Verifying Annex B and Annex C against the images, and the EL/ECL dates and finals.
5. Neutral final venues and any UEFA calendar exceptions; European dates known to the domestic
   calendar generators (so leagues are dated around them rather than moved afterwards).
6. A real-money prize model, and the manager-facing surfaces beyond Tournaments (dashboard card,
   inbox and news for European draws, a "your draw" screen).
7. A GUI acceptance pass on the new screens, in both themes, with a screen reader and a measured
   contrast check.
