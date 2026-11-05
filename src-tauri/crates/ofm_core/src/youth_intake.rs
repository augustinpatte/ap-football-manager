//! A club's annual youth intake: the youngsters who join its academy at each
//! season's end.
//!
//! The intake is the world's one ordinary source of new players. Without it a
//! world only loses them — players retire, the academies the generator seeded
//! graduate within a few seasons, the free-agent pool drains, and from then on
//! clubs live on the squad floor's emergency top-up.
//!
//! One rule for every club, the player's included. It is split in two on
//! purpose: [`plan_for`] decides how many youngsters a club takes and where they
//! play, from its academy alone and with no randomness, so a later academy cost
//! can price a plan before anything is generated; [`apply_youth_intake`] then
//! brings the planned youngsters in.

use crate::game::Game;
use crate::squad_floor::{group_index, thinnest_first};
use chrono::{Datelike, NaiveDate};
use domain::contract_ledger::ContractSource;
use domain::message::{InboxMessage, MessageCategory, MessagePriority};
use domain::player::{Player, PlayerMovementKind, Position, SquadRole};
use domain::team::Team;
use rand::RngExt;
use std::collections::HashMap;

/// Academy players a club aims to hold in each position group, in the squad
/// floor's `[GK, DEF, MID, FWD]` order. Shaped like a squad, a keeper included,
/// so the players who graduate from it can replace the ones who retire.
const ACADEMY_TARGET_PER_GROUP: [usize; 4] = [1, 2, 2, 1];

/// Youngsters every club takes in a season, however full its academy is. A
/// club always renews itself a little; the minimum is what keeps one whose
/// academy is full from going a whole season without a newcomer.
const MIN_INTAKE: usize = 1;

/// Youngsters a club takes in a season at most, however empty its academy is.
/// A club rebuilding an academy does it over a few seasons, not in one summer.
const MAX_INTAKE: usize = 3;

/// The ages a youngster joins an academy at. Younger than a youth scout's
/// recruit, who is found ready to play: an intake joins to be brought on.
const INTAKE_AGES: std::ops::RangeInclusive<u32> = 15..=17;

/// Which youngsters a club takes in this season: one position group per
/// recruit, a keeper first when the academy has none.
///
/// Pure and free: what the finance epic will attach an academy cost to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntakePlan {
    pub groups: Vec<Position>,
}

/// The intake a club with `academy` takes: as many youngsters as it is short of
/// [`ACADEMY_TARGET_PER_GROUP`], between [`MIN_INTAKE`] and [`MAX_INTAKE`]; a
/// keeper first when it has none, then its thinnest groups.
pub fn plan_for<'a>(academy: impl IntoIterator<Item = &'a Player>) -> IntakePlan {
    let mut have = [0usize; 4];
    for player in academy {
        have[group_index(&player.position)] += 1;
    }
    let size = intake_size(have);

    let keeper = group_index(&Position::Goalkeeper);
    let mut groups = Vec::with_capacity(size);
    if have[keeper] == 0 {
        groups.push(Position::Goalkeeper);
        have[keeper] += 1;
    }
    while groups.len() < size {
        let thinnest = thinnest_first(have, ACADEMY_TARGET_PER_GROUP).remove(0);
        have[group_index(&thinnest)] += 1;
        groups.push(thinnest);
    }
    IntakePlan { groups }
}

/// How many youngsters a club whose academy holds `have` per group takes: what
/// it lacks of [`ACADEMY_TARGET_PER_GROUP`], between [`MIN_INTAKE`] and
/// [`MAX_INTAKE`]. The one place an intake's size is decided.
fn intake_size(have: [usize; 4]) -> usize {
    let lacking: usize = ACADEMY_TARGET_PER_GROUP
        .iter()
        .zip(have)
        .map(|(target, have)| target.saturating_sub(have))
        .sum();
    lacking.clamp(MIN_INTAKE, MAX_INTAKE)
}

/// Every club takes its season's intake into its academy on `date`, the end of
/// `season`.
pub fn apply_youth_intake(game: &mut Game, date: NaiveDate, season: u32) {
    let clubs: Vec<String> = game.teams.iter().map(|team| team.id.clone()).collect();
    for club in clubs {
        take_youth_intake(game, &club, date, season);
    }
}

/// One club takes its season's intake into its academy on `date`, the end of its
/// `season`. The player's club is told who joined; an AI club's refusals are
/// logged.
pub fn take_youth_intake(game: &mut Game, team_id: &str, date: NaiveDate, season: u32) {
    let Some(team_index) = game.teams.iter().position(|team| team.id == team_id) else {
        log::error!("[youth_intake] no club {team_id} to take an intake");
        return;
    };
    // An academy player out on loan belongs to the academy of the club that
    // owns him and gets him back, not the one borrowing him for a season.
    let plan = plan_for(game.players.iter().filter(|player| {
        player.squad_role == SquadRole::Youth && player.contract_club_id() == Some(team_id)
    }));

    let intake = take_in(game, team_index, &plan, date);
    if game.manager.team_id.as_deref() == Some(team_id) {
        tell_the_player(game, team_index, &intake, date, season);
    } else if intake.refused > 0 {
        log::info!(
            "[youth_intake] the board at {team_id} turned away {} of {} recruits on wages",
            intake.refused,
            plan.groups.len()
        );
    }
}

