# I4B — World and competition intelligence

Implementation record for the long-career competition lifecycle, calendars, season transitions and
promotion/relegation work on `deepseek/i4-world-competitions`.

* Base: `0dc3582dc8a4c0a7b7a87d6746377c7618f90d36`
* Branch: `deepseek/i4-world-competitions`
* Evidence and run results: [`APFM_I4B_VALIDATION.md`](APFM_I4B_VALIDATION.md)

## 1. The confirmed defect: a foreign country waits for the manager's rollover

`end_of_season/mod.rs` carried a `TODO(hemisphere-stall)`. It was accurate, not stale.

The rollover was the only path that archived a finished edition, paid its prize money, moved clubs
between divisions and regenerated next season's fixtures. It fires when the **manager's** league
finishes. A foreign league is not on that calendar: Portugal can finish in May with its next opener
on 1 August, and the manager's English league may roll over in late May, so the Portuguese league
was regenerated from the rollover anchor instead of its own opener. Argentina is worse: its Apertura
opened in February, the rollover ran in April/May, and the competition was restarted at the rollover
anchor every season. The old `TODO` described the other half of the same bug: a league still playing
at the rollover was correctly skipped, but then had no hook of its own, so it sat finished until the
*next* rollover and missed an entire edition.

Two observable failures followed:

1. **Calendar drift.** Archived editions of a foreign league opened months after their authored
   month, at the rollover anchor rather than on the league's date.
2. **Lost or idle seasons.** A foreign league could stay finished for most of a year, or fail to
   advance at all when its own season ended between two rollovers.

## 2. What now owns a season transition

The rollover remains the global trigger for the manager's world. A new daily hook,
`turn::country_renewal::renew_due_countries`, owns a **foreign country's domestic ladder** when that
country's own next edition is due. It runs after the day's matches and before the calendar sweep, in
both `process_day` and `process_end_of_season`.

A country is renewed as a unit when **all** of its ladder tiers:

* are league tables on that country's ladder (`is_ladder_tier`),
* have finished the edition they were playing (`is_league_season_ended`), and
* carry an authored start month/day.

The hook then performs, in this order:

| Step | Operation | Why this order |
| --- | --- | --- |
| 1 | Archive every finished tier | An edition that cannot be archived is not renewed; the country is left untouched rather than losing a season |
| 2 | Record and pay each finished division | `record_and_pay_finished_division` is the same function the rollover uses: club history, prize money, reputation and next season's transfer envelope |
| 3 | Promotion/relegation inside the country | `apply_pyramid_promotion_relegation_in_country_with` moves clubs on this country's ladder only |
| 4 | Regenerate each tier | `renewal_edition` chooses the edition; `regenerate_league_for_season` builds its fixtures |
| 5 | Re-seed any unstarted domestic cup | A cup opener can fall before the league opener, so the cup is re-read from the rosters step 3 has just rebuilt |

Archive and payment happen **before** the tables are cleared, so the edition is crowned and paid
exactly once, from the same ranking the rollover would have used. The archive is idempotent
(`archive_completed_edition` refuses a duplicate), and the hook's own second run on the next day
finds the renewed edition unstarted, so it does nothing.

### Choosing the edition

`season_calendar::renewal_edition` keeps the authored opener whenever it is still ahead, advances
exactly one season, and never starts an edition on the day the hook runs (the day's football is
already over; the sweep settles from tomorrow). It handles three cases:

* **Authored opener ahead** — keep it. This is the normal case and removes the drift.
* **Authored opener long past** (a save written before the hook existed) — wait for the following
  authored opener when that would idle the competition for at most 270 days, instead of stretching
  a season from today for the rest of the year.
* **Authored opener just past** — play a short recovery edition starting 21 days from today rather
  than sit idle for most of a year. The new edition never starts in the past.

February 29 openers reuse the existing calendar clamping (`next_season_start`); a non-leap year
opens on February 28.

### Split seasons

Argentina, and any country declared split by `nations::is_split_season_country`, plays the same
clubs through two halves and has no ladder to climb. Each half therefore renews on **its own**
opener: the closing half runs into the opening half's next edition. Only the half that ends the
year pays prize money and records the clubs' season rows (`closes_split_season`), so a split country
is paid once a year, not once a half.

### What the hook deliberately leaves alone

| Case | Owner | Reason |
| --- | --- | --- |
| The manager's own countries | Rollover | The end-of-season summary, messages and board review describe one season at a time to the player |
| A country with a domestic `PositionRange` berth in or out of its ladder | Rollover berth pass | The generic pyramid pass leaves berth-fed targets off the ladder; renewing the country early without that pass would rewrite rosters without moving the clubs the authored berths send. See §6 |
| Continental competitions | Rollover | Their field is read from the whole season's results |
| World Cup and its qualifying | Existing world-cup calendar | One-shot competitions, retired and staged by date |
| A country with a tier still playing, or no authored opener | Nothing yet | Promotion needs every rung finished; an undated ladder is a legacy save |

