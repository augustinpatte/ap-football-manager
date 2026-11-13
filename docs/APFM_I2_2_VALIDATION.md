# APFM I2.2: validation

Evidence and limits for [`APFM_I2_2_GLOBAL_MARKET_UX.md`](APFM_I2_2_GLOBAL_MARKET_UX.md). Branch
`deepseek/i2-2-global-market-ux`, base `73db8d43ff1e123948054a665004a5cb9b9a1a49`. Nothing pushed,
nothing merged. Host: macOS, Apple Silicon. Labels: **PASSED**, **FAILED**, **NOT RUN**. A local
pass is not a claim about CI.

## 1. Verdict

**CONDITIONAL GO.**

For: every requirement named for the screen is implemented and covered by a test or a browser
check; the Global Market passes the layout sweep at the five review widths plus two German
worst cases; a headless Chrome run drives selection, negotiation, confirmation, signing, paging and
focus and reads every result back as correct; the full frontend suite, lint, format, build, knip and
the i18n gate pass.

Against a plain GO: the Tauri desktop window was **not** launched. The interaction evidence comes
from the real React app in headless Chrome through the dev page harness, not from the packaged
shell, and there was no human visual review or screen-reader session. The manual desktop checklist
in §5 is therefore still required before merge.

Why not NO-GO: no defect in the reviewed screen remains open, no backend or save-format change is
carried, and the only failed sweep cases are the pre-existing Squad sideways scroll outside this
scope (§4.2).

## 2. Gates run on the working tree

| Gate | Command | Result |
| --- | --- | --- |
| Full frontend suite | `npm test` | **PASSED**: 344 files passed, 1 skipped; **3,039 passed**, 4 skipped (baseline 3,012; +27) |
| Focused Global Market tests | `vitest run src/components/transfers/GlobalMarketTab.test.tsx src/components/transfers/globalMarketModel.test.ts` | **PASSED**: 2 files, 28 tests |
| Harness tests | `vitest run src/dev/pageHarness` | **PASSED**: 4 files, 21 tests |
| Dev-script tests | `vitest run scripts/page-layout-check.test.mjs scripts/global-market-gui-check.test.mjs` | **PASSED**: 2 files, 16 tests |
| i18n gate | `vitest run src/i18n` | **PASSED**: 9 files, 35 tests |
| i18n reporter | `npm run audit:i18n` | **PASSED** (exits 0): "All supported locales match the English key set"; 188 pre-existing hard-coded candidates elsewhere, none in the Global Market |
| Lint | `npm run lint` | **PASSED**: 0 errors, 0 warnings; 3 informational `useSemanticElements` suggestions |
| Format | `npm run format:check` | **PASSED**: 867 files |
| Type check + build | `npm run build` | **PASSED** (`tsc` clean, Vite built) |
| Dead code | `npm run knip` | **PASSED**: two configuration hints (`google-chrome` in `ignoreBinaries`, `.css` excluded) |
| Design-system rules | included in `npm test` (`src/designSystem.test.ts`) | **PASSED** with the Global Market files added to `FINISHED_FOLDERS` |

### Not run

- `cargo test`, `cargo clippy`, `cargo fmt`: no Rust file changed; the backend suite was not
  re-run for this frontend-only branch.
- `/preflight` as one invocation: its frontend components were run individually above.
- CI: this is a local branch.
- Tauri desktop app, a real screen reader, a human visual pass, OS display scaling and the system
  browser's own zoom.

## 3. Browser harness checks

The dev harness mounts the real dashboard on the Global Market tab with a deterministic 30-player
registry. Chrome ran headless through the DevTools protocol (`CHROME_PATH` or the installed Chrome).

### 3.1 Interaction and focus

`node scripts/global-market-gui-check.mjs http://localhost:1420`:

```text
ok   Global Market: 25 rows, signing, paging and focus behave
```

What that one run actually read back:

| Step | Observed |
| --- | --- |
| Results table | 25 rows on page 1; non-empty caption |
| Select the first player | active element is the named profile region; one row has `aria-current="true"` |
| Check the terms | outcome status reads "Agreed"; Sign is enabled |
| Open the confirmation | `role="dialog"` exists and contains focus |
| Confirm the signing | dialog closed; success status present and focused |
| Next page | label reads "Page 2 of 2" |
| Previous page | label reads "Page 1 of 2"; previous is disabled; focus moved to Next page |
| Text sizes | no visible text node below 12 px |
| Page width | `scrollWidth` is not wider than `innerWidth` |

The check fails on any missing step, a focus target left behind, text under 12 px or a sideways
page scroll. Its `judgeInteraction` function is unit-tested against a clean run and one failing
run per rule.

