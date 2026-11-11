# APFM manual GUI acceptance checklist

**Status: nothing on this page has been performed.** (P5 added section 11.) (P3 added rows 2.7-2.10, 4.8, 7.7, 7.8 and sharpened 2.3, 4.6, 7.5, 10.2; see `APFM_P3_MANAGEMENT_UX_REPORT.md`.) No graphical session has been run by the
agent that wrote it, in this or any earlier APFM session. Every "Result" cell is empty on purpose.
Until a person has filled them in, the career journey is **not** verified from the window; see
`APFM_PLAYABLE_CAREER_IMPLEMENTATION_REPORT.md` for what the automated tests do and do not cover.

## What an agent did instead

On this branch an agent ran a **launch smoke test** only: `scripts/gui-validation/run-isolated.sh --no-build --smoke 25` against a fresh debug build. The binary started, wrote only into the isolated root, left real saves and `world.db` untouched. No screen was opened or clicked. It replaces none of the rows below.

## How to run

```bash
npm ci
scripts/gui-validation/run-isolated.sh --world-db ~/apfootballmanager/data/world.db
```

The script uses an isolated data root, so it cannot touch your real saves (details in
`docs/GUI_VALIDATION.md`). Do the whole checklist **four times**: light and dark theme, English and
French (Settings). Record each defect with a screenshot and the step number; a ticked box means the
step did what "Expect" says, not that it was merely reachable.

Fill the Result cell with `OK`, or `DEFECT #n`, or `BLOCKED` (could not reach it).

## 1. Career

| # | Do | Expect | Result |
|---|---|---|---|
| 1.1 | Launch; main menu shows | Menu renders, no console error dialog | |
| 1.2 | New career: fill the manager form, choose a world | Labels focus their field; Start is disabled until the form is valid | |
| 1.3 | Choose a club | Dashboard opens on that club; sidebar visible and stays visible on every tab | |
| 1.4 | Open every sidebar tab in turn | Each opens without a blank page or error; switching is quick (no full-page reload flash) | |

## 2. Squad

| # | Do | Expect | Result |
|---|---|---|---|
| 2.1 | Squad tab | Players, positions, key attributes, contract, fitness, availability (injury/suspension) shown | |
| 2.2 | Sort by each column header | Order changes and the sort indicator moves | |
| 2.3 | Type a search, pick a position and a status filter, sort by a column, open a player profile and go back | Back returns to the Squad tab with the **same search, filters and sort** (held since P3). **Scroll position is not kept** (known gap): note whether the jump to the top is a nuisance | |
| 2.4 | Choose a formation | Pitch redraws with that shape | |
| 2.5 | Swap a bench player into the eleven | The player takes **the vacated slot**, not the end of the list | |
| 2.7 | Click **Columns** on the squad filter bar | A panel lists nine optional columns (not the name), all ticked. Unticking Age removes the Age header and its cells at once; Show all columns brings everything back | |
| 2.8 | Hide a few columns, close the game window completely, relaunch, open Squad | The same columns are still hidden (the choice is saved in the settings file) | |
| 2.9 | Sort by Age, then hide Age | The table falls back to the default order (by position); no column you cannot see orders the rows | |
| 2.10 | Keyboard only: Tab to a player's name and press Enter | That player's profile opens (the name is now a button). In the Columns panel Space toggles a box and Esc closes the panel and returns focus to the button | |
| 2.6 | Place a left-back at right-back, save, close to menu, load the save | The eleven is **identical** before and after: the inverted pair is kept (fixed in `3ce15a34`). Only a save written before format 9 is repaired, once, on its first load. Report any reordering as a defect | |

## 3. Tactics

