//! A club's academy players: which of them are ready for the senior squad.
//!
//! The match-day pool is the senior squad. An academy player (`SquadRole::Youth`) is in it only
//! when a club has fewer than eighteen healthy seniors, so before this module a youngster could
//! be a first-eleven player in everything but his shirt and still play two minutes a season until
//! the day the academy's age limit made him a senior. A ten-season gate study (P4.7) counted
//! twelve such players in the opening world's three seeds and sixty to seventy by the tenth
//! season, all of them under 450 minutes, 99 % of them under ten.
//!
//! A youngster is promoted when the club's own squad model says he makes its side better. That is
//! the question the market asks of every signing, and it is asked of him in the same terms: would
//! he start, would he cover a slot the squad leaves thin, would he be needed within the horizon.
//! Nothing here sets a number of minutes or a number of promotions: a player the model does not
//! want stays in the academy and keeps developing, and one it wants is a senior, where the
//! lineup picker plays him if he is the best player for the slot and the bench otherwise.
//!
//! The manager's trust in youth moves the bar a player has to clear, not the verdict on how good
//! he is: a manager who builds around young players promotes a youngster who adds a little, a
//! sceptic one who adds a lot. Nobody's bar is zero.

use chrono::NaiveDate;
use domain::player::{Player, SquadRole};
use domain::team::Team;

use crate::game::Game;
use crate::manager_identity::ManagerIdentity;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::impact::{MIN_GAIN, Marginal};

/// Weeks between a club's looks at its academy, on the weekly review.
const ACADEMY_LOOK_EVERY_WEEKS: u64 = 4;

/// Whether the club's weekly review falling on `today` is one that looks at the academy: one week
/// in four, a different one for each club.
pub(crate) fn is_academy_week(team_id: &str, today: NaiveDate) -> bool {
    let week = today.signed_duration_since(NaiveDate::default()).num_days() as u64 / 7;
    let phase = crate::stable_hash::stable_hash(team_id.as_bytes(), ACADEMY_SALT)
        % ACADEMY_LOOK_EVERY_WEEKS;
    week % ACADEMY_LOOK_EVERY_WEEKS == phase
}

const ACADEMY_SALT: u64 = 0x6163_6164_656d_7900; // "academy\0"

/// Promotions one club makes in one review. A club does not empty its academy in a week; it looks
/// again at its next review, with the squad as the promotion has left it.
const MAX_PROMOTIONS_PER_REVIEW: usize = 2;

/// Younger than this a player is a child in the academy and not a succession plan: the model's
/// reading of a sixteen-year-old's third year is a guess, and he has nothing to learn in a senior
/// dressing room that the academy does not teach.
const SUCCESSION_MIN_AGE: i32 = 18;

/// Where a youngster stands with his club's senior squad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pathway {
    /// He would be in the side the club fields today.
    Starter,
    /// He would not start, but the squad is better with him in it this season (cover for a slot,
    /// an absence, a position it is short in).
    Cover,
    /// He is not needed this season and will be.
    Succession,
    /// The squad does not need him yet: he stays where he is developing.
    NotYet,
}

/// What a signing has to add, in the model's own rating points, for a club whose manager trusts
/// youth this much (0 to 1, 0.5 being the neutral manager): the market's floor at 0.5, half of it
/// at full trust, one and a half times at none.
pub(crate) fn bar(youth_trust: f64) -> f32 {
    MIN_GAIN * (1.5 - youth_trust.clamp(0.0, 1.0) as f32)
}

/// The pathway of a youngster of `age` whom the squad model values at `marginal`.
pub(crate) fn pathway_of(marginal: &Marginal, age: i32, youth_trust: f64) -> Pathway {
    let bar = bar(youth_trust);
    if marginal.now_gain >= bar {
        if marginal.starts_at.is_some() {
            Pathway::Starter
        } else {
            Pathway::Cover
        }
    } else if marginal.gain >= bar && age >= SUCCESSION_MIN_AGE {
        Pathway::Succession
    } else {
        Pathway::NotYet
    }
}

/// How far below the cover behind a group's starters a youngster, as rated a year on, may sit and
/// still be worth asking the squad model about. The screen is deliberately generous: the model has
/// the last word, the screen only keeps it from being built for an academy that is nowhere near.
const SCREEN_MARGIN: u8 = 3;

