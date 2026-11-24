//! The dates of an edition, from a versioned template.
//!
//! A template is authored for one reference season. A later season shifts every date by the same
//! whole number of weeks (the number closest to the elapsed years), so a Tuesday stays a Tuesday
//! and the edition drifts at most three and a half days from the calendar date it was authored
//! for. The template says what UEFA published; the shifted dates for other seasons are generated,
//! never official.

use chrono::{Datelike, Days, NaiveDate};
use domain::league_phase::{KnockoutStage, LeaguePhaseCalendar};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarError {
    /// The rules carry no dates at all.
    NoTemplate,
    UnreadableDate(String),
    /// The template has a different number of matchdays than the league phase plays.
    MatchdayCount {
        expected: usize,
        found: usize,
    },
    /// A knockout round has no window, or a two-legged one has no second leg.
    MissingWindow(KnockoutStage),
}

impl CalendarError {
    pub fn key(&self) -> &'static str {
        match self {
            Self::NoTemplate => "be.error.leaguePhase.calendar.noTemplate",
            Self::UnreadableDate(_) => "be.error.leaguePhase.calendar.unreadableDate",
            Self::MatchdayCount { .. } => "be.error.leaguePhase.calendar.matchdayCount",
            Self::MissingWindow(_) => "be.error.leaguePhase.calendar.missingWindow",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundDates {
    pub stage: KnockoutStage,
    pub first_leg: Vec<NaiveDate>,
    pub second_leg: Vec<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditionDates {
    /// For each league-phase matchday, the dates its matches are spread across.
    pub league: Vec<Vec<NaiveDate>>,
    pub knockout: Vec<RoundDates>,
}

impl EditionDates {
    pub fn round(&self, stage: KnockoutStage) -> Option<&RoundDates> {
        self.knockout.iter().find(|round| round.stage == stage)
    }
}

fn parse(date: &str) -> Result<NaiveDate, CalendarError> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| CalendarError::UnreadableDate(date.to_string()))
}

fn parse_all(dates: &[String]) -> Result<Vec<NaiveDate>, CalendarError> {
    dates.iter().map(|date| parse(date)).collect()
}

/// Whole weeks, in days, that carry the reference season to `years` seasons later.
pub fn week_shift_days(years: i32) -> i64 {
    let weeks = (f64::from(years) * 365.25 / 7.0).round() as i64;
    weeks * 7
}

fn shifted(date: NaiveDate, days: i64) -> NaiveDate {
    let moved = if days >= 0 {
        date.checked_add_days(Days::new(days as u64))
    } else {
        date.checked_sub_days(Days::new(days.unsigned_abs()))
    };
    moved.unwrap_or(date)
}

/// The edition's dates for the season that opens in `season_year`.
pub fn edition_dates(
    calendar: &LeaguePhaseCalendar,
    season_year: i32,
    matchdays: usize,
) -> Result<EditionDates, CalendarError> {
    let first = calendar
        .league_matchdays
        .first()
        .and_then(|day| day.first())
        .ok_or(CalendarError::NoTemplate)?;
    let reference_year = parse(first)?.year();
    let days = week_shift_days(season_year - reference_year);
    if calendar.league_matchdays.len() != matchdays {
        return Err(CalendarError::MatchdayCount {
            expected: matchdays,
            found: calendar.league_matchdays.len(),
        });
    }
    let shift_all = |dates: &[String]| -> Result<Vec<NaiveDate>, CalendarError> {
        Ok(parse_all(dates)?
            .into_iter()
            .map(|date| shifted(date, days))
            .collect())
    };
    let league = calendar
        .league_matchdays
        .iter()
        .map(|day| shift_all(day))
        .collect::<Result<Vec<_>, _>>()?;
    let mut knockout = Vec::new();
    for stage in KnockoutStage::ALL {
        let window = calendar
            .knockout_windows
            .iter()
            .find(|window| window.stage == stage)
            .ok_or(CalendarError::MissingWindow(stage))?;
        let first_leg = shift_all(&window.first_leg)?;
        let second_leg = shift_all(&window.second_leg)?;
        if first_leg.is_empty() || (stage.legs() == 2 && second_leg.is_empty()) {
            return Err(CalendarError::MissingWindow(stage));
        }
        knockout.push(RoundDates {
            stage,
            first_leg,
            second_leg,
        });
    }
    Ok(EditionDates { league, knockout })
}

/// Every date the three shipped European competitions play on in the edition that opens in
/// `season_year`: all league-phase matchdays and every knockout leg. A club of an association with
/// clubs in the competitions plays on some of them; which clubs is only known after the draw, the
/// dates are known from the templates alone. Used to keep a league's own midweek rounds off those
/// weeks.
pub fn european_matchday_dates(season_year: i32) -> std::collections::BTreeSet<NaiveDate> {
    use super::profiles;
    let mut dates = std::collections::BTreeSet::new();
    for rules in [
        profiles::champions_league_2026(),
        profiles::europa_league_2026(),
        profiles::conference_league_2026(),
    ] {
        let Ok(edition) = edition_dates(&rules.calendar, season_year, rules.matches_per_club())
        else {
            continue;
        };
        dates.extend(edition.league.iter().flatten().copied());
        for round in &edition.knockout {
            dates.extend(round.first_leg.iter().copied());
            dates.extend(round.second_leg.iter().copied());
        }
    }
    dates
}

/// Moves a window forward in whole weeks until it starts no earlier than `earliest`, so a round
/// that follows a late one keeps its weekday pattern instead of being dated arbitrarily.
pub fn window_not_before(dates: &[NaiveDate], earliest: NaiveDate) -> Vec<NaiveDate> {
    let Some(first) = dates.iter().min() else {
        return Vec::new();
    };
    let late_by = (earliest - *first).num_days();
    let weeks = if late_by > 0 { (late_by + 6) / 7 } else { 0 };
    dates.iter().map(|date| shifted(*date, weeks * 7)).collect()
}

#[cfg(test)]
mod tests {
    use super::super::profiles;
    use super::*;
    use chrono::Weekday;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Given the Champions League 2026/27 template
    /// When its reference edition is read
    /// Then the dates are exactly the published ones, and the matchdays fall Tue–Thu.
    #[test]
    fn the_reference_edition_reproduces_the_published_dates() {
        let rules = profiles::champions_league_2026();
        let dates = edition_dates(&rules.calendar, 2026, 8).unwrap();

        assert_eq!(
            dates.league[0],
            vec![day(2026, 9, 8), day(2026, 9, 9), day(2026, 9, 10)]
        );
        assert_eq!(dates.league[6], vec![day(2027, 1, 19), day(2027, 1, 20)]);
        assert_eq!(dates.league[7], vec![day(2027, 1, 27)]);
        let po = dates.round(KnockoutStage::Playoffs).unwrap();
        assert_eq!(po.first_leg, vec![day(2027, 2, 16), day(2027, 2, 17)]);
        assert_eq!(po.second_leg, vec![day(2027, 2, 23), day(2027, 2, 24)]);
        assert_eq!(
            dates.round(KnockoutStage::Final).unwrap().first_leg,
            vec![day(2027, 6, 5)]
        );
        for matchday in &dates.league {
            assert!(
                matchday.iter().all(|date| matches!(
                    date.weekday(),
                    Weekday::Tue | Weekday::Wed | Weekday::Thu
                ))
            );
        }
    }