### 3.2 Layout sweep

`node scripts/page-layout-check.mjs http://localhost:1420`: **14 of 16 cases pass**.

```text
ok   globalMarket 1440×900 normal en
ok   globalMarket 1280×800 normal en
ok   globalMarket 1024×768 normal en
ok   globalMarket 900×650 normal en
ok   globalMarket 640×800 normal en
ok   globalMarket 1024×768 normal de
ok   globalMarket 960×640 xlarge de
```

The other pages were already green at their three base windows, except:

```text
FAIL squad 960×640 normal en: the page scrolls sideways (1194 > 960)
FAIL squad 960×640 xlarge de: the page scrolls sideways (1522 > 960)
```

Those two failures are the Squad tab, are present outside this branch's scope, and were not
changed. The Global Market itself has no sideways scroll and nothing past the right edge at any
measured window.

## 4. Test evidence and its limits

### 4.1 What the new tests cover

`GlobalMarketTab.test.tsx` and `globalMarketModel.test.ts` add cases for:

- a table caption, column headers and `aria-current` on the selected row;
- focus moving to the profile after selection, and back to the row after closing;
- a success status that exists before it has a message, and takes focus after signing;
- the three Sign hints (check, counter, refused);
- focus recovery when paging disables the control that had focus;
- no success message before the backend answers, and the generic sentence when it fails;
- long names wrapping without breaking the row;
- error-key resolution, including a key with encoded parameters and an unknown key;
- a request page and page size, and the paging range.

`fixtureAnswers.test.ts` covers the new dev registry: two pages of results, details and
negotiation for a known id, and declining an unknown id. `harnessPage.test.ts` covers
`?page=globalMarket`. `page-layout-check.test.mjs` and `global-market-gui-check.test.mjs` cover the
new windows, the Chrome path lookup and every interaction failure rule.

### 4.2 Red-then-green honesty

The first focused run had three failures, all in test setup: a `change` event sent to the custom
`Select` instead of clicking its combobox and option, an ambiguous "Signing…" label queried outside
the dialog, and an alert read before the asynchronous error content arrived. They were corrected,
after which the focused suite passed (28 tests). This document does **not** claim that each
behavioural test was independently observed red before its implementation in this session; the
tests and the behaviour were developed together, and the browser check is the independent
end-to-end witness for focus and signing.

## 5. Manual desktop checklist (NOT RUN)

Not run here. Required before merge on a real Tauri build:

1. Install/import a registry, open Transfers → Global Market.
2. With the keyboard only, Tab to a player, press Enter: focus lands on the profile region; the row
   shows a check, a left border and is announced as current by a screen reader.
3. Check the terms; accept a counter-offer; confirm the explanation under a disabled Sign button.
4. Sign; confirm the dialog; the success message appears only after the call, receives focus, and a
   screen reader reads it once.
5. Close a profile with the Close button; focus returns to the player's row.
6. Page to the last page, then back to the first: focus never stays on a disabled control.
7. Shrink the window to about 900×650: the sticky profile scrolls internally and the Sign button is
   reachable. At about 640 px wide, only the table region scrolls sideways.
8. Break the registry file or deny the command: a readable alert with Try again appears, with no raw
   key or English exception.
9. Switch to pt-BR, cs and ru: read the header counts, results count, contract years, counter-offer
   status and the new hints.

## 6. Limitations

- Headless Chrome is not the Tauri webview; font metrics and native scrolling can differ. The
  layout sweep is the same mechanism the project already uses, and the interaction check reads the
  DOM rather than pixels.
- The UI scale is applied through the harness's root font size, not the desktop app's own control.
- Only English and German were laid out. The other ten locales are key-complete and the i18n gate
  passes, but they were not visually measured at every width.
- There is no automated contrast measurement; the screen uses the existing tokens and the design
  system's focus-ring and colour rules.
- The dev registry fixture is synthetic by design; it proves the screen, not the Rust registry's
  data quality.
- The Squad sideways scroll at 960×640 is pre-existing and remains open.

## 7. Integration instructions

1. Confirm the tree is clean and the branch is `deepseek/i2-2-global-market-ux` off the stated base.
2. Re-run §2's frontend gates; run the Rust gates only if a later change touches Rust.
3. Run §5 in a Tauri window and record the result.
4. Open a PR against `develop`, disclose AI assistance, and leave the backend untouched.
5. If a reviewer asks for the browser checks, start `npm run dev` and run:
   `node scripts/page-layout-check.mjs http://localhost:1420` and
   `node scripts/global-market-gui-check.mjs http://localhost:1420`.