| # | Do | Expect | Result |
|---|---|---|---|
| 3.1 | Tactics tab, first open | Formation, play style and presets visible; the nine phase dials are **folded** | |
| 3.2 | Apply a preset | Style and shape change; confirmed by reopening the tab | |
| 3.3 | Unfold the phase blueprint, change a dial | Value persists after leaving and returning to the tab | |
| 3.4 | Set a role on a token | Role shows and survives a tab change | |
| 3.5 | Play a match after changing style to High press and again to Defensive (live viewer) | The two matches look different in shape/pressing. This is the only human-visible check that tactics reach the simulation; the claim rests on the F4 report otherwise | |

## 4. Calendar and inbox

| # | Do | Expect | Result |
|---|---|---|---|
| 4.1 | Continue (one day) | Date moves one day; nothing else happens on a day without a fixture | |
| 4.2 | Fast-forward several days | Stops at the user's match day or at a decision; the report lists why | |
| 4.3 | Double-click Continue quickly | One day advances, not two; no duplicate simulation | |
| 4.4 | Schedule tab | Upcoming matches, past results, other events | |
| 4.5 | Inbox: open a message, use its action button | Lands on the relevant tab/modal for that message | |
| 4.6 | Alerts and sidebar badges | Unread/urgent count agrees with the Inbox; an alert click goes to the tab it names, and with a screen reader or keyboard the focus lands on the opened screen (not on the alert); a new alert is announced politely | |
| 4.8 | Inbox: make an action fail if you can (for example answer a message whose situation has changed) | A red notice in your language says why; the button stays; it clears on the next attempt. No silent dead button | |
| 4.7 | Leave the XI incomplete on a match day | Blocker modal appears before the match and names what to fix | |

## 5. Match

| # | Do | Expect | Result |
|---|---|---|---|
| 5.1 | Match day: confirm lineup, start | Match starts once; Continue is blocked while it is open | |
| 5.2 | Make a substitution and a tactical change mid-match (interactive path) | Takes effect; appears in the report | |
| 5.3 | Finish | Post-match screen shows score, scorers, ratings; Continue returns to the dashboard | |
| 5.4 | Table / Schedule after the match | The result appears exactly once | |
| 5.5 | Spectator and delegate modes | Both finish and report a result | |

## 6. Transfers and contracts

| # | Do | Expect | Result |
|---|---|---|---|
| 6.1 | Search and filter the market | Filters narrow the list; empty result shows an empty state | |
| 6.2 | Open a profile and bid above your transfer budget | Refused with a readable message in the current language (not a raw key) | |
| 6.3 | Bid within budget | A response (accept/counter/reject/player talks); counter offer can be answered | |
| 6.4 | Complete a signing | **Expect the player to join on the registration date, not instantly.** Squad and finances change once | |
| 6.5 | Renew a contract; offer a free agent a contract; make a loan offer | Each answers and the squad/finances reflect the result | |

## 7. Delegation

| # | Do | Expect | Result |
|---|---|---|---|
| 7.1 | Staff tab without an assistant, ask for Approval mode | Refused with a readable message; Manual always available | |
| 7.2 | With an assistant: set Approval, then Full, then back to Manual | Mode shows each time; duty toggles agree | |
| 7.3 | Advance days until a proposal appears | **Dashboard shows an alert "N staff proposal(s) waiting…"** even on a single-day advance. On a match day with an unfit XI, the lineup alert is listed above it | |
| 7.4 | Click that alert | Opens the Staff tab **already on the Management view**, with Accept and Decline visible without another click (fixed in `a64e30a8`) | |
| 7.5 | Accept one proposal and reject another | The action happens or not; alert count drops and disappears at zero. Each proposal shows its duty and date under the reason | |
| 7.7 | Accept a proposal whose conditions have since changed (for example the budget dropped) | The reason is shown, **and the proposal leaves the list and the dashboard count** (it no longer lingers to fail twice) | |
| 7.8 | Open Staff > Management with a slow or failing backend if you can | It says Loading, or why it failed with a Retry button; never a blank panel | |
| 7.6 | Protect a player from sale | The assistant does not list or propose selling them | |

