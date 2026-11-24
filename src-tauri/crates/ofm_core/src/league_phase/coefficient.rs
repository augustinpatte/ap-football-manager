//! Club and association coefficients, computed from the editions this game has archived.
//!
//! **These are APFM coefficients, not UEFA's.** UEFA publishes club coefficients on its rankings
//! page, which was not retrievable for INTEGRATION5, and the first season's history (2021/22 to
//! 2025/26) is not in the game. So the first edition is seeded by reputation (flagged as the
//! fallback), and from the second season on the game's own archived editions give a reproducible
//! number, built from the one part of UEFA's method (Annex D, as described on uefa.com's
//! methodology page) that the simulation can reproduce:
//!
//! * league-phase points: 2 for a win, 1 for a draw, 0 for a defeat;
//! * a bonus for each knockout stage from the round of 16 a club reached: 1.5 in the Champions
//!   League, 1 in the Europa League, 0.5 in the Conference League;
//! * a club's coefficient is the sum of its points over the last five seasons, or 20% of its
//!   association's five-season coefficient, whichever is higher;
//! * an association's season coefficient is its clubs' points divided by the clubs it had in
//!   Europe that season.
//!
//! Not modelled: points for knockout matches, qualifying-round points, the ten-season revenue
//! coefficient, and the seasons before the game began (the window is shorter until it fills).
//! Values are in thousandths so no float enters a saved field.

use domain::edition_archive::CompletedEdition;
use domain::league_phase::KnockoutStage;
use std::collections::{BTreeMap, BTreeSet};

/// Points for a league-phase win and draw, in thousandths.
pub const WIN_MILLIS: u32 = 2_000;
pub const DRAW_MILLIS: u32 = 1_000;
/// Seasons a club coefficient looks back over.
pub const WINDOW_SEASONS: u32 = 5;
/// The share of its association's coefficient a club always has.
pub const ASSOCIATION_FLOOR_PERCENT: u32 = 20;

/// Bonus per knockout stage reached from the round of 16, by competition tier (`priority`: 1 is
/// the Champions League), in thousandths.
pub fn stage_bonus_millis(priority: u32) -> u32 {
    match priority {
        1 => 1_500,
        2 => 1_000,
        _ => 500,
    }
}

/// The league-phase editions on record, plus the ones finished but not yet archived (the rollover
/// reads qualification before the daily renewal archives them).
pub(crate) fn european_editions(
    game: &crate::game::Game,
) -> Vec<domain::edition_archive::CompletedEdition> {
    let mut editions: Vec<_> = game
        .edition_archive
        .iter()
        .filter(|edition| edition.league_phase.is_some())
        .cloned()
        .collect();
    for competition in &game.competitions {
        if crate::league_phase::rules_of(competition).is_some()
            && competition.league_phase.as_ref().is_some_and(|state| {
                state.stage == domain::league_phase::CompetitionStage::Completed
            })
        {
            // Same proof as the archive: an edition that does not prove complete is not counted.
            let _ = crate::edition_archive::archive_completed_edition(&mut editions, competition);
        }
    }
    editions
}

/// The European history a game can read: every club's association (its football nation) and each
/// archived season's points. The one place that decides who counts as an association, so the club
/// coefficient floor and the European performance places cannot disagree about it.
pub(crate) struct EuropeanHistory {
    nation_of: std::collections::HashMap<String, String>,
    pub records: BTreeMap<u32, SeasonRecord>,
}

impl EuropeanHistory {
    pub(crate) fn of(game: &crate::game::Game) -> Self {
        let nation_of = game
            .teams
            .iter()
            .map(|team| (team.id.clone(), team.football_nation.clone()))
            .collect();
        let bonus_of = |competition_id: &str| {
            game.competitions
                .iter()
                .find(|competition| {
                    competition.id == competition_id && super::rules_of(competition).is_some()
                })
                .map(|competition| stage_bonus_millis(competition.priority))
        };
        let records = season_records(&european_editions(game), &bonus_of);
        Self { nation_of, records }
    }

