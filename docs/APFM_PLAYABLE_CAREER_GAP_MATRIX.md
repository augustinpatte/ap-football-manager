# APFM playable-career gap matrix

Companion to `APFM_CLAUDE_TAKEOVER_AUDIT.md` (HEAD `92b2c616`, 9 November 2026).

Evidence levels used in the last column:

* **UI-test** — a Vitest/Testing Library test covers the component (passed in the 2,920-test run).
* **BE-test** — a Rust test covers the backend behaviour (passed in the 4,698-test run).
* **READ** — wiring confirmed by reading source only.
* **GUI: none** — nobody has driven the real window. This applies to **every row**; it is not
  repeated per row.

"Reachable from UI" means there is a React entry point that calls the backend command by a literal
`invoke` name. It does not mean the screen works well to play.

## 1. The journey

| # | Step | UI entry point | Backend | Reachable from UI | Evidence | Gap |
|---|---|---|---|---|---|---|
| 1 | Launch | `src/main.tsx` → `App.tsx` route `/` → `pages/MainMenu.tsx` | settings load (`get_settings`) | yes | UI-test `MainMenu.test.tsx` | none known |
| 2 | Create career | `components/menu/CreateManagerForm`, `WorldSelect`, `WorldDatabaseStep`; `handleStartGame` → `start_new_game` | `ofm_core::career`, `generator` | yes | UI-test; BE-test `career/tests.rs`, `generated_world_integrity` | none known |
| 3 | Choose club | `pages/TeamSelection.tsx` → `select_team` → navigate `/dashboard` | `select_team` | yes | UI-test `TeamSelection.test.tsx`, `useTeamSelection.test.ts` | none known |
| 4 | Inspect squad | `components/squad/SquadTab` (sortable roster) | `get_squad`, `get_squad_fitness`, `get_players_page` | yes | UI-test `SquadTab.test.tsx` | no column chooser (G2) |
| 5 | Choose formation / XI | `TacticsTab`, `TacticsPitch`, `TacticsCommandBar` | `set_formation`, `set_starting_xi` | yes | UI-test `TacticsTab.test.tsx`; BE-test `slices_squad_tests` | none known |
| 6 | Configure tactics | `TacticsCommandBar` presets; `TacticsRightPanel` roles and `PhaseBlueprintPanel`; custom tactics | `apply_tactic_preset`, `set_play_style`, `set_tactics_phase`, `set_player_role`, `save_custom_tactic` | yes | UI-test `TacticsTab.preset.test.tsx`; M12 propagation: F4 report §6 (200 paired seeds per comparison, DOC) | **G1**: advanced (Blueprint) controls open by default, no basic/advanced split |
| 7 | Advance calendar | `DashboardHeader` Continue, `hooks/useAdvanceTime`, `FastForwardDialog` | `advance_time_with_mode`, `skip_to_match_day`, `fast_forward_to_date`, `cancel_fast_forward` | yes | UI-test `useAdvanceTime.test.tsx`, `FastForwardDialog.test.tsx`; BE-test `fast_forward`, `f3_fast_forward` | none known |
| 8 | Handle inbox | `components/inbox/InboxTab`; Home recent-messages card | `get_messages_page`, `mark_message_read`, `resolve_message_action` | yes | UI-test; BE-test `messages_tests`, `inbox_offer_actions_tests` | whether each message action deep-links to the right tab is unverified |
| 9 | Prepare match | `DashboardMatchConfirmModal`, `check_blocking_actions` → `DashboardBlockerModal`, `PreMatchSetup`, `PreMatchLineup` | `check_blocking_actions`; the live session is created by `advance_time_with_mode` (modes `live`/`spectator`); `start_live_match` only restores a saved match | yes | UI-test `PreMatchSetup.test.tsx`, `DashboardBlockerModal.test.tsx` | none known |
| 10 | Simulate match | `pages/MatchSimulation.tsx`: legacy live (`MatchLive`), M12 replay (`M12ReplayViewer`), M12 interactive (`InteractiveMatchScreen`) | `start_live_match`, `step_live_match`, `simulate_spatial_match`, `start_interactive_match`, `advance_interactive_match` | yes | UI-test `MatchSimulation.test.tsx`, `InteractiveMatchScreen.test.tsx`; BE-test `live_match_manager_tests`, `interactive_match/tests.rs` | AI matches use the legacy engine, not M12 (audit F4) |
| 11 | Inspect result | `PostMatchScreen`, `RoundDigestScreen`, `PressConference` | `finish_live_match`, `finish_interactive_match`, `get_match_snapshot` | yes | UI-test `PostMatchScreen.test.tsx`, `RoundDigestScreen.test.tsx` | none known |
| 12 | Continue season | Continue loop; `HomeTab`, `ScheduleTab` | as step 7 | yes | BE-test `season_lifecycle`, `delegated_career` | none known |
| 13 | Negotiate transfer | `TransfersTab` + `useTransfer*` flows, `TransferBidModal`, `TransferCounterOfferModal`, `LoanOfferModal`, `FreeAgentContractModal` | `make_transfer_bid`, `counter_offer`, `respond_to_offer`, `make_loan_offer`, `offer_free_agent_contract`, `propose_renewal` | yes | UI-test `transfersService.test.ts`, `useTransfer*`; BE-test `transfers_tests` 143 incl. 26 realism, `deal_terms_tests`, `free_agent_talks_tests` | none known |
| 14 | Delegate a responsibility | `StaffTab` → `DelegationModeCard`, `StaffManagementPanel` | `set_delegation_mode`, `set_duty_delegation`, `set_delegation_limits`, `resolve_delegation_proposal` | yes | UI-test `StaffTab.test.tsx`, `StaffManagementPanel.test.tsx`; BE-test `delegation_scenarios`, `delegated_career`, `delegated_recruitment` | **G7**: a pending proposal stops a fast-forward and is listed in its report (`ofm_core::fast_forward::DecisionKind::DelegationProposal`, `FastForwardDialog`), but `dashboardHelpers.ts` has no alert for it, so a single-day advance gives no indication outside the Staff tab (READ) |
| 15 | Save | `DashboardHeader` save button → `saveGame` | `save_game` | yes | BE-test `db` round-trips and `season_lifecycle` (`application/saving.rs` itself has no `#[test]`) | none known |
| 16 | Quit | exit-to-menu flow (`DashboardExitConfirmModal`, `DashboardExitSavingModal`) | `exit_to_menu` | yes | UI-test `DashboardOverlays.test.tsx` | none known |
| 17 | Reload | `MainMenu` → `SavesList` → `load_game` → `routeForActiveCareer` | `load_game`, `get_saves` | yes | BE-test `season_lifecycle` (four seasons with a SQLite reload at every stage), `legacy_save_migration_tests` | none known |
| 18 | Finish season | Home swaps to `EndOfSeasonScreen`; `AwardsCeremonyScreen` | `check_season_complete` is **not** called by the UI; completeness comes from the game state (`lib/seasonContext.ts`) | yes | UI-test `EndOfSeasonScreen.test.tsx` | none known |
| 19 | Start next season | `EndOfSeasonScreen` → `advance_to_next_season` | `end_of_season` | yes | BE-test `end_of_season_tests`, `season_lifecycle` | none known |