/// What a recruit is offered when the board will not pay what he asks: the least
/// any player in the game is paid a week.
/// Most points of potential the academy's staff add to or take from an intake's promise.
pub const YOUTH_POTENTIAL_SWING: u8 = 3;

/// How far a club's standing moves the promise of its intake, either way, in points of potential.
/// An edge for the big academies (and a handicap for the small), well inside the bell's spread of
/// about 5, so a small club still turns up an exceptional youngster now and then.
const ACADEMY_STANDING_SWING: f64 = 1.0;

/// Points of potential the academy adds to or takes from an intake's promise.
///
/// The youth department adds up to [`YOUTH_POTENTIAL_SWING`] over an empty one and never costs
/// anything: an empty department is the baseline the world's bell was calibrated on. It used to be
/// centred on a rating of 50, which made an empty department cost two points, and since no AI
/// club ever staffed an academy the whole world's intakes were two points below the opening
/// world's, which halved the number of players over 80 within a decade (F5.1 world study).
/// A club's standing adds a symmetric edge of up to [`ACADEMY_STANDING_SWING`].
pub(crate) fn academy_potential_shift(youth_rating: f64, reputation: u32) -> f64 {
    let vacant = crate::staff_system::profile::VACANT_RATING;
    let staff = ((youth_rating - vacant) / (100.0 - vacant)).clamp(0.0, 1.0)
        * f64::from(YOUTH_POTENTIAL_SWING);
    let standing = (crate::club_economy::stature(reputation) - 0.5) * 2.0 * ACADEMY_STANDING_SWING;
    staff + standing
}

const YOUTH_MINIMUM_WAGE: u32 = crate::contracts::MINIMUM_DEFAULT_WAGE as u32;

/// The most a recruit may ask a week at this club: a quarter above what the club's academy
/// already pays (its median), or, with fewer than three academy players to go by, above the lowest
/// quarter of its seniors' wages — never less than the youth minimum.
///
/// The generator sizes a youngster's wage from his market value on its own scale, which in a world
/// whose pay was not generated from values (an FM24 club paying 700 a week) is ten to twenty
/// times what the club pays anyone: a club whose whole wage budget was 13 000 took four recruits
/// at 11 000 each, because the board's grace for a club already over budget let every one through.
fn recruit_wage_ceiling(game: &Game, team_id: &str) -> u32 {
    let wages = |role: SquadRole| -> Vec<u32> {
        let mut wages: Vec<u32> = game
            .players
            .iter()
            .filter(|player| {
                player.team_id.as_deref() == Some(team_id) && player.squad_role == role
            })
            .map(Player::wage)
            .filter(|wage| *wage > 0)
            .collect();
        wages.sort_unstable();
        wages
    };
    let academy = wages(SquadRole::Youth);
    let scale = if academy.len() >= 3 {
        academy[academy.len() / 2]
    } else {
        let seniors = wages(SquadRole::Senior);
        seniors.get(seniors.len() / 4).copied().unwrap_or(0)
    };
    (scale + scale / 4).max(YOUTH_MINIMUM_WAGE)
}

/// The wage the board lets `team` pay a recruit who asks `asking` a week: what
/// he asks, else the youth minimum, else nothing — he is not taken. The board's
/// one wage rule decides, as for every contract a club offers, and a recruit is
/// judged as a newcomer to the bill. No academy player holds up the squad floor,
/// so the floor's waiver never applies.
fn agreed_wage(game: &Game, team: &Team, recruit: &Player, asking: u32) -> Option<u32> {
    // An AI club's recruit also has to fit its own books when he asks more than the youth minimum:
    // the board's policy tolerates a little more at every decision for a club already over budget
    // (see `ai_finance_guard`), which let every recruit of an over-budget academy through at
    // whatever he asked. The minimum is not held to it: a club at its budget still takes its
    // intake, at the least any player is paid, or the academies of every club that spends what it
    // has would empty and the world's supply of youngsters with them (F5.3 measured 14 000 players
    // falling to 10 300 in ten seasons). The player's club is left to its board.
    let books = (game.manager.team_id.as_deref() != Some(team.id.as_str())).then(|| {
        crate::ai_finance_guard::SigningBooks {
            cash: team.finance,
            weekly_bill: crate::finances::calc_wages(game, &team.id),
            weekly_income: 0,
            wage_budget: team.wage_budget,
        }
    });
    let minimum = YOUTH_MINIMUM_WAGE.min(asking);
    [asking, minimum].into_iter().find(|&wage| {
        let fits_books = wage <= minimum
            || books
                .as_ref()
                .is_none_or(|books| books.can_add_wage(i64::from(wage)));
        fits_books
            && crate::contract_wage_policy::joining_wage_policy_verdict(game, team, recruit, wage)
                .permits()
    })
}