/// For each position group, the rating of the first senior behind its starters (0 where the group
/// has no cover at all: any youngster fills that).
fn cover_levels(game: &Game, club: &[usize]) -> [u8; 4] {
    use crate::squad_floor::{STARTERS_PER_GROUP, group_index};
    let mut seniors: [Vec<u8>; 4] = Default::default();
    for &index in club {
        let player = &game.players[index];
        if player.squad_role == SquadRole::Senior && !player.retired {
            seniors[group_index(&player.position)].push(player.ovr);
        }
    }
    let mut covers = [0u8; 4];
    for (group, ratings) in seniors.iter_mut().enumerate() {
        ratings.sort_unstable_by(|a, b| b.cmp(a));
        covers[group] = ratings.get(STARTERS_PER_GROUP[group]).copied().unwrap_or(0);
    }
    covers
}

/// The player as the market would see him if he were a senior: the model values only seniors, and
/// the question here is what he would add to the side.
fn as_senior(player: &Player) -> Player {
    let mut senior = player.clone();
    senior.squad_role = SquadRole::Senior;
    senior
}

/// Promotes the club's academy players who make its side better, best first, and returns their
/// ids. `club` is where the club's players stand in `game.players`.
pub(crate) fn promote_ready_academy_players(
    game: &mut Game,
    team: &Team,
    club: &[usize],
) -> Vec<String> {
    let today = game.clock.current_date.date_naive();
    let youth_trust = ManagerIdentity::for_club(game, Some(team)).youth_trust;
    let mut promoted = Vec::new();
    while promoted.len() < MAX_PROMOTIONS_PER_REVIEW {
        let Some(next) = most_ready(game, team, club, today, youth_trust) else {
            break;
        };
        game.players[next].squad_role = SquadRole::Senior;
        promoted.push(game.players[next].id.clone());
    }
    promoted
}

