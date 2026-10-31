//! The season lifecycle rules: what has to be true of fixtures, tables, club histories, the edition
//! archive, the inbox and contracts as seasons follow one another.
//!
//! [`crate::invariants`] asks whether the world is well formed (tables add up, nobody is stranded,
//! the ladder holds). These rules ask whether the *bookkeeping of a season* is right: that the
//! tables are the sum of the results actually played, that no fixture exists twice, that a
//! finished season leaves exactly one record per club and one archived champion per competition,
//! and that the milestone mail is sent once. Like the invariants they are independent of how the
//! matches fell, and they are game rules read from `ofm_core`, never restated here.
//!
//! Every rule is a public function returning faults, so the test that attaches [`Lifecycle`] to a
//! run can also hand each rule a deliberately broken game and watch it fail.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use domain::league::{CompetitionFormat, FixtureStatus, League};
use ofm_core::end_of_season::{EndOfSeasonSummary, is_regenerated_at_rollover};
use ofm_core::game::Game;

use crate::driver::SeasonObserver;
use crate::invariants::Violation;

type DayRule = fn(&Game) -> Vec<String>;
type RolloverRule = fn(&Snapshot, &Game, &EndOfSeasonSummary) -> Vec<String>;

const EVERY_DAY: &[(&str, DayRule)] = &[
    ("fixture-ids-unique", fixture_ids_unique),
    (
        "league-pairings-not-duplicated",
        league_pairings_not_duplicated,
    ),
    ("standings-match-results", standings_match_results),
    ("postponements-move-forward", postponements_move_forward),
    (
        "cup-champions-won-their-final",
        cup_champions_won_their_final,
    ),
];

const AT_ROLLOVER: &[(&str, RolloverRule)] = &[
    (
        "history-records-follow-seasons",
        history_records_follow_seasons,
    ),
    (
        "edition-archive-keeps-the-champion",
        edition_archive_keeps_the_champion,
    ),
    ("milestone-mail-sent-once", milestone_mail_sent_once),
    (
        "ai-contracts-outlive-the-rollover",
        ai_contracts_outlive_the_rollover,
    ),
];

/// What a rollover replaces and the rules need to compare against, read just before it runs.
pub struct Snapshot {
    /// Per finished league table: its season and the champion the table crowned.
    finished_tables: BTreeMap<String, FinishedTable>,
    /// Per club: how many season records it held.
    history_len: BTreeMap<String, usize>,
}

struct FinishedTable {
    season: u32,
    champion: Option<String>,
    clubs: Vec<String>,
}

impl Snapshot {
    pub fn capture(game: &Game) -> Self {
        let finished_tables = game
            .competitions
            .iter()
            .filter(|competition| is_domestic_table(competition))
            .filter(|competition| is_regenerated_at_rollover(competition))
            .map(|competition| {
                (
                    competition.id.clone(),
                    FinishedTable {
                        season: competition.season,
                        champion: competition
                            .sorted_standings()
                            .first()
                            .map(|row| row.team_id.clone()),
                        clubs: competition.participant_ids.clone(),
                    },
                )
            })
            .collect();
        let history_len = game
            .teams
            .iter()
            .map(|team| (team.id.clone(), team.history.len()))
            .collect();
        Self {
            finished_tables,
            history_len,
        }
    }
}

fn is_domestic_table(competition: &League) -> bool {
    competition.rules.format == CompetitionFormat::LeagueTable
        && competition.kind == domain::league::CompetitionType::League
        && competition.scope == domain::league::CompetitionScope::Domestic
}

/// The observer: attach it beside [`crate::invariants::Invariants`] and read it afterwards.
#[derive(Default)]
pub struct Lifecycle {
    before: Option<Snapshot>,
    violations: Vec<Violation>,
    seen: BTreeSet<(&'static str, String)>,
}

impl Lifecycle {
    pub fn new(game: &Game) -> Self {
        let mut lifecycle = Self::default();
        lifecycle.check_every_day(game, 0, None);
        lifecycle
    }

    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn report(&self) -> String {
        self.violations
            .iter()
            .map(Violation::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn check_every_day(&mut self, game: &Game, season: u32, day: Option<u32>) {
        for (rule, check) in EVERY_DAY {
            for message in check(game) {
                // A daily fault persists on every later day: record it once, where it appeared.
                if self.seen.insert((rule, message.clone())) {
                    self.violations.push(Violation {
                        season,
                        day,
                        rule,
                        message,
                    });
                }
            }
        }
    }
}

impl SeasonObserver for Lifecycle {
    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        self.check_every_day(game, season, Some(day));
    }

    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        self.before = Some(Snapshot::capture(game));
    }

    fn on_season_end(&mut self, game: &Game, summary: &EndOfSeasonSummary, season: u32) {
        self.check_every_day(game, season, None);
        let Some(before) = self.before.take() else {
            return;
        };
        for (rule, check) in AT_ROLLOVER {
            for message in check(&before, game, summary) {
                self.violations.push(Violation {
                    season,
                    day: None,
                    rule,
                    message,
                });
            }
        }
    }
}

// ── Every day ───────────────────────────────────────────────────────────────

/// Given every competition of the world,
/// When two fixtures share an id,
/// Then that is reported: a fixture id names one match, and a result is applied by id.
pub fn fixture_ids_unique(game: &Game) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut faults = Vec::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            if !seen.insert((competition.id.as_str(), fixture.id.as_str())) {
                faults.push(format!(
                    "{} holds fixture {} twice",
                    competition.id, fixture.id
                ));
            }
        }
    }
    faults
}

