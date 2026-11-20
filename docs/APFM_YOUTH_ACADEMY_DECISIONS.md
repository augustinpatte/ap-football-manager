# Mission 3/5 — youth academy and manager decision quality (2026-11-20)

Branch `apfm/main`, from `9cf4db5d` (mission 2). Nothing was pushed, reset or deleted. The serialized
`Game`/`Player` models were not changed (no migration, no new serialized field).

## 1. The rule

The development model's `potential` is the simulation's ceiling and stays exact on the player. The manager is
never given it. His **youth staff assess it** (`ofm_core::academy_assessment`) and hand over a band that always
holds it; everything the academy screen decides or orders is decided on that band.

| Youth department rating (`StaffProfile::youth`) | Band | Confidence |
| --- | --- | --- |
| 70 or more (a strong head of youth) | 6 rating points | high |
| 50 to 69 (an average department) | 12 | moderate |
| below 50, or nobody in the seats (`VACANT_RATING` 20) | 24 | low |

* **No randomness, nothing stored.** The band is a pure function of the true ceiling, the player's rating and the
  department's rating, so it reads the same on every render, after every save and reload and on every screen.
* **A grid, not a centred error.** Bands sit on a fixed grid (1–6, 7–12 … / 1–12 … / 1–24 …), each width a
  multiple of the one before. A band centred on the truth with an error would move with the band's width, and a
  manager who changed his staff and read again could solve for the ceiling. A grid band gives the same facts at
  every width: a better department only ever shows a smaller band inside the larger one. Two players whose
  ceilings share a band read **identically**, in every field.
