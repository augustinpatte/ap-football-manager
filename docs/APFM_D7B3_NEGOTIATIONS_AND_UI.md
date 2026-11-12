# APFM D7B3 — Negotiations and global market UI

**Status:** implemented and locally validated, with a 2026-11-12 completion pass. **Date:** 2026-11-11.

Companion to [`APFM_D7B3_GLOBAL_MARKET.md`](APFM_D7B3_GLOBAL_MARKET.md). This document covers the
external-club and player negotiation protocol, its IPC surface, and the screen built on it.

## 1. Why D7B2's signing was replaced

D7B2 exposed `recruit_external_player(player_id)`: one click, a fee implied by the registry, and a
completed transfer. That was acceptable as a proof of the projection, but it is not a recruitment
market. It gave the manager no way to:

- offer less than the asking price and hear the club's answer;
- be turned down by the player;
- see whether the wage, bonus, window or squad rules allow the move;
- distinguish “the club wants more” from “the player refuses”;
- know that nothing had changed until a deal actually existed.

D7B3 keeps the old command for compatibility and routes it through the same negotiation and
completion path. The new path is explicit.

## 2. Protocol

### 2.1 Request

```ts
interface ExternalNegotiationRequest {
  fee?: number | null;          // offered to the selling club
  weeklyWage?: number | null;   // offered to the player
  contractYears?: number | null;
  signingBonus?: number | null; // paid on completion
}
```

Every field is optional. `null` or omission means “the buyer's standard term for this field”, which
is what makes the one-click path a real default offer rather than a hidden shortcut. The Rust
struct carries `#[serde(default)]`, so an older client payload still deserialises.

### 2.2 Outcome

```ts
type ExternalNegotiationStatus =
  | "agreed"
  | "club_countered"
  | "club_rejected"
  | "player_countered"
  | "player_refused"
  | "blocked";

interface ExternalNegotiationOutcome {
  playerId: string;
  freeAgent: boolean;
  status: ExternalNegotiationStatus;
  fee: number;             // the fee that would complete, or the club's counter
  minimumFee: number;      // the club's own floor, whatever was offered
  clubDecision: "accepted" | "countered" | "rejected";
  weeklyWage: number;      // the wage that would complete, or the player's ask
  contractYears: number;
  signingBonus: number;
  contractEnd: string;
  playerVerdict: "accepts" | "counters" | "refuses";
  playerScore: number;     // explainable, may be negative
  mainObstacle: string | null; // a FreeAgentTalkReason name
  canAffordFee: boolean;
  wagePolicyAllows: boolean;
  canAffordBonus: boolean;
  windowOpen: boolean;
  blocker: string | null;  // translation key
  fromClub: string | null;
}
```

A counter is **not** an agreement. `finalize` refuses `club_countered` and `player_countered` with
`be.error.registry.negotiationNotAgreed`. The manager accepts a counter by sending the countered
terms back as a new request; only then does the status become `agreed`.

### 2.3 Order of decisions

`negotiate_external_transfer` is pure and deterministic. It:

1. resolves the managed team and the career clock;
2. resolves the standard wage and length, then applies the manager's overrides;
3. rejects terms a contract cannot carry (`be.error.registry.invalidOffer`);
4. asks `ExternalOfferContext` what the selling club wants;
5. asks the player through the P5 appeal rules;
6. checks window, duplicate, squad, fee, wage policy and signing bonus;
7. returns every decision and every check, with no mutation.

The status precedence is deliberate: a club rejection is reported before a player refusal, a
permanent/financial blocker before a counter, and a counter before an agreement. The screen can
therefore present one authoritative state instead of guessing from the numbers.

### 2.4 Player willingness

The external path reuses P5 rather than writing a second consent model:

- `facts_for` / `appeal_of` / `counter_terms` for the player's score and counter;
- `Incumbent`, `club_standing` and `push_to_leave` for the selling club's standing;
- `expected_ovr_for_reputation` plus `ClubSquad::from_levels` to give the external seller a squad
  the P5 response can read.

`push_to_leave` was widened from private to `pub(crate)`; no rule changed.

### 2.5 Completion

`finalize_external_transfer` re-runs the whole negotiation against the current game, then:

- returns the `blocker` if any rule still blocks the move;
- refuses any status other than `agreed`;
- calls `complete_external_transfer`, the only mutating function.

`complete_external_transfer`:

- promotes exactly one player (duplicate identity is checked before and after);
- pays the fee from the transfer budget;
- pays the signing bonus with `CashKind::SigningBonus`, validated by the P5
  `ensure_bonus_affordable` and `finances::validate_posts` path;
- puts the player on the agreed wage and contract end;
- records the completed transfer and the outcome.

A failed validation mutates nothing. A second finalize for the same player fails with
`be.error.registry.playerAlreadyInWorld`.

## 3. IPC surface

| Command | Purpose |
|---|---|
| `search_external_players(request)` | One page of the global market. |
| `get_external_player(playerId)` | The profile behind a row. |
| `preview_external_recruitment(playerId)` | The pre-negotiation preview, unchanged. |
| `negotiate_external_transfer(playerId, request)` | The pure club-and-player answer. |
| `finalize_external_transfer(playerId, request)` | Complete an agreed deal, returning `{ game, outcome }`. |
| `recruit_external_player(playerId)` | Compatibility shortcut; delegates to finalize with standard terms. |

`negotiate_recruitment_internal` and `finalize_recruitment_internal` sit under the commands so the
same logic is callable without Tauri, matching the repository's `_internal` convention. The
commands are registered in `src-tauri/src/lib.rs`.

