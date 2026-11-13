# APFM I2.2: Global Market UX

Branch `deepseek/i2-2-global-market-ux`, based on `73db8d43ff1e123948054a665004a5cb9b9a1a49`.
Local commits only: nothing pushed, nothing merged. Companion:
[`APFM_I2_2_VALIDATION.md`](APFM_I2_2_VALIDATION.md) (commands, results, verdict).

The scope is the frontend Global Market only: the tab, its filters, results table and profile
card, the model helpers they read, the shared `Button` ref they needed, the dev page harness, two
dev-only browser checks, tests, locales and these docs. No Rust, registry, transfer-engine,
financial, M12, formation or schema change.

## 1. What was wrong

The defects are the ones recorded in
[`APFM_I2_INTEGRATION.md`](APFM_I2_INTEGRATION.md) §5 item 2, re-read in the source before changing
anything:

| Defect | Where it showed |
| --- | --- |
| Selecting a player changed a tint, not the keyboard's place | `GlobalMarketResults` row button → detail card |
| Signing dropped focus on the row that had just closed | `confirmSign` → notice |
| Paging to an end disabled the control that had focus | previous/next buttons |
| The selected row was colour only | `tr` had no `aria-current`, no text or icon |
| The sticky profile could hide the Sign button on short windows | `xl:sticky` card had no height or scroll |
| Text under 12 px and raw `text-xs`/`text-2xl` sizes | header, tabs, dialog, profile |
| The results table had no caption | `table` |
| Live regions were inserted with their message already inside | notice, error, range, count |
| A backend refusal could surface as a raw key or an English exception | market calls |
| The Sign button explained nothing when it was disabled | negotiation panel |
| A native `<select>` was rebuilt in the filter grid | position, availability, sort |
| Search only ran when a filter changed, not on Enter | filter form |

## 2. What changed

### `GlobalMarketResults.tsx`

- A real `<caption>` (`globalMarket.tableCaption`) names the page; every header cell is
  `scope="col"`.
- The selected row carries `aria-current="true"`, a check icon, a left border and the visually
  hidden word `globalMarket.selected`, so selection is not a tint alone.
- The table sits in a named, focusable `role="region"` so a keyboard can scroll it sideways on a
  narrow window (with a line-level, reason-carrying Biome suppression for the required `tabIndex`).
- The result range is a permanent, polite live region. It exists before its text changes, so a
  screen reader announces the new page.
- Paging records the direction in a ref. When the page lands on an end and the pressed control
  becomes disabled, focus moves to the control that is still usable.
- Closing the profile returns focus to the row button that opened it, unless a signing has moved
  focus to the confirmation. The element is checked with `isConnected` before use.
- Player names wrap (`break-words`) and the table has a `min-w-[48rem]` inside its own scroller, so
  a long name does not push the page sideways.

### `GlobalMarketFilters.tsx`

- The filter grid is a `<form role="search">`: Enter in any field submits, and the Search button is
  `type="submit"`. Reset and sort-direction buttons are `type="button"`.
- Position, availability and sort use the shared `Select`, named with `aria-labelledby`.
- The result count is a permanent polite live region, not a message node re-created on every
  search.

### `GlobalMarketDetail.tsx`

- The profile is a focusable `role="region"` named by `globalMarket.profile`; loading a player moves
  focus to it, so choosing a row is navigation for keyboard and screen-reader users.
- On `xl` the card is sticky, capped at `calc(100dvh - 8rem)` and its body scrolls
  (`min-h-0`, `overflow-y-auto`, `overscroll-contain`): the Sign button stays reachable on a short
  window instead of being clipped below the fold.
- The outcome is a permanent polite live region. The action error is a permanent assertive region
  with a visible card when it has content.
- The Sign button is followed by an explanation when it is disabled: check the terms, accept a
  counter-offer, or this move cannot be signed as it stands (`signHintCheck`,
  `signHintCounter`, `signHintRefused`).
- Summary, attributes, terms and the outcome use one- or two-column grids and wrap long names and
  club names.
- All type sizes are role tokens (`text-caption`, `text-body`, `text-heading`, `text-title`).

### `GlobalMarketTab.tsx`

- One permanent polite status region carries loading, empty and success messages; a signing focuses
  it. The previous implementation mounted a new `role="status"` with the message already inside, so
  the announcement depended on when React inserted the node.
- One permanent assertive alert region carries a market failure and a retry button; it is not
  re-created after the failure.
- The confirmation dialog is capped at the viewport and scrolls its own body.
- Header and dialog use role type tokens.

### `globalMarketModel.ts`

- `marketErrorMessage(error, t)` resolves a backend translation key (including encoded parameters)
  and replaces a key no locale knows, and a transport failure, with `globalMarket.errorGeneric`.
  Raw keys and English exceptions no longer reach the player.
- `signHintKey(outcome)` is the one rule for which explanation the disabled Sign button shows.

### `ui/Button.tsx`

- Optional `ref` support (React 19 ref-as-prop) so pagination can move focus to a button without
  duplicating the button's visual classes.

### Dev harness and browser checks

- `HARNESS_PAGES.globalMarket` mounts the real dashboard on the Global Market tab.
- `globalMarketFixture.ts` supplies 30 deterministic market players, five pages of results, long
  names and clubs, free agents and contracted players, and three negotiation outcomes: agreed,
  player-refused and blocked by a full squad. Unknown ids are declined, never invented.
- `scripts/global-market-gui-check.mjs` drives selection, negotiation, confirmation, signing,
  paging and focus in headless Chrome, and reads back the caption, `aria-current`, focus targets,
  page labels and computed text sizes.
