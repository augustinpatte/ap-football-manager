# APFM I7 — scouting and youth development

Branch `claude/i7-scouting-youth`, baseline `11bbb6d4` (I5/I9 on `apfm/main`). Local only: nothing was pushed
or merged. Evidence, commands and measurements are in [`APFM_I7_VALIDATION.md`](APFM_I7_VALIDATION.md). Short
design notes also live in [`GAME_SYSTEMS.md`](GAME_SYSTEMS.md#scouting-and-the-academy-review).

## 1. What existed, what was missing

Already there at the baseline: scouts and scouting assignments (one job at a time), an inbox report with fuzzed
headline attributes, youth recruitment searches, a Youth Academy tab, the I4A squad model, the AI clubs' academy
promotion (`youth_pathway`), the season-end intake, named duty owners (I9) and a delegated scouting duty.

Missing or wrong:

| Gap | Now |
| --- | --- |
| A finished job stored the player's **true** attributes as the profile snapshot (scout quality did not matter there) | The snapshot is what the scout read, each attribute off by up to his error (2, 5, 8 or 12 points by judging) |
| No persistent structured report; only an inbox message | The *scouting dossier*: reports as ranges, saved with the game |
| No shortlist, no comparison, no cancel or reassign of a player job | Shortlist (max 30), comparison of 2 to 4, cancel and reassign |
| No academy decision support | The academy review: ready, loan candidate, keep developing, unlikely to progress, with reasons |
| The delegated scouting duty ranked targets by **true potential** | It ranks by what the club can see and follows the shortlist |
| A manager could promote a 31st senior from the academy | Promotion respects the 30-senior limit |
| A free agent let go by an academy joined his new club's academy, not its senior squad | Fixed (section 8) |

## 2. The scouting model and its uncertainty

`ofm_core::scouting_dossier`. A report holds an **ability range** and, when the scout can judge it, a **potential
range**. Nothing in it is a true hidden value.

* The truth is always inside a range; the centre is not the truth (a deterministic error of up to 60 % of the
  half-width, drawn from the assignment). A range slides rather than being clipped at 1 or 99, so players at the
  extremes do not look more certain.
* Half-width by the scout's judging (after the department's help, `effective_judging`): ability 2 / 4 / 6 / 9
  (80+, 60+, 40+, below); potential 4 / 7 / 10 / 14 (80+, 60+, 40+, 30+), plus 2 for a player of 19 or younger.
  Below 30 judging potential the scout gives no potential range at all ("not judged").
* A second report on the same player in the **same season** is intersected with the first (both contain the
  truth, so the intersection does too) and counted; a report from an earlier season replaces the old one, because
  the player has changed.
* Confidence is the width of the ability range: 5 or less high, 10 or less moderate, wider low. Potential width is
  shown as it is.
* Strengths and weaknesses are the three highest and two lowest position-relevant attributes **as the scout read
  them**, when clearly above or below the player's own average.
* A job takes 2 to 5 days by the scout's judging, plus 2 for a player outside the manager's division. Reports are
  deterministic (seeded from the assignment) and capped at 400 in the dossier.

The screen projection (`scouting_view`) adds only public facts and club-side judgements: age, positions, public
rating, value, wage, contract; the **outlook** (high growth, growing, near peak, declining, unclear — from the gap
between the middle of the two ranges and age; never "will become"); the **squad impact** from the I4A model asked
about the player *as the club believes him* (middle of the range, or the public rating when unscouted); whether the
fee fits the transfer budget and the wage fits the board's policy; and the **risks** (contract expiring within a
year, injured, over 30, unscouted, stale report, wide estimate, fee over budget, wage over policy).

**Not invented:** no regions, countries or scouting networks. The world model has no such thing; the existing youth
search keeps its domestic/international option as it was. **Pre-existing limitation:** the serialized `Game` sent to
the frontend still carries every player's true values (see the `get_player_knowledge` row in ARCHITECTURE.md). The
new screens never read them; the guarantee is for the new surfaces, not at the wire.

## 3. Search, assignments, shortlist, comparison

