# APFM I2: P6 + P6.1 + D7B3 integration

Branch `integration/i2-unified`. Nothing pushed, nothing merged into `apfm/main`. No original branch or other worktree was modified.
Companion: [`APFM_I2_VALIDATION.md`](APFM_I2_VALIDATION.md) (evidence, commands, verdict).

## 1. What was integrated

| Line | Branch | Commits | Role |
| --- | --- | --- | --- |
| P6 + P6.1 | `claude/p6-match-realism`, `claude/p6-1-tactical-alignment` | `78715a44`, then `adb44b4a` (P6.1 is stacked on P6) | Integration baseline. Engine realism, role support, financial integrity, formation intelligence. |
| D7B3 | `deepseek/d7b3-world-market` | `8893208c`, `b5c9c6f6` | Global player/club registry as a market: search, negotiation, newgens, Tauri commands, Global Market UI, translations. |

All four expected commits existed with the expected hashes. P6 and P6.1 were already in the starting branch (`adb44b4a`); D7B3 was not.
The working tree was clean before the merge.

## 2. Git ancestry and merge strategy

Both lines fork from the I1 integration commit `ea469f3a` (P5 + D7B2). The merge base of `adb44b4a` and `b5c9c6f6` is `ea469f3a`.

```
*   801461fe style(ui): format the global market wage line
*   04f0f585 test(i2): the combined-career study follows D7B3's sort and negotiation semantics
*   393caa2f fix(registry): a signing bonus is judged with its fee; blockers and keys reach the player
*   ee594813 merge(d7b3): integrate DeepSeek D7B3 global market into P6.1      <- the merge
|\
| * b5c9c6f6 docs(d7b3): ...
| * 8893208c feat(registry): add the D7B3 global market and negotiation
* | adb44b4a docs(p6.1): ...   (P6.1 tip; its history down to 78715a44 and ea469f3a is intact)
```

A normal `git merge --no-ff --no-commit deepseek/d7b3-world-market`, resolved by hand, then committed. No squash, no rebase, no
cherry-pick, no `-X ours/theirs`. The original commits are reachable and unchanged. Three follow-up commits sit on top of the
merge, kept separate so each is reviewable.

## 3. Conflicts and how they were resolved

Two textual conflicts, both in `ofm_core/src/transfers/`, both about one function:

| File | Conflict | Resolution |
| --- | --- | --- |
| `transfers/registration.rs` | P6 and D7B3 each added `pending_incoming_purchases`. The two bodies were **byte-identical** (same filters, same doc comment); only blank-line placement differed. | One definition kept. |
| `transfers/mod.rs` | The `pub(crate) use registration::{...}` list: P6 added `pending_fee_commitments`, D7B3 added `pending_incoming_purchases`. | Both exported. |

Identical text is not the same as a coherent integration, so the *callers* were audited, which is where the real overlap was:

* P6 applies the senior-list limit to an in-game purchase through `execution.rs::ensure_place_for` (at agreement, counting promised
  places, and again at registration with none promised).
* D7B3 applies it to a global-market purchase through `external_registry.rs::squad_blocker`.
* They were the **same predicate** written twice (`free_places > promised`; `blocks_signing` is `free_places == 0`).

Unification: `transfers::registration::senior_place_refusal(game, team_id, promised)` is now the one place the limit is applied to a
purchase. `ensure_place_for` and `squad_blocker` both call it. `pending_incoming_purchases` remains the one source of `promised`.
Behaviour of both callers is unchanged; the existing tests of each still pass.

No semantic conflict touched the match engine, the AI, or M12: the merge changes **zero** files under `engine/`, `ai_formation.rs`,
`turn/`, or anything M12-related relative to `adb44b4a` (checked with `git diff --name-only adb44b4a ee594813`).

## 4. Compatibility decisions

| Contract | Decision | Evidence |
| --- | --- | --- |
| `MatchConfig::role_support_weight` | Untouched. `Default` is 1.0; `#[serde(default)]` yields 0.0 for a configuration saved before the field existed, as documented in `engine/src/types.rs`. P6 baseline behaviour at weight 0 and P6.1's role-support tests are unchanged. | No engine file changed; full workspace passes. |
| `FormationScore.fit` | Preserved. It is computed and reported; **it is not used to rank**. The shipped rule is P6's. | `ai_formation.rs` is byte-identical to P6.1. A three-seed, ten-season run produces output **identical** to P6.1 (see validation §5). |
| M12 / spatial engine / adapter | Not touched. P6.1's documented tactical differences remain future work. | Zero diff. |
| Senior squad limit of 30, including pending purchases | One implementation (above). Enforced at: in-game bid and offer, registration execution, global-market negotiation and signing. A free-agent signing is judged against the live list only; see §6 item 1. | New cross-path tests, one mutation-checked. |
| Global registry, negotiation, newgens, commands, UI, translations | Preserved as merged. | D7B3's tests all pass. |

## 5. Defects found and fixed

Found by running the integrated system and by the reviewer agents, each confirmed by reading the code before fixing.