## 8. Save, quit, reload

| # | Do | Expect | Result |
|---|---|---|---|
| 8.1 | Save from the header | Confirmation shown | |
| 8.2 | Exit to menu, then load the save | Same date, club, squad, eleven, formation, table, inbox, finances, pending proposals | |
| 8.3 | Quit the app, relaunch, load | As 8.2 | |
| 8.4 | Save in the middle of an interactive match, reload | Match resumes, or a notice explains why not | |

## 9. Two seasons

| # | Do | Expect | Result |
|---|---|---|---|
| 9.1 | Play (any mix of live, spectator, delegate) to the last matchday | End-of-season screen appears; awards ceremony works | |
| 9.2 | Start the next season | New fixtures, empty table, squad aged a year, contracts processed | |
| 9.3 | Reload at this point | Same as 8.2 | |
| 9.4 | Play the second season to its end and roll over again | No error, no stuck Continue, manager not unexpectedly without a club | |
| 9.5 | Time check | Note how long one season takes to advance in the window; report anything over a minute per day | |

## 10. Cross-cutting

| # | Do | Expect | Result |
|---|---|---|---|
| 10.1 | Switch English ↔ French on every tab above | No raw `be.error.*` or `dashboard.alerts.*` key visible; no English left in French | |
| 10.2 | Keyboard only through a tab, a modal and a table | Focus ring visible; Esc closes modals; nothing traps focus. Staff tab: the three view buttons and role filters say pressed/not pressed to a screen reader | |
| 10.3 | Resize to a small window | Sidebar and tables remain usable | |

## 11. P5 additions (match engines, saved eleven, deals waiting to register)

Not performed. Added by P5 (`docs/APFM_P5_MATCH_REALISM_AND_FINAL_AI.md`) for behaviour that now has automated tests but that
no person has seen in the window. Use a career whose club has a veteran aged 35 or over who starts.

| # | Do | Expect | Result |
|---|---|---|---|
| 11.1 | Pick a manual career, note the saved eleven (Squad tab), play to the end of the season and through the rollover (Continue) | A starter who retired is **replaced in his own slot** by a bench player; the eleven is still eleven and nobody else has moved a position. Before P5 the eleven fell to ten and every later starter slid up a slot | |
| 11.2 | Same, in a **fully delegated** career (Staff tab, Full mode) | The eleven stays eleven after every day; no day-end error | |
| 11.3 | Release a starter (Squad > player > Release) or sell one to an AI bid | The slot is filled at once by a healthy player; the eleven is still eleven | |
| 11.4 | In the closed transfer window, agree a signing that takes most of your transfer budget, then bid for a second player | The second bid is **refused** with a readable budget or cash message; the projection panel shows the budget overdrawn before you send it | |
| 11.5 | In the closed window, agree one signing, then open the projection for a second | The projected **wage bill already includes the agreed player**; when the window opens and the first registers, the wage and the fee are counted **once** (not twice) | |
| 11.6 | Change formation in Tactics, Save, exit to menu, Load | The same formation shows, and the eleven is unchanged; start the match and the viewer shows that shape | |
| 11.7 | Play one league match with **Quick simulation** (spectator or delegate) and the next with the **M12 viewer** | Both name the same eleven for your club and the same formation as the Tactics tab; neither fields an injured or suspended player. Scores need not agree | |
| 11.8 | Watch several AI v AI results over a month | Goals a match are **about 2.5 to 3.5**, with some 0-0 draws and the odd 5-goal match; a week of 5+ goal matches is a defect (before P5 the engine averaged 3.9) | |
| 11.9 | In a match where one of your players is injured (interactive path, no substitution made by you) | The injury is shown in the match; the AI opponent **replaces its own injured player** with a bench player of the same position | |

## Sign-off

A person fills in: name, date, commit (`git rev-parse HEAD`), platform, theme/language combinations
covered, list of defects filed. Only then may the journey be described as manually verified.
