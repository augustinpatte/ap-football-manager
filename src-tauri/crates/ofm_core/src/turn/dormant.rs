//! Tiered simulation: competitions outside the player's active scope are
//! "dormant" and resolved by scoreline only — no minute-by-minute events,
//! player match stats, morale, stamina, or news. This keeps the wider world
//! moving cheaply while the player's region is simulated in full.

use std::collections::HashMap;

use crate::game::Game;
use domain::league::FixtureStatus;

/// Minutes a club's likely eleven are credited with for a dormant fixture.
const FULL_MATCH_MINUTES: u32 = 90;

/// Resolve every fixture due `today` in the dormant competition at
/// `competition_index` with a scoreline-only model: update the fixture result
/// and league standings, and advance any group/knockout progression — but skip
/// the expensive per-player post-match work that the full engine path does.
///
/// The one thing kept from that work is the record that a player took part: each club's likely
/// eleven (its best eligible players) is credited with an appearance and ninety minutes. Without
/// it every player of a dormant league would reach the season's end having played nothing, and the
/// rules that read minutes (a young player's development, a veteran's decline) would treat a whole
/// league as benched. Goals, ratings, condition and injuries are not simulated here. The squads
/// are grouped once for the day, so the cost is a pass over the players, not one per fixture.
pub(super) fn simulate_dormant_competition_day(
    game: &mut Game,
    competition_index: usize,
    today: &str,
) {
    let due: Vec<usize> = game.competitions[competition_index]
        .fixtures
        .iter()
        .enumerate()
        .filter(|(_, fixture)| fixture.date == today && fixture.status == FixtureStatus::Scheduled)
        .map(|(index, _)| index)
        .collect();
    if due.is_empty() {
        return;
    }

    // Each club that plays today: how many times, the strength of its likely eleven, and who they
    // are. A player is credited once for every fixture his club plays today (normally one).
    struct Playing {
        times: u32,
        strength: f64,
        eleven: Vec<String>,
    }
    let mut playing: HashMap<String, Playing> = HashMap::new();
    {
        let by_club = crate::match_day_eligibility::players_by_club(&game.players);
        let fixtures = &game.competitions[competition_index].fixtures;
        for &fixture_index in &due {
            for club_id in [
                &fixtures[fixture_index].home_team_id,
                &fixtures[fixture_index].away_team_id,
            ] {
                playing
                    .entry(club_id.clone())
                    .or_insert_with(|| {
                        let club = by_club.get(club_id.as_str()).map_or(&[][..], Vec::as_slice);
                        let eleven = crate::catchup::likely_eleven(club);
                        Playing {
                            times: 0,
                            strength: crate::catchup::strength_of_eleven(&eleven),
                            eleven: eleven.iter().map(|p| p.id.clone()).collect(),
                        }
                    })
                    .times += 1;
            }
        }
    }

    for fixture_index in due {
        // A stream per fixture, keyed by the competition and the two clubs: fixtures are not
        // played in any order a save is bound to keep.
        let fixture = &game.competitions[competition_index].fixtures[fixture_index];
        let tag = format!(
            "dormant/{}/{}/{}",
            game.competitions[competition_index].id, fixture.home_team_id, fixture.away_team_id
        );
        let mut rng = game.rng_for(&tag, today);
        crate::catchup::resolve_fixture_with_strengths(
            &mut game.competitions[competition_index],
            fixture_index,
            |club_id| playing.get(club_id).map_or(50.0, |club| club.strength),
            &mut rng,
        );
    }

    let credited: HashMap<&str, u32> = playing
        .values()
        .flat_map(|club| club.eleven.iter().map(|id| (id.as_str(), club.times)))
        .collect();
    for player in game.players.iter_mut() {
        if let Some(&times) = credited.get(player.id.as_str()) {
            player.stats.appearances += times;
            player.stats.minutes_played += FULL_MATCH_MINUTES * times;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::simulate_dormant_competition_day;
    use crate::clock::GameClock;
    use crate::game::Game;
    use chrono::{TimeZone, Utc};
    use domain::league::{
        Fixture, FixtureCompetition, FixtureStatus, KnockoutRoundState, League, StandingEntry,
    };
    use domain::manager::Manager;
    use domain::team::Team;

    fn make_team(id: &str) -> Team {
        Team::new(
            id.to_string(),
            format!("{id} FC"),
            id.to_uppercase(),
            "England".to_string(),
            "Town".to_string(),
            "Ground".to_string(),
            20_000,
        )
    }

    fn make_dormant_game(today: &str) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2030, 8, 10, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "mgr".to_string(),
            "A".to_string(),
            "B".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        let mut game = Game::new(
            clock,
            manager,
            vec![make_team("home"), make_team("away")],
            vec![],
            vec![],
            vec![],
        );
        game.competitions = vec![League {
            id: "dormant-league".to_string(),
            name: "Dormant League".to_string(),
            season: 2030,
            fixtures: vec![Fixture {
                postponement: None,
                id: "fix-1".to_string(),
                competition_id: "dormant-league".to_string(),
                matchday: 1,
                date: today.to_string(),
                home_team_id: "home".to_string(),
                away_team_id: "away".to_string(),
                competition: FixtureCompetition::League,
                status: FixtureStatus::Scheduled,
                result: None,
            }],
            standings: vec![
                StandingEntry::new("home".to_string()),
                StandingEntry::new("away".to_string()),
            ],
            ..League::default()
        }];
        game
    }

    fn add_squad(game: &mut Game, club: &str, base_ovr: u8) {
        for index in 0..14_u8 {
            let mut player = domain::player::Player::new(
                format!("{club}-{index:02}"),
                "P".to_string(),
                format!("{club} {index}"),
                "1998-01-01".to_string(),
                "England".to_string(),
                domain::player::Position::Midfielder,
                crate::test_support::uniform_attributes(base_ovr),
            );
            // The first eleven are the best; the last three are the bench.
            player.ovr = base_ovr.saturating_sub(index);
            player.team_id = Some(club.to_string());
            game.players.push(player);
        }
    }

    /// Given two clubs with squads of fourteen
    /// When their dormant fixture is played
    /// Then each club's best eleven get an appearance and ninety minutes, and the bench nothing:
    /// the dormant tier leaves the same kind of record the full one does, only cheaper.
    #[test]
    fn a_dormant_fixture_credits_each_clubs_likely_eleven_with_minutes() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);
        add_squad(&mut game, "home", 70);
        add_squad(&mut game, "away", 60);

        simulate_dormant_competition_day(&mut game, 0, today);

        for player in &game.players {
            let index: usize = player.id[player.id.len() - 2..].parse().unwrap();
            let (appearances, minutes) = if index < 11 { (1, 90) } else { (0, 0) };
            assert_eq!(player.stats.appearances, appearances, "{}", player.id);
            assert_eq!(player.stats.minutes_played, minutes, "{}", player.id);
        }
    }

    /// Given an injured player among a club's best
    /// When the dormant fixture is played
    /// Then he is not credited and the next player takes his place.
    #[test]
    fn an_injured_player_is_not_credited_and_the_next_one_plays() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);
        add_squad(&mut game, "home", 70);
        add_squad(&mut game, "away", 60);
        game.players[0].injury = Some(domain::player::Injury {
            name: "Knee".to_string(),
            days_remaining: 10,
        });

        simulate_dormant_competition_day(&mut game, 0, today);

        assert_eq!(game.players[0].stats.minutes_played, 0);
        assert_eq!(
            game.players[11].stats.minutes_played, 90,
            "the twelfth plays"
        );
    }

    /// Given the same clubs and the same day
    /// When the dormant fixture is played, and again with the earlier per-fixture resolver
    /// Then the score is the same: crediting minutes and grouping the squads changed no result.
    #[test]
    fn the_scoreline_is_what_the_per_fixture_resolver_gives() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);
        add_squad(&mut game, "home", 72);
        add_squad(&mut game, "away", 58);
        let mut reference = game.clone();
        let mut rng = reference.rng_for("dormant/dormant-league/home/away", today);
        crate::catchup::resolve_fixture_by_scoreline(
            &reference.players,
            &mut reference.competitions[0],
            0,
            &mut rng,
        );

        simulate_dormant_competition_day(&mut game, 0, today);

        let goals = |g: &Game| {
            let result = g.competitions[0].fixtures[0].result.clone().unwrap();
            (result.home_goals, result.away_goals)
        };
        assert_eq!(goals(&game), goals(&reference));
    }

    #[test]
    fn dormant_competition_day_plays_due_fixtures_and_updates_standings() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);

        simulate_dormant_competition_day(&mut game, 0, today);

        let competition = &game.competitions[0];
        let fixture = &competition.fixtures[0];
        assert_eq!(fixture.status, FixtureStatus::Completed);
        let result = fixture
            .result
            .as_ref()
            .expect("fixture should have a result");
        // Cheap path: scoreline only, no per-minute report.
        assert!(result.report.is_none());
        assert!(result.home_scorers.is_empty());

        // Both clubs recorded exactly one played match in the standings.
        let total_played: u32 = competition.standings.iter().map(|entry| entry.played).sum();
        assert_eq!(total_played, 2);
    }

    #[test]
    fn dormant_knockout_draw_is_settled_by_shootout() {
        // Regression: a level knockout tie used to persist with no shootout
        // score, so advance_knockout_competition_round always sent the home
        // side through. Scan seeds until the scoreline model produces a draw
        // and assert the shootout decided it.
        let today = "2030-08-10";
        let mut saw_draw = false;
        for seed in 0..500 {
            let mut game = make_dormant_game(today);
            let competition = &mut game.competitions[0];
            competition.fixtures[0].competition = FixtureCompetition::Cup;
            competition.knockout_rounds = vec![KnockoutRoundState {
                id: "round-1".to_string(),
                name: "Final".to_string(),
                fixture_ids: vec!["fix-1".to_string()],
                bye_team_ids: Vec::new(),
                completed: false,
            }];
            game.seed = seed;

            simulate_dormant_competition_day(&mut game, 0, today);

            let result = game.competitions[0].fixtures[0]
                .result
                .as_ref()
                .expect("fixture should have a result");
            if result.home_goals == result.away_goals {
                saw_draw = true;
                let home_pens = result.home_penalties.expect("level knockout needs pens");
                let away_pens = result.away_penalties.expect("level knockout needs pens");
                assert_ne!(
                    home_pens, away_pens,
                    "shootout must have a winner (seed {seed})"
                );
                break;
            }
            assert!(
                result.home_penalties.is_none() && result.away_penalties.is_none(),
                "decisive results must not carry a shootout (seed {seed})"
            );
        }
        assert!(
            saw_draw,
            "expected at least one drawn knockout in 500 seeds"
        );
    }

    #[test]
    fn dormant_competition_day_ignores_fixtures_on_other_dates() {
        let mut game = make_dormant_game("2030-08-10");

        simulate_dormant_competition_day(&mut game, 0, "2030-08-11");

        assert_eq!(
            game.competitions[0].fixtures[0].status,
            FixtureStatus::Scheduled
        );
    }
}