    pub(crate) fn association_of(&self, club: &str) -> Option<String> {
        self.nation_of.get(club).cloned()
    }
}

/// The game's APFM coefficient of every club, from the European editions it has archived (and the
/// ones just finished). Empty until a first edition has been played: nothing is invented.
pub fn european_coefficients(game: &crate::game::Game) -> BTreeMap<String, u32> {
    let history = EuropeanHistory::of(game);
    let clubs: Vec<String> = game.teams.iter().map(|team| team.id.clone()).collect();
    club_coefficients(&history.records, &clubs, &|club| {
        history.association_of(club)
    })
}

/// What one season's European editions gave each club, and who was there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeasonRecord {
    pub club_points: BTreeMap<String, u32>,
    pub participants: BTreeSet<String>,
}

/// The points one edition earned each club: league-phase results plus the stage bonus for every
/// club in a round of 16, quarter-final, semi-final or final tie.
pub fn edition_points(edition: &CompletedEdition, bonus_millis: u32) -> BTreeMap<String, u32> {
    let mut points: BTreeMap<String, u32> = BTreeMap::new();
    let Some(state) = &edition.league_phase else {
        return points;
    };
    let knockout = state.knockout_fixture_ids();
    for fixture in edition
        .fixtures
        .iter()
        .filter(|fixture| !knockout.contains(fixture.id.as_str()))
    {
        let Some(result) = &fixture.result else {
            continue;
        };
        let (home, away) = match result.home_goals.cmp(&result.away_goals) {
            std::cmp::Ordering::Greater => (WIN_MILLIS, 0),
            std::cmp::Ordering::Less => (0, WIN_MILLIS),
            std::cmp::Ordering::Equal => (DRAW_MILLIS, DRAW_MILLIS),
        };
        *points.entry(fixture.home_team_id.clone()).or_default() += home;
        *points.entry(fixture.away_team_id.clone()).or_default() += away;
    }
    for tie in &state.ties {
        if tie.stage == KnockoutStage::Playoffs {
            continue;
        }
        for club in [&tie.seeded_team_id, &tie.unseeded_team_id] {
            *points.entry(club.clone()).or_default() += bonus_millis;
        }
    }
    points
}

/// Every season's record from the archive. `bonus_of` names the stage bonus of a competition and
/// is `None` for one that is not a European competition the world has (its editions are skipped).
pub fn season_records(
    archive: &[CompletedEdition],
    bonus_of: &impl Fn(&str) -> Option<u32>,
) -> BTreeMap<u32, SeasonRecord> {
    let mut seasons: BTreeMap<u32, SeasonRecord> = BTreeMap::new();
    for edition in archive.iter().filter(|e| e.league_phase.is_some()) {
        let Some(bonus) = bonus_of(&edition.competition_id) else {
            continue;
        };
        let record = seasons.entry(edition.season).or_default();
        record
            .participants
            .extend(edition.participant_ids.iter().cloned());
        for (club, points) in edition_points(edition, bonus) {
            *record.club_points.entry(club).or_default() += points;
        }
    }
    seasons
}

/// An association's coefficient for one season: its clubs' points over the clubs it had in Europe.
fn association_season_millis(
    record: &SeasonRecord,
    association: &str,
    association_of: &impl Fn(&str) -> Option<String>,
) -> u32 {
    let clubs = record
        .participants
        .iter()
        .filter(|club| association_of(club).as_deref() == Some(association))
        .count() as u32;
    if clubs == 0 {
        return 0;
    }
    let points: u32 = record
        .club_points
        .iter()
        .filter(|(club, _)| association_of(club).as_deref() == Some(association))
        .map(|(_, points)| *points)
        .sum();
    points / clubs
}

/// The associations that had a club in Europe in the latest archived season, best first by that
/// season's association coefficient (ties broken by code), with the coefficient. Empty before any
/// edition is archived.
pub fn association_ranking(
    seasons: &BTreeMap<u32, SeasonRecord>,
    association_of: &impl Fn(&str) -> Option<String>,
) -> Vec<(String, u32)> {
    let Some((_, latest)) = seasons.iter().next_back() else {
        return Vec::new();
    };
    let associations: BTreeSet<String> = latest
        .participants
        .iter()
        .filter_map(|club| association_of(club))
        .collect();
    let mut ranking: Vec<(String, u32)> = associations
        .into_iter()
        .map(|association| {
            let millis = association_season_millis(latest, &association, association_of);
            (association, millis)
        })
        .collect();
    ranking.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranking
}

