//! One European trophy per club per edition.
//!
//! Where the fields of several league-phase competitions are inferred from the same domestic
//! results, each would offer the same strongest clubs. A club cannot hold a Champions League and a
//! Europa League place at once, so the fields are made disjoint, highest competition first: the
//! higher tier keeps what it was offered, and each lower one gives up the clubs already taken and is
//! refilled, strongest first, from the clubs the world has in its regions.
//!
//! Competitions with authored berths are not touched here: their fields are resolved together by
//! the berth machinery, cascade and exclusivity included.

use crate::game::Game;
use domain::league::{CompetitionScope, CompetitionType, League};
use std::collections::{BTreeSet, HashMap};

/// Competitions in the order they claim clubs: tier (`priority`, lower first), then id.
fn claiming_order(game: &Game) -> Vec<&League> {
    let mut order: Vec<&League> = game
        .competitions
        .iter()
        .filter(|competition| {
            competition.scope == CompetitionScope::Continental
                && competition.kind == CompetitionType::ContinentalClub
                && crate::league_phase::rules_of(competition).is_some()
                // A field the berth machinery resolves (cascade and exclusivity included) is its own.
                && !crate::end_of_season::competition_has_incoming_berths(game, &competition.id)
        })
        .collect();
    order.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    order
}

/// The clubs a competition may be refilled from: those in its declared regions (every region when
/// it declares none), strongest first.
fn refill_pool<'a>(game: &'a Game, competition: &League) -> Vec<&'a str> {
    let mut teams: Vec<&domain::team::Team> = game
        .teams
        .iter()
        .filter(|team| {
            competition.required_region_ids.is_empty()
                || competition
                    .required_region_ids
                    .iter()
                    .any(|region| *region == game.region_for_country(&team.football_nation))
        })
        .collect();
    teams.sort_by(|a, b| {
        b.reputation
            .cmp(&a.reputation)
            .then_with(|| a.id.cmp(&b.id))
    });
    teams.into_iter().map(|team| team.id.as_str()).collect()
}

/// Makes the inferred fields of the league-phase competitions in `fields` disjoint, and keeps any
/// one association to the competition's access limit. A field that cannot be filled to its size
/// (the world has too few clubs, or too few associations) is left short, for the caller to refuse
/// rather than for this to pad.
pub fn make_fields_exclusive(game: &Game, fields: &mut HashMap<String, Vec<String>>) {
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for competition in claiming_order(game) {
        let Some(offered) = fields.get(&competition.id) else {
            continue;
        };
        let Some(rules) = crate::league_phase::rules_of(competition) else {
            continue;
        };
        let (size, limit) = (
            rules.field_size(),
            usize::from(rules.max_clubs_per_association),
        );
        let association_of = |club: &str| {
            game.teams
                .iter()
                .find(|team| team.id == club)
                .map_or("", |team| team.football_nation.as_str())
        };
        let mut per_association: HashMap<&str, usize> = HashMap::new();
        let mut field: Vec<String> = Vec::with_capacity(size);
        // What it was offered first, then the strongest clubs of its regions.
        let pool = refill_pool(game, competition);
        let candidates = offered.iter().map(String::as_str).chain(pool);
        for club in candidates {
            if field.len() >= size {
                break;
            }
            let association = association_of(club);
            if taken.contains(club)
                || field.iter().any(|held| held == club)
                || per_association.get(association).copied().unwrap_or(0) >= limit
            {
                continue;
            }
            *per_association.entry(association).or_default() += 1;
            field.push(club.to_string());
        }
        taken.extend(field.iter().cloned());
        fields.insert(competition.id.clone(), field);
    }
}

#[cfg(test)]
mod tests {
    use super::super::profiles;
    use super::super::tests::competition;
    use super::*;
    use domain::team::Team;