/// What came of a club's intake: who joined, in plan order, and how many of the
/// plan did not — the board would not pay for them.
struct Intake {
    joined: Vec<String>,
    refused: usize,
}

/// Bring `plan`'s youngsters into the academy of the club at `team_index`, each
/// on the wage the board agrees to. The first recruit the board will not pay
/// even the youth minimum ends the club's intake: every later one would cost at
/// least as much against the same bill.
fn take_in(game: &mut Game, team_index: usize, plan: &IntakePlan, date: NaiveDate) -> Intake {
    let recruits = draw_recruits(game, team_index, plan, date);
    let mut intake = sign_until_refused(game, team_index, recruits, date);
    // Every planned recruit who did not join counts against the intake, whatever
    // stopped him, so the player is never told an empty list joined.
    intake.refused = plan.groups.len() - intake.joined.len();
    intake
}

/// One planned recruit, drawn: the youngster, the wage he asks, and when the
/// contract he asks for ends.
struct Drawn {
    recruit: Player,
    asking: u32,
    end: NaiveDate,
}

/// Draw `plan`'s youngsters for the club at `team_index`, in plan order, each
/// with the terms he asks: those he was generated with when they run past
/// `date`, the club's standard terms otherwise.
fn draw_recruits(game: &Game, team_index: usize, plan: &IntakePlan, date: NaiveDate) -> Vec<Drawn> {
    // The club's own stream of the save's seed for the day, so a replayed season
    // end draws the same youngsters whatever order the clubs come in (their ids
    // are still fresh), and two careers from one package do not.
    let mut rng = game.rng_for(
        &format!("youth-intake/{}", game.teams[team_index].id),
        &date.format("%Y-%m-%d").to_string(),
    );
    let team = &game.teams[team_index];
    // The academy's staff decide how much of a youngster's promise is found and kept: up to
    // `YOUTH_POTENTIAL_SWING` points of potential either way, around the generator's own
    // calibration. A club with no youth staff loses a couple of points, never gains any.
    let youth_rating = crate::staff_system::profile::StaffProfile::of(game, &team.id).youth;
    let potential_shift = academy_potential_shift(youth_rating, team.reputation);
    plan.groups
        .iter()
        .filter_map(|group| {
            let age = rng.random_range(INTAKE_AGES);
            let mut recruit = crate::generator::generate_youth_intake_recruit(
                team,
                group,
                age,
                date.year() as u32,
                &mut rng,
            );
            recruit.potential = (f64::from(recruit.potential) + potential_shift)
                .round()
                .clamp(f64::from(recruit.ovr), 99.0) as u8;
            let own_terms = recruit
                .contract_end()
                .and_then(crate::contracts::parse_contract_date)
                .filter(|end| *end > date)
                .map(|end| (recruit.wage(), end))
                .filter(|(wage, _)| *wage > 0);
            let (asking, end) = own_terms
                .or_else(|| crate::contracts::standard_contract_terms(&recruit, team, date, 0))?;
            // He asks what a youngster asks at this club, not what the generator sizes a
            // youngster's wage at.
            let asking = asking.min(recruit_wage_ceiling(game, &team.id));
            Some(Drawn {
                recruit,
                asking,
                end,
            })
        })
        .collect()
}

/// Sign `recruits` into the academy of the club at `team_index`, in order, each
/// on the wage the board agrees to, until the first the board will not pay even
/// the youth minimum: that ends the club's intake. Joining is a contract made
/// mid-career, so it goes in his history, as a scouted youngster's signing does.
fn sign_until_refused(
    game: &mut Game,
    team_index: usize,
    recruits: Vec<Drawn>,
    date: NaiveDate,
) -> Intake {
    let mut joined = Vec::with_capacity(recruits.len());
    for Drawn {
        mut recruit,
        asking,
        end,
    } in recruits
    {
        let team = &game.teams[team_index];
        let Some(wage) = agreed_wage(game, team, &recruit, asking) else {
            break;
        };
        crate::contracts::record_movement(
            &mut recruit,
            crate::contracts::contract_entry(
                PlayerMovementKind::FreeAgentSigning,
                date,
                team,
                crate::contracts::contract_record(date, end, wage, ContractSource::FreeAgent),
            ),
        );
        recruit.jersey_number = crate::roster::resolve_jersey_for(game, &recruit, team);
        joined.push(recruit.full_name.clone());
        game.players.push(recruit);
    }
    Intake { joined, refused: 0 }
}

