# APFM manual GUI acceptance checklist

**Status: nothing on this page has been performed.** No graphical session has been run by the
agent that wrote it, in this or any earlier APFM session. Every "Result" cell is empty on purpose.
Until a person has filled them in, the career journey is **not** verified from the window; see
`APFM_PLAYABLE_CAREER_IMPLEMENTATION_REPORT.md` for what the automated tests do and do not cover.

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
| 2.3 | Open a player profile and go back | Back returns to the same tab and scroll position | |
| 2.4 | Choose a formation | Pitch redraws with that shape | |
| 2.5 | Swap a bench player into the eleven | The player takes **the vacated slot**, not the end of the list | |
| 2.6 | Place a left-back at right-back, save, close to menu, load the save | Compare the eleven before and after. **Known behaviour to record, not assume:** the save path swaps a mirrored side pair (LB/RB, LWB/RWB, LM/RM, LW/RW) when that raises fit (`canonicalize_game_starting_xi_ids`), so a deliberate inverted full-back may come back swapped. Note whether this surprises you | |

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
| 4.6 | Alerts and sidebar badges | Unread/urgent count agrees with the Inbox; an alert click goes to the tab it names | |
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
| 7.3 | Advance days until a proposal appears | **Dashboard shows an alert "N staff proposal(s) waiting…"** (added in this branch) even on a single-day advance | |
| 7.4 | Click that alert | Opens the Staff tab on the proposals | |
| 7.5 | Accept one proposal and reject another | The action happens or not; alert count drops and disappears at zero | |
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
| 10.2 | Keyboard only through a tab, a modal and a table | Focus ring visible; Esc closes modals; nothing traps focus | |
| 10.3 | Resize to a small window | Sidebar and tables remain usable | |

## Sign-off

A person fills in: name, date, commit (`git rev-parse HEAD`), platform, theme/language combinations
covered, list of defects filed. Only then may the journey be described as manually verified.
