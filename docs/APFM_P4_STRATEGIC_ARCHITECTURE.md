# APFM P4 strategic architecture

Branch `claude/apfm-playable-career`, starting from `e5282b67` (P3 plus the merged F5.5 performance work).
This is the Phase A audit: how an AI club decides today, where that is inconsistent, what was measured,
and the smallest extension of the existing design that fixes it. The results of building it are in
`APFM_P4_IMPLEMENTATION_REPORT.md`; the long-run evidence in `APFM_P4_LONG_CAREER_VALIDATION.md`.

Evidence labels: **READ** (read in source), **RUN** (measured by this phase, command given), **DOC**
(from an earlier report, not re-measured).

## 1. The decision pipeline as it stood

There is one strategic AI, spread over four places. It is coherent in its parts and was not coherent
between them.

| Question | Where it is decided | Stored or derived | State at `e5282b67` |
| --- | --- | --- | --- |
| What is the club aiming for? | `transfers/strategy.rs::classify` gives a `ClubProfile` (ambition, means, youth focus) | **Derived** each sweep from state, never stored | READ. A pure function of reputation rank in the club's league, last season's finish, cash against squad value, wage bill, squad age and academy level. Changes when the club does. |
| What does the board expect? | `board_objectives.rs` (`ObjectiveTargets::new(reputation, league_size)`) | Stored in `game.board_objectives` | READ. **The player's club only.** An AI club has no stored objective; its aim is the derived ambition above. |
| What does the manager prefer? | nowhere | — | READ. `domain::Manager` holds reputation, satisfaction, career history. `Manager::rating()` is the only trait anything reads. **No tactical, rotation, youth or risk disposition existed.** |
| How does the club play? | `ai_tactics.rs::blueprint_for(team.play_style)`, reviewed weekly by `apply_ai_tactical_reviews` | `team.play_style` and `team.tactics_phase` are stored | READ. The blueprint is the club's (its badge), recomputed weekly from style, squad legs/pace and a five-match form window. Every club of a style started from the same nine dials. |
| Which formation? | set at world generation | stored in `team.formation` | READ. **No AI code path changes an AI club's formation** (search of `ofm_core` for writers). Only the in-match AI (`interactive_match/ai.rs::formation_for`) changes shape, and only for a match. |
| Who starts? | `turn/squad.rs::ai_select_starting_xi` → `first_choice_eleven`, then load management | derived per fixture | READ. See section 2. |
| What does the club buy? | `squad_intelligence` (needs, impact, appetite) read by `transfers/market.rs` | derived | READ. The squad model prices each signing against the side the club would field, weighted over this season and the next two. The profile tilts the ranking (`rank_weight`) and the horizon weights (`Appetite::of`). |
| What does it sell or renew? | `ai_squad_sales.rs`, `ai_contracts.rs`, `transfers/sale_view.rs` | derived | READ. Priced by `ClubSquad::sale` (the cost of losing a player to the side). **The profile does not reach this side.** |
| How do prospects get chances? | `transfers/loans.rs::list_loans_by_status`, `top_up_loan_listings`; the squad model's succession horizon | derived | READ. A club lists a backup or surplus aged 22 or under with 6+ points to grow. The picker has no youth preference at all. |
| How does it react to poor results? | weekly form window → `shade` moves (`ai_tactics.rs`); `firing.rs` → `ai_manager_appointments.rs` | form is read from fixtures; appointment matches **reputation only** | READ. |
| What persists between seasons? | `team.play_style`, `team.tactics_phase`, `team.formation`, `team.history`, contracts, finances | stored | READ. Strategy itself persists by being a function of persisted state. |

### What this means

- "Persistent club identity" already existed in the right form: derived from state, so it survives a
  reload and fast-forward by construction. This phase did **not** add stored strategy.
- "Persistent manager identity" did **not** exist. Two managers of one rating decided identically.
- The squad model, the market and the picker each had a part of the truth, and the model and the picker
  disagreed. That is the highest-impact finding and is measured below.