| # | Defect | Cause | Fix | Evidence |
| --- | --- | --- | --- | --- |
| 1 | **A global-market signing could overdraw cash and the transfer budget.** | `negotiation_facts` judged the fee with a zero bonus and the signing bonus separately against raw cash, ignoring cash held for agreed purchases. Each check passed; together they overdrew. | The fee and bonus are tested together, against pending commitments, by the same `fund_block_after` the in-game path uses (`fee_and_bonus_block_after_pending`). | Two tests, seen failing first (status read `agreed`), now green. |
| 2 | A player could see a raw key. | `be.error.registry.negotiationNotAgreed` existed in no locale; `invalidOffer` and the contract-end error were spelled keys that exist nowhere. | New key in all 12 locales; the other two reuse the existing free-agent and contracts keys. | `src/i18n` gates pass; locale diff verified key-for-key. |
| 3 | A full squad showed as a raw key in the negotiation panel. | `t(outcome.blocker)` on an encoded `key?count=..` blocker. | `resolveTranslatedErrorMessage`. | Regression test fails without the fix (observed), passes with it. |
| 4 | The wage unit was an English `" / w"` in every locale. | Hard-coded string. | `formatWeeklyWage` (translated). | Existing component tests; i18n audit. |
| 5 | **The I1 combined-career study silently shopped for the worst players.** | D7B3 fixed an inverted registry sort; the study compensated for the old inversion with `descending: false`. | `descending: true`. | Study reports best OVR 87-95 of the market after the fix. |
| 6 | The study aborted on a normal refusal. | It assumed every refusal had a preview blocker; one-click signing is now a negotiation, and a counter or an unwilling player is a refusal without one. | The study checks the negotiation the screen shows against what signing does, in both directions. | Three 10-season runs pass. |

Defect 5 is the kind that would have produced a clean-looking, meaningless long-career result. It was caught only because the
study's own comment named the inversion.

## 6. Remaining technical debt and limitations

Ranked. None blocks a conditional GO; items 1-3 should be decided before the next feature phase.

1. **MEDIUM. A free agent can take the place promised to an agreed purchase.** A free-agent signing (the global market and P5's talks
   workflow) is judged against the *live* senior list, so with 29 seniors and one purchase agreed and waiting to register, a free
   agent can fill the 30th place. When the purchase's registration date arrives it fails and is withdrawn. It fails **cleanly**: the
   limit is never exceeded, no fee is taken, nothing is left pending. Pinned by
   `a_free_agent_can_take_the_place_an_agreed_purchase_was_promised_and_the_deal_fails_cleanly`. Not fixed here: the shared signing
   step is used by AI clubs too, so reserving there changes simulation outcomes against the P6.1 baseline, which is a decision for a
   phase that can re-measure it. Proposed: reserve in both paths and re-run the long-career baseline.
2. **MEDIUM. The Global Market UI has not been exercised in a running window, and a static accessibility review found gaps.**
   Not fixed (UI redesign is out of scope): focus is not moved after selecting a player, after a signing, or when paging disables its
   own button; the sticky detail card may cut off the Sign button on a short window; the selected row is a tint only; type below 12 px
   and raw sizes in the new folder (not yet in `FINISHED_FOLDERS`, so no gate fails); the table has no caption; several live regions
   are inserted with their content already inside.
3. **MEDIUM. Reservation untested on two in-game routes.** `respond_to_offer` and `counter_offer` reach the limit through
   `ensure_buyer_can_pay_standard_wage` with no boundary test of their own.
4. **LOW-MEDIUM. Duplication inside D7B3** (named for epic #589): `validate_negotiation_terms` is a verbatim copy of the free-agent
   `validate_terms`; the signing-bonus affordability check and payment are copies of the free-agent workflow's; the permanent-transfer
   mutation in `complete_contracted_transfer` re-implements part of `execute_transfer_with_terms`; the season-advance loop exists three
   times (the cache builder, `projected_market`, a test helper) and the copies already differ on a newgen that retires on arrival.
   Frontend: a native `<select>` and pagination footer rebuilt where primitives exist.
5. **LOW. Legacy `preview_external_recruitment` / `recruit_external_player` commands are still registered but the UI never calls them.**
   They no longer agree with the negotiation path (the preview does not foresee a player who refuses the move). Remove or make
   them delegate.
6. **LOW. Registry cache has no rule-version stamp.** Newgen intake changed from a fixed 900 a season to one-for-one replacement, but
   `REGISTRY_SCHEMA_VERSION` did not. A cache built under the old rule would be advanced under the new one. Matters only if a D7B2
   cache has shipped; not verified.
7. **LOW. Pending loan-ins are not reserved against the 30 limit.** Pre-existing; `pending_incoming_purchases` counts only purchases.
8. **LOW. Ordering in `complete_*_transfer`.** The player is inserted and money posted before fallible date and ledger steps. Each
   later error looks unreachable today; not reproduced.
9. **LOW. Translation quality** (reported by the i18n auditor, not independently verified): pt-BR strings in European forms
   ("A ler", "Prémio"); cs masculine participles addressing the manager; `{{count}} years` has no plural forms for ru/cs.
10. **OBSERVATION. In the imported 240-club world, 0-2 AI clubs are in debt each season**, with or without the user recruiting
    (control run). Pre-existing world behaviour, not caused by this integration.
11. **LIMITATION. The user's own club in the generated-world study ends a season with 10-20 seniors**, so the 30-limit is stressed
    only on the imported world (216 refusals), not the generated one.

## 7. Combined functionality delivered

One build with: P6's engine realism (price per contest, style defence, role support), financial integrity (cash rule, squad-floor
waiver, senior-list limit on purchases), M12 seat geometry; P6.1's role support through the resolvers and the formation evaluator
that **reports** an engine fit without ranking by it; and D7B3's 66 k-player global market (search, filter, sort, ownership,
negotiation with club and player counters, signing bonus, newgens, Tauri commands, Global Market tab, 12 locales), now under P6's
financial and squad rules and with the two fixes above.
