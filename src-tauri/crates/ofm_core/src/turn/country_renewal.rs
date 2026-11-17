//! Renewing a whole country's ladder the day its own next edition is due.
//!
//! The rollover renews every competition together, when the user's league ends. A foreign league is
//! not on the user's calendar: it can finish in October with its next opener in February, and waiting
//! for an April rollover left it idle for months and then started it from the rollover anchor instead
//! of its authored date, every season. So a finished country is archived, paid, promoted and
//! relegated, and renewed by its own calendar, on the day its next edition is four weeks away.
//!
//! Only a country's domestic ladder. Cups renew themselves ([`super::cup_renewal`]); continental
//! fields and berth-fed pyramids are the rollover's, because they are read from the whole season's
//! results and from the berth pass that moves clubs between sibling competitions. The manager's own
//! countries are the rollover's too, so the end-of-season summary and the player's messages keep
//! describing one season at a time.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use domain::league::{
    BerthRule, CompetitionFormat, CompetitionScope, CompetitionType, FixtureStatus, League,
};
use log::{debug, warn};

use crate::edition_archive::archive_completed_edition;
use crate::end_of_season::{
    finished_division_of, finished_divisions_of_country, is_ladder_tier, is_league_season_ended,
    record_and_pay_finished_division,
    season_calendar::{RENEWAL_LEAD_DAYS, renewal_edition},
};
use crate::game::Game;

/// Whether the rollover's berth pass owns this country's promotion and relegation. A country with a
/// domestic `PositionRange` berth anywhere in or out of it is renewed there, not here, so the berth
/// pass and the renewal that rewrites the same rosters never disagree.
fn country_has_domestic_berths(game: &Game, country: &str) -> bool {
    let in_country: BTreeSet<&str> = game
        .competitions
        .iter()
        .filter(|competition| competition.country_id.as_deref() == Some(country))
        .map(|competition| competition.id.as_str())
        .collect();
    let domestic_tables: BTreeSet<&str> = game
        .competitions
        .iter()
        .filter(|competition| is_ladder_tier(competition))
        .map(|competition| competition.id.as_str())
        .collect();
    game.competitions.iter().any(|source| {
        source.berths.iter().any(|berth| {
            matches!(berth.rule, BerthRule::PositionRange { .. })
                && domestic_tables.contains(berth.target.as_str())
                && (in_country.contains(source.id.as_str())
                    || in_country.contains(berth.target.as_str()))
        })
    })
}

/// The countries whose ladder the hook must leave to the rollover: the manager's own.
fn user_countries(game: &Game, user_team_id: &str) -> BTreeSet<String> {
    game.competitions
        .iter()
        .filter(|competition| competition.rules.format == CompetitionFormat::LeagueTable)
        .filter(|competition| {
            competition
                .participant_ids
                .iter()
                .any(|club| club == user_team_id)
        })
        .filter_map(|competition| competition.country_id.clone())
        .collect()
}

/// Parse the clock's day into the midnight instant the renewal calendar works in.
fn midnight(today: &str) -> Option<DateTime<Utc>> {
    NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|day| DateTime::<Utc>::from_naive_utc_and_offset(day, Utc))
}

/// Archive every tier of the country before anything moves a club. Returns false, having left the
/// country alone, when a finished edition cannot be archived: renewing it would lose the edition.
fn archive_tiers(game: &mut Game, tiers: &[usize], country: &str) -> bool {
    for &index in tiers {
        let competition = &game.competitions[index];
        if let Err(failure) = archive_completed_edition(&mut game.edition_archive, competition) {
            warn!(
                "[country-renewal] {country} not renewed: {} season {} cannot be archived: {failure:?}",
                competition.id, competition.season
            );
            return false;
        }
    }
    true
}

/// The ladder tiers of one country, if its whole next edition is due today.
struct DueCountry {
    country: String,
    tiers: Vec<usize>,
}

