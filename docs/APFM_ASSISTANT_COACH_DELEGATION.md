# APFM — Assistant coach delegation (Run A)

Branch `claude/gameplay-delegation`, based on `validation/runs1-4-m12`. Builds on the per-duty
delegation of mission 9 (`APFM_DELEGATION_REPORT.md`); nothing there was replaced, so there is still
one delegation architecture: `domain::delegation::DelegationState` on `Game`, run from
`ofm_core::staff_system::delegation` at the two points of `turn/mod.rs` that already existed.

## What changed

| Requirement | Where |
|---|---|
| Manual / with approval / full | `DelegationMode` (`domain/delegation.rs`). **Derived** from the duty settings by `DelegationState::mode()` (no stored copy that could disagree), applied as a preset by `apply_mode`. `Custom` means the duties were tuned by hand. Command `set_delegation_mode`. |
| Lineup selection and rotation | New duty `Lineup` (`staff_system/delegation_squad.rs`). The evening before a match it names the eleven with `turn::squad::delegated_eleven`, which is the AI managers' own selector (`ai_select_starting_xi`): best positional fit, tired starters rested only for a deputy of the same position group who is clearly fresher and close in quality. Written to `Team::starting_xi_ids`, so the manager sees what will play. Set-piece holders who drop out are replaced; roles nobody held stay empty. |
| Training and recovery | Existing `Training` duty (the AI club planner, recovery before heavy weeks). Unchanged. |
| Basic tactics, fitness | Existing `MatchPreparation` duty (style adjustments and swaps from the recommendation engine). It now runs after `Lineup`. |
| Routine squad / admin | `Youth` (promotions), `Scouting`, `StaffRecruitment`, `StaffRenewals`, and the new `PlayerRenewals`. |
| Player contracts | New duty `PlayerRenewals`: contracts within six months of ending, at the terms the player would ask. Goes through the existing `delegated_renewals::delegate_renewals` (board wage policy, the assistant's own judgement, "too important to settle without you" for valuable players). |
| Separate permissions for money | `staff_system/delegation_policy.rs`: `authorize(delegation, duty, Spend)`; `DelegationLimits` (staff wage / player wage per week, total commitment of one contract, transfer fee). Anything above a ceiling is a **proposal**, whatever the duty's own setting says. |
| Observable, not noisy | `DelegationLogEntry` now has an `outcome` (`executed`, `approved`, `declined`, `leftToManager`); capacity 120. Inbox: one digest per fortnight at most (only if something happened), one reminder at most every 3 days while proposals wait, one notice per season if the assistant leaves while duties are handed over (`staff_system/delegation_report.rs`). Routine days (training, an unchanged eleven) write nothing. |
| Save/load, rollover | The state is one JSON row (`staff_delegation`); every addition is `#[serde(default)]`, so **no migration** was needed. Old saves load with the default limits and `Custom`/`Manual` as appropriate. The rollover touches nothing in it. |

### Decisions that are the manager's, by design

* `confirm_major` ("ask me") only governs *major* steps: hires, renewals, promotions. Lineup, training,
  match preparation and scouting are reversible and cost nothing, so when handed over they act
  directly in every mode.
* **Transfers are never executed by this run** (Run A). `max_transfer_fee` defaults to 0; Run E adds the recruiter that reads it.
* Releasing/replacing staff stays a proposal; the assistant never dismisses anyone alone.
* The assistant cannot be handed anything without being in post (`be.error.delegation.noAssistant`);
  taking back always works. If he leaves, nothing runs and the manager is told once per season.

### Determinism

No RNG is used. The only variation is the existing seeded "misjudgement" of who can deputise
(`selection_seed(team, date)`), floored at an assistant quality of 0.6 so a poor assistant rests
players later but never picks a side an AI club's manager would not. Same state, same eleven
(`the_lineup_is_deterministic_for_the_same_state`).

## Integration contract (other runs)

* **Transfers (Run E: done)**: `Duty::PlayerRecruitment` now calls `authorize` with a `Spend::Transfer` for
  every bid it makes; see `APFM_PAID_TRANSFER_REALISM.md` §3. A zero fee ceiling now sends every acquisition,
  including a fee-free one, to the manager. The text below is the original contract and still describes
  the call. Before executing a transfer on the assistant's behalf, call

  ```rust
  use ofm_core::staff_system::delegation_policy::{authorize, Authorization, Spend};
  match authorize(&game.delegation, Duty::Scouting, Spend::Transfer { fee, weekly_wage, years }) {
      Authorization::Auto => { /* execute, then game.delegation.push_log(...) with outcome Executed */ }
      Authorization::NeedsApproval(reason) => { /* game.delegation.propose(...) */ }
      Authorization::NotDelegated => { /* do nothing */ }
  }
  ```

  The fee counts toward `max_commitment`. A new `ProposalAction` variant and its arm in
  `delegation::resolve_proposal` are the only other places to touch. Pick the duty that owns the
  decision (a new `Duty` variant also needs its row in `Duty::ALL`, `period_days`, the front-end
  `Duty` union/`DUTIES` list and the `staff.delegation.duties.*` key).
* **Calendar fast-forward (Run B)**: nothing to call. Delegation runs inside
  `process_day_common`, so a fast-forward that advances days through `turn::process_day` gets it for
  free. A fast-forward that stops on "decisions waiting" can read `game.delegation.proposals`.
* **Viewer (Run D)**: no dependency. The delegated eleven is `Team::starting_xi_ids` like any other.
* Front end: `get_staff_overview` now returns `delegation_mode` and `delegation.limits`;
  `set_delegation_mode(mode)` and `set_delegation_limits(limits)` are the new commands.

## Tests

Rust (all green): `domain::delegation` (3 new: mode derivation, old-save loading),
`staff_system::delegation_policy` (4: undelegated, approval-required, each ceiling, transfers),
`staff_system::delegation` (6 new: unauthorised hire becomes a proposal in full mode, history
outcomes, manual⇄full switching, digest rate limit, proposal reminders, missing assistant, JSON
round trip), `staff_system::delegation_squad` (6: valid eleven + rest, determinism, injured starter,
too-early, renewal within limits, renewal above the limit then approved), `db::delegation_repo`
(SQLite round trip at non-default values), `commands::staff` (2: presets need an assistant, limits
validation) and the season-harness end-to-end `delegated_career` (a whole season under full
delegation on the 80-club world, SQLite save/reload at day 50 and after the rollover, 30 days of the
next season; the eleven is legal every day, no transfer is made, the assistant acts again after the
rollover). Front end: `StaffManagementPanel.test.tsx` (+4: mode switch, limits save, new duties,
history outcomes) and the locale coverage gates.

## Limitations

* The mode is a preset: switching to Manual and back to Full does not restore per-duty tuning
  (Manual clears `delegated` on every duty, `apply_mode` rewrites all of them).
* Renewals of valuable players usually come back to the manager: `delegate_renewals` judges them
  too important for the assistant to settle alone. That is intended; a career that never touches
  the contracts of its stars will lose them on a free.
* The lineup duty acts the evening before a match. A match on the first day of a career is played
  with whatever eleven the career opened with.
* Lineup rotation follows the AI managers' rules, so a delegated eleven is as good as a mid-table
  AI manager's, not better. That is deliberate (fatigue, form and tactics still decide matches).
* Locale strings for the 11 non-English locales were written by the author, not by a native
  reviewer.
* No MCP tool was added (the shared `_internal` functions are `set_mode`/`set_limits` in
  `commands/staff.rs` if one is wanted).
* The M12 crate `apfm-match-lab` is absent from this environment (see
  `APFM_CONSOLIDATION_RUN1_4.md` §6); the six M12-dependent tests fail by construction here and
  were not touched.
