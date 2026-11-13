# APFM I3: integration

Branch `integration/i3-unified`, merged locally into `apfm/main`. Nothing was pushed.
Evidence, commands and limits: [`APFM_I3_VALIDATION.md`](APFM_I3_VALIDATION.md).

## 1. What I3 is

I3 brings the completed, validated lines of work onto the canonical branch. `apfm/main` had not moved since
31 Oct 2026 (`4e5dd353`); I3 is 260 commits ahead of it. I3 adds no experiment: the only new production
change is the one cross-branch defect I2.1 named (§5).

## 2. Git ancestry

Verified with `git merge-base --is-ancestor` before and after the merges.

| Line | Tip | Ancestor of I3 |
| --- | --- | --- |
| `apfm/main` before I3 | `4e5dd353` | yes (strict ancestor; main had no commit of its own) |
| P6 match realism (`claude/p6-match-realism`) | `78715a44` | yes |
| P6.1 tactical alignment (`claude/p6-1-tactical-alignment`) | `adb44b4a` | yes |
| I2 (`integration/i2-unified`, the I3 start) | `73db8d43` | yes |
| I2.1 transfer integrity | `1f13ddc7` | yes, merged here |
| I2.2 Global Market UX | `4eae2db4` | yes, merged here |
| D7B3 (`deepseek/d7b3-world-market`) | `a9b70f2c` | yes, see the note below |

**D7B3 note.** I2 had merged D7B3 at `b5c9c6f6`. The branch carries one later commit, `a9b70f2c`
(`docs(d7b3): record the final full-tree validation run`), which changes only
`docs/APFM_D7B3_LONG_CAREER_VALIDATION.md`. It was not in I2, so D7B3 was not a strict ancestor of I3. It was
merged (`--no-ff`) to meet the requirement that main contain the complete D7B3 history. This is an addition
to the two merges the brief listed; it carries no code.

I2.1 and I2.2 both descend from `73db8d43`; they share it as merge base and touch disjoint files.

## 3. Commits on I3 after `73db8d43`

| Commit | What |
| --- | --- |
| `bfe87121` | `merge(i2.1)`: reserve a promised senior place on every signing route (`--no-ff`) |
| `4e475b60` | `merge(i2.2)`: keyboard-complete, readable Global Market (`--no-ff`) |
| `5f790fc7` | `fix(contracts)`: a free-agent talk lapsing for want of a senior place says so |
| `ed13f93e` | `style(contracts)`: `cargo fmt` on the line above (the first `fmt --check` run caught it) |
| `90b850cd` | `test(market)`: every backend refusal reaches the player as its own translated sentence |
| `1a8c9263` | `merge(d7b3)`: the docs-only D7B3 commit (`--no-ff`) |
| (this commit) | these two reports |

No source branch was rebased, squashed, modified or deleted.

## 4. Conflicts and resolutions

**None.** Both feature merges completed with Git's `ort` strategy and no conflict markers. The two branches
touch disjoint files (I2.1: Rust transfer, contract and registry code plus its tests; I2.2: React, locales,
dev scripts), which was checked on the file lists before merging, not assumed. The D7B3 merge changed one
documentation file. No `ours`/`theirs` shortcut was used.

Textual cleanliness is not behavioural compatibility, so the cross-branch contract was tested separately (§6).

## 5. The cross-branch fix

**Problem (I2.1 §9 item 2).** `free_agent_talks::workflow` closed a talk with `ClubCannotAfford` whenever
`complete_signing` failed, for any reason. When the cause was a full or fully promised senior list, the player
was told the club could no longer afford the deal.

**Change.**

* `domain::free_agent_talk::FreeAgentTalkReason` gains `NoSeniorPlace` (serialised `no_senior_place`),
  appended after the existing variants.
* `workflow::lapse_reason` classifies the error `complete_signing` returned: a `be.error.freeAgents.squadFull`
  error gives `NoSeniorPlace`; every other failure stays `ClubCannotAfford`, exactly as before.
* Frontend: `"no_senior_place"` added to `FreeAgentTalkReason`; `freeAgents.reason.no_senior_place` translated in
  all 12 locales (`en, es, pt, fr, de, it, ru, pt-BR, zh-CN, cs, tr, id`), using each locale's existing
  word for the senior list. The panel already renders `freeAgents.reason.<value>`, so no component changed.
* Docs: `APFM_FREE_AGENT_NEGOTIATIONS.md` states the new reason.