fn find_due_countries(
    game: &Game,
    today: DateTime<Utc>,
    user_countries: &BTreeSet<String>,
) -> Vec<DueCountry> {
    let mut by_country: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, competition) in game.competitions.iter().enumerate() {
        if !is_ladder_tier(competition) {
            continue;
        }
        let Some(country) = competition.country_id.clone() else {
            continue;
        };
        by_country.entry(country).or_default().push(index);
    }

    let mut due = Vec::new();
    for (country, mut tiers) in by_country {
        if user_countries.contains(&country) || country_has_domestic_berths(game, &country) {
            continue;
        }
        tiers.sort_by_key(|&index| game.competitions[index].priority);
        // A split season plays the same clubs through two halves and has no ladder to climb, so
        // its halves renew on their own openers instead of waiting for the country to finish: the
        // closing half runs into the opening half's next edition, and waiting for it would start
        // the opener months late, every season.
        if crate::nations::is_split_season_country(&country) {
            for index in tiers {
                let competition = &game.competitions[index];
                if !is_league_season_ended(competition) || competition.season_start_month == 0 {
                    continue;
                }
                let edition = renewal_edition(competition, &game.clock, today);
                if (edition.start - today).num_days() <= RENEWAL_LEAD_DAYS {
                    due.push(DueCountry {
                        country: country.clone(),
                        tiers: vec![index],
                    });
                }
            }
            continue;
        }
        // A country renews as a unit: promotion and relegation need every tier finished, and a
        // half-renewed pyramid would move clubs across two editions.
        let every_tier_finished = tiers
            .iter()
            .all(|&index| is_league_season_ended(&game.competitions[index]));
        let every_tier_dated = tiers
            .iter()
            .all(|&index| game.competitions[index].season_start_month != 0);
        if !every_tier_finished || !every_tier_dated {
            continue;
        }
        let due_now = tiers.iter().any(|&index| {
            let edition = renewal_edition(&game.competitions[index], &game.clock, today);
            (edition.start - today).num_days() <= RENEWAL_LEAD_DAYS
        });
        if due_now {
            due.push(DueCountry { country, tiers });
        }
    }
    due
}

/// The day a competition last played, which is how the closing half of a split season is known.
fn last_completed(competition: &League) -> Option<&str> {
    competition
        .fixtures
        .iter()
        .filter(|fixture| fixture.status == FixtureStatus::Completed)
        .map(|fixture| fixture.date.as_str())
        .max()
}

fn roster(competition: &League) -> BTreeSet<&str> {
    competition
        .participant_ids
        .iter()
        .map(String::as_str)
        .collect()
}

/// Whether this split-season edition is the last one its season finishes on, so it is the table
/// that carries the year's prize money and the clubs' season records.
///
/// The opening half ends before the closing half of the same season, even though its next edition
/// has already started by the time the closing half renews. Comparing the competition itself, the
/// other live edition of the same season, and the archive is what keeps a split country paid once
/// a year rather than once a half.
fn closes_split_season(game: &Game, index: usize) -> bool {
    let competition = &game.competitions[index];
    let Some(this_end) = last_completed(competition) else {
        return false;
    };
    let this_roster = roster(competition);
    for (other_index, other) in game.competitions.iter().enumerate() {
        if other_index == index
            || !is_ladder_tier(other)
            || other.country_id != competition.country_id
            || roster(other) != this_roster
        {
            continue;
        }
        if other.season == competition.season {
            // Still playing the same season: it will end after this edition.
            if !is_league_season_ended(other) {
                return false;
            }
            if last_completed(other).is_some_and(|end| end > this_end) {
                return false;
            }
        } else if let Some(edition) = game.edition_archive.iter().find(|edition| {
            edition.competition_id == other.id && edition.season == competition.season
        }) && edition.completed_on.as_str() > this_end
        {
            return false;
        }
    }
    true
}

