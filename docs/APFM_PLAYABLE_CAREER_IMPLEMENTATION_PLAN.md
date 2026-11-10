# APFM playable-career implementation plan

Based on `APFM_CLAUDE_TAKEOVER_AUDIT.md` and `APFM_PLAYABLE_CAREER_GAP_MATRIX.md`
(HEAD `92b2c616`, branch `claude/apfm-playable-career`).

Principles: reuse the integrated F4–F5.4.5 code; no React copy of a Rust rule; no mock data on a
career screen; do not touch the official finalisation path, transfer authorisation, or save
schema; one stage = one commit = tests first.

## 1. Baseline to beat (all RUN at HEAD `92b2c616`)

| Gate | Value |
|---|---|
| `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -D warnings` | pass |
| Rust workspace `--no-fail-fast` | 122 targets, 4,698 passed, 0 failed, 75 ignored |
| `npm test` | 332 files, 2,920 passed, 4 skipped |
| `npm run build` | pass |
| Gate-world squad study, 5 seeds × 2 seasons (release) | median **10.147 s**/seed (9.470–10.216), 4,184 buyer evaluations/seed, XI fit 67.97–68.41, unused joiners 9.1–18.4 % (audit §7) |
| GUI playthrough | **none performed** |

## Status update (later in the same day)

* Stage 1: **already done** at HEAD `5f5a4dfc`, committed after this plan was written.
* Stage 2: **done**, `9ef59840`. No backend change was needed: `delegation.proposals` already reaches
  the frontend in the serialized `Game`; only the TypeScript type omitted it.
* Stage 3: **done as a gap-fill**, `c2533ba6` (`src-tauri/src/application/playable_journey.rs`).
* Stage 4: checklist written, **not performed**. Stages 5-8 not started.

See `APFM_PLAYABLE_CAREER_IMPLEMENTATION_REPORT.md` for evidence and limits.

## 2. Stages

Risk scale: **L**ow / **M**edium / **H**igh = chance the stage needs rework or exposes a deeper defect.

### Stage 1 — Tactics: basic by default, advanced on request (gap G1) · risk L

* Change: the Blueprint (nine-dial) panel in `TacticsRightPanel` starts collapsed; its header says it is
  advanced. No new backend call, no rule moved to React.
* Tests first: Testing Library test that the panel body is absent on first render and appears after
  the header is activated, and that presets/roles remain visible.
* Acceptance: existing tactics tests still pass; `npm test`, `npm run build`; en/fr strings unchanged
  or added through `/add-ui-string` in every locale.

### Stage 2 — Pending delegation decisions visible on the dashboard (gap G7) · risk M

* Change: add one alert kind to the dashboard alert model for pending proposals (count > 0),
  navigating to the Staff tab. **The data does not reach the dashboard today**: the frontend
  `GameStateData.delegation` type carries only `protected_players` (`src/store/types.ts:1051`);
  proposals are read by the Staff tab through the staff service. So this stage needs either a
  small backend projection of the pending-proposal count (with `#[serde(default)]`, and the
  `/add-domain-field` checks if it is persisted — it should not be) or an extra service call from
  the dashboard.
* Tests first: alert-model unit test (no proposals → none; N proposals → one alert with count and
  target tab); component test for navigation.
* Acceptance: alert disappears after `resolve_delegation_proposal`; new string translated in all
  12 locales; no change to delegation policy.
* Risk: choosing between a DTO projection and a second call; prefer the projection so the alert
  updates with every game-state refresh.

### Stage 3 — Automated journey test through the application layer · risk M

* Change: one Rust integration test that plays the whole journey with the same `application::` functions the Tauri commands call:
  create career → select club → set formation/XI/preset → advance days → play the user match (both
  `delegate` and the M12 path) → bid for a player → set delegation → save → load into a fresh
  `StateManager` → finish season → next season → second season → reload. It asserts invariants
  (one finalisation per fixture, table equals results, cash ≥ ledger, same state after reload).
* Reuses `season-harness` drivers and existing scenarios; adds no new rule.
* Acceptance: red first for a deliberately broken step (e.g. skip save) to prove it can fail; green
  at HEAD; runtime recorded.
* Risk: much of this exists in `season_lifecycle`, `delegated_career`, `interactive_season`; the new
  value is the single end-to-end path including a transfer and the M12 branch. If those overlap fully
  the stage reduces to a gap-fill.

### Stage 4 — Manual GUI acceptance checklist and first human playthrough · risk H (unknown unknowns)

* Deliverable: `APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md`; a person runs
  `scripts/gui-validation/run-isolated.sh` and ticks it, light/dark and EN/FR.
* Acceptance: every checklist row ticked or filed as a defect with a screenshot. Only then may the
  journey be described as "verified".
* Risk: this is where defects hidden behind green component tests will appear. Reserve time.

### Stage 5 — Column chooser for squad and transfer tables (gap G2) · risk M

* Change: persist a per-table set of visible columns in settings; no new data, only visibility.
* Tests first: model test (default set, toggle, persistence round-trip); component test.
* Risk: touches `settings` schema → needs `#[serde(default)]` and an owner decision on whether
  this may change the settings file (CLAUDE.md rule 4). Do not start without that approval.

### Stage 6 — Model ↔ picker consistency (audit F1) · risk H

* Decision first (audit §9.1). Either make `squad_intelligence::model` use the greedy walk the AI
  actually plays, or change the AI picker. Both move transfer behaviour and therefore every balance
  number.
* Tests first: a property test asserting model XI == fielded XI for the generated squads.
* Acceptance: gate-world squad study before/after, five seeds; unused-joiner and XI-fit not worse.

### Stage 7 — `engine` versus M12 distribution study (audit F4) · risk M

* Measurement only. Run both engines on identical inputs; compare goals, shots, cards, home
  advantage. Output is a report, not a change.

### Stage 8 — Shipped-world performance and long career (audit F2/F3) · risk M

* After stages 1–3: repeat the gate-world five-seed timing, then one shipped 440-club season, then
  the 30-season run (multi-hour, run alone on a quiet machine). Gate: no regression in the
  harness metrics relative to the recorded baseline.

## 3. Order and dependencies

1 → 2 → 3 can ship independently. 4 needs a person and should follow 1–3 so the human tests the
current UI. 5 waits for the schema decision. 6 waits for the owner's choice. 7–8 are studies and
must not run while anything else is using the machine.

## 4. Definition of "playable" used by this plan

A new player completes a two-season career through the window alone, with no developer console and
no placeholder data, **and** a person has ticked the manual checklist. Passing stages 1–3 raises
confidence; it does not meet this definition.