    fn game_with(count: usize, competitions: Vec<League>) -> Game {
        let teams: Vec<Team> = (0..count)
            .map(|i| {
                let mut team = Team::new(
                    format!("c{i:03}"),
                    format!("c{i:03}"),
                    format!("c{i:03}"),
                    "ENG".into(),
                    "City".into(),
                    "Ground".into(),
                    1,
                );
                team.reputation = 1_000 - i as u32;
                // Twenty-four nations of five clubs, interleaved by strength.
                team.football_nation = format!("N{:02}", i % 24);
                team
            })
            .collect();
        let manager = domain::manager::Manager::new(
            "m".into(),
            "A".into(),
            "B".into(),
            "1980-01-01".into(),
            "ENG".into(),
        );
        let clock = crate::clock::GameClock::new(
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2027, 6, 1, 12, 0, 0).unwrap(),
        );
        let mut game = Game::new(clock, manager, teams, vec![], vec![], vec![]);
        game.competitions = competitions;
        game
    }

    fn named(id: &str, priority: u32, rules: domain::league_phase::LeaguePhaseRules) -> League {
        let mut league = competition(rules);
        league.id = id.into();
        league.priority = priority;
        league
    }

    /// Given three competitions offered the same 36 strongest clubs
    /// When the fields are made exclusive
    /// Then the Champions League keeps them, the Europa League gets the next 36 and the Conference
    ///      League the 36 after, with no club in two.
    #[test]
    fn one_club_never_holds_two_european_places() {
        let competitions = vec![
            named("ecl", 3, profiles::conference_league_2026()),
            named("cl", 1, profiles::champions_league_2026()),
            named("el", 2, profiles::europa_league_2026()),
        ];
        let game = game_with(120, competitions);
        let offered: Vec<String> = (0..36).map(|i| format!("c{i:03}")).collect();
        let mut fields: HashMap<String, Vec<String>> = ["cl", "el", "ecl"]
            .into_iter()
            .map(|id| (id.to_string(), offered.clone()))
            .collect();

        make_fields_exclusive(&game, &mut fields);

        assert_eq!(fields["cl"], offered);
        assert_eq!(fields["el"][0], "c036");
        assert_eq!(fields["ecl"][0], "c072");
        let all: Vec<&String> = fields.values().flatten().collect();
        let unique: BTreeSet<&&String> = all.iter().collect();
        assert_eq!((all.len(), unique.len()), (108, 108));
    }

    /// Given a world whose strongest clubs all belong to one association
    /// When a field is built
    /// Then no association sends more than its access limit, and the field is still full.
    #[test]
    fn no_association_exceeds_its_access_limit() {
        let mut game = game_with(120, vec![named("cl", 1, profiles::champions_league_2026())]);
        for team in game.teams.iter_mut().take(30) {
            team.football_nation = "DOM".into();
        }
        let mut fields = HashMap::from([(
            "cl".to_string(),
            (0..36).map(|i| format!("c{i:03}")).collect::<Vec<_>>(),
        )]);

        make_fields_exclusive(&game, &mut fields);

        assert_eq!(fields["cl"].len(), 36);
        let dominant = fields["cl"]
            .iter()
            .filter(|club| {
                game.teams
                    .iter()
                    .find(|team| &team.id == *club)
                    .is_some_and(|team| team.football_nation == "DOM")
            })
            .count();
        assert_eq!(dominant, 4, "the Champions Cup's limit is four");
    }

    /// Given a world too small to fill the lower competitions
    /// When the fields are made exclusive
    /// Then they stay short (never padded with a duplicate) for the caller to refuse.
    #[test]
    fn a_world_too_small_leaves_the_field_short_rather_than_padded() {
        let game = game_with(
            50,
            vec![
                named("cl", 1, profiles::champions_league_2026()),
                named("el", 2, profiles::europa_league_2026()),
            ],
        );
        let offered: Vec<String> = (0..36).map(|i| format!("c{i:03}")).collect();
        let mut fields: HashMap<String, Vec<String>> = ["cl", "el"]
            .into_iter()
            .map(|id| (id.to_string(), offered.clone()))
            .collect();

        make_fields_exclusive(&game, &mut fields);

        assert_eq!(fields["cl"].len(), 36);
        assert_eq!(
            fields["el"].len(),
            14,
            "only the 14 clubs not already in the Champions League"
        );
    }

    /// Given a competition of another format
    /// When the fields are made exclusive
    /// Then it keeps what it was offered.
    #[test]
    fn other_formats_keep_their_fields() {
        let mut cup = League::new("cup".into(), "Cup".into(), 2026, &[]);
        cup.kind = CompetitionType::ContinentalClub;
        cup.scope = CompetitionScope::Continental;
        let game = game_with(10, vec![cup]);
        let mut fields = HashMap::from([("cup".to_string(), vec!["c000".to_string()])]);

        make_fields_exclusive(&game, &mut fields);

        assert_eq!(fields["cup"], vec!["c000".to_string()]);
    }

    /// Given a league-phase competition whose field the berth machinery resolves
    /// When the fields are made exclusive
    /// Then its field is left to the berths: authored qualification is never overridden.
    #[test]
    fn a_field_fed_by_authored_berths_is_left_alone() {
        use domain::league::{Berth, BerthRule};
        let mut source = League::new("domestic".into(), "Domestic".into(), 2026, &[]);
        source.berths = vec![Berth {
            target: "cl".into(),
            rule: BerthRule::PositionRange { from: 1, to: 4 },
            fallback_to: None,
        }];
        let mut game = game_with(120, vec![named("cl", 1, profiles::champions_league_2026())]);
        game.competitions.push(source);
        for team in game.teams.iter_mut().take(30) {
            team.football_nation = "DOM".into();
        }
        let offered: Vec<String> = (0..36).map(|i| format!("c{i:03}")).collect();
        let mut fields = HashMap::from([("cl".to_string(), offered.clone())]);

        make_fields_exclusive(&game, &mut fields);

        assert_eq!(
            fields["cl"], offered,
            "thirty clubs of one nation stay: the berths decided"
        );
    }
}