/// The rung a split-season competition belongs to: its roster's place among the country's distinct
/// rosters, in priority order. Both halves of one division share a rung.
fn split_tier(game: &Game, index: usize) -> u32 {
    let competition = &game.competitions[index];
    let Some(country) = competition.country_id.clone() else {
        return 0;
    };
    let mut tiers: Vec<usize> = game
        .competitions
        .iter()
        .enumerate()
        .filter(|(_, other)| {
            is_ladder_tier(other) && other.country_id.as_deref() == Some(country.as_str())
        })
        .map(|(index, _)| index)
        .collect();
    tiers.sort_by_key(|&index| game.competitions[index].priority);
    let mut rosters: Vec<BTreeSet<&str>> = Vec::new();
    for &rung in &tiers {
        let other = roster(&game.competitions[rung]);
        if !rosters.contains(&other) {
            rosters.push(other);
        }
        if rung == index {
            return rosters.len().saturating_sub(1) as u32;
        }
    }
    0
}

/// Archive, pay and renew every country whose own next edition is due, and return how many
/// countries it renewed.
pub(crate) fn renew_due_countries(game: &mut Game, today: &str) -> usize {
    if game.competitions.is_empty() {
        return 0;
    }
    let Some(user_team_id) = game.manager.team_id.clone() else {
        return 0;
    };
    let Some(today_start) = midnight(today) else {
        return 0;
    };
    let user_countries = user_countries(game, &user_team_id);
    if user_countries.is_empty() {
        return 0;
    }

    let due = find_due_countries(game, today_start, &user_countries);
    let mut renewed = 0;
    for DueCountry { country, tiers } in due {
        if !archive_tiers(game, &tiers, &country) {
            continue;
        }

        // Crown and pay the country's divisions before their tables are cleared. This is the same
        // ranking and the same payout the rollover uses, so a country the hook renews is never paid
        // twice and never pays a split season twice.
        let split = crate::nations::is_split_season_country(&country);
        if split {
            // Each half renews on its own opener; only the half its season ends on pays.
            if closes_split_season(game, tiers[0]) {
                let division = finished_division_of(
                    &game.competitions[tiers[0]],
                    split_tier(game, tiers[0]),
                    today,
                );
                record_and_pay_finished_division(game, division);
            }
        } else {
            for division in finished_divisions_of_country(game, &country) {
                record_and_pay_finished_division(game, division);
            }
        }

        // A split season plays the same clubs through two halves, so it has no ladder to climb.
        // Both halves still renew; only the movement is skipped, exactly as the rollover does.
        if !split {
            let movements = {
                let mut ties = crate::playoff_sim::SquadTies {
                    players: &game.players,
                    seed: game.seed,
                    date: today.to_string(),
                };
                crate::end_of_season::apply_pyramid_promotion_relegation_in_country_with(
                    &mut game.competitions,
                    &country,
                    &mut ties,
                )
            };
            for record in &movements {
                crate::edition_archive::attach_movements(&mut game.edition_archive, record);
            }
        }

        let editions: Vec<_> = tiers
            .iter()
            .map(|&index| renewal_edition(&game.competitions[index], &game.clock, today_start))
            .collect();
        for (&index, edition) in tiers.iter().zip(editions) {
            let previous = game.competitions[index].season;
            let competition = &mut game.competitions[index];
            crate::schedule::regenerate_league_for_season(
                competition,
                edition.season,
                edition.start,
            );
            debug!(
                "[country-renewal] {} season {previous} renewed as season {} on {}",
                competition.id,
                edition.season,
                edition.start.format("%Y-%m-%d")
            );
        }
        renewed += 1;
        // A cup opener can fall before the league opener: the cup was renewed earlier, from the
        // rosters of the division the country has just rewritten.
        reseed_unstarted_cups(game, &country, today);
    }
    renewed
}