    /// Given the template and later seasons
    /// When the dates are generated
    /// Then each date keeps its weekday and stays within four days of its calendar date.
    #[test]
    fn later_seasons_keep_the_weekday_pattern() {
        let rules = profiles::champions_league_2026();
        let reference = edition_dates(&rules.calendar, 2026, 8).unwrap();
        for year in 2027..2040 {
            let later = edition_dates(&rules.calendar, year, 8).unwrap();
            for (old, new) in reference
                .league
                .iter()
                .flatten()
                .zip(later.league.iter().flatten())
            {
                assert_eq!(old.weekday(), new.weekday(), "{year}");
                let same_date_that_year = old.with_year(old.year() + (year - 2026)).unwrap();
                let drift = (*new - same_date_that_year).num_days();
                assert!(drift.abs() <= 4, "{year}: {drift}");
            }
        }
    }

    /// Given a template with the wrong number of matchdays
    /// When dates are generated
    /// Then it is an error, not a short calendar.
    #[test]
    fn a_template_of_the_wrong_length_is_refused() {
        let rules = profiles::champions_league_2026();

        assert_eq!(
            edition_dates(&rules.calendar, 2026, 6),
            Err(CalendarError::MatchdayCount {
                expected: 6,
                found: 8
            })
        );
        assert_eq!(
            edition_dates(&Default::default(), 2026, 8),
            Err(CalendarError::NoTemplate)
        );
    }

    /// Given a round that must follow a late one
    /// When its window is moved
    /// Then it moves by whole weeks and keeps its weekdays.
    #[test]
    fn a_late_round_moves_by_whole_weeks() {
        let window = vec![day(2027, 3, 9), day(2027, 3, 10)];

        let moved = window_not_before(&window, day(2027, 3, 12));

        assert_eq!(moved, vec![day(2027, 3, 16), day(2027, 3, 17)]);
        assert_eq!(window_not_before(&window, day(2027, 3, 1)), window);
    }
}