/// Tell the player who joined the academy, and that the board would not take on
/// the rest when it turned any away.
fn tell_the_player(
    game: &mut Game,
    team_index: usize,
    intake: &Intake,
    date: NaiveDate,
    season: u32,
) {
    let body_key = match (intake.joined.is_empty(), intake.refused) {
        (_, 0) => "be.msg.youthIntake.body",
        (false, _) => "be.msg.youthIntake.bodyPartlyRefused",
        (true, _) => "be.msg.youthIntake.bodyAllRefused",
    };
    let team = &game.teams[team_index];
    let mut params = HashMap::new();
    params.insert("team".to_string(), team.name.clone());
    params.insert("players".to_string(), intake.joined.join(", "));
    let message = InboxMessage::new(
        format!("youth_intake_{}_{season}", team.id),
        String::new(),
        String::new(),
        String::new(),
        date.format("%Y-%m-%d").to_string(),
    )
    .with_category(MessageCategory::ScoutReport)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("")
    .with_i18n("be.msg.youthIntake.subject", body_key, params)
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager");
    crate::inbox::emit(game, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given a club of middling standing with nobody in its youth department, when its intake's
    /// promise is shifted, then nothing is lost: an empty department is the world's baseline, so
    /// every AI club (none of which staffs one) draws the same bell as the opening world did.
    #[test]
    fn an_unstaffed_academy_of_middling_standing_draws_the_worlds_own_bell() {
        let vacant = crate::staff_system::profile::VACANT_RATING;
        assert_eq!(academy_potential_shift(vacant, 590), 0.0);
    }

    /// Given the same club with the best possible youth staff, when shifted, then it gains the
    /// whole staff swing and no more.
    #[test]
    fn the_best_youth_staff_add_the_whole_swing() {
        let shift = academy_potential_shift(100.0, 590);
        assert!(
            (shift - f64::from(YOUTH_POTENTIAL_SWING)).abs() < 1e-9,
            "{shift}"
        );
    }

    /// Given an unstaffed top club and an unstaffed bottom club, when shifted, then the big
    /// club's academy is better on average and the small club's worse, by the same amount and
    /// by less than a quarter of the bell's spread: an edge, not a monopoly.
    #[test]
    fn a_bigger_club_has_a_better_academy_without_a_monopoly() {
        let vacant = crate::staff_system::profile::VACANT_RATING;
        let (top, bottom) = (
            academy_potential_shift(vacant, 780),
            academy_potential_shift(vacant, 400),
        );
        assert!(top > 0.0 && bottom < 0.0);
        assert!(
            (top + bottom).abs() < 1e-9,
            "symmetric around the world's bell"
        );
        assert!(
            top <= 1.25,
            "{top} is more than a quarter of a 5-point spread"
        );
    }

    /// Given a youth rating below the vacant baseline (not reachable today, but a future staff
    /// rule could), when shifted, then the staff term never goes negative.
    #[test]
    fn staff_never_cost_potential() {
        assert_eq!(academy_potential_shift(0.0, 590), 0.0);
    }

    use crate::clock::GameClock;
    use crate::squad_floor::MIN_PLAYERS_PER_GROUP;
    use crate::test_support::uniform_attributes;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::team::Team;

    const SEASON: u32 = 2026;

    fn season_end() -> NaiveDate {
        NaiveDate::from_ymd_opt(2027, 5, 30).unwrap()
    }

    fn youngster(id: &str, team_id: &str, position: Position) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "2009-03-01".to_string(),
            "ENG".to_string(),
            position,
            uniform_attributes(50),
        );
        player.team_id = Some(team_id.to_string());
        player.squad_role = SquadRole::Youth;
        player
    }

    /// An academy with `per_group` youngsters, `[GK, DEF, MID, FWD]`.
    fn academy(team_id: &str, per_group: [usize; 4]) -> Vec<Player> {
        MIN_PLAYERS_PER_GROUP
            .iter()
            .zip(per_group)
            .flat_map(|((group, _), count)| {
                (0..count).map(move |n| {
                    youngster(&format!("{team_id}-{group:?}-{n}"), team_id, group.clone())
                })
            })
            .collect()
    }

    fn team(id: &str) -> Team {
        Team::new(
            id.to_string(),
            format!("{id} FC"),
            "CLB".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Ground".to_string(),
            20_000,
        )
    }

    /// The player's club `user` and an AI club `rival`, with these academies.
    fn world(user_academy: [usize; 4], rival_academy: [usize; 4]) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2027, 5, 30, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("user".to_string());
        let mut players = academy("user", user_academy);
        players.extend(academy("rival", rival_academy));
        Game::new(
            clock,
            manager,
            vec![team("user"), team("rival")],
            players,
            vec![],
            vec![],
        )
    }

    fn newcomers<'a>(before: &Game, after: &'a Game, team_id: &str) -> Vec<&'a Player> {
        after
            .players
            .iter()
            .filter(|player| player.team_id.as_deref() == Some(team_id))
            .filter(|player| !before.players.iter().any(|old| old.id == player.id))
            .collect()
    }

    fn intake(game: &Game) -> Game {
        let mut after = game.clone();
        apply_youth_intake(&mut after, season_end(), SEASON);
        after
    }

    // -- the rule ------------------------------------------------------------

    #[test]
    fn every_club_takes_youngsters_in_each_season() {
        let before = world([1, 3, 3, 2], [0, 1, 1, 0]);
        let after = intake(&before);
        for club in ["user", "rival"] {
            assert!(
                !newcomers(&before, &after, club).is_empty(),
                "{club} took nobody into its academy"
            );
        }
    }

    #[test]
    fn the_intake_fills_what_the_academy_lacks() {
        let at_target = academy("club", ACADEMY_TARGET_PER_GROUP);
        assert_eq!(plan_for(&at_target).groups.len(), MIN_INTAKE);

        let empty: Vec<Player> = Vec::new();
        assert_eq!(plan_for(&empty).groups.len(), MAX_INTAKE);

        // A defender and a forward short, between the bounds: exactly those two.
        let mut shape = ACADEMY_TARGET_PER_GROUP;
        shape[group_index(&Position::Defender)] -= 1;
        shape[group_index(&Position::Forward)] -= 1;
        let two_short = academy("club", shape);
        assert_eq!(
            plan_for(&two_short).groups,
            vec![Position::Defender, Position::Forward]
        );
    }

    #[test]
    fn a_club_with_a_full_academy_still_takes_the_minimum() {
        let overfull = academy("club", [2, 5, 5, 3]);
        assert_eq!(plan_for(&overfull).groups.len(), MIN_INTAKE);
    }

    #[test]
    fn the_intake_includes_a_keeper_when_the_academy_has_none() {
        // Further short of defenders and midfielders than of a keeper, and at
        // the maximum intake: the keeper is still the first recruit.
        let no_keeper = academy("club", [0, 0, 0, 1]);
        let plan = plan_for(&no_keeper);
        assert_eq!(plan.groups.len(), MAX_INTAKE);
        assert_eq!(plan.groups[0], Position::Goalkeeper);

        let with_keeper = academy("club", [1, 1, 1, 1]);
        assert!(
            !plan_for(&with_keeper)
                .groups
                .contains(&Position::Goalkeeper)
        );
    }

    #[test]
    fn recruits_are_academy_players_of_the_club() {
        let mut before = world([0, 0, 0, 0], [0, 0, 0, 0]);
        // A senior squad already wearing the shirts a generated player's
        // squad slot would hand him.
        for number in 1..=40u8 {
            let mut senior = youngster(&format!("senior-{number}"), "rival", Position::Midfielder);
            senior.squad_role = SquadRole::Senior;
            senior.jersey_number = Some(number);
            before.players.push(senior);
        }
        let after = intake(&before);
        let recruits = newcomers(&before, &after, "rival");
        assert_eq!(recruits.len(), MAX_INTAKE);
        let mut shirts: std::collections::HashSet<u8> = (1..=40).collect();
        for recruit in recruits {
            assert_eq!(recruit.squad_role, SquadRole::Youth);
            let age = crate::generator::opening_player_age(&recruit.date_of_birth, 2027)
                .expect("a recruit has a date of birth");
            assert!(
                (14..=crate::roster::YOUTH_ACADEMY_MAX_AGE).contains(&age),
                "a recruit of academy age, not {age}"
            );
            assert_eq!(recruit.contract_start(), Some("2027-05-30"));
            assert!(
                recruit.contract_end().is_some_and(|end| end > "2027-05-30"),
                "a recruit holds a contract that runs past the day he joins"
            );
            assert!(recruit.wage() > 0);
            // Joining is a contract made mid-career, recorded in his history as
            // a signing from no club, for this club.
            let entry = recruit
                .movement_history
                .last()
                .expect("joining is in his history");
            assert_eq!(
                entry.kind,
                domain::player::PlayerMovementKind::FreeAgentSigning
            );
            assert_eq!(entry.to_team_id.as_deref(), Some("rival"));
            assert_eq!(
                entry.contract.as_ref().map(|record| &record.source),
                Some(&ContractSource::FreeAgent)
            );
            assert!(!recruit.transfer_listed && !recruit.loan_listed);
            let shirt = recruit.jersey_number.expect("a recruit wears a shirt");
            assert!(
                shirts.insert(shirt),
                "shirt {shirt} is already worn at the club"
            );
        }
    }

    #[test]
    fn the_intake_is_one_plan_a_cost_can_attach_to() {
        let before = world([0, 2, 1, 2], [1, 3, 3, 2]);
        let after = intake(&before);
        // No money moves today: the finance epic prices the plan later.
        for (old, new) in before.teams.iter().zip(&after.teams) {
            assert_eq!(old.finance, new.finance, "{} paid for its intake", old.id);
        }
        // What was taken in is exactly the plan.
        for club in ["user", "rival"] {
            let academy: Vec<&Player> = before
                .players
                .iter()
                .filter(|player| player.team_id.as_deref() == Some(club))
                .collect();
            let mut planned = plan_for(academy).groups;
            let mut taken: Vec<Position> = newcomers(&before, &after, club)
                .into_iter()
                .map(|recruit| recruit.position.to_group_position())
                .collect();
            planned.sort_by_key(group_index);
            taken.sort_by_key(group_index);
            assert_eq!(
                taken, planned,
                "{club} took in something other than its plan"
            );
        }
    }

    #[test]
    fn a_replayed_season_end_takes_in_the_same_youngsters() {
        let before = world([0, 0, 0, 0], [0, 1, 1, 1]);
        let names = |game: &Game| {
            newcomers(&before, game, "rival")
                .into_iter()
                .map(|recruit| (recruit.full_name.clone(), recruit.date_of_birth.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&intake(&before)), names(&intake(&before)));
    }

    /// Given two careers in the same world — the same club ids, as two careers
    /// started from one package have — but different saves, when a season
    /// ends, then each draws its own intake: the draw comes from the save's
    /// seed, not from the club id alone.
    #[test]
    fn two_saves_of_one_world_draw_different_intakes() {
        let drawn = |seed: u64| {
            let mut before = world([0, 0, 0, 0], [0, 0, 0, 0]);
            before.seed = seed;
            let after = intake(&before);
            newcomers(&before, &after, "rival")
                .into_iter()
                .map(|recruit| (recruit.full_name.clone(), recruit.date_of_birth.clone()))
                .collect::<Vec<_>>()
        };
        assert_ne!(drawn(1), drawn(2));
    }

    /// Given an AI club with no seniors to spare and nothing on the market, when
    /// the season end's squad turnover runs, then the youngsters it takes in are
    /// still in its academy afterwards: a fifteen-year-old joins to be brought
    /// on, not to be promoted by the rebuild on the day he arrives.
    #[test]
    fn a_recruit_is_not_promoted_on_the_day_he_joins() {
        let before = world([0, 0, 0, 0], [0, 0, 0, 0]);
        let mut after = before.clone();
        crate::end_of_season::apply_season_end_squad_turnover(&mut after, season_end(), SEASON);

        let recruits = newcomers(&before, &after, "rival");
        assert_eq!(recruits.len(), MAX_INTAKE);
        for recruit in recruits {
            assert_eq!(
                recruit.squad_role,
                SquadRole::Youth,
                "{} was promoted the day he joined",
                recruit.full_name
            );
        }
    }

    /// Given a youth forward the rival owns, out on loan at the player's club,
    /// when the season ends, then he counts for the academy he belongs to and
    /// returns to: the rival, which has its forward, takes a keeper; the
    /// borrower, which has none of its own, takes a forward.
    #[test]
    fn a_loaned_youngster_counts_for_the_club_that_owns_him() {
        let mut shape = ACADEMY_TARGET_PER_GROUP;
        shape[group_index(&Position::Forward)] = 0;
        let mut before = world(shape, shape);
        let mut loanee = youngster("loanee", "user", Position::Forward);
        loanee.active_loan = Some(domain::player::ActiveLoan {
            parent_team_id: "rival".to_string(),
            loan_team_id: "user".to_string(),
            loan_team_name: None,
            start_date: "2026-08-01".to_string(),
            end_date: "2027-06-30".to_string(),
            wage_contribution_pct: 100,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
        });
        before.players.push(loanee);

        let after = intake(&before);
        let groups = |club: &str| {
            newcomers(&before, &after, club)
                .into_iter()
                .map(|recruit| recruit.position.to_group_position())
                .collect::<Vec<_>>()
        };
        assert_eq!(groups("user"), vec![Position::Forward]);
        assert_eq!(groups("rival"), vec![Position::Goalkeeper]);
    }

    // -- the board and the wages ------------------------------------------------

    /// Both clubs with empty academies, a weekly wage budget of `budget`, and
    /// one senior paid `senior_wage` — the rest of the wage bill.
    fn world_on_a_budget(budget: i64, senior_wage: u32) -> Game {
        let mut game = world([0, 0, 0, 0], [0, 0, 0, 0]);
        for team in &mut game.teams {
            team.wage_budget = budget;
        }
        for club in ["user", "rival"] {
            let mut senior = youngster(&format!("{club}-senior"), club, Position::Midfielder);
            senior.squad_role = SquadRole::Senior;
            senior.stage_wage(senior_wage);
            game.players.push(senior);
        }
        game
    }

    /// A youngster registered to `club`'s academy, as a recruit is when the
    /// board judges him.
    fn a_recruit_of(club: &str) -> Player {
        youngster("recruit", club, Position::Forward)
    }

    fn rival(game: &Game) -> &Team {
        game.teams.iter().find(|team| team.id == "rival").unwrap()
    }

    /// Given a club whose academy pays 600 a week and whose board has money to spare
    /// When it takes its intake
    /// Then no recruit joins above a quarter more than the academy pays: a youngster is not paid
    ///      what the generator sizes his wage at, which is far above a pay scale like this one.
    #[test]
    fn a_recruit_joins_at_the_pay_scale_of_the_academy_he_joins() {
        let mut before = world([0, 0, 0, 0], [2, 3, 3, 2]);
        for team in &mut before.teams {
            team.wage_budget = 100_000_000;
            team.finance = 100_000_000;
        }
        for player in before
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("rival"))
        {
            player.stage_wage(600);
        }

        let after = intake(&before);

        let hired = newcomers(&before, &after, "rival");
        assert!(!hired.is_empty());
        for recruit in hired {
            assert!(
                recruit.wage() <= 750,
                "{} joined on {} against an academy paying 600",
                recruit.id,
                recruit.wage()
            );
        }
    }

    /// Given an AI club exactly at its wage budget, and the same club as the player's
    /// When a recruit asking 3 000 is judged
    /// Then the AI club takes him at the youth minimum, not at his ask (its books have no room for
    ///      more), while the player's club is left to its board and pays the ask. Both take him: an
    ///      academy is not emptied because the first team spends what it has.
    #[test]
    fn an_ai_clubs_recruit_has_to_fit_its_books_unless_he_comes_at_the_minimum() {
        let mut game = world_on_a_budget(100_000, 100_000);
        let recruit = a_recruit_of("rival");
        assert_eq!(
            agreed_wage(&game, rival(&game), &recruit, 3_000),
            Some(YOUTH_MINIMUM_WAGE)
        );
        // The board's policy alone (the 25 000 grace for a club over budget) would have paid him
        // what he asked, which is the licence this closes.
        assert!(
            crate::contract_wage_policy::joining_wage_policy_verdict(
                &game,
                rival(&game),
                &recruit,
                3_000,
            )
            .permits()
        );
        game.manager.hire("rival".to_string());
        assert_eq!(
            agreed_wage(&game, rival(&game), &recruit, 3_000),
            Some(3_000)
        );
    }

    /// Given a club the board lets spend, a recruit joins on the wage he asks.
    #[test]
    fn a_recruit_the_board_can_afford_joins_on_the_wage_he_asks() {
        let game = world_on_a_budget(200_000, 1_000);
        let recruit = a_recruit_of("rival");
        assert_eq!(
            agreed_wage(&game, rival(&game), &recruit, 3_000),
            Some(3_000)
        );
    }

    /// Given a club whose board would refuse a recruit's asking wage but not the
    /// youth minimum, he joins on the minimum.
    #[test]
    fn a_recruit_the_board_refuses_at_his_wage_joins_on_the_youth_minimum() {
        // A budget of 10,000 lets the bill reach 11,000: 9,000 + 3,000 is over,
        // 9,000 + the minimum is not.
        let game = world_on_a_budget(10_000, 9_000);
        let recruit = a_recruit_of("rival");
        assert_eq!(
            agreed_wage(&game, rival(&game), &recruit, 3_000),
            Some(YOUTH_MINIMUM_WAGE)
        );
    }

    /// Given a club whose board would refuse even the youth minimum, the recruit
    /// is not taken.
    #[test]
    fn a_recruit_the_board_refuses_even_at_the_minimum_is_not_taken() {
        let game = world_on_a_budget(1_000, 1_000);
        let recruit = a_recruit_of("rival");
        assert_eq!(agreed_wage(&game, rival(&game), &recruit, 3_000), None);
    }

    /// Given a recruit registered to the club before the board judges him, his
    /// own wage still counts as new to the bill — the renewal verdict takes a
    /// registered player's wage off first, which would let him pay for himself.
    #[test]
    fn a_recruit_is_judged_as_a_newcomer_whose_wage_is_new_to_the_bill() {
        let game = world_on_a_budget(1_000, 1_000);
        let mut recruit = a_recruit_of("rival");
        recruit.stage_wage(3_000);
        assert_eq!(agreed_wage(&game, rival(&game), &recruit, 3_000), None);
    }

    /// Given a club with room on its bill for one youth minimum and no more,
    /// when the season ends, then it takes one youngster: each recruit is judged
    /// with the season's earlier ones already on the bill, and the rest of the
    /// plan is not taken.
    #[test]
    fn each_recruit_is_judged_with_the_earlier_ones_on_the_bill() {
        // A budget of 1,000 lets the bill reach 1,100: 500 + one recruit at no
        // more than 600 fits, a second recruit does not.
        let before = world_on_a_budget(1_000, 500);
        let after = intake(&before);
        assert_eq!(newcomers(&before, &after, "rival").len(), 1);
    }

    /// Given a club whose board refuses its first recruit even at the youth
    /// minimum, when a later recruit asks less than the minimum and would fit,
    /// then he is not taken either: the first refusal ends the club's intake.
    #[test]
    fn the_first_refusal_ends_the_clubs_intake() {
        // A budget of 1,000 lets the bill reach 1,100: 700 leaves room for 400.
        // The first recruit is refused at 3,000 and at the minimum; the second,
        // asking 300, would fit.
        let mut game = world_on_a_budget(1_000, 700);
        let end = season_end() + chrono::Months::new(36);
        let drawn = |id: &str, asking: u32| Drawn {
            recruit: youngster(id, "rival", Position::Forward),
            asking,
            end,
        };
        let recruits = vec![drawn("dear", 3_000), drawn("cheap", 300)];
        let rival_index = game
            .teams
            .iter()
            .position(|team| team.id == "rival")
            .unwrap();

        let intake = sign_until_refused(&mut game, rival_index, recruits, season_end());

        assert!(
            intake.joined.is_empty(),
            "joined after a refusal: {:?}",
            intake.joined
        );
        assert!(!game.players.iter().any(|player| player.id == "cheap"));
    }

    /// Given the player's club and an AI club on the same tight budget, they take
    /// the same number of youngsters: one wage rule for every club.
    #[test]
    fn the_board_judges_the_players_club_as_it_judges_an_ai_club() {
        let before = world_on_a_budget(1_000, 500);
        let after = intake(&before);
        assert_eq!(
            newcomers(&before, &after, "user").len(),
            newcomers(&before, &after, "rival").len()
        );
    }

    /// Given clubs whose boards refuse every recruit, when the season ends, then
    /// nobody joins, no money moves, and no one is created for anyone.
    #[test]
    fn a_refused_intake_creates_no_one_and_moves_no_money() {
        let before = world_on_a_budget(1_000, 1_000);
        let after = intake(&before);
        assert_eq!(after.players.len(), before.players.len());
        for (old, new) in before.teams.iter().zip(&after.teams) {
            assert_eq!(old.finance, new.finance, "{} was given money", old.id);
        }
    }

    fn intake_message(game: &Game) -> &InboxMessage {
        let told: Vec<&InboxMessage> = game
            .messages
            .iter()
            .filter(|message| message.subject_key.as_deref() == Some("be.msg.youthIntake.subject"))
            .collect();
        assert_eq!(told.len(), 1, "one intake message for the player's club");
        told[0]
    }

    /// Given the player's club whose board turns some of its intake away, the
    /// message names who joined and says the board would not take on the rest.
    #[test]
    fn the_player_is_told_when_the_board_turns_some_of_the_intake_away() {
        let before = world_on_a_budget(1_000, 500);
        let after = intake(&before);
        let message = intake_message(&after);
        assert_eq!(
            message.body_key.as_deref(),
            Some("be.msg.youthIntake.bodyPartlyRefused")
        );
        let joined = &newcomers(&before, &after, "user")[0].full_name;
        assert!(message.i18n_params["players"].contains(joined.as_str()));
    }

    /// Given the player's club whose board turns all of its intake away, the
    /// player is told nobody joined, and why.
    #[test]
    fn the_player_is_told_when_the_board_turns_the_whole_intake_away() {
        let after = intake(&world_on_a_budget(1_000, 1_000));
        assert_eq!(
            intake_message(&after).body_key.as_deref(),
            Some("be.msg.youthIntake.bodyAllRefused")
        );
    }

    // -- the player's club -----------------------------------------------------

    #[test]
    fn the_player_is_told_who_joined_the_academy() {
        let before = world([0, 0, 0, 0], [0, 0, 0, 0]);
        let after = intake(&before);
        let joined: Vec<String> = newcomers(&before, &after, "user")
            .into_iter()
            .map(|recruit| recruit.full_name.clone())
            .collect();
        let told: Vec<&InboxMessage> = after
            .messages
            .iter()
            .filter(|message| message.subject_key.as_deref() == Some("be.msg.youthIntake.subject"))
            .collect();
        assert_eq!(told.len(), 1, "one message, for the player's club only");
        assert_eq!(told[0].body_key.as_deref(), Some("be.msg.youthIntake.body"));
        let named = told[0]
            .i18n_params
            .get("players")
            .expect("the message names them");
        for name in &joined {
            assert!(
                named.contains(name.as_str()),
                "{name} joined but was not named"
            );
        }
    }

    #[test]
    fn the_players_club_takes_the_same_intake_as_an_ai_club() {
        let before = world([0, 2, 1, 2], [0, 2, 1, 2]);
        let after = intake(&before);
        let groups = |club: &str| {
            let mut groups: Vec<Position> = newcomers(&before, &after, club)
                .into_iter()
                .map(|recruit| recruit.position.to_group_position())
                .collect();
            groups.sort_by_key(group_index);
            groups
        };
        assert!(!groups("user").is_empty());
        assert_eq!(groups("user"), groups("rival"));
    }
}
