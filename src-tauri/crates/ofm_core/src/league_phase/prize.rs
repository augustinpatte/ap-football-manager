//! Participation and prize money for a league-phase edition: exactly once.
//!
//! What is owed is *derived* from the competition: its clubs, its recorded results, the rounds its
//! ties have reached and its champion. Every payment carries a stable key (`participation:club`,
//! `win:fixture`, `stage:round:club`, `champion:club`), and the keys already posted are stored in
//! the competition's state. A day that runs twice, a save reloaded between two days and a result
//! recorded twice all derive the same owed set and find it already posted, so a payment is made
//! once and only once.
//!
//! The amounts are the rules' [`domain::league_phase::ContinentalPrize`]: **game-balance
//! assumptions, never UEFA's published distribution**. A competition that authors none pays none.

use crate::finances::journal::{PostRequest, post_all};
use crate::game::Game;
use domain::finance::CashKind;
use domain::league::{FixtureStatus, League};
use domain::league_phase::CompetitionStage;
use std::collections::BTreeSet;

/// One payment owed to a club, with the key that makes it post once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payment {
    pub key: String,
    pub club_id: String,
    pub amount: i64,
}

/// Everything the competition owes so far, posted or not. Pure.
pub fn owed(league: &League) -> Vec<Payment> {
    let (Some(rules), Some(state)) = (super::rules_of(league), league.league_phase.as_ref()) else {
        return Vec::new();
    };
    let prize = &rules.prize;
    if !prize.is_authored() || state.stage == CompetitionStage::Created {
        return Vec::new();
    }
    let mut payments = Vec::new();
    let mut owe = |key: String, club: &str, amount: i64| {
        if amount != 0 {
            payments.push(Payment {
                key,
                club_id: club.to_string(),
                amount,
            });
        }
    };
    for club in &league.participant_ids {
        owe(format!("participation:{club}"), club, prize.participation);
    }
    let knockout = state.knockout_fixture_ids();
    for fixture in league
        .fixtures
        .iter()
        .filter(|fixture| !knockout.contains(fixture.id.as_str()))
        .filter(|fixture| fixture.status == FixtureStatus::Completed)
    {
        let Some(result) = fixture.result.as_ref() else {
            continue;
        };
        match result.home_goals.cmp(&result.away_goals) {
            std::cmp::Ordering::Greater => owe(
                format!("win:{}", fixture.id),
                &fixture.home_team_id,
                prize.per_win,
            ),
            std::cmp::Ordering::Less => owe(
                format!("win:{}", fixture.id),
                &fixture.away_team_id,
                prize.per_win,
            ),
            std::cmp::Ordering::Equal => {
                for club in [&fixture.home_team_id, &fixture.away_team_id] {
                    owe(format!("draw:{}:{club}", fixture.id), club, prize.per_draw);
                }
            }
        }
    }
    for tie in &state.ties {
        let amount = prize
            .per_stage_reached
            .get(tie.stage.index())
            .copied()
            .unwrap_or(0);
        for club in [&tie.seeded_team_id, &tie.unseeded_team_id] {
            owe(format!("stage:{:?}:{club}", tie.stage), club, amount);
        }
    }
    if let Some(champion) = &state.champion_id {
        owe(format!("champion:{champion}"), champion, prize.champion);
    }
    payments
}