* The band starts no lower than the player's own rating (a ceiling is not below what he already is).
* **Current ability is not assessed**: an academy player is the club's own, so the manager reads his rating
  exactly (`rating_view`, Mission 2's rule, unchanged).
* The staff effect on *development* is not new: `youth_progression_mult` (floored at neutral, via
  `training::compute_coaching_bonus`) is shown as "your youth coaches speed up the academy by N %", the figure the
  training model already applies.

## 2. What was exact, and what is now assessed

| Surface | Before | Now |
| --- | --- | --- |
| `AcademyRow.potential`, `expected_ovr_next_year` (`get_academy_review`) | exact | removed; `ceiling` is an assessed band (`RatingView::Estimate`, Mission 2's type) |
| Verdict, reasons, order of the review | decided on the true ceiling (`ceilingBelowCover`, `stillImproving`, `succession`, the sort by `potential`) | decided on a **believed copy** of the player whose ceiling is the middle of the band; "unlikely to progress" only when even the top of the band falls short |
| Youth Academy tab: prospects table | `potential`, tier letter, growth bar `+N`, sorted by potential; "Avg Potential" / "High Potential" tiles | the band, in the review's order; "Avg ceiling (est.)" / "High ceilings" from the review summary |
| Inbox, a signed youth prospect | exact ceiling, growth bar, tier letter | the band; growth stated as only known as a range |
| MCP `academy_get_review` | `potential_ceiling`, `expected_rating_next_year` | `ceiling` range + `ceiling_confidence` |

### The one deliberate refinement of Mission 2

`youth_prospect_readings` gave a **signed** prospect an exact ceiling ("then exact"). A signed prospect is an
academy player, so it now gives the same band the academy screen does (read from the live player, not the
message's snapshot). His *rating* still reads exactly and the unsigned standings are untouched. The Mission 2
test of that function keeps every assertion except the signed ceiling, which now asserts a band and that the exact
figure is not on the wire. Nothing else of Mission 2's rules or tests changed; the inbox test that captioned an
exact signed ceiling with its tier letter was replaced by one asserting the band and no tier letter.

## 3. Decisions the manager can now take, and why

`AcademyRow.advice` (`academy_advice.rs`) says for each action whether it is **recommended**, **available** or
**blocked**, with reasons. It decides nothing new: every block is a rule the backend already enforces.

| Action | Backend | Blocked when | Recommended when |
| --- | --- | --- | --- |
| Promote to the senior squad | `set_player_squad_role` | out on loan (`promote.onLoan`), no senior place (`promote.noPlace`, the one `signing_place_refusal` rule) | the verdict is *ready*, or he is past the academy age limit |
| List for loan | `toggle_loan_list` | already out on loan | the verdict is *loan candidate*; says promote instead when he is ready |
| Stay in the academy | (nothing: it is what happens) | never | the verdict is *keep developing*; notes his last year and the age limit |
| Renew the contract | the player's own renewal flow | n/a | his contract ends within the renewal horizon (183 days), unless he is unlikely to progress |
| Individual training focus | `set_player_training_focus` | n/a | a selector on the row |
| Release | the player's termination flow | n/a | unchanged |

Present in the backend but **not reachable from the academy screen before**: the individual training focus (the
training screen lists only the first team) and contract renewal. Both are now on the row. No loan marketplace and
no training engine were added.

## 4. Old saves

No serialized type changed. Both `Game` and `Player` still carry the exact `potential`; the assessment is computed
on read, so an old save has assessments from the first load, and a save written now loads in the previous build.
`db/tests/academy_assessment_reload_tests.rs` writes a real `.db`, reads it with a fresh `SaveManager`, and checks
the bands and the youth summary are identical and the save still holds each player's exact potential.

## 5. Tests

Mutation checks (the fix removed, the named test seen failing, restored):

| Mutation | Test that fails |
| --- | --- |
| list order sorts on the true ceiling | `the_default_order_does_not_follow_the_true_ceiling_inside_a_band` |
| the believed copy keeps the true potential | `two_players_whose_ceilings_share_a_band_read_identically`, `the_default_order_…` |
| the grid replaced by a centred error band | five tests: `the_band_always_holds_the_true_ceiling`, `two_ceilings_in_one_band_read_the_same`, `the_band_never_starts_below_the_players_own_rating`, and the two above |

A first version of `two_players_whose_ceilings_share_a_band_read_identically` **did not** catch the second
mutation (both players happened to get the same verdict from the true ceiling). Its fixture now finds a rating at
which the two true ceilings give different verdicts when read directly, asserts that, then asserts the rows match.

Scenarios: unit (band holds the truth for every rating pair at every width, nesting, department precision, the
review's wire has no `potential`, order, save round trip of `Game`), advice rules, command-level (loan, full
squad, training focus, overage youngster), a real-database reload, and **a three-season career in a temporary
database**.

### Multi-season scenario

`season-harness/tests/m3_academy_decisions.rs`, `#[ignore]`d like the other long studies:

```text
M3_SEASONS=3 cargo test --release -p season-harness --test m3_academy_decisions -- --ignored --nocapture
```

A scripted manager acts only on the review (promotes what the advice recommends while a place is free, lists loan
candidates). Each season one academy youngster is also **lent out for three weeks** (the club keeps his contract),
which is the only way a scripted run meets a blocked promotion; the daily loan processing must bring him back.
At every season end the game is written to a `.db` in a temporary directory, read by a fresh `SaveManager`, and
**the career continues from the reloaded copy**. Result (seed 1, 3 seasons, 31 s in release): 152 weekly reviews,
9 promotions, 2 loan listings, 12 blocked rows, 13 academy players seen, an intake in each of the 3 seasons, each
lent youngster back at his club with one record and no loan. Held on every day and week: no duplicate id; no
academy player ever leaves `game.players`; registered seniors never above the limit; no `potential` in the review;
every band holds the true ceiling within 24 points; two reads on a day agree; the review is identical across each
reload; advice is blocked **exactly** when the player is out on loan or no place is free.

Not exercised by the career: a **full senior squad** (no run reached the limit), so `promote.noPlace` is covered by
the command tests (`commands/academy_decisions_tests.rs`: refusal by key, the player left a Youth, the same
promotion succeeding once a place is free), not by the scenario. The scripted run applies a promotion by setting the
role directly (as the I7 study does); the command path is the one those tests drive.

## 6. Exceptions kept

* The serialized `Game` still carries every true value (documented since Mission 2). A person with developer tools
  can read `potential`; screens and the review API never carry it.
* `market_value` of a prospect and a youngster is derived from rating and potential in the generator and stays a
  public market figure (Mission 2's exception).
* AI clubs, the development model, aging and the squad-planning code read the true `potential` (the engine
  boundary): only the manager's review reads the believed copy.
* The Player Profile screens were not changed in this mission (they never printed `potential`).

## 7. Reviewers (run on `9cf4db5d..HEAD`) and what was done

| Reviewer | Result | Action |
| --- | --- | --- |
| `i18n-auditor` | no missing or English-copied key, placeholders match, every advice reason has a key; Czech and Turkish addressed the manager informally where their neighbours are formal; three locales named staff posts differently from `staff.posts.*` | fixed: formal address in cs/tr, the existing post names in de/ru/tr. Not changed: `id` mixes `kamu`/`Anda` in older keys |
| `ui-accessibility-reviewer` | no blocker; a px-literal width on the new Select; `aria-describedby` could dangle; new text on raw palette classes in two unmigrated files | fixed: `max-w-32`, the attribute only when reasons exist, the new inbox note on role tokens. Not done: the two stat tiles still use the file's old gray classes; contrast, keyboard use of the listbox in a table cell and focus return after an action were **not verified** |
| `ofm-architecture-reviewer` | the review path never reads the true potential; no serialized type changed; simulation and AI still read the truth. Two fail-open spots: a signed prospect later sold read as an exact stranger, and the believed copy of a non-own player kept the true ceiling | both fixed to fail closed, with a test written first (seen failing); `AcademyLens` narrowed to the crate. **Not done:** `YouthAcademyTab()` (about 520 lines) and `InboxMessageDetailPane()` (about 430) exceed the 200-line frontend limit; they were already over and this change made them smaller, not compliant. `academy_review`/`academy_advice` import each other (in-crate, compiles) |
| `ofm-test-reviewer` | 16 mutations: the privacy mutations were killed; four survived (optimistic top of the band, ownership guard, the 70 cut-off, the high-ceiling count) plus the harness meeting no blocked row, the MCP mapping and the advice-key coverage unproved | added: the optimistic rule at `judge` and through the review (mutation re-run, killed), the summary figures, the ownership guard with a loaned-out player, the 70 / 69.9 / 50 / 49.9 cut-offs, the remaining advice reasons and an age-limit boundary, a test that every reason advice can give has an `en.json` key, the MCP mapping as a pure function with a test, command tests asserting error keys with a positive control and state-unchanged checks, a loaned youngster in the career, a staffed youth department across a real reload. Removed a vacuous test. Not added: keyboard-only tests of the new controls |
| `ofm-dedup-reviewer` | one-point band printed as "72-72" in the MCP text; the confidence formatter twice; the sort key not taken from `sort_value` | fixed all three. The copied `administered` setup in the new harness file stays (also in four other test files): consolidating into `season_harness` is the right home, **not done**, named for #589 |

One finding was mine, not a reviewer's: the stored **Wonderkid** trait is awarded only to a young player whose true
ceiling is S-tier with 14 or more to grow, and its badge tooltip lists "Potential 90+". Any trait list (profile,
squad, academy tables) therefore revealed a bound on the hidden ceiling. `TraitList` now leaves it out (the
standalone `TraitBadge` and its test are unchanged); the band and the "High ceilings" tile carry the information
instead. This changes the profile and squad screens as well as the academy, and is a judgement call: the trait is
still stored and still read by the game.

## 8. Validation and final state

See the end of this file.

### Final run (code state `3a5720ac`; this document is committed after it)

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 5312 passed, 0 failed (134 ignored: need a display, the nightly, or are the long studies) |
| `cargo test --lib --features mcp`, `-p mcp-results` | 705 and 14 passed, 0 failed |
| `m3_academy_decisions` (3 seasons, `--release -- --ignored`) | passed in 31 s |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean |
| `cargo fmt --check` | clean |
| `npm test` | 3217 passed, 4 skipped, 0 failed (includes `src/i18n`, `architecture.test.ts`, `designSystem.test.ts`) |
| `npm run build` (typechecked), `format:check`, `lint`, `knip` | clean (lint reports 3 pre-existing infos) |
| Mutation checks | 3 of 3 of mine caught, one fixture found weak and repaired; the test reviewer's 16 (12 killed, 4 survived and now covered, one re-run and killed) |
| Reviewers | 5 of 5 run (section 7) |

**Not verified:** the real desktop window was not operated, so no screen was seen by eye, no screen reader or
keyboard-only pass was done on the new controls (the training-focus listbox inside a table cell in particular),
and contrast of the new text in both themes is unmeasured. `npm run audit:i18n` reports nothing for the touched
files, but it is only a heuristic. The scripted career never filled the senior squad, so that block is covered
by command tests rather than by the career.