Frontend wrappers live in `src/services/registryService.ts`; `invoke` appears only in
`src/services/`.

## 4. Screen

### 4.1 Placement

A new **Global market** tab in the world section of the sidebar, between **Transfer centre** and
**Hall of fame**. The tab is lazy-loaded like its neighbours and is labelled through
`globalMarket.title`.

### 4.2 Architecture

| File | Responsibility |
|---|---|
| `GlobalMarketTab.tsx` | Header, counts, notice, layout, confirmation dialog |
| `GlobalMarketFilters.tsx` | Search, nation, club, position, availability, age, sort, direction, reset |
| `GlobalMarketResults.tsx` | Server-paged table, empty/loading/error states, pagination |
| `GlobalMarketDetail.tsx` | Profile, contract and club, attributes, negotiation panel, outcome |
| `useGlobalMarket.ts` | Controller: debounced search, detail load, negotiation and signing state |
| `globalMarketModel.ts` | Pure query/form mapping and presentation predicates |

The frontend holds no game rule. It maps controls to the backend's request, renders the backend's
outcome, and calls finalize only when the backend says `agreed` with no blocker.

### 4.3 Behaviour

- Default sort is OVR descending, page size 25, page 1.
- Search is debounced by 250 ms; every filter change returns to page 1; paging alone does not.
- Results show name, nationality, position, age, club, OVR, market value, wage and asking price.
- Free agents show “Free agent” instead of an asking price.
- Selecting a row loads the profile; the detail panel is sticky on wide screens.
- Attribute groups reuse `buildPlayerAttributeGroups`; the helper accepts the registry detail's
  partial attribute shape through `PlayerAttributeSource`, so the profile does not fork its own
  attribute labels.
- The negotiation form leaves a field empty to accept the club's request or the buyer's standard
  term.
- **Check the terms** is pure and repeatable. A counter offers **Use their terms**, which fills the
  form with the countered terms and re-submits them. Signing stays disabled until an agreement
  comes back.
- The outcome panel names the status, the party's message, the main concern and the four checks
  (fee, wage policy, bonus, window). A blocker is shown as a translation key resolved by the
  screen.
- **Sign player** opens a confirmation dialog with the final fee, wage, length, bonus and contract
  end. Only **Confirm signing** calls finalize. Cancelling changes nothing.
- Success updates the game through `onGameUpdate`, closes the profile and reloads the current page.
- An error from search, profile, negotiation or signing is shown as a translated backend key; the
  screen never invents an English message.

### 4.4 Accessibility and responsiveness

- The results table has column headers and an accessible name for each profile button.
- Pagination buttons have labels; the page range is text, not colour.
- The outcome panel has `role="status"`; a blocker has `role="alert"`.
- Each check exposes “passes”/“fails” to assistive technology, not only a glyph.
- The confirmation uses the shared `Dialog` shell: labelled heading, focus on open, Escape and
  backdrop rules, and `busy` while signing.
- The layout is a single column below `xl` and a results/detail split above it.

### 4.5 Locales

The `globalMarket` block is present in all 12 locales. Dynamic keys used by the screen exist in
every locale:

- `globalMarket.status.*` (six statuses);
- `globalMarket.position.*` and `globalMarket.availability.*`;
- `globalMarket.sort.*` (five keys);
- `freeAgents.reason.*` for the player's main concern.

`en.json` is the reference. Keys whose translation is genuinely the same word (for example
`Club` in Spanish and Italian) are declared in `src/i18n/INTENTIONAL_SAME.json` rather than
silently left untranslated.

## 5. Tests

Backend:

- `external_registry::tests::a_lowball_offer_is_rejected_before_any_mutation`
- `external_registry::tests::a_counter_is_deterministic_and_payable`
- `external_registry::tests::a_low_wage_gets_a_counter_and_only_the_counter_signs`
- `external_registry::tests::a_player_refuses_a_club_far_below_his_level_without_mutation`
- `external_registry::tests::a_club_counter_is_not_an_agreement`
- `external_registry::tests::an_agreed_signing_bonus_is_paid_and_budgeted`
- `external_registry::tests::invalid_negotiation_terms_are_refused`
- `transfers_tests::external::*` — the P5 integration path, including the two-step counter.
- `commands::registry::tests::negotiating_through_the_command_needs_the_counter_put_back_on_the_table`
- `commands::registry::tests::invalid_negotiation_terms_reach_the_command_with_a_key`

Frontend:

- `src/components/transfers/globalMarketModel.test.ts` — every sort key, availability and
  position; page reset; counter detection; signing gate; paging ranges.
- `src/components/transfers/GlobalMarketTab.test.tsx` — sort/direction/filter/page requests,
  profile load, agreed signing through the confirmation, counter acceptance, refusal and reason.
- `src/services/registryService.test.ts` — both negotiation commands and their payloads.

## 6. Compatibility and limits

- Old saves load: the new Rust fields are `#[serde(default)]`; no stored game shape changed.
- The registry cache format did not change; a D7B2 cache is reused if its season and key match.
- The old `recruit_external_player` command still works and now cannot bypass a refusal.
- A contracted purchase meets the 30-senior limit, including places promised to deals already
  agreed, on the P6 rule. The external route shares that reservation instead of adding a second.
- Inherited P5 limitation K3 is documented in the companion document and is not changed.
- The market remains a projection. External clubs do not negotiate with each other, do not have
  their own budgets, and do not simulate a competition.