/// Posts what is owed and not yet posted, for every league-phase competition, and records the keys.
/// Returns how many payments it made. A club the world does not know cannot be paid: the payment
/// stays owed and is logged, never recorded as posted.
pub fn post_pending(game: &mut Game) -> usize {
    let date = game.clock.current_date.date_naive();
    let mut made = 0;
    for index in 0..game.competitions.len() {
        let competition = &game.competitions[index];
        let posted: BTreeSet<&str> = competition
            .league_phase
            .as_ref()
            .map(|state| state.payments_posted.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let pending: Vec<Payment> = owed(competition)
            .into_iter()
            .filter(|payment| !posted.contains(payment.key.as_str()))
            .collect();
        if pending.is_empty() {
            continue;
        }
        // One club at a time: a club the world no longer has must not hold back everyone else's
        // prize money, and a payment that failed stays owed (its key is not recorded).
        let id = competition.id.clone();
        let mut clubs: Vec<&str> = pending
            .iter()
            .map(|payment| payment.club_id.as_str())
            .collect();
        clubs.sort_unstable();
        clubs.dedup();
        let mut recorded = Vec::new();
        for club in clubs {
            let own: Vec<&Payment> = pending.iter().filter(|p| p.club_id == club).collect();
            let requests: Vec<PostRequest> = own
                .iter()
                .map(|payment| {
                    PostRequest::new(&payment.club_id, payment.amount, CashKind::PrizeMoney, date)
                })
                .collect();
            match post_all(game, &requests) {
                Ok(_) => {
                    made += own.len();
                    recorded.extend(own.iter().map(|payment| payment.key.clone()));
                }
                Err(error) => {
                    log::error!("[league-phase] {id}: prize money for {club} not posted: {error}");
                }
            }
        }
        if let Some(state) = game.competitions[index].league_phase.as_mut() {
            state.payments_posted.extend(recorded);
        }
    }
    made
}

#[cfg(test)]
mod tests {
    use super::super::profiles;
    use super::super::tests::{drawn, play, play_everything};
    use super::*;
    use domain::league_phase::ContinentalPrize;
    use domain::team::Team;

    fn paying_rules() -> domain::league_phase::LeaguePhaseRules {
        let mut rules = profiles::champions_league_2026();
        rules.prize = ContinentalPrize {
            participation: 1_000,
            per_win: 100,
            per_draw: 30,
            per_stage_reached: vec![5, 10, 20, 40, 80],
            champion: 500,
        };
        rules
    }

    fn game_with(league: League) -> Game {
        let teams: Vec<Team> = league
            .participant_ids
            .iter()
            .map(|id| {
                let mut team = Team::new(
                    id.clone(),
                    id.clone(),
                    id.clone(),
                    "ENG".into(),
                    "C".into(),
                    "G".into(),
                    1,
                );
                team.finance = 10_000;
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
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2027, 6, 10, 12, 0, 0).unwrap(),
        );
        let mut game = Game::new(clock, manager, teams, vec![], vec![], vec![]);
        game.competitions = vec![league];
        game
    }

    fn total_cash(game: &Game) -> i64 {
        game.teams.iter().map(|team| team.finance).sum()
    }

    /// Given a competition that pays nothing (the default)
    /// When pending prizes are posted
    /// Then nothing is posted: a competition that authors no amounts pays none.
    #[test]
    fn a_competition_with_no_authored_amounts_pays_nothing() {
        let mut rules = profiles::champions_league_2026();
        rules.prize = ContinentalPrize::default();
        let mut game = game_with(drawn(rules, 1));
        let before = total_cash(&game);

        assert_eq!(post_pending(&mut game), 0);
        assert_eq!(total_cash(&game), before);
    }

    /// Given an edition that pays
    /// When the same day is processed twice
    /// Then participation is posted to all 36 clubs once, and the second pass posts nothing.
    #[test]
    fn participation_posts_once_however_often_the_day_runs() {
        let mut game = game_with(drawn(paying_rules(), 2));
        let before = total_cash(&game);

        let first = post_pending(&mut game);
        let second = post_pending(&mut game);

        assert_eq!((first, second), (36, 0));
        assert_eq!(total_cash(&game), before + 36 * 1_000);
    }

    /// Given an edition played to its end, posting after every single result
    /// When the totals are compared with what the edition owes
    /// Then every club was paid exactly what the competition's results say, once: wins, draws,
    ///      rounds reached and the champion's prize, with no payment twice and none missing.
    #[test]
    fn an_edition_pays_exactly_what_it_owes_whenever_it_is_posted() {
        let mut game = game_with(drawn(paying_rules(), 3));
        let before = total_cash(&game);
        // Post after every result, and a second time right away: the keys make it idempotent.
        for index in 0..400 {
            let Some(open) = game.competitions[0]
                .fixtures
                .iter()
                .position(|f| f.status == FixtureStatus::Scheduled)
            else {
                break;
            };
            play(&mut game.competitions[0], open, None);
            post_pending(&mut game);
            assert_eq!(post_pending(&mut game), 0, "step {index}");
        }

        let owed_in_full: i64 = owed(&game.competitions[0]).iter().map(|p| p.amount).sum();

        assert_eq!(total_cash(&game) - before, owed_in_full);
        let champion = game.competitions[0]
            .league_phase
            .as_ref()
            .and_then(|s| s.champion_id.clone())
            .expect("the edition finished");
        assert!(
            owed(&game.competitions[0])
                .iter()
                .any(|p| p.key == format!("champion:{champion}") && p.amount == 500)
        );
        // 36 participation + wins/draws + 16+16+8+4+2 stage entries ... all keys distinct.
        let mut keys: Vec<String> = owed(&game.competitions[0])
            .into_iter()
            .map(|p| p.key)
            .collect();
        let total = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), total, "no payment is owed twice");
    }

    /// Given an edition that finished and was saved and reloaded
    /// When pending prizes are posted again
    /// Then nothing is paid a second time, because the posted keys travelled with the save.
    #[test]
    fn a_reload_does_not_pay_again() {
        let mut league = drawn(paying_rules(), 4);
        play_everything(&mut league);
        let mut game = game_with(league);
        post_pending(&mut game);
        let paid = total_cash(&game);

        game.competitions =
            serde_json::from_str(&serde_json::to_string(&game.competitions).unwrap()).unwrap();

        assert_eq!(post_pending(&mut game), 0);
        assert_eq!(total_cash(&game), paid);
    }

    /// Given a club the world no longer has
    /// When prizes are posted
    /// Then every other club is paid, the missing club's payment stays owed (never recorded as
    ///      posted), and posting again pays nobody twice.
    #[test]
    fn an_unknown_club_does_not_hold_back_everyone_elses_prize_money() {
        let mut game = game_with(drawn(paying_rules(), 5));
        let gone = game.teams.pop().unwrap().id;

        let first = post_pending(&mut game);
        let second = post_pending(&mut game);

        assert_eq!((first, second), (35, 0));
        let posted = &game.competitions[0]
            .league_phase
            .as_ref()
            .unwrap()
            .payments_posted;
        assert_eq!(posted.len(), 35);
        assert!(!posted.contains(&format!("participation:{gone}")));
    }

    /// Given a hand-built edition: one win, one draw, one tie that reached the round of 16, no champion yet
    /// When what is owed is totalled per club
    /// Then each club's total is the literal sum of the rules' amounts, not what the code says it owes.
    #[test]
    fn the_amounts_owed_are_the_rules_amounts_for_what_each_club_did() {
        use domain::league::{Fixture, MatchResult};
        use domain::league_phase::{KnockoutStage, KnockoutTie, LeaguePhaseState};
        let mut league = League::new(
            "cl".into(),
            "CL".into(),
            2026,
            &["a".into(), "b".into(), "c".into(), "d".into()],
        );
        league.rules.format = domain::league::CompetitionFormat::LeaguePhaseAndKnockout;
        league.rules.league_phase = Some(paying_rules());
        let played = |id: &str, home: &str, away: &str, score: (u8, u8)| Fixture {
            id: id.into(),
            home_team_id: home.into(),
            away_team_id: away.into(),
            status: FixtureStatus::Completed,
            result: Some(MatchResult {
                home_goals: score.0,
                away_goals: score.1,
                ..Default::default()
            }),
            ..Default::default()
        };
        league.fixtures = vec![
            played("win", "a", "b", (2, 0)),
            played("draw", "c", "d", (1, 1)),
        ];
        league.league_phase = Some(LeaguePhaseState {
            stage: CompetitionStage::RoundOf16,
            ties: vec![KnockoutTie {
                stage: KnockoutStage::RoundOf16,
                seeded_team_id: "a".into(),
                unseeded_team_id: "c".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let total = |league: &League, club: &str| -> i64 {
            owed(league)
                .iter()
                .filter(|p| p.club_id == club)
                .map(|p| p.amount)
                .sum()
        };

        // participation 1000; win 100; draw 30; reaching the round of 16 is the second stage entry: 10.
        assert_eq!(total(&league, "a"), 1_000 + 100 + 10);
        assert_eq!(total(&league, "b"), 1_000);
        assert_eq!(total(&league, "c"), 1_000 + 30 + 10);
        assert_eq!(total(&league, "d"), 1_000 + 30);

        league.league_phase.as_mut().unwrap().champion_id = Some("a".into());
        assert_eq!(total(&league, "a"), 1_000 + 100 + 10 + 500);
    }
}