/// Every club's APFM coefficient: its points over the last [`WINDOW_SEASONS`] archived seasons, or
/// [`ASSOCIATION_FLOOR_PERCENT`] of its association's coefficient over the same window, whichever
/// is higher. Clubs of an association with no European history are absent (never invented).
/// `clubs` are the clubs the coefficient is wanted for.
pub fn club_coefficients(
    seasons: &BTreeMap<u32, SeasonRecord>,
    clubs: &[String],
    association_of: &impl Fn(&str) -> Option<String>,
) -> BTreeMap<String, u32> {
    let Some(latest) = seasons.keys().next_back().copied() else {
        return BTreeMap::new();
    };
    let window = seasons.range(latest.saturating_sub(WINDOW_SEASONS - 1)..=latest);
    let mut own: BTreeMap<&str, u32> = BTreeMap::new();
    let mut association_total: BTreeMap<String, u32> = BTreeMap::new();
    for (_, record) in window {
        let seen: BTreeSet<String> = record
            .participants
            .iter()
            .filter_map(|club| association_of(club))
            .collect();
        for association in seen {
            *association_total.entry(association.clone()).or_default() +=
                association_season_millis(record, &association, association_of);
        }
        for (club, points) in &record.club_points {
            *own.entry(club.as_str()).or_default() += points;
        }
    }
    clubs
        .iter()
        .filter_map(|club| {
            let floor = association_of(club)
                .and_then(|association| association_total.get(&association).copied())
                .map_or(0, |total| total * ASSOCIATION_FLOOR_PERCENT / 100);
            let sum = own.get(club.as_str()).copied().unwrap_or(0);
            let value = sum.max(floor);
            (value > 0).then(|| (club.clone(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::league::{Fixture, FixtureCompetition, FixtureStatus, MatchResult};
    use domain::league_phase::{KnockoutTie, LeaguePhaseState};

    fn played(id: &str, home: &str, away: &str, goals: (u8, u8)) -> Fixture {
        Fixture {
            id: id.into(),
            competition_id: "eu".into(),
            matchday: 1,
            date: "2027-01-01".into(),
            home_team_id: home.into(),
            away_team_id: away.into(),
            competition: FixtureCompetition::ContinentalClub,
            status: FixtureStatus::Completed,
            result: Some(MatchResult {
                home_goals: goals.0,
                away_goals: goals.1,
                home_scorers: vec![],
                away_scorers: vec![],
                report: None,
                home_penalties: None,
                away_penalties: None,
            }),
            postponement: None,
        }
    }

    fn tie(stage: KnockoutStage, a: &str, b: &str, fixture: &str) -> KnockoutTie {
        KnockoutTie {
            id: format!("{stage:?}-{a}"),
            stage,
            slot: 0,
            path: 1,
            seeded_team_id: a.into(),
            unseeded_team_id: b.into(),
            fixture_ids: vec![fixture.into()],
            resolution: None,
        }
    }

    fn edition(
        id: &str,
        season: u32,
        fixtures: Vec<Fixture>,
        ties: Vec<KnockoutTie>,
    ) -> CompletedEdition {
        let participants: Vec<String> = fixtures
            .iter()
            .flat_map(|f| [f.home_team_id.clone(), f.away_team_id.clone()])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        CompletedEdition {
            competition_id: id.into(),
            season,
            completed_on: format!("{season}-06-01"),
            champion_id: "a".into(),
            participant_ids: participants,
            standings: vec![],
            groups: vec![],
            knockout_rounds: vec![],
            fixtures,
            movements: None,
            league_phase: Some(LeaguePhaseState {
                ties,
                ..Default::default()
            }),
        }
    }

    /// Given a league-phase win, a draw and a defeat, and a club in a round of 16 and a play-off
    /// When the edition's points are counted for the Champions League bonus
    /// Then a win is worth 2, a draw 1, a defeat 0, a round-of-16 club earns 1.5 and a play-off
    ///      earns no bonus; knockout matches carry no points of their own.
    #[test]
    fn points_follow_the_league_phase_and_the_stage_bonus() {
        let edition = edition(
            "eu-ucl",
            2026,
            vec![
                played("m1", "a", "b", (2, 0)),
                played("m2", "a", "c", (1, 1)),
                played("ko", "a", "d", (5, 0)),
            ],
            vec![
                tie(KnockoutStage::Playoffs, "b", "c", "x"),
                tie(KnockoutStage::RoundOf16, "a", "d", "ko"),
            ],
        );

        let points = edition_points(&edition, 1_500);

        assert_eq!(points["a"], 2_000 + 1_000 + 1_500);
        assert_eq!(points["b"], 0);
        assert_eq!(points["c"], 1_000);
        assert_eq!(
            points["d"], 1_500,
            "the knockout match itself is worth nothing"
        );
    }

    fn by_prefix(club: &str) -> Option<String> {
        Some(club.split('-').next()?.to_string())
    }

    fn two_seasons() -> BTreeMap<u32, SeasonRecord> {
        let archive = vec![
            edition(
                "eu-ucl",
                2026,
                vec![
                    played("m1", "en-a", "fr-a", (3, 0)),
                    played("m2", "en-b", "fr-b", (0, 1)),
                ],
                vec![],
            ),
            edition(
                "eu-ucl",
                2027,
                vec![played("m3", "fr-a", "en-a", (2, 0))],
                vec![],
            ),
        ];
        season_records(&archive, &|id| (id == "eu-ucl").then_some(1))
    }

    /// Given two archived seasons
    /// When the associations are ranked
    /// Then the latest season decides: France (one win in 2027) over England (a defeat), and an
    ///      association with no club that season is absent.
    #[test]
    fn the_association_ranking_reads_the_latest_season() {
        let ranking = association_ranking(&two_seasons(), &by_prefix);
        assert_eq!(
            ranking,
            vec![("fr".to_string(), 2_000), ("en".to_string(), 0)]
        );
    }

    /// Given two seasons of results and a club that was never in Europe
    /// When the coefficients are computed
    /// Then each club has its own points, a newcomer of an active association has the 20% floor, and
    ///      a club of an association with no history has none (never invented).
    #[test]
    fn a_coefficient_is_the_own_sum_or_the_association_floor() {
        let clubs: Vec<String> = ["en-a", "fr-a", "en-new", "xx-nobody"]
            .into_iter()
            .map(str::to_string)
            .collect();

        let coefficients = club_coefficients(&two_seasons(), &clubs, &by_prefix);

        // en-a: 2026 win 2000, 2027 defeat 0 -> 2000. fr-a: 0 + 2000.
        assert_eq!(coefficients["en-a"], 2_000);
        assert_eq!(coefficients["fr-a"], 2_000);
        // England's association coefficient: 2026 (3 pts-clubs: en-a 2000 + en-b 0)/2 = 1000; 2027 0
        // -> 1000; 20% of it is 200.
        assert_eq!(coefficients["en-new"], 200);
        assert!(!coefficients.contains_key("xx-nobody"));
    }

    /// Given more archived seasons than the window
    /// When coefficients are computed
    /// Then only the last five count.
    #[test]
    fn only_the_last_five_seasons_count() {
        let archive: Vec<CompletedEdition> = (2020..=2026)
            .map(|season| {
                edition(
                    "eu-ucl",
                    season,
                    vec![played(&format!("m{season}"), "en-a", "fr-a", (1, 0))],
                    vec![],
                )
            })
            .collect();
        let seasons = season_records(&archive, &|_| Some(1));

        let coefficients = club_coefficients(&seasons, &["en-a".to_string()], &by_prefix);

        assert_eq!(
            coefficients["en-a"],
            5 * WIN_MILLIS,
            "five seasons, not seven"
        );
    }
}