/// The academy player the club would promote first, if any: the one who adds most to the side
/// now, then to its future, the lowest id deciding a tie.
fn most_ready(
    game: &Game,
    team: &Team,
    club: &[usize],
    today: NaiveDate,
    youth_trust: f64,
) -> Option<usize> {
    let covers = cover_levels(game, club);
    let academy: Vec<usize> = club
        .iter()
        .copied()
        .filter(|&index| game.players[index].squad_role == SquadRole::Youth)
        .filter(|&index| !game.players[index].retired && game.players[index].can_play())
        .filter(|&index| {
            let player = &game.players[index];
            let a_year_on = crate::squad_outlook::expected_ovr_for(player, today);
            a_year_on + SCREEN_MARGIN >= covers[crate::squad_floor::group_index(&player.position)]
        })
        .collect();
    if academy.is_empty() {
        return None;
    }
    let squad = ClubSquad::for_planning(
        club.iter().map(|&index| &game.players[index]),
        team,
        today,
        Appetite::NEUTRAL,
    );
    academy
        .into_iter()
        .filter_map(|index| {
            let player = &game.players[index];
            let marginal = squad.signing(&as_senior(player), 0.0)?;
            let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
            (pathway_of(&marginal, age, youth_trust) != Pathway::NotYet)
                .then_some((index, marginal))
        })
        .max_by(|(left, a), (right, b)| {
            a.now_gain
                .total_cmp(&b.now_gain)
                .then(a.gain.total_cmp(&b.gain))
                .then_with(|| game.players[*right].id.cmp(&game.players[*left].id))
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::squad_intelligence::impact::Reason;
    use crate::squad_intelligence::test_squads::{Squad, player, team, today};
    use domain::player::Position;

    fn marginal(now_gain: f32, gain: f32, starts: bool) -> Marginal {
        Marginal {
            gain,
            now_gain,
            starts_at: starts.then_some(3),
            points: 0,
            reason: Reason::Depth,
        }
    }

    /// Given the same small gain
    /// When managers who trust youth to different degrees read it
    /// Then the trusting one promotes and the sceptic does not, and a gain nobody can see is
    /// never promoted by anyone.
    #[test]
    fn a_managers_trust_in_youth_moves_the_bar_a_youngster_has_to_clear() {
        let small = marginal(MIN_GAIN * 0.8, MIN_GAIN * 0.8, false);
        assert_eq!(pathway_of(&small, 19, 1.0), Pathway::Cover);
        assert_eq!(pathway_of(&small, 19, 0.0), Pathway::NotYet);
        let nothing = marginal(0.0, 0.0, false);
        for trust in [0.0, 0.5, 1.0] {
            assert_eq!(pathway_of(&nothing, 19, trust), Pathway::NotYet);
        }
        assert!(bar(0.0) > bar(0.5) && bar(0.5) > bar(1.0) && bar(1.0) > 0.0);
    }

    /// Given a gain that is all future
    /// When a seventeen-year-old and a nineteen-year-old are read
    /// Then the older is a succession plan and the child stays in the academy.
    #[test]
    fn a_child_is_not_a_succession_plan() {
        let future = marginal(0.0, MIN_GAIN * 2.0, false);
        assert_eq!(pathway_of(&future, 17, 0.5), Pathway::NotYet);
        assert_eq!(pathway_of(&future, 19, 0.5), Pathway::Succession);
    }

    /// Given a gain that starts
    /// When it is read
    /// Then it is a Starter and not merely cover.
    #[test]
    fn a_player_who_would_start_is_a_starter() {
        let starts = marginal(MIN_GAIN * 2.0, MIN_GAIN * 2.0, true);
        assert_eq!(pathway_of(&starts, 19, 0.5), Pathway::Starter);
    }

    /// A game with one AI club holding `squad` and the youngsters given, as academy players, and
    /// the player's own club besides.
    fn game_with(squad: &Squad, academy: Vec<Player>, manager_id: Option<&str>) -> Game {
        use chrono::TimeZone;
        let mut user_manager = domain::manager::Manager::new(
            "user-manager".into(),
            "Test".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        user_manager.hire("user".into());
        let mut ai = team("4-4-2", 1_000_000, 600_000);
        ai.manager_id = manager_id.map(str::to_string);
        let mut user = team("4-4-2", 1_000_000, 600_000);
        user.id = "user".into();
        user.name = "user".into();
        let clock = crate::clock::GameClock::new(
            chrono::Utc.from_utc_datetime(&today().and_hms_opt(12, 0, 0).unwrap()),
        );
        let mut players = squad.players.clone();
        for mut youngster in academy {
            youngster.squad_role = SquadRole::Youth;
            players.push(youngster);
        }
        Game::new(clock, user_manager, vec![ai, user], players, vec![], vec![])
    }

    fn club_of(game: &Game) -> Vec<usize> {
        crate::match_day_eligibility::club_positions(&game.players, "club")
    }

    /// Given an academy striker rated above the club's first-choice striker, and another rated
    /// far below the squad
    /// When the club's academy is reviewed
    /// Then the ready one is promoted and the other stays, so no quota moves a player who is not
    /// ready.
    #[test]
    fn an_academy_player_who_would_start_is_promoted_and_one_who_would_not_is_not() {
        let squad = Squad::balanced();
        let ready = player("ready-striker", Position::Striker, 80, 19);
        let raw = player("raw-striker", Position::Striker, 40, 17);
        let mut game = game_with(&squad, vec![ready, raw], None);
        let team = game.teams[0].clone();
        let club = club_of(&game);
        let promoted = promote_ready_academy_players(&mut game, &team, &club);
        assert_eq!(promoted, vec!["ready-striker".to_string()]);
        let role = |id: &str| game.players.iter().find(|p| p.id == id).unwrap().squad_role;
        assert_eq!(role("ready-striker"), SquadRole::Senior);
        assert_eq!(role("raw-striker"), SquadRole::Youth);
    }

    /// Given a ready academy player and a club that already has a better player in every slot
    /// When the academy is reviewed
    /// Then nothing is promoted: ready for the academy's standard is not ready for this squad.
    #[test]
    fn a_youngster_who_would_not_improve_the_side_stays_in_the_academy() {
        let squad = Squad::balanced();
        let decent = player("decent-striker", Position::Striker, 60, 19);
        let mut game = game_with(&squad, vec![decent], None);
        let team = game.teams[0].clone();
        let club = club_of(&game);
        assert!(promote_ready_academy_players(&mut game, &team, &club).is_empty());
    }

    /// Given three equally ready academy players
    /// When the club reviews its academy
    /// Then at most two are promoted in one review, and a later review takes the rest.
    #[test]
    fn a_club_promotes_at_most_two_a_review() {
        let squad = Squad::balanced();
        let academy = (0..4)
            .map(|i| player(&format!("star-{i}"), Position::Striker, 85 + i as u8, 19))
            .collect();
        let mut game = game_with(&squad, academy, None);
        let team = game.teams[0].clone();
        let club = club_of(&game);
        assert_eq!(
            promote_ready_academy_players(&mut game, &team, &club).len(),
            MAX_PROMOTIONS_PER_REVIEW
        );
    }

    fn manager_with(trust: impl Fn(f64) -> bool) -> String {
        (0..4000)
            .map(|i| format!("manager-{i}"))
            .find(|id| trust(ManagerIdentity::of(id).youth_trust))
            .expect("the world has managers of every disposition")
    }

    /// Given a club and the weeks of a year
    /// When its academy looks are counted
    /// Then it looks about one week in four, and two clubs do not share every week.
    #[test]
    fn a_club_looks_at_its_academy_about_one_week_in_four() {
        let looks = |club: &str| {
            (0..52)
                .filter(|week| is_academy_week(club, today() + chrono::Duration::days(week * 7)))
                .count()
        };
        assert!((12..=14).contains(&looks("club")), "{}", looks("club"));
        let weeks_of = |club: &str| -> Vec<i64> {
            (0..52)
                .filter(|w| is_academy_week(club, today() + chrono::Duration::days(w * 7)))
                .collect()
        };
        assert!((0..40).any(|i| weeks_of(&format!("club-{i}")) != weeks_of("club")));
    }

    /// Given a youngster who adds a little to the side, and two clubs identical but for a manager
    /// who builds around young players and one who does not
    /// When each club's academy is reviewed
    /// Then the trusting manager's club promotes him and the sceptic's does not: trust moves a
    /// real decision, not only the transfer preferences.
    #[test]
    fn a_trusting_manager_promotes_the_youngster_a_sceptic_leaves_in_the_academy() {
        let believer = manager_with(|t| t > 0.9);
        let sceptic = manager_with(|t| t < 0.1);
        let squad = Squad::balanced();
        // Find the rating at which the two managers part ways: a youngster the model values
        // between the sceptic's bar and the believer's.
        let promoted_by = |id: &str, ovr: u8| {
            let youngster = player("prospect", Position::Striker, ovr, 19);
            let mut game = game_with(&squad, vec![youngster], Some(id));
            let team = game.teams[0].clone();
            let club = club_of(&game);
            !promote_ready_academy_players(&mut game, &team, &club).is_empty()
        };
        let ovr = (55..=80)
            .find(|&ovr| promoted_by(&believer, ovr) && !promoted_by(&sceptic, ovr))
            .expect("some rating separates the two managers");
        assert!(promoted_by(&believer, ovr));
        assert!(!promoted_by(&sceptic, ovr));
    }

    /// Given an AI club with a ready academy player
    /// When every AI club plans its squad, as they do at a season's end
    /// Then he is a senior afterwards: the promotion is wired into the planning the game runs.
    #[test]
    fn squad_planning_promotes_a_ready_academy_player() {
        let squad = Squad::balanced();
        let star = player("academy-star", Position::Striker, 85, 19);
        let mut game = game_with(&squad, vec![star], None);
        crate::ai_contracts::plan_every_ai_squad(&mut game);
        let star = game
            .players
            .iter()
            .find(|p| p.id == "academy-star")
            .unwrap();
        assert_eq!(star.squad_role, SquadRole::Senior);
    }

    /// Given an injured academy player who would otherwise start
    /// When the academy is reviewed
    /// Then he is not promoted this week: a promotion is for who can play.
    #[test]
    fn an_injured_academy_player_waits_for_his_fitness() {
        let squad = Squad::balanced();
        let mut hurt = player("hurt-striker", Position::Striker, 85, 19);
        hurt.injury = Some(domain::player::Injury {
            name: "knock".into(),
            days_remaining: 10,
        });
        let mut game = game_with(&squad, vec![hurt], None);
        let team = game.teams[0].clone();
        let club = club_of(&game);
        assert!(promote_ready_academy_players(&mut game, &team, &club).is_empty());
    }
}
