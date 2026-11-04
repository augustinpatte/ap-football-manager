//! Tiered simulation: competitions outside the player's active scope are
//! "dormant" and resolved by scoreline only — no minute-by-minute events,
//! player match stats, morale, stamina, or news. This keeps the wider world
//! moving cheaply while the player's region is simulated in full.

use std::collections::HashMap;

use crate::game::Game;
use domain::league::FixtureStatus;

/// Resolve every fixture due `today` in the dormant competition at `competition_index`. See
/// [`simulate_dormant_day`], which does the work for all of a day's dormant competitions at once.
#[cfg(test)]
pub(super) fn simulate_dormant_competition_day(
    game: &mut Game,
    competition_index: usize,
    today: &str,
) {
    simulate_dormant_day(game, &[competition_index], today);
}

/// Resolve every fixture due `today` in the given dormant competitions with a scoreline-only
/// model: update the fixture result and league standings, and advance any group/knockout
/// progression — but skip the expensive per-player post-match work that the full engine path does.
///
/// What is kept from that work is the consequence for the players. Each club's likely eleven (its
/// best eligible players, a tired one rested for a fresher one) plays the match as a lightweight
/// report ([`super::dormant_report`]) and that report goes through the same consequence stage an
/// active match ends in: appearances, minutes, goals, assists, cards, condition, injuries and
/// bans. Without it a dormant league's players would reach the season's end unhurt, undisciplined
/// and unrested, and the clubs of the player's own competitions would be the only ones the
/// physical and disciplinary rules applied to. Not kept: player match records, morale, news and
/// the compact match report, which are the storage and the cost of the full path.
///
/// The cost is one pass over the players for the whole day, however many competitions are dormant
/// and however many fixtures they play: squads are grouped once, and the credit goes to the
/// players concerned by position, not by another scan.
pub(super) fn simulate_dormant_day(game: &mut Game, competition_indices: &[usize], today: &str) {
    let due: Vec<(usize, usize)> = competition_indices
        .iter()
        .flat_map(|&competition_index| {
            game.competitions[competition_index]
                .fixtures
                .iter()
                .enumerate()
                .filter(|(_, fixture)| {
                    fixture.date == today && fixture.status == FixtureStatus::Scheduled
                })
                .map(move |(fixture_index, _)| (competition_index, fixture_index))
        })
        .collect();
    if due.is_empty() {
        return;
    }

    // Each club that plays today: the strength of its likely eleven, and where those eleven and
    // the rest of the squad stand in `game.players`. The squad matters as much as the eleven:
    // a banned player who sits out the match has served it.
    struct Playing {
        strength: f64,
        eleven: Vec<usize>,
        squad: Vec<usize>,
    }
    let positions = crate::match_day_eligibility::positions_by_club(&game.players);
    let mut playing: HashMap<String, Playing> = HashMap::new();
    for &(competition_index, fixture_index) in &due {
        let fixture = &game.competitions[competition_index].fixtures[fixture_index];
        for club_id in [&fixture.home_team_id, &fixture.away_team_id] {
            playing.entry(club_id.clone()).or_insert_with(|| {
                let at = positions.get(club_id).map_or(&[][..], Vec::as_slice);
                let club: Vec<&domain::player::Player> =
                    at.iter().map(|&position| &game.players[position]).collect();
                let eleven = super::dormant_report::rested_eleven(&club);
                Playing {
                    strength: crate::catchup::strength_of_eleven(&eleven),
                    eleven: eleven
                        .iter()
                        .filter_map(|player| {
                            at.iter()
                                .copied()
                                .find(|&p| game.players[p].id == player.id)
                        })
                        .collect(),
                    squad: at.to_vec(),
                }
            });
        }
    }

    for (competition_index, fixture_index) in due {
        // A stream per fixture, keyed by the competition and the two clubs: fixtures are not
        // played in any order a save is bound to keep.
        let fixture = &game.competitions[competition_index].fixtures[fixture_index];
        let (home_id, away_id) = (fixture.home_team_id.clone(), fixture.away_team_id.clone());
        let tag = format!(
            "dormant/{}/{}/{}",
            game.competitions[competition_index].id, home_id, away_id
        );
        let mut rng = game.rng_for(&tag, today);
        crate::catchup::resolve_fixture_with_strengths(
            &mut game.competitions[competition_index],
            fixture_index,
            |club_id| playing.get(club_id).map_or(50.0, |club| club.strength),
            &mut rng,
        );
        let (Some(home), Some(away)) = (playing.get(&home_id), playing.get(&away_id)) else {
            continue;
        };
        let competition = &mut game.competitions[competition_index];
        let fixture = &mut competition.fixtures[fixture_index];
        let Some(result) = fixture.result.as_mut() else {
            continue;
        };
        let fixture_id = fixture.id.clone();

        // The people in the score: who scored, who was booked, who was hurt.
        let eleven_of = |club: &Playing| -> Vec<&domain::player::Player> {
            club.eleven.iter().map(|&p| &game.players[p]).collect()
        };
        let report = super::dormant_report::lightweight_report(
            (result.home_goals, result.away_goals),
            &eleven_of(home),
            &eleven_of(away),
            &mut rng,
        );
        for goal in &report.goals {
            let scorers = match goal.side {
                engine::Side::Home => &mut result.home_scorers,
                engine::Side::Away => &mut result.away_scorers,
            };
            scorers.push(domain::league::GoalEvent {
                player_id: goal.scorer_id.clone(),
                minute: goal.minute,
            });
        }
        let both_clubs = super::post_match::merged_ascending(&home.squad, &away.squad);
        super::post_match::apply_player_consequences(
            game,
            &fixture_id,
            super::post_match::PlayedMatch {
                home: (&home_id, &home.squad),
                away: (&away_id, &away.squad),
                both_clubs: &both_clubs,
            },
            &report,
        );
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
    /// Then each club's best eleven get an appearance and up to ninety minutes, and the bench nothing:
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
            // A dismissal or an injury ends a match early: the eleven play up to ninety minutes.
            let played = index < 11;
            assert_eq!(player.stats.appearances, u32::from(played), "{}", player.id);
            assert_eq!(player.stats.minutes_played > 0, played, "{}", player.id);
            assert!(player.stats.minutes_played <= 90, "{}", player.id);
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
        assert!(
            game.players[11].stats.minutes_played > 0,
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

    /// Given two dormant competitions with a fixture each on the same day
    /// When the day is played in one go and, from the same state, one competition after the other
    /// Then results, standings and every player's record are identical: grouping the day changed
    /// nothing but the cost.
    #[test]
    fn a_day_of_two_dormant_competitions_equals_one_competition_after_the_other() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);
        game.teams.push(make_team("north"));
        game.teams.push(make_team("south"));
        add_squad(&mut game, "home", 70);
        add_squad(&mut game, "away", 60);
        add_squad(&mut game, "north", 66);
        add_squad(&mut game, "south", 64);
        let mut second = game.competitions[0].clone();
        second.id = "second-league".to_string();
        second.fixtures[0].id = "fix-2".to_string();
        second.fixtures[0].competition_id = "second-league".to_string();
        second.fixtures[0].home_team_id = "north".to_string();
        second.fixtures[0].away_team_id = "south".to_string();
        second.standings = vec![
            StandingEntry::new("north".to_string()),
            StandingEntry::new("south".to_string()),
        ];
        game.competitions.push(second);
        let mut one_after_the_other = game.clone();

        super::simulate_dormant_day(&mut game, &[0, 1], today);
        simulate_dormant_competition_day(&mut one_after_the_other, 0, today);
        simulate_dormant_competition_day(&mut one_after_the_other, 1, today);

        let results = |g: &Game| {
            g.competitions
                .iter()
                .map(|c| {
                    let r = c.fixtures[0].result.clone().unwrap();
                    (r.home_goals, r.away_goals)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(results(&game), results(&one_after_the_other));
        let records = |g: &Game| {
            g.players
                .iter()
                .map(|p| (p.stats.appearances, p.stats.minutes_played))
                .collect::<Vec<_>>()
        };
        assert_eq!(records(&game), records(&one_after_the_other));
        assert!(records(&game).iter().any(|r| r.1 == 90));
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

    fn find<'a>(game: &'a Game, id: &str) -> &'a domain::player::Player {
        game.players.iter().find(|p| p.id == id).unwrap()
    }

    /// Given two dormant clubs and a day of football between them,
    /// When the fixture is played,
    /// Then every goal of the score has a scorer on the fixture's result and on a player's record,
    /// and the players' goals add up to the score: the score is the input, nothing is added to it.
    #[test]
    fn every_goal_of_a_dormant_score_belongs_to_a_player() {
        for day in 1..=20 {
            let today = format!("2030-08-{day:02}");
            let mut game = make_dormant_game(&today);
            add_squad(&mut game, "home", 70);
            add_squad(&mut game, "away", 62);

            simulate_dormant_competition_day(&mut game, 0, &today);

            let result = game.competitions[0].fixtures[0].result.clone().unwrap();
            assert_eq!(result.home_scorers.len(), usize::from(result.home_goals));
            assert_eq!(result.away_scorers.len(), usize::from(result.away_goals));
            let credited: u32 = game.players.iter().map(|p| p.stats.goals).sum();
            assert_eq!(credited, u32::from(result.home_goals + result.away_goals));
            let assists: u32 = game.players.iter().map(|p| p.stats.assists).sum();
            assert!(assists <= credited, "no assist without a goal");
        }
    }

    /// Given a player of a dormant club serving a two-match ban, and a fit twelfth player,
    /// When the club plays a dormant fixture,
    /// Then he does not play, has served one match, and the ban does not wait for a match the
    /// club's eleven would have to be in to count.
    #[test]
    fn a_ban_is_served_in_a_dormant_match_the_banned_player_sits_out() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);
        add_squad(&mut game, "home", 70);
        add_squad(&mut game, "away", 60);
        game.players[0].suspended_matches = 2;

        simulate_dormant_competition_day(&mut game, 0, today);

        assert_eq!(find(&game, "home-00").stats.appearances, 0);
        assert_eq!(find(&game, "home-00").suspended_matches, 1);
        assert_eq!(find(&game, "away-00").suspended_matches, 0);
    }

    /// Given a dormant match,
    /// When it is played,
    /// Then the players who played are tired (the match cost condition) and the bench is not.
    #[test]
    fn a_dormant_match_tires_the_players_who_played_it() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);
        add_squad(&mut game, "home", 70);
        add_squad(&mut game, "away", 60);
        for player in &mut game.players {
            player.condition = 100;
        }

        simulate_dormant_competition_day(&mut game, 0, today);

        for player in &game.players {
            let index: usize = player.id[player.id.len() - 2..].parse().unwrap();
            if index < 11 {
                assert!(
                    player.stats.minutes_played > 0,
                    "{} did not play",
                    player.id
                );
                if player.stats.minutes_played > 30 {
                    assert!(player.condition < 100, "{} played and is fresh", player.id);
                }
            }
            if index >= 11 {
                assert_eq!(player.condition, 100, "{} did not play", player.id);
            }
        }
    }

    /// Given a first choice who is exhausted and a fresher player almost as good,
    /// When the club's eleven is chosen,
    /// Then the tired one is rested; and a tired one with no near-equal replacement still plays.
    #[test]
    fn a_worn_out_first_choice_is_rested_for_a_nearly_as_good_replacement() {
        let today = "2030-08-10";
        let mut game = make_dormant_game(today);
        add_squad(&mut game, "home", 70);
        // home-00 (ovr 70) is exhausted; home-11 (ovr 59) is on the bench; home-10 is the weakest starter (60).
        game.players[0].condition = 20;
        game.players[11].ovr = 69;
        let club: Vec<&domain::player::Player> = game.players.iter().collect();
        let eleven: Vec<&str> = super::super::dormant_report::rested_eleven(&club)
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert!(!eleven.contains(&"home-00"), "rested: {eleven:?}");
        assert!(eleven.contains(&"home-11"));

        // Everyone else is exhausted too: nobody is better rested, so the best still play.
        for player in &mut game.players {
            player.condition = 20;
        }
        game.players[11].ovr = 40;
        let club: Vec<&domain::player::Player> = game.players.iter().collect();
        let eleven: Vec<&str> = super::super::dormant_report::rested_eleven(&club)
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert!(eleven.contains(&"home-00"));
    }

    /// Given the same world played twice,
    /// When a dormant day is simulated both times,
    /// Then every player's record, condition, injury and ban is identical: the lightweight
    /// tier is as deterministic as the seed it is played from.
    #[test]
    fn a_dormant_day_is_deterministic() {
        let today = "2030-08-10";
        let mut first = make_dormant_game(today);
        add_squad(&mut first, "home", 70);
        add_squad(&mut first, "away", 66);
        let mut second = first.clone();

        simulate_dormant_competition_day(&mut first, 0, today);
        simulate_dormant_competition_day(&mut second, 0, today);

        for (a, b) in first.players.iter().zip(&second.players) {
            assert_eq!(a.stats.goals, b.stats.goals, "{}", a.id);
            assert_eq!(a.stats.yellow_cards, b.stats.yellow_cards, "{}", a.id);
            assert_eq!(a.condition, b.condition, "{}", a.id);
            assert_eq!(a.suspended_matches, b.suspended_matches, "{}", a.id);
            assert_eq!(
                a.injury.as_ref().map(|i| i.days_remaining),
                b.injury.as_ref().map(|i| i.days_remaining)
            );
        }
    }

    /// What the consequence stage adds to a dormant fixture, against the scoreline alone.
    ///
    /// `cargo test --release -p ofm_core --lib dormant_fixture_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "measurement, run by hand in release mode"]
    fn dormant_fixture_cost() {
        use std::time::Instant;
        const FIXTURES: u32 = 4_000;
        let today = "2030-08-10";
        let mut template = make_dormant_game(today);
        add_squad(&mut template, "home", 70);
        add_squad(&mut template, "away", 62);

        let started = Instant::now();
        for _ in 0..FIXTURES {
            let mut game = template.clone();
            let mut rng = game.rng_for("dormant/dormant-league/home/away", today);
            crate::catchup::resolve_fixture_by_scoreline(
                &game.players,
                &mut game.competitions[0],
                0,
                &mut rng,
            );
        }
        let scoreline = started.elapsed();

        let started = Instant::now();
        for _ in 0..FIXTURES {
            let mut game = template.clone();
            simulate_dormant_competition_day(&mut game, 0, today);
        }
        let with_consequences = started.elapsed();
        println!(
            "dormant fixture (clone included): scoreline only {:.1} us, with consequences {:.1} us, difference {:.1} us",
            scoreline.as_secs_f64() * 1e6 / f64::from(FIXTURES),
            with_consequences.as_secs_f64() * 1e6 / f64::from(FIXTURES),
            (with_consequences - scoreline).as_secs_f64() * 1e6 / f64::from(FIXTURES),
        );
    }
}