- `scripts/lib/chrome-cdp.mjs` holds the Chrome launch, DevTools and CDP helpers now shared by the
  layout sweep and the interaction check. `CHROME_PATH` or the installed macOS/Linux Chrome is
  used.
- `scripts/page-layout-check.mjs` adds the Global Market at 1440×900, 1280×800, 1024×768, 900×650
  and a narrow 640×800, plus German at 1024×768 and the existing extra-large 960×640 worst case.

## 3. Feedback for refusals and failures

The screen already received a structured outcome from `negotiate_external_transfer`: status, four
booleans and a `blocker` translation key. It now presents all of them:

| Backend answer | What the player sees |
| --- | --- |
| `status: agreed`, `blocker: null` | "Agreed", the terms, four passed checks, an enabled Sign button |
| `status: club_countered` / `player_countered` | The tone, the other side's terms, "Use their terms", and a disabled Sign button that says to accept first |
| `status: club_rejected` / `player_refused` / `blocked` | The tone, the reason, a failed check, and a disabled Sign button with an explanation |
| `blocker: "be.error.*"` | Translated with its parameters; an unknown key becomes `globalMarket.errorGeneric` |
| invoke rejection or transport failure | `globalMarket.errorGeneric`, never an English exception |
| search failure | An assertive alert card with a retry action |

No success message is shown before `finalize_external_transfer` resolves with the mutated career.
The confirmation dialog is the only path to signing, and it stays open, busy, while the call runs.

## 4. Accessibility

- Selection, signing, closing and paging all place focus deliberately; the browser check reads the
  active element back after each step.
- A selected row is exposed as `aria-current`, with a check icon and a visually hidden "Selected".
- Every live region exists before its message changes.
- The table has a caption and column headers; the table's sideways scroller is named and focusable.
- The filter grid is a search landmark; the shared select carries an accessible name.
- The dialog keeps the shared `Dialog` focus, Escape and Tab-trap behaviour; its content scrolls.
- Type is drawn only with role tokens, so `ui_scale` moves all of it and nothing is under 12 px.
- Focus rings use `ring-focus` on `focus-visible` only; no raw palette colours; the design-system
  checks now include the Global Market files in `FINISHED_FOLDERS`.

## 5. Responsive behaviour

Checked in the real layout engine at 1440×900, 1280×800, 1024×768, 900×650 and 640×800, in English,
and at 1024×768 and 960×640 in German at extra-large scale. The page does not scroll sideways and
nothing reaches past the right edge; the table scrolls inside its own region. Below `xl` the profile
stacks under the table; at `xl` it is sticky with an internal scroll. Results: validation §3.

## 6. Locales

Six new keys in English, translated into all eleven other locales:

`globalMarket.tableCaption`, `selected`, `signHintCheck`, `signHintCounter`, `signHintRefused`,
`errorGeneric`.

Targeted quality fixes, limited to defects that could be verified in the file:

- **pt-BR**: `A ler` → `Carregando`; `Prémio` → `Bônus`; `se juntou ao` → `assinou com`;
  `Pedido` → `Preço pedido`; a clearer confirmation sentence and counter-offer wording.
- **cs**: `Proti nabídka` → `Protinabídka`; count lines reworded so the number is not inflected
  (`Na trhu: {{count}}`); `years` reduced to the bare count where the label already says
  "Contract years"; the counter-offer sentence reworded.
- **ru**: the same count-line and bare-year treatment, and the counter-offer sentence reworded, so
  no plural form of the number is asserted.

The audit still reports pre-existing hard-coded strings elsewhere (`SimLab`, the style guide and
Rust-facing crates); none is in the Global Market and none was changed here.

## 7. Out of scope, left alone

- The Squad tab scrolls sideways at 960×640 in the layout sweep. It is pre-existing, outside the
  Global Market, and not caused by this branch; the sweep reports it and validation records it.
- The in-game transfer and free-agent screens, the Rust commands, the registry, the negotiation
  model and the 30-player limit are unchanged.
- `preview_external_recruitment` / `recruit_external_player` are still registered and still unused
  by this screen (integration §5 item 5).
- The local pagination footer remains local: no shared pagination primitive exists to reuse. The
  buttons it draws are the shared `Button`, and the native `<select>` is gone.
- A human visual pass and a screen-reader pass on the Tauri desktop app were not available; see
  validation's honest limits.

## 8. What I2.1 needs from the backend

No backend change is required for this UX pass. To keep it working as the market evolves:

1. Keep every `blocker` and error a translation key (`be.error.*`), optionally with encoded
   parameters; the UI now falls back to `globalMarket.errorGeneric` rather than showing a raw key.
2. Return `canAffordFee`, `wagePolicyAllows`, `canAffordBonus`, `windowOpen`, `blocker`,
   `clubDecision` and `playerVerdict` on every negotiation outcome. The screen presents these values
   and does not infer them.
3. Return `null` from `get_external_player` for an id no longer in the registry; the UI shows
   "profile unavailable" rather than inventing a player.
4. Keep `finalize_external_transfer` atomic: the success message appears only after the mutated
   career arrives.
5. If a page or count semantic changes, keep `items`, `total`, `page`, `pageSize` and `counts`
   consistent; the caption, range and page label are built from them.

## 9. Integration

The branch is ready to review as a local commit series. To integrate:

1. Run the frontend gates in validation §2 on the branch.
2. Open a PR against `develop`; disclose AI assistance in the description.
3. In a real Tauri window, run the manual checklist in validation §5.
4. Merge without changing the backend; no migration or save-format change is carried.