/// Given a league table,
/// When one ordered pairing (home, away) is scheduled more often than its legs allow,
/// Then that is reported. A double round robin plays each ordered pairing once; four legs, twice.
pub fn league_pairings_not_duplicated(game: &Game) -> Vec<String> {
    let mut faults = Vec::new();
    for table in game
        .competitions
        .iter()
        .filter(|competition| competition.rules.format == CompetitionFormat::LeagueTable)
    {
        let legs = table
            .calendar
            .as_ref()
            .and_then(|calendar| calendar.league_legs)
            .unwrap_or(2)
            .max(1);
        let allowed = usize::from(legs).div_ceil(2);
        let mut pairings: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for fixture in table
            .fixtures
            .iter()
            .filter(|fixture| fixture.counts_for_league_standings())
        {
            *pairings
                .entry((fixture.home_team_id.as_str(), fixture.away_team_id.as_str()))
                .or_default() += 1;
        }
        for ((home, away), count) in pairings.into_iter().filter(|(_, count)| *count > allowed) {
            faults.push(format!(
                "{}: {home} v {away} is scheduled {count} times, {allowed} allowed",
                table.id
            ));
        }
    }
    faults
}

/// Given a league table,
/// When a club's row is not the sum of the completed league results naming it,
/// Then that is reported. This is the check that a result was applied once: not twice by a live
/// path and a batch, not dropped by a reload.
pub fn standings_match_results(game: &Game) -> Vec<String> {
    let mut faults = Vec::new();
    for table in game
        .competitions
        .iter()
        .filter(|competition| competition.rules.format == CompetitionFormat::LeagueTable)
    {
        let mut expected: BTreeMap<&str, [u32; 5]> = BTreeMap::new();
        for fixture in table.fixtures.iter().filter(|fixture| {
            fixture.counts_for_league_standings() && fixture.status == FixtureStatus::Completed
        }) {
            let Some(result) = &fixture.result else {
                continue;
            };
            for (club, scored, conceded) in [
                (
                    fixture.home_team_id.as_str(),
                    result.home_goals,
                    result.away_goals,
                ),
                (
                    fixture.away_team_id.as_str(),
                    result.away_goals,
                    result.home_goals,
                ),
            ] {
                let row = expected.entry(club).or_default();
                row[0] += 1;
                row[1] += u32::from(scored);
                row[2] += u32::from(conceded);
                row[3] += match scored.cmp(&conceded) {
                    std::cmp::Ordering::Greater => 3,
                    std::cmp::Ordering::Equal => 1,
                    std::cmp::Ordering::Less => 0,
                };
            }
        }
        for row in &table.standings {
            let want = expected
                .get(row.team_id.as_str())
                .copied()
                .unwrap_or_default();
            let have = [row.played, row.goals_for, row.goals_against, row.points, 0];
            if have[..4] != want[..4] {
                faults.push(format!(
                    "{}: {} table says played/gf/ga/pts {:?} but the results say {:?}",
                    table.id,
                    row.team_id,
                    &have[..4],
                    &want[..4]
                ));
            }
        }
    }
    faults
}

/// Given a fixture that records a postponement,
/// When it now sits on or before the date it was first scheduled for,
/// Then that is reported: a match is only ever moved later.
pub fn postponements_move_forward(game: &Game) -> Vec<String> {
    let mut faults = Vec::new();
    for competition in &game.competitions {
        for fixture in &competition.fixtures {
            let Some(postponement) = &fixture.postponement else {
                continue;
            };
            let (Ok(original), Ok(now)) = (
                NaiveDate::parse_from_str(&postponement.original_date, "%Y-%m-%d"),
                NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d"),
            ) else {
                faults.push(format!(
                    "{} {} has an unreadable postponement date",
                    competition.id, fixture.id
                ));
                continue;
            };
            if now <= original {
                faults.push(format!(
                    "{} {} was postponed from {original} but sits on {now}",
                    competition.id, fixture.id
                ));
            }
        }
    }
    faults
}

/// Given an archived knockout edition decided by one final match,
/// When the champion on record is not the side that won that match (a shootout included),
/// Then that is reported. The archive is immutable, so a wrong champion here is wrong for good.
pub fn cup_champions_won_their_final(game: &Game) -> Vec<String> {
    let mut faults = Vec::new();
    for edition in &game.edition_archive {
        let Some(final_round) = edition.knockout_rounds.last() else {
            continue;
        };
        let [final_id] = final_round.fixture_ids.as_slice() else {
            continue; // a two-legged final is decided on aggregate, not by one match
        };
        let Some(final_match) = edition
            .fixtures
            .iter()
            .find(|fixture| &fixture.id == final_id)
        else {
            continue;
        };
        let (FixtureStatus::Completed, Some(result)) = (&final_match.status, &final_match.result)
        else {
            continue;
        };
        let winner = if result.advancing_is_home() {
            &final_match.home_team_id
        } else {
            &final_match.away_team_id
        };
        if winner != &edition.champion_id {
            faults.push(format!(
                "{} season {}: {winner} won the final, the archive crowns {}",
                edition.competition_id, edition.season, edition.champion_id
            ));
        }
    }
    faults
}