## 2. The model/picker mismatch: it exists, and it is larger than the audit thought

The takeover audit (F1) said the model solves an optimal assignment while the picker walks slots in
order. It also reported the damage as small (RUN, 5 seeds: "2 model-only pathways of 12"), measured with
the model's own greedy mirror. That mirror turned out not to mirror the picker.

Measured by `p4_lineup_gap` (RUN: `P4_SEEDS=1,2,3 P4_SEASONS=2 cargo test --release -p season-harness
--test p4_lineup_gap -- --ignored --nocapture`, gate world, 79 AI clubs a seed): every club's model side
against the side `first_choice_eleven` fields, from the same healthy seniors, scored in the game's own
raw slot rating so neither side's rounding biases it. Values are for the three seeds.

| At | Picker side differs from the optimal side | Model starters the picker leaves out, per club | Raw rating per slot the picker leaves on the table (mean; p90) |
| --- | --- | --- | --- |
| Opening | 95-99% of clubs | 1.1-1.2 | 0.68-0.76; 1.65-2.17 (but see below) |
| Season 1, day 100 | 89-96% | 0.70-0.92 | 0.95-1.08; 1.8-2.4 |
| Season 2, day 100 | 87-91% | 0.84-0.89 | 0.78-0.81; 1.7-1.8 |

Three findings, in order of consequence:

1. **The slot walk wastes players.** It spends a player on whichever slot it reaches first. A synthetic
   but exact case: in a 4-2-3-1, an 88-rated striker who also fits left back better than the left back
   played at left back and left the forward line to a 62 reserve (test
   `a_striker_is_not_spent_on_the_slot_that_is_filled_before_his_own`, red before the fix). On average a
   club's side sat about one rating point per slot below its own best.
2. **The model could not see players on loan to the club.** A loaned player carries the borrower's
   `team_id`, so the picker fields him, but `Member::of` rejected every `active_loan`. At day 100 of
   season 1, **42-58%** of AI clubs fielded a loaned-in starter that the model did not know existed
   (1.0-1.3 loaned-in players per club), falling to 8-20% in season 2.