## 2. Requested product requirements

| Requirement | Status | Where |
|---|---|---|
| Permanent sidebar | present | `DashboardSidebar.tsx` |
| Fast transitions / no full reloads | plausible, unmeasured | tabs are `lazy()` inside one route; no timing taken |
| Dense sortable squad/transfer tables | sorting present; customisable columns **missing** | `SquadSortHeader`, `TransfersMarketTable` |
| Functional inbox + alerts | present | `InboxTab`, `DashboardAlerts` |
| Next-action indicator | present (blockers, badges) | `check_blocking_actions` |
| Simple by default, advanced separate | **missing for tactics**; unassessed elsewhere | G1 |
| English + French | present, gated by tests | `en.json`/`fr.json`, `localeCoverage.test.ts` |
| Tactics reach the real simulation | verified in the engine reports, not re-run | `spatial_adapter::tactics`, F4 report |
| Delegation modes: automatic / approval / manual | present | `DelegationMode`, `delegation_scenarios` |

## 3. Ranked gaps

| Rank | ID | Gap | Why it matters | Size |
|---|---|---|---|---|
| 1 | V1 | No automated or manual GUI verification of the journey | Every "reachable" above is wiring, not play. A broken modal or unreachable state would be invisible. | needs a person or a UI-driving harness (does not exist) |
| 2 | G1 | Tactics: advanced dials open by default | Requirement 8 of the product direction | S |
| 3 | G7 | A pending delegation proposal has no dashboard alert (only the Staff tab and fast-forward report show it) | "Pending decisions visible and actionable" | S–S–M: add one alert kind to the existing alert model |
| 4 | G2 | No column chooser | Requirement 5 | M |
| 5 | F1 | Squad model ≠ AI picker | Transfer AI quality; affects balance numbers | M, needs an owner decision |
| 6 | F4 | `engine` vs M12 distributions never compared | AI table realism | L (study) |
| 7 | G6 | `set_team_kit_pattern`, `assign_jersey_number` have no UI | cosmetic | S each |
| — | P1 | Shipped-world performance after F5.4.3–5 not re-measured at scale | release gate | study, multi-hour |