**Compatibility.** The value is only ever written by the lapse path. Old saves cannot contain it and load
unchanged; no migration and no `#[serde(default)]` is needed (no field was added). A save written by this build
and opened by an older build that meets a lapsed talk with the new reason would not deserialise that talk; that
is ordinary forward incompatibility and is not guarded here.

**Not changed.** Contract semantics, the capacity rule, wage-policy failures (still reported as money), the
simulation. The generated and imported careers are identical to I2.1 cell for cell (validation §5).

**Tests.** The I2.1 test that pinned the old behaviour,
`a_free_agent_lapses_when_the_last_place_is_promised_before_he_answers`, was rewritten to expect `NoSeniorPlace`
and seen to fail for the predicted reason first (`left: ClubCannotAfford, right: NoSeniorPlace`).
`an_offer_the_club_can_no_longer_pay_lapses_instead_of_signing` still expects `ClubCannotAfford`, so the two
causes stay distinct. Two domain tests cover loading an older reason and the `"no_senior_place"` round trip. A
`FreeAgentTalkPanel` test renders the new reason. The locale gate was seen red with only `en.json` changed.

## 6. Combined functionality

Everything below is present on the merged tree and covered by the suites in the validation report. Nothing was
removed or disabled.

| Line | What the integrated game has |
| --- | --- |
| P6 | Match realism, financial integrity, contracts and transfer rules, M12 integration |
| P6.1 | Functional tactical roles, role-support weighting, formation-fit measurement; existing formation ranking unchanged |
| D7B3 | Global player registry and Global Market screen, search/filter/sort, club and player negotiation, counter-offers, signing bonuses, newgens, Tauri commands, all 12 locales |
| I2 | One senior-squad capacity rule, fee-plus-bonus affordability, translated errors, career-study corrections |
| I2.1 | A senior place promised to an agreed purchase is reserved on every signing route (in-game bid and registration, free-agent talk offer and completion, Global Market free agent and purchase, AI free-agent signing); a deal's own pending offer is excepted so it registers into its own place; a free agent's bonus is judged with pending fees; the legacy external preview delegates to the real negotiation |
| I2.2 | Keyboard-complete Global Market: table caption, `aria-current` selected row with a non-colour cue, focus to the profile and back, paging focus recovery, permanent live regions, scrollable sticky profile, role-token type, a search form, disabled-Sign explanations, backend refusals shown as translated sentences, and the browser interaction and layout checks |
| I3 | The `NoSeniorPlace` reason (§5) and a contract test between the I2.1 backend keys and the I2.2 presentation |

## 7. Cross-branch contract: I2.2 frontend against I2.1 backend

The Global Market shows a backend refusal through `marketErrorMessage`. The I2.1 backend returns every
squad-place refusal as the same encoded key, `be.error.freeAgents.squadFull?count=&max=&release=`, on every route.
`globalMarketBackendContract.test.ts` takes 13 refusal keys the market and signing paths can return (squad
place, cash, transfer budget, board wage policy, window closed, player refuses the move, already in world, not
available, incomplete, offer rejected, negotiation not agreed, no team, managed team not found), resolves each
through the real locale files in all 12 locales, and requires the key's own sentence: never a raw `be.error.*`,
never an unresolved `{{placeholder}}`, never the generic fallback that would hide the real reason. It was
proven able to fail: with `squadFull` deleted from `fr.json` it failed on exactly that key and passed again on
restore.

Scenario-by-scenario evidence, and what it is, is in the validation report §6. The honest limit is stated
there: the screen and the backend are each tested against the other's contract, not against each other through a
live IPC session driven by a person.

## 8. Main update

| | |
| --- | --- |
| `apfm/main` before | `4e5dd353ab3c744400692672a40722659a22282c` (equal to GitHub's live `apfm/main` when checked) |
| Backup reference | `backup/apfm-main-before-i3-20261113` at the commit above |
| Merge | `integration/i3-unified` into `apfm/main`, `--no-ff`, in a new worktree, `apfm/main` having no worktree before |
| After | the merge commit; because it cannot name itself, its hash is in the completion report. Main's tree is verified identical to the I3 tip's tree |

`apfm/main` had no commits that I3 lacked, so nothing on main could be overwritten and no reconciliation was
needed.

## 9. What this document does not claim

* No person used the desktop UI. See the validation report §8.
* The generated-world study shows four AI-club season rows with 31 seniors; this is identical on I2 and I2.1,
  so it pre-dates this integration. It is an open issue (validation §9), not a fixed one.