3. **The model's greedy mirror never matched the real picker.** Before any change it named a different
   player from `first_choice_eleven` in 3.8-3.9 slots a club at the opening and about one slot a club later
   (anchored, integer-rounded ratings break ties differently from the picker's continuous ones). Every
   earlier "the real picker would give him a start" figure came from that mirror. The study columns that
   still print it are not valid on the P4 tree (section 5).

A further observation: at the opening, stored `ovr` differs from the attribute-derived rating by more than
rounding for 38% of rostered players (mean offset 2.1), and by 0.25 (none beyond rounding) by day 100. The
model anchors its slot ratings to stored `ovr`; the picker uses the attributes. This is a generated-world
quirk of the opening only; it is why the opening row above is not used as evidence.

## 3. Proposal: the smallest coherent extension

Not a second AI. Three moves inside the existing one, in the order the evidence ranks them.

**3.1 One side, one solver.** The squad model and the picker solve the same assignment with the same
code: the Hungarian solver moves from `model/mod.rs` to `squad_intelligence/assignment.rs`, and
`first_choice_eleven` calls it over the game's raw `positional_fit_for_assignment`, a natural keeper in
goal, ties on id. A squad too short for the formation is still named in slot order. Cost: one 11×N solve
per club per fixture where the walk did 11×N fit comparisons; measured at +1.2% instructions.

**3.2 One squad, one pool.** A borrower's model holds the players on loan to it (`Member::loaned_in`), for
the horizon years the loan covers and no longer, and never as the club's to sell or renew. A loan then
fills a need for as long as it lasts and opens a succession need when it ends.

**3.3 Identity beside philosophy.** A manager has four dispositions, derived from his id, that tilt the
club's blueprint, its reaction to results, its rotation and its youth focus. They tilt; they do not
replace: the club's play style, squad and books remain the philosophy, the engine's ration of under-priced
dials and the squad's legs and pace still bind, and the neutral manager changes nothing (asserted).

Why derived rather than stored: it needs no schema change, no migration and no backfill; a reload and a
fast-forward cannot disagree; and the manager takes his disposition to his next club. The cost is that an
identity does not develop over a career. That is stated, not hidden, in section 6.

### What was considered and not done

| Option | Why not now |
| --- | --- |
| Make the model greedy, as the audit's decision 9.1 offered | It would copy the picker's defect into every transfer valuation. The picker is the defect. |
| Store a `ClubStrategy` on `Team` | The derived profile already persists by being a function of persisted state, and moves about once per club in three seasons (section 4). Stored state would need a migration and a reset rule for no behaviour the derivation lacks. |
| Hysteresis on the profile | Measured flip-flops are 0.35-0.84% of intervals. Nothing to damp. |
| Let AI clubs change formation | A larger sporting change with no measurement behind it; listed in section 6. |
| Quotas or caps on transfers | Ruled out by the brief; none added. |

## 4. How stable is a club's strategy? (RUN)

`p4_strategy_stability`, gate world, 3 seeds, 3 seasons, every club sampled every 50 days (20 samples):

| | Seed 1 | Seed 2 | Seed 3 |
| --- | --- | --- | --- |
| Ambition changed, share of club-intervals | 5.7% | 5.1% | 5.1% |
| Means changed, share of club-intervals | 6.9% | 6.5% | 6.1% |
| Clubs whose ambition or means ever changed in 3 seasons | 91% | 92% | 96% |
| Flip-flops (A, B, A) as a share of triples | 0.70% | 0.84% | 0.35% |
| Youth-focus move per interval, p90 | 0.109 | 0.099 | 0.100 |

A club's ambition changes about once in three seasons and almost never changes back. That is the
behaviour the brief asks for ("evolve when circumstances change", "no random yearly resets"), without
any stored state.

**One defect found.** A season record stores the finishing position but not the division it was earned
in, so `profiles()` reads last season's finish against the club's *current* league. A promoted champion
(finished first below) is lifted a step to Established although it is the smallest club in its new
division; a relegated bottom club is lowered a step. Executable specification:
`a_promoted_champion_is_still_a_fighter_in_its_new_division` (`#[ignore]`d with the reason; it fails today
with `left: Established, right: Fighter`). The fix is a `serde(default)` tier on `TeamSeasonRecord`
(stored in a JSON column, so no SQL change), set where `end_of_season` writes the record; not done here.

## 5. Consequences for measurement

- `p4_lineup_gap` replaces the model's greedy mirror as the way to ask "does the model agree with the
  picker?". The older `f54_squad_study` columns "model starts / greedy starts / model-only pathway" read
  `greedy_picks_with`, a mirror of an algorithm the picker no longer runs. They print, and mean nothing,
  on this tree. `greedy_picks_with` is still used by `lineup_gap`; retiring both is a cleanup, listed in
  the implementation report.
- Runtime guard: the F5.5 baseline (699.2B instructions, 34.5 s for 5 seeds × 2 seasons, gate) was
  reproduced exactly (RUN) before any change, so later numbers are against a verified baseline.

## 6. What this architecture still does not do

- AI clubs never change formation; a club with strong wingers and a 4-4-2 plays a 4-4-2 for ever.
- A manager's identity is assigned, not formed. He does not learn, and his preferences are not shaped by
  what has worked for him.
- Manager appointments still match on reputation alone; a club's play style and a candidate's identity
  are not compared.
- The profile does not reach sales, renewals or lineups (youth minutes are earned on merit alone); only
  the market's buy side and the horizon weights read it, and now the youth side through the manager.
- In-match adaptation (`interactive_match/ai.rs`) was not changed; pre-match planning is what this phase
  touched.
- The user's own assistant plans with the neutral identity.