/// Whether a cup has been renewed for an edition that has not kicked off.
fn is_unstarted_cup(competition: &League, today: &str) -> bool {
    competition.kind == CompetitionType::Cup
        && competition.scope == CompetitionScope::Domestic
        && !competition.fixtures.is_empty()
        && competition
            .fixtures
            .iter()
            .all(|fixture| fixture.status == FixtureStatus::Scheduled)
        && competition
            .fixtures
            .iter()
            .filter_map(|fixture| NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d").ok())
            .min()
            .is_some_and(|start| {
                start > NaiveDate::parse_from_str(today, "%Y-%m-%d").unwrap_or(start)
            })
}

/// Re-read a country's unstarted cups from the rosters the hook has just rebuilt.
///
/// [`super::cup_renewal`] re-reads a cup's entrants when *it* renews the cup. When a cup opener falls
/// before the league opener, the cup is renewed first and would otherwise keep the previous
/// division's clubs: a promoted side left out, a relegated one still entered. Only cups already
/// renewed for the next edition, and not yet kicked off, are re-seeded; a bracket the world has
/// started is never rewritten.
pub(crate) fn reseed_unstarted_cups(game: &mut Game, country: &str, today: &str) {
    let seed = game.seed;
    for index in 0..game.competitions.len() {
        let cup = &game.competitions[index];
        if cup.country_id.as_deref() != Some(country) || !is_unstarted_cup(cup, today) {
            continue;
        }
        let renewed = crate::edition_archive::latest_edition_before(
            &game.edition_archive,
            &cup.id,
            cup.season,
        )
        .is_some();
        if !renewed {
            continue;
        }
        let Some(entrants) =
            crate::cup_entrants::entrants_from_feeders(cup, &game.competitions, &game.teams)
        else {
            continue;
        };
        let season = cup.season;
        let start = cup
            .fixtures
            .iter()
            .filter_map(|fixture| {
                NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d")
                    .ok()
                    .and_then(|day| day.and_hms_opt(0, 0, 0))
                    .map(|day| DateTime::<Utc>::from_naive_utc_and_offset(day, Utc))
            })
            .min();
        let Some(start) = start else {
            continue;
        };
        let competition = &mut game.competitions[index];
        competition.participant_ids = entrants;
        competition.rules.cup.draw_salt = seed;
        match competition.rules.format {
            CompetitionFormat::Knockout => {
                crate::schedule::regenerate_knockout_for_season(competition, season, start);
            }
            CompetitionFormat::GroupAndKnockout => {
                crate::group_stage::regenerate_for_season(competition, season, start);
            }
            CompetitionFormat::LeagueTable => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::competition_test_support::finish;
    use chrono::{DateTime, Datelike, TimeZone, Utc};
    use domain::competition_calendar::CalendarMetadata;
    use domain::league::{Berth, BerthRule, FixtureCompetition};
    use domain::manager::Manager;
    use domain::team::{FinancialTransactionKind, Team};

    fn at(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
    }

    fn clubs(prefix: &str, count: usize) -> Vec<String> {
        (0..count).map(|i| format!("{prefix}-{i}")).collect()
    }

    /// A played-out table on its own authored calendar. The opener is in `opener.year()`, so the
    /// edition renewed next is the one dated by that year.
    fn finished_table(
        id: &str,
        country: &str,
        priority: u32,
        opener: DateTime<Utc>,
        clubs: &[String],
    ) -> League {
        let mut competition = League::new(id.into(), id.into(), opener.year() as u32, clubs);
        competition.country_id = Some(country.into());
        competition.priority = priority;
        competition.season_start_month = opener.month() as u8;
        competition.season_start_day = opener.day() as u8;
        competition.calendar = Some(CalendarMetadata {
            league_legs: Some(1),
            matchday_gap_days: Some(7),
            ..Default::default()
        });
        competition.fixtures = crate::schedule::build_round_robin_fixtures_with(
            &competition.id,
            &competition.participant_ids,
            opener,
            FixtureCompetition::League,
            1,
            7,
        );
        finish(&mut competition);
        competition
    }

    fn teams_for(competitions: &[League]) -> Vec<Team> {
        let mut teams = Vec::new();
        for competition in competitions {
            for id in &competition.participant_ids {
                if teams.iter().any(|team: &Team| &team.id == id) {
                    continue;
                }
                let mut team = Team::new(
                    id.clone(),
                    id.clone(),
                    "TST".into(),
                    "England".into(),
                    "City".into(),
                    "Ground".into(),
                    20_000,
                );
                team.football_nation = competition.country_id.clone().unwrap_or_default();
                teams.push(team);
            }
        }
        teams
    }

    /// A game whose manager runs `user-club` in an English table, with the given foreign
    /// competitions alongside it.
    fn game_with(competitions: Vec<League>, now: DateTime<Utc>) -> Game {
        let mut manager = Manager::new(
            "manager".into(),
            "First".into(),
            "Last".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        manager.team_id = Some("user-club".into());
        let teams = teams_for(&competitions);
        let mut game = Game::new(GameClock::new(now), manager, teams, vec![], vec![], vec![]);
        game.competitions = competitions;
        game
    }

    fn user_table() -> League {
        finished_table(
            "eng-d1",
            "ENG",
            0,
            at(2033, 8, 1),
            &[
                "user-club".to_string(),
                "eng-0".to_string(),
                "eng-1".to_string(),
                "eng-2".to_string(),
            ],
        )
    }

    fn spanish_pyramid() -> Vec<League> {
        vec![
            finished_table("esp-d1", "ESP", 0, at(2033, 8, 1), &clubs("esp-top", 4)),
            finished_table("esp-d2", "ESP", 1, at(2033, 8, 1), &clubs("esp-low", 4)),
        ]
    }

    fn game_with_spanish_pyramid(now: DateTime<Utc>) -> Game {
        let mut competitions = spanish_pyramid();
        competitions.push(user_table());
        game_with(competitions, now)
    }

    /// Given a finished foreign two-tier country whose own opener is four weeks away,
    /// When the day's renewal runs,
    /// Then both tiers renew on their authored opener, after promotion and relegation,
    /// and each played edition is archived exactly once.
    #[test]
    fn a_finished_foreign_country_renews_on_its_own_authored_opener() {
        let mut game = game_with_spanish_pyramid(at(2034, 7, 5));

        let renewed = renew_due_countries(&mut game, "2034-07-05");

        assert_eq!(renewed, 1);
        let top = game
            .competitions
            .iter()
            .find(|competition| competition.id == "esp-d1")
            .unwrap();
        let low = game
            .competitions
            .iter()
            .find(|competition| competition.id == "esp-d2")
            .unwrap();
        assert_eq!((top.season, low.season), (2034, 2034));
        assert_eq!(top.fixtures[0].date, "2034-08-01");
        assert_eq!(low.fixtures[0].date, "2034-08-01");
        assert!(
            top.fixtures
                .iter()
                .chain(&low.fixtures)
                .all(|fixture| fixture.status == FixtureStatus::Scheduled)
        );

        let mut archived: Vec<_> = game
            .edition_archive
            .iter()
            .map(|edition| (edition.competition_id.as_str(), edition.season))
            .collect();
        archived.sort();
        assert_eq!(archived, vec![("esp-d1", 2033), ("esp-d2", 2033)]);

        let old_top: Vec<String> = game
            .edition_archive
            .iter()
            .find(|edition| edition.competition_id == "esp-d1")
            .map(|edition| {
                edition
                    .standings
                    .iter()
                    .map(|standing| standing.team_id.clone())
                    .collect()
            })
            .unwrap();
        let promoted = game
            .edition_archive
            .iter()
            .find(|edition| edition.competition_id == "esp-d2")
            .and_then(|edition| edition.standings.first())
            .map(|standing| standing.team_id.clone())
            .unwrap();
        let relegated = game
            .edition_archive
            .iter()
            .find(|edition| edition.competition_id == "esp-d1")
            .and_then(|edition| edition.standings.last())
            .map(|standing| standing.team_id.clone())
            .unwrap();
        assert!(top.participant_ids.contains(&promoted));
        assert!(!top.participant_ids.contains(&relegated));
        assert!(low.participant_ids.contains(&relegated));
        assert_eq!(top.participant_ids.len(), 4);
        assert_eq!(low.participant_ids.len(), 4);
        assert_eq!(
            old_top.len(),
            4,
            "the archived table still records the old division"
        );
    }

    /// Given the same finished country, When the renewal runs twice on two consecutive days,
    /// Then the second day changes nothing and no edition is archived twice.
    #[test]
    fn a_renewed_country_is_not_renewed_or_paid_twice() {
        let mut game = game_with_spanish_pyramid(at(2034, 7, 5));
        assert_eq!(renew_due_countries(&mut game, "2034-07-05"), 1);
        let once = serde_json::to_string(&game.competitions).unwrap();
        let archived = game.edition_archive.len();

        assert_eq!(renew_due_countries(&mut game, "2034-07-06"), 0);

        assert_eq!(serde_json::to_string(&game.competitions).unwrap(), once);
        assert_eq!(game.edition_archive.len(), archived);
        for team in game.teams.iter().filter(|team| team.id.starts_with("esp-")) {
            let prizes = team
                .financial_ledger
                .iter()
                .filter(|entry| entry.kind == FinancialTransactionKind::PrizeMoney)
                .count();
            assert_eq!(prizes, 1, "{} is paid for one season once", team.id);
            let seasons = team
                .history
                .iter()
                .filter(|record| record.season == 2033)
                .count();
            assert_eq!(seasons, 1, "{} holds one 2033 record", team.id);
        }
    }

    /// Given the manager's own country is finished and due, When the renewal runs,
    /// Then it is left to the rollover, so the player's summary keeps one season at a time.
    #[test]
    fn the_managers_own_country_waits_for_the_rollover() {
        let mut game = game_with(vec![user_table()], at(2034, 7, 5));

        assert_eq!(renew_due_countries(&mut game, "2034-07-05"), 0);

        assert_eq!(game.competitions[0].season, 2033);
        assert!(game.edition_archive.is_empty());
    }

    /// Given a foreign country whose pyramid is fed by a domestic PositionRange berth,
    /// When its opener is due,
    /// Then the renewal leaves it to the rollover's berth pass.
    #[test]
    fn a_berth_fed_country_waits_for_the_rollover() {
        let mut pyramid = spanish_pyramid();
        pyramid[0].berths.push(Berth {
            target: "esp-d2".into(),
            rule: BerthRule::PositionRange { from: 4, to: 4 },
            fallback_to: None,
        });
        pyramid.push(user_table());
        let mut game = game_with(pyramid, at(2034, 7, 5));

        assert_eq!(renew_due_countries(&mut game, "2034-07-05"), 0);
        assert!(game.edition_archive.is_empty());
    }

    /// Given one tier of a foreign country is still playing, When the other's opener is due,
    /// Then the whole country waits: promotion needs every rung finished.
    #[test]
    fn a_country_with_a_tier_still_playing_is_left_alone() {
        let mut pyramid = spanish_pyramid();
        let fixture = &mut pyramid[1].fixtures[0];
        fixture.status = FixtureStatus::Scheduled;
        fixture.result = None;
        pyramid.push(user_table());
        let mut game = game_with(pyramid, at(2034, 7, 5));

        assert_eq!(renew_due_countries(&mut game, "2034-07-05"), 0);
        assert!(game.edition_archive.is_empty());
    }

    /// Given a split-season country whose Apertura is due, When the renewal runs,
    /// Then only the Apertura renews on its own opener, so it no longer waits for the closing half
    /// that runs into the next year; the closing half renews and pays the year later.
    #[test]
    fn a_split_season_country_renews_each_half_on_its_own_opener() {
        let same_clubs = clubs("arg", 4);
        let mut competitions = vec![
            finished_table("ar-d1-apertura", "AR", 0, at(2033, 2, 1), &same_clubs),
            finished_table("ar-d1-clausura", "AR", 1, at(2033, 7, 1), &same_clubs),
            user_table(),
        ];
        competitions[1].season = 2033;
        let mut game = game_with(competitions, at(2034, 1, 5));

        assert_eq!(renew_due_countries(&mut game, "2034-01-05"), 1);

        let apertura = &game.competitions[0];
        let clausura = &game.competitions[1];
        assert_eq!(apertura.season, 2034);
        assert_eq!(apertura.fixtures[0].date, "2034-02-01");
        assert_eq!(
            clausura.season, 2033,
            "the closing half is still its own season"
        );
        let mut expected = same_clubs.clone();
        expected.sort();
        let mut actual = apertura.participant_ids.clone();
        actual.sort();
        assert_eq!(actual, expected);
        assert_eq!(clausura.participant_ids, same_clubs);
        assert_eq!(game.edition_archive.len(), 1);
        assert!(
            game.teams
                .iter()
                .filter(|team| team.id.starts_with("arg-"))
                .all(|team| team.history.is_empty()),
            "the opening half does not pay: the closing half is the year's table"
        );

        assert_eq!(renew_due_countries(&mut game, "2034-06-05"), 1);

        let clausura = &game.competitions[1];
        assert_eq!(clausura.season, 2034);
        assert_eq!(clausura.fixtures[0].date, "2034-07-01");
        assert_eq!(game.edition_archive.len(), 2);
        for team in game.teams.iter().filter(|team| team.id.starts_with("arg-")) {
            let seasons = team
                .history
                .iter()
                .filter(|record| record.season == 2033)
                .count();
            assert_eq!(seasons, 1, "{} is recorded for 2033 once", team.id);
            let prizes = team
                .financial_ledger
                .iter()
                .filter(|entry| entry.kind == FinancialTransactionKind::PrizeMoney)
                .count();
            assert_eq!(prizes, 1, "{} is paid for 2033 once", team.id);
        }
    }

    /// Given a finished country whose own opener has already gone by, When the hook runs on a save
    /// written before it existed,
    /// Then the edition is not stretched from today for most of a year: it is recovered on a short
    /// calendar.
    #[test]
    fn a_missed_opener_is_recovered_on_a_short_calendar() {
        let mut competitions = spanish_pyramid();
        competitions.push(user_table());
        let mut game = game_with(competitions, at(2034, 9, 1));

        assert_eq!(renew_due_countries(&mut game, "2034-09-01"), 1);
        let top = &game.competitions[0];
        assert_eq!(top.season, 2034);
        assert_eq!(top.fixtures[0].date, "2034-09-22");
    }

    /// Given no manager, When the renewal runs, Then it does nothing: a sandbox has no user
    /// country to protect and no rollover to wait for.
    #[test]
    fn a_game_without_a_manager_does_not_renew_foreign_countries() {
        let mut game = game_with_spanish_pyramid(at(2034, 7, 5));
        game.manager.team_id = None;

        assert_eq!(renew_due_countries(&mut game, "2034-07-05"), 0);
        assert!(game.edition_archive.is_empty());
    }

    /// Given a foreign country renewed by the hook, When the day is processed,
    /// Then the hook is wired into the turn.
    #[test]
    fn processing_a_day_renews_a_due_foreign_country() {
        let mut game = game_with_spanish_pyramid(at(2034, 7, 5));
        game.sync_legacy_league();

        crate::turn::process_day(&mut game);

        let top = game
            .competitions
            .iter()
            .find(|competition| competition.id == "esp-d1")
            .unwrap();
        assert_eq!(top.season, 2034);
        assert_eq!(game.edition_archive.len(), 2);
    }
}