* The player search gained **scouting status** (any, not scouted, scouted, shortlisted), **estimated potential at
  least** (the middle of a scout's range; unscouted and unjudged players never pass, the filter does not guess) and
  **contract ends within a year**, beside position, age, rating, value, club and "in budget". Rows show the club's
  ability and potential ranges, and a star follows or unfollows a player.
* Assignments: send (existing), **cancel** and **reassign** (new, player jobs; reassignment needs an idle scout and
  keeps the days already spent). Review is the existing active-assignments list.
* The dossier screen shows the shortlist and the reports; a report opens a dialog (ranges, confidence, strengths,
  weaknesses, outlook, finance, risks) with **make a bid** / **offer a contract** (the existing transfer and
  free-agent flows, not a new mechanism), scout again, shortlist and open profile. The comparison puts 2 to 4
  players side by side; a figure nobody has earned reads "unknown", not zero.

## 4. The academy and youth development

What was **not** changed, on purpose: the season-end intake (`youth_intake`: target academy of 1/2/2/1, one to
three a season, the staff and club-standing swing of F5.1), the AI academy promotion, and the development model
(`development.rs`). All three were calibrated by earlier studies and the brief forbids untested multipliers. The
measurements (validation doc, section 4) found the academy plausible: AI academies average 1.8 to 4.5 players,
at most 8; intake potential averages about 70; about 1 to 4 % of an intake is given 80 or more; no club exceeds the
senior limit; the user's club keeps promoting players the squad model wants.

New: `academy_review` ("is he ready, does he need a loan, will he get there"). A player is

* **ready for the senior squad** when `youth_pathway::pathway_of` says he would start or cover for the side (the
  same question the AI clubs ask), whatever his age;
* **unlikely to progress** from 18 when even his ceiling plus 3 is below the senior squad's cover in his position;
* a **loan candidate** from 18 when healthy, not already on loan, 5 or more points of headroom and within 8 points of
  that cover;
* otherwise **keep developing**, and always so before 18.

Every verdict carries reasons shown on the screen, and the table shows the ceiling beside the rating the development
model **expects a year on**, labelled as a ceiling and not a promise. The thresholds are named constants in
`academy_review.rs` and are advice only: the review changes no player and no rating.

Actions use existing commands: **promote** (`set_player_squad_role`, now refused with the squad-full key when no
senior place is free, counting places promised to deals and seniors on loan), **list for loan** (`toggle_loan_list`,
then the existing loan offers) and **release** (opens the profile on the existing termination dialog with its
severance preview).

**What the measurement says about "potential must not guarantee ability".** Over ten seasons the academy players
aged 24 or more ended on average 1.2 rating points below the potential they were given at intake (sd 2.0 to 2.2,
10th percentile -4, 90th +1, worst -12). The existing development model, which lets a ceiling drift to age 24 and
then delivers almost all of it, makes potential a strong predictor. About two thirds, not all, ended at or above
the median senior of the world. I did not touch the model; I made sure no new surface presents potential as a
promise (ranges, "ceiling", expected rating) and I record the finding. A wider spread would be a development-model
change needing its own calibration.

## 5. AI clubs

**Unchanged by design and a stated limitation.** AI clubs have no scouting-assignment model: the I4A planner and the
market read the world as it is (`squad_intelligence::perception`: the assistant, who works for the manager, is the
one who reads it through scouts). They already keep academies to a target, promote through the squad model, respect
the senior limit and the wage policy, and the study shows no accumulation or starvation (1.8 rising to 4.5 academy
players on average, 115 to 230 promotions a season across the AI clubs). Giving AI clubs a scouting resource would
change every transfer decision of a calibrated world, which the brief asked not to do casually. The one AI-visible
change is the free-agent role fix in section 8, which makes every club's signed free agent count as a senior.

## 6. Delegation

`staff_system/delegation_scouting.rs` (the duty moved out of the 1,300-line `delegation.rs`). Whoever is in charge
(assistant or a named head of scouting; the I9 owner and its `effective` rating are unchanged) decides where the
club's own **idle scouts** go, and the scout he sends writes the report with his own judging. In order:

1. shortlisted players with no current report (the manager asked);
2. players for the positions the squad model calls weak, else the most precocious youngsters the club can afford,
   ranked on the **public rating** and on what the squad model says they would add as rated.

Each new report is then reviewed **once** (a date marker, `delegate_reviewed_through`, starting today so existing
reports are never re-judged): a player who would start or cover, whose fee fits and whose wage the policy allows,
goes on the shortlist (at most two a run). Shortlisting spends nothing, so it needs no approval; buying stays with
the recruitment duty and its approvals. A player the manager removes is not put back. The youth duty (I9) is
unchanged: the head of youth or a youth coach promotes through the advice engine, and a promotion still has to pass
the senior-place check. No new daily action or repeated proposal was added: one idle scout, one job.

## 7. Frontend workflows

Scouting tab: search with the new filters, assignments with cancel and reassign, the shortlist and reports section
(table, details dialog, comparison dialog), the existing youth recruitment card. Youth Academy tab: the development
review above the existing prospects, with promote, list for loan and release. The Staff tab's duty owners (I9) are
the staff configuration. New code is under `scouting/dossier/` and `youthAcademy/review/`, both added to the design
system's finished folders (role tokens only), with loading, empty and error states, labelled controls, a disabled
fifth comparison checkbox and no stacked modals. Translations: 114 keys in the 12 locales (written by a model,
not by native speakers).

MCP: `scout_get_dossier`, `scout_shortlist_add`, `scout_shortlist_remove`, `scout_cancel`, `academy_get_review`,
sharing the commands' `_internal` functions (docs/MCP_SERVER.md).

## 8. The senior-squad floor

Reproduced on the I5/I9 baseline (seed 2, manual arm, 10 seasons: 1 day at 14 seniors, 2038-05-01). It was
**not** a day with no player available. The assistant's daily keeper found and signed a free-agent forward on the
last day of season 5 (`signed=[274e…]`, but seniors after the call stayed 14), because that forward was an academy
graduate let go earlier and still carried `SquadRole::Youth`: `contracts::sign_free_agent` set the club and the
contract but never the role, so he joined the club's **academy**. The next day the same pass promoted the same
player from the academy (`promoted=[274e…]`). The one-day breach was a genuine gameplay defect that reached every
route that signs a free agent (manager's talks, the assistant, an AI club's planning, the global market), all of
which had already checked for and counted a **senior** place. Fix: `sign_free_agent` sets `Senior`. Evidence:
two tests that fail without the line and pass with it; the baseline with only that line has 0 breach days in all
four manual arms (without it, 1).

The floor itself remains a **documented best-effort** contract and that is appropriate: the game never creates a
player or money, never ignores wages and never bypasses registration. When neither the academy nor the affordable
free agents have anyone, the club stays short and the manager is told (`be.msg.squadToppedUp`,
`tell_user_club_it_cannot_be_filled`); an AI club logs an error. No test was weakened; the I9 study's strict assertion
stays strict and now holds on the seeds measured.

## 9. Save compatibility

* `Game.scouting_dossier` is `#[serde(default)]`; `ScoutingDossier` is `#[serde(default)]` at container level.
* **Migration v070** (`v070_scouting_dossier.sql`, one JSON row like `staff_delegation`); `EXPECTED_MIGRATION_COUNT`
  70, fingerprint pinned; a save with no row loads an empty dossier; `new_game_from_save` clears it. The run3
  integration test helper now drops the table when imitating older builds.
* `PlayersPageQuery` gained three fields, all `#[serde(default)]`.
* Old saves keep their exact attribute snapshots until the player is scouted again (then the new, noisy reading
  replaces it) and have no dossier: pre-I7 `scouted_players` rows are not converted into reports, so such a player
  reads "not scouted" in the new search though his profile still shows the old snapshot, and the delegated duty may
  scout him again.
* Reload mid-career replays identically (test `the_dossier_and_shortlist_survive_a_reload_in_the_middle_of_a_career`).

## 10. Remaining limitations

* Potential is a strong predictor in the existing development model (section 4); AI clubs do not scout (section 5).
* Hidden values are still on the wire (section 2). The GUI was **not operated** (validation doc, section 7).
* Translations are model-written. `ScoutingTab` (about 390 lines) and the search card (about 260) exceed the 200-line
  frontend ceiling after this change; `build_scout_report` is over 100 Rust lines: extraction into hooks was not done.
* The reassign-scouting error reuses the youth search key `scoutAlreadyAssignedToSearch`.
* Reviewer items not actioned: shared table style constants, one `ensure_assistant` for the harness, judging-tier
  breakpoints shared between three functions, focus return when a shortlist row disappears, live-region wording,
  command and MCP adapter tests beyond the shortlist round trip, a same-division control for the travel days.

## 11. For the I6 merge

Shared files touched, all small: `src-tauri/src/lib.rs` (invoke handler list, 7 commands after `send_scout`),
`commands/mod.rs`, `commands/squad.rs` (promotion check), `crates/db` (**migration v070**: renumber if I6 also adds
one; `migrations.rs` count and fingerprint, `game_persistence.rs`, `save_manager.rs`, `scouting_repo.rs`, the run3
test helper), `ofm_core/src/game.rs` and `lib.rs`, `slices/players.rs` (query fields), `contracts/free_agent.rs`,
`youth_pathway.rs` (`cover_levels`, `as_senior` visibility), all 12 locale files (new keys appended inside
`scouting`, `academy`, `be.error.scouting`, `delegation.log`: expect textual conflicts), `designSystem.test.ts`
(two finished folders), `docs/ARCHITECTURE.md`, `GAME_SYSTEMS.md`, `MCP_SERVER.md`. No match engine, M12, tactical
evaluator, playback or rendering file was modified.
