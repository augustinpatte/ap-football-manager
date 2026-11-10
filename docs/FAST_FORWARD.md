# Fast-forward: advancing the career to a date

Run B of the APFM validation programme. The player picks a date; the game plays every day up to it
with the ordinary simulation and tells them what it did, what stopped it, and what it went past.

There is **no second simulation**. A fast-forward is the loop a player runs by hand:

| Day kind | Function (unchanged) |
|---|---|
| Quiet day | `turn::process_day_with_capture` |
| Day the user's club plays | `matchday::play_user_matchday_with_capture` — the application's `delegate` mode |
| Season over | `end_of_season::advance_to_next_season` — the button the player clicks |

preceded each day by `turn::begin_day`, exactly as `advance_time_with_mode` does. Equivalence is
therefore structural, and it is also tested (see [Tests](#tests)).

## Where it lives

| Layer | File | Owns |
|---|---|---|
| Rules | `crates/ofm_core/src/fast_forward.rs` | policies, the day loop (`FastForward::step`), decisions, stop reasons, request validation |
| Application | `src/application/fast_forward.rs` | game lock (one day per `update_game`), the operation gate, cancel flag, stats store, blockers, progress throttle |
| Tauri | `src/commands/fast_forward.rs` | `fast_forward_to_date`, `cancel_fast_forward`, the `fast-forward-progress` event |
| Frontend | `src/services/fastForwardService.ts`, `src/hooks/useFastForward*.ts`, `src/components/dashboard/FastForwardDialog.tsx` | the dialog, progress, cancel, result |

`ofm_core::fast_forward::run` drives a whole run on a `&mut Game` for callers with no lock to release
(the season harness, benchmarks, tests). The application drives `FastForward::step` itself.

## Request and policies

```jsonc
{ "target_date": "2033-09-01", "policy": "stop_for_decisions" }   // fast_forward_to_date
```

The run ends with the clock reading `target_date` and nothing dated that day played, as after any
"Continue". Targets not after today are refused (`be.error.fastForward.targetNotInFuture`), as are
non-dates (`invalidDate`) and anything beyond `MAX_FAST_FORWARD_DAYS` (800, two seasons —
`targetTooFar`).

| Policy | User's matches | Season end | Decisions |
|---|---|---|---|
| `stop_at_next_match` | stops **before** the next one | stops (`season_complete`) | stops |
| `stop_for_decisions` (default) | played by the run (`delegate` mode) | stops (`season_complete`) | stops |
| `to_target_date` | played by the run | **rolled over** by the run | never stops; listed in the report |

Every policy also ends on: target reached, the manager being dismissed (`manager_fired`), a cancel,
the day limit, or an error. After any of them the world is **between two days**.

### What counts as a decision

Anything awaiting the player that was **not already open when the run began** (those are
*acknowledged*: the player saw them — the UI shows the blocker list before it starts — and chose to
carry on). New ones:

| Kind | Source |
|---|---|
| `blocker` | `application::time_blockers::compute_blocking_actions` (squad crisis, key contract risk, ...), identified by `id` + params so the same problem about other players is new |
| `delegation_proposal` | `game.delegation.proposals` — an assistant proposal awaiting an answer |
| `incoming_transfer_offer` / `incoming_loan_offer` | a `Pending` offer from another club for one of the user's players |
| `transfer_deadline` | the window closes **today** and talks the user is party to are still `Pending`; they end unanswered when the day is played (`transfers::expire_negotiations_at_deadline`) so this is the last moment to answer them |

A `to_target_date` run does not stop for these. It records each in `passed_decisions` (once, with the
date it was passed), so nothing is skipped silently.

## Cancellation, interruption, saves

* **One day per lock.** Each `step` is a whole day inside one `update_game`; the lock is free between
  days, so an MCP agent or a command that edits tactics lands *between* days.
* **Cancel** (`cancel_fast_forward`) is read at the top of each iteration: the run ends `cancelled`
  with whole days played. Nothing is rolled back and nothing is half-applied.
* **One operation.** The run holds `live_session::idle_operation` for its whole length: a save or a
  live match requested meanwhile waits for it instead of seeing a half-advanced world. A second
  fast-forward is refused (`alreadyRunning`).
* **Career token.** If the active career changes between days the run stops
  (`be.error.fastForward.careerChanged`) rather than playing into another career.
* **Save/reload.** A save taken at a stop is a world a player could have reached by clicking; loading
  it and finishing the run ends where the uninterrupted run ends (tested).

## Duplicate protection (finalisation, payments, news)

The run adds no write path of its own, so it inherits the single-day guards: a fixture is applied only
while `Scheduled`; payments run in `process_weekly_finances`/`process_transfer_obligations` once per
processed day; news and mail are guarded by `emitted_events`. What the run adds is *not* calling a day
twice: `step` plays a day or returns, and a stop leaves the clock on the unplayed day. The harness
invariants (results applied once, no stranded fixture, one record per season) are checked on every day
of a 150-day run in `season-harness/tests/fast_forward.rs`.

## Integration contract with the assistant coach (Run A)

The assistant works **inside the day**: `process_day` calls `staff_system::delegation::run_training`
and `run_after_day`. A delegated duty therefore runs on every day of a fast-forward with no code in
this feature. The contract Run A must keep:

1. Delegated duties act inside `turn::process_day` / `finish_live_match_day` (not in a command).
2. A step that needs the player's approval leaves a `domain::delegation::Proposal` in
   `game.delegation.proposals` (identity: `Proposal::id`) and stops acting until it is resolved.
   The run stops on a *new* proposal. If Run A moves proposals elsewhere, change only
   `ofm_core::fast_forward::pending_decisions`.
3. Auto-resolving a proposal (accept/refuse) must go through `staff_system::delegation::resolve_proposal`.

If delegation is off, or there is no assistant, `proposals` is empty and the run is unaffected: that
is the safe fallback.

## Tests

* `ofm_core::fast_forward` unit tests — validation, quiet run, decisions (new vs acknowledged),
  target-date report, stop-at-next-match.
* `season-harness/tests/fast_forward.rs` (80-club generated world, fingerprint comparison of results,
  tables, money, squads, contracts, tactics, inbox, news and the stats handed to the store):
  fast-forward ≡ single days over 60 days; across a season rollover; stop + resume ≡ one run;
  save → load → finish ≡ uninterrupted; deadline stop and deadline passed; postponed fixture;
  invariants on every day of a long run; policy behaviour.
* `application::fast_forward` tests — ≡ `advance_time_with_mode("delegate")` through the real state
  manager, single-slot refusal, cancel at an exact day boundary.
* Frontend: date helpers (month/year/leap boundaries), hook (progress, double click, cancel, fired,
  refusal, resume), dialog, header menu entry.

## Benchmarks

See the section at the end of this file, produced by
`cargo run -p season-harness --example apfm_ff_bench -- <seed> <gate|shipped> [rounds]`.

### Results

`apfm_ff_bench`, seed 1, one run per window, single machine (4 vCPU container, `dev` profile as the
workspace configures it: `opt-level = 1`, dependencies at 3). The windows start from the opening
snapshot of the career (July 2033). Every row also asserts that the fast-forward and the day-by-day
reference end in the **same fingerprint** (results, tables, money, squads, contracts, tactics, inbox,
news).

*daily* = one day at a time the way the screen plays it: the day, then `Game::ui_clone()` and its JSON
serialization for the screen (what IPC must do after each day). *fast-forward* = the same days in one
call plus a single `ui_clone` + serialization at the end.

**Generated world, 80 clubs / 3.7k players** (England 2 tiers + Portugal + Argentina):

| Window | Days | Daily (s) | Fast-forward (s) | ms/day (daily → ff) | Speed-up |
|---|---:|---:|---:|---:|---:|
| 1 week | 7 | 0.21 | 0.11 | 30.1 → 15.9 | 1.9× |
| 1 month | 30 | 1.01 | 0.39 | 33.7 → 13.0 | 2.6× |
| 3 months | 90 | 3.05 | 1.24 | 33.8 → 13.8 | 2.4× |
| 1 season (+ rollover) | 292 | 11.94 | 4.36 | 40.9 → 14.9 | 2.7× |

**Shipped world, 440 clubs / 13.9k players:**

| Window | Days | Daily (s) | Fast-forward (s) | ms/day (daily → ff) | Speed-up |
|---|---:|---:|---:|---:|---:|
| 1 week | 7 | 2.26 | 1.77 | 323 → 253 | 1.3× |
| 1 month | 30 | 11.6 | 8.69 | 387 → 290 | 1.3× |
| 3 months | 90 | 35.6 | 26.2 | 395 → 291 | 1.4× |
| 1 season (+ rollover) | 292 | 118.9 | 83.3 | 407 → 285 | 1.4× |

### Where the time goes

Per-phase timing of `process_day` (temporary instrumentation, not committed), shipped world, 30 days,
before any optimisation, 364 ms/day:

| Phase | Share |
|---|---:|
| `ai_squad_sales::apply_ai_listing_decisions` | 41 % |
| `ai_contracts::apply_ai_squad_planning` | 24 % |
| `transfers::generate_incoming_transfer_offers` | 13 % |
| `ai_contracts::apply_ai_contract_decisions` | 9 % |
| `run_training_ground` | 5 % |
| `simulate_the_rest_of_the_world` (matches) | 4 % |
| everything else (finances, news, board, scouting, ...) | < 4 % |

The cost is the AI transfer market, not match simulation: each AI club's weekly review scans every
player in the world (`calc_wages(game, club)` alone reads the contract ledger of all 14k players for
each reviewing club). A fast-forward cannot remove that work without changing the market, so it is
the remaining bottleneck (owned by Run C, below).

In the 80-club world, `daily − fast-forward` is the screen's state copy + serialization, which the
fast-forward pays once (the `ui_state_ms` column of the bench: 7.1 s of the season's 11.9 s).

### Optimisations made (no rule changed)

Both are behaviour-identical, and checked by the world fingerprint after 40 days (generated world:
`80d871f173b191f4` before and after) and 30 days (shipped world: `4c05e74deb1f5da9` before and after).

1. `finances::player_weekly_wage_for_team` decides whether the player can be paying the club (his club,
   or either side of a loan) **before** reading the wage from the contract ledger. `calc_wages` called
   it for every player of the world for every club.
2. `ai_squad_sales::apply_ai_listing_decisions` reads every wage bill once per pass with the existing
   `finances::wage_bills_by_club` instead of `calc_wages` twice per reviewing club.

Together: shipped world 340 → 281 ms/day (−17 %) on 30 days of `process_day`; generated world
−18 %.

### Remaining bottlenecks (not touched: Run C's files)

`ai_squad_sales`/`transfers::loans` (`list_loans_by_status`, `top_up_loan_listings`),
`ai_contracts::{apply_ai_squad_planning, apply_ai_contract_decisions}`, `transfers::market` and
`transfers::generate_incoming_transfer_offers` each rescan `game.players` per club. An index of
players by club built once per day (as `match_day_eligibility::positions_by_club` already does for
planning) would remove most of the remaining ~280 ms/day in a full world. That is a transfer-module
refactor, so it is left to the run that owns those files.