## 3. Rollover changes

`is_season_complete` now:

* keeps the authored-cup rule it already had: an authored national cup of the manager's country is
  not renewed over unplayed, while the generated cups keep their old behaviour (deliberately out of
  scope here, so no career starts blocking on a bracket the old world never waited for);
* counts only a **reachable** remaining tie (dated today or later) as unfinished, so a stranded tie
  cannot block the rollover for ever;
* ignores a cup already renewed for the next season, which belongs to the season after the one being
  settled and must not deadlock the rollover.

The regeneration step keeps its old behaviour for a competition that is still playing, but the
comment there now states why: a country renewed by the hook is already on its next edition on its
own opener, and the rollover only renews early, never late.

## 4. Continental qualification

A continental field must reward **last completed** season. A country renewed early by the hook has
already cleared its table: the finished edition is in `edition_archive`, and its live table belongs
to a season now in progress.

`standings_for_berth` therefore reads the live table only when that table's season has ended. A live
table still playing (or not yet kicked off) defers to the most recent archived edition before the
current season, and only a world with no archived edition at all falls back to the live order. This
matters because a renewed league can have played two months of its next season by the time the
manager's rollover resolves the cup field; reading the live table then handed the cup the new
season's early leader instead of last season's champion.

The cup-winner route has the matching rule, keyed on the country's own season rather than on the
bracket state: the archive is read when the cup's season is past its top division's, or when it is
equal but that top division has not ended. Either way the live cup is the new, unplayed edition and
the archived champion is the settled one. A cup whose season equals an **ended** top division is the
edition being settled — a final the sweep postponed past the last round — so it contributes nobody
rather than crediting last season's champion a second time. Fixture status is deliberately not the
signal: a kept cup can have played no tie at all and still be the edition under way.

## 5. Promotion, relegation and club identity

`apply_pyramid_promotion_relegation_with` was split into a country-restricted pass over the same
implementation. The pass:

* moves clubs only between adjacent rungs of the named country's ladder;
* keeps berth-fed targets off the linear swap, exactly as the global pass does;
* refuses a run where a tier has not finished, where two tiers share a priority, or where a tier is
  missing;
* logs a silent refusal at `warn` and an already-renewed country's unstarted tiers at `debug`, so a
  dead ladder is still visible without drowning the log in expected messages.

Roster sizes are preserved by the existing pass (promotions and relegations are paired), and the
30-season study checks division sizes every day and at every rollover.

## 6. Edition archive

`latest_edition_before` was extracted from `champion_of_latest_edition_before` so the berth code can
read the archived **table**, not only the champion. Archived editions already store the sorted final
standings, the participant list, results and movement record; nothing new is persisted and no
migration is needed.

## 7. Save compatibility

* No new serialized field was added to `Game`, `League`, `Team` or `CompletedEdition`.
* The hook writes only existing state: archived editions, club history, financial ledger, transfer
  budget, participant lists, fixtures and standings.
* An old save with a finished foreign league and no hook history is recovered on the next day: a
  missed opener is either awaited when it is close or played on a short recovery calendar.
* Reloading across a season boundary is covered by `a_reloaded_career_keeps_the_renewed_calendar`
  and by the 5-season imported-world study; the fingerprint is compared before and after each save.

## 8. Known limitations

1. **A foreign country whose pyramid is fed by authored domestic berths still waits for the
   rollover.** Its calendar can still drift. Extending the hook to it needs the berth pass to run
   country-restricted at the same moment; the safe subset implemented here leaves it to the
   rollover rather than risk moving a club twice. Shipped and imported worlds use the generic ladder
   and are unaffected.
2. **Continental fields and the World Cup remain rollover/world-calendar driven.** They aggregate
   every country's season and cannot be settled from one country's results.
3. **`regenerate_league_for_season` keeps the existing fixture generator.** The hook changes *when*
   and *on which date* an edition starts, not how a round-robin is laid out.
4. **The hook is calendar-driven, not fixture-driven.** A league whose last fixture is abandoned
   without being completed is not renewed; this is the same completeness rule the rollover uses.
5. **Cups use `next_edition`, not `renewal_edition`.** A cup that finishes after its own opener is
   clamped to the shared four-week break rather than skipped to the following authored opener. It
   does not stall, but a late cup can drift by the delay.

## 9. Cross-branch integration

The parallel I4A session owns AI recruitment, squad needs, contract decisions, tactical formation
and the Global Market frontend. This change touches none of those modules. The only shared seams
are:

* `turn::process_day` — one new call after matches;
* `end_of_season::process_end_of_season` — one call before the payout;
* `end_of_season::berths` — an internal reader and a country-restricted pass;
* `season_calendar` — a new edition chooser next to the existing one.

The changes are additive and keep the existing public functions' meaning. A merge with I4A should
not conflict outside `turn/mod.rs` and `end_of_season/mod.rs` adjacency.