// ── At each rollover ────────────────────────────────────────────────────────

/// Given a league table that finished,
/// When a club does not gain exactly one season record for it (stamped with that season), or any
///      club holds two records for one season,
/// Then that is reported.
pub fn history_records_follow_seasons(
    before: &Snapshot,
    after: &Game,
    _summary: &EndOfSeasonSummary,
) -> Vec<String> {
    let mut faults = Vec::new();
    for (table_id, finished) in &before.finished_tables {
        for club in &finished.clubs {
            let Some(team) = after.teams.iter().find(|team| &team.id == club) else {
                continue;
            };
            let was = before.history_len.get(club).copied().unwrap_or(0);
            let gained = team.history.len().saturating_sub(was);
            let stamped = team
                .history
                .iter()
                .skip(was)
                .filter(|record| record.season == finished.season)
                .count();
            if gained != 1 || stamped != 1 {
                faults.push(format!(
                    "{club} gained {gained} records ({stamped} for season {}) after {table_id}",
                    finished.season
                ));
            }
        }
    }
    for team in &after.teams {
        let mut seasons = BTreeSet::new();
        for record in &team.history {
            if !seasons.insert(record.season) {
                faults.push(format!(
                    "{} holds two history records for season {}",
                    team.id, record.season
                ));
            }
        }
    }
    faults
}

/// Given a league table that finished,
/// When the archive does not hold its edition with the champion the table crowned, or holds one
///      edition twice,
/// Then that is reported.
pub fn edition_archive_keeps_the_champion(
    before: &Snapshot,
    after: &Game,
    _summary: &EndOfSeasonSummary,
) -> Vec<String> {
    let mut faults = Vec::new();
    let mut editions = BTreeSet::new();
    for edition in &after.edition_archive {
        if !editions.insert((edition.competition_id.as_str(), edition.season)) {
            faults.push(format!(
                "{} season {} is archived twice",
                edition.competition_id, edition.season
            ));
        }
    }
    for (table_id, finished) in &before.finished_tables {
        let archived = after
            .edition_archive
            .iter()
            .find(|edition| edition.is_edition_of(table_id, finished.season));
        match (archived, &finished.champion) {
            (Some(edition), Some(champion)) if &edition.champion_id != champion => {
                faults.push(format!(
                    "{table_id} season {}: table crowned {champion}, archive says {}",
                    finished.season, edition.champion_id
                ));
            }
            (None, _) => faults.push(format!(
                "{table_id} season {} finished but was never archived",
                finished.season
            )),
            _ => {}
        }
    }
    faults
}

/// Given the rollover of the season just played,
/// When its review or the next season's schedule notice is missing from the ledger, is in the
///      inbox twice, or any inbox id repeats,
/// Then that is reported.
pub fn milestone_mail_sent_once(
    _before: &Snapshot,
    after: &Game,
    summary: &EndOfSeasonSummary,
) -> Vec<String> {
    let mut faults = Vec::new();
    for id in [
        format!("season_end_{}", summary.season),
        format!("new_season_{}", summary.season + 1),
    ] {
        let in_inbox = after
            .messages
            .iter()
            .filter(|message| message.id == id)
            .count();
        if !after.emitted_events.contains(&id) {
            faults.push(format!("{id} was never emitted"));
        }
        if in_inbox > 1 {
            faults.push(format!("{id} is in the inbox {in_inbox} times"));
        }
    }
    let mut ids = BTreeSet::new();
    for message in &after.messages {
        if !ids.insert(message.id.as_str()) {
            faults.push(format!("inbox id {} repeats", message.id));
        }
    }
    faults
}

/// Given the rollover has run,
/// When an AI club still fields a player whose contract ended before today,
/// Then that is reported: the board renews or releases at the rollover, it does not leave a lapsed
///      deal on the books.
pub fn ai_contracts_outlive_the_rollover(
    _before: &Snapshot,
    after: &Game,
    _summary: &EndOfSeasonSummary,
) -> Vec<String> {
    let today = after.clock.current_date.date_naive();
    let user_club = after.manager.team_id.as_deref();
    let mut faults = Vec::new();
    for player in &after.players {
        let Some(club) = player.team_id.as_deref() else {
            continue;
        };
        if Some(club) == user_club || player.active_loan.is_some() || player.retired {
            continue;
        }
        let Some(end) = player
            .contract_end()
            .and_then(|end| NaiveDate::parse_from_str(end, "%Y-%m-%d").ok())
        else {
            continue;
        };
        if end < today {
            faults.push(format!(
                "{} at {club} has a contract that ended {end}, before {today}",
                player.id
            ));
        }
    }
    faults
}
