//! A club's staff as an organisation: which posts are filled, what the staff may cost, and the
//! hire, renew and release rules that the manager, the delegated recruiter and the AI clubs all
//! go through. One rulebook, so a delegated hire is exactly the hire the manager could make.

use chrono::{Months, NaiveDate};
use domain::finance::CashKind;
use domain::staff::{Staff, StaffPost};

use crate::finances::{calc_wages, journal};
use crate::game::Game;

/// Share of the club's weekly wage budget that the staff may cost.
pub const STAFF_BUDGET_PCT: i64 = 15;
/// A club always has room for this much staff before the share bites (weekly).
pub const STAFF_BUDGET_FLOOR: i64 = 6_000;
/// Longest staff contract, in years.
pub const MAX_YEARS: u8 = 5;
/// An offer within this share of the ask is answered with a counter-offer, not a refusal.
pub const COUNTER_BAND_PCT: i64 = 10;
/// Share of the wages left on a contract that a released member is paid.
pub const SEVERANCE_PCT: i64 = 50;
/// The least a release costs, in weeks of wage.
pub const SEVERANCE_MIN_WEEKS: i64 = 4;
/// Share of the weekly wage paid when a member signs.
pub const SIGNING_FEE_WEEKS: i64 = 1;

const ERR_NOT_FOUND: &str = "be.error.staffMemberNotFound";
const ERR_EMPLOYED: &str = "be.error.staffMemberAlreadyEmployed";
const ERR_TEAM: &str = "be.error.teamNotFound";
const ERR_NOT_YOURS: &str = "be.error.staff.notAtClub";
const ERR_POST_FULL: &str = "be.error.staff.postFull";
const ERR_BUDGET: &str = "be.error.staff.overBudget";
const ERR_CASH: &str = "be.error.staff.notEnoughCash";
const ERR_REFUSED: &str = "be.error.staff.offerRefused";
const ERR_YEARS: &str = "be.error.staff.badContractLength";

/// One seat of a club's staff.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PostStatus {
    pub post: StaffPost,
    pub count: usize,
    pub limit: usize,
    pub required: usize,
}

impl PostStatus {
    pub fn is_vacant(&self) -> bool {
        self.count < self.required
    }
    pub fn has_room(&self) -> bool {
        self.count < self.limit
    }
}

/// How full each post of `team_id` is.
pub fn structure(game: &Game, team_id: &str) -> Vec<PostStatus> {
    structure_of(&super::profile::team_staff(game, team_id))
}

/// How full each post is, given a club's staff.
pub fn structure_of(members: &[&Staff]) -> Vec<PostStatus> {
    StaffPost::ALL
        .iter()
        .map(|post| PostStatus {
            post: *post,
            count: members.iter().filter(|s| s.post() == *post).count(),
            limit: post.limit(),
            required: post.required(),
        })
        .collect()
}

/// A member whose contract has run out leaves the club for the market; nothing is owed.
pub fn release_expired(game: &mut Game, staff_id: &str) -> Result<(), String> {
    let member = game
        .staff
        .iter_mut()
        .find(|s| s.id == staff_id)
        .ok_or(ERR_NOT_FOUND)?;
    member.team_id = None;
    member.contract_end = None;
    Ok(())
}

/// What the club's staff costs per week.
pub fn weekly_staff_bill(game: &Game, team_id: &str) -> i64 {
    game.staff
        .iter()
        .filter(|s| s.team_id.as_deref() == Some(team_id))
        .map(|s| i64::from(s.wage))
        .sum()
}

/// The most the club's staff may cost per week. A club already above the share is not forced
/// to shed anyone; it just cannot add.
pub fn staff_budget(game: &Game, team_id: &str) -> i64 {
    let wage_budget = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map_or(0, |t| t.wage_budget);
    (wage_budget * STAFF_BUDGET_PCT / 100).max(STAFF_BUDGET_FLOOR)
}

/// Room left in the staff budget (never negative).
pub fn staff_headroom(game: &Game, team_id: &str) -> i64 {
    (staff_budget(game, team_id) - weekly_staff_bill(game, team_id)).max(0)
}

/// What a member asks the club for, per week, given the club's standing: the best ask a
/// prestigious club, the worst a small one.
pub fn asking_wage(staff: &Staff, club_reputation: u32) -> u32 {
    let base = i64::from(staff.wage.max(1));
    // A strong member holds out for a club that can win: up to +15 % below reputation 400,
    // nothing above 700. Reputation is on OFM's 0-1000 scale.
    let quality = i64::from(staff.overall());
    let shortfall = (700 - i64::from(club_reputation.min(700))).max(0);
    let premium_pct = (quality - 50).max(0) * shortfall / 700 * 30 / 100;
    u32::try_from(base * (100 + premium_pct) / 100).unwrap_or(u32::MAX)
}

/// The answer to an offer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum OfferResponse {
    Accepted,
    /// Not at that wage; at this one, yes.
    Counter { wage: u32 },
    Refused,
}

/// How a member answers `wage` per week for `years` (pure: nothing changes).
pub fn answer_offer(staff: &Staff, club_reputation: u32, wage: u32, years: u8) -> OfferResponse {
    let mut ask = i64::from(asking_wage(staff, club_reputation));
    // An older member wants more security than a long deal gives them, a young one is
    // happy to commit: a small ask adjustment for the length asked.
    if years >= 4 && staff_age(staff) >= 58 {
        ask = ask * 105 / 100;
    }
    let offer = i64::from(wage);
    if offer >= ask {
        OfferResponse::Accepted
    } else if offer * 100 >= ask * (100 - COUNTER_BAND_PCT) {
        OfferResponse::Counter {
            wage: u32::try_from(ask).unwrap_or(u32::MAX),
        }
    } else {
        OfferResponse::Refused
    }
}

fn staff_age(staff: &Staff) -> i32 {
    staff
        .date_of_birth
        .get(0..4)
        .and_then(|y| y.parse::<i32>().ok())
        .map_or(40, |born| 2026 - born)
}

fn club_reputation(game: &Game, team_id: &str) -> Option<u32> {
    game.teams.iter().find(|t| t.id == team_id).map(|t| t.reputation)
}

fn today(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

fn contract_end_for(date: NaiveDate, years: u8) -> String {
    date.checked_add_months(Months::new(12 * u32::from(years)))
        .unwrap_or(date)
        .format("%Y-%m-%d")
        .to_string()
}

/// Why `staff_id` cannot be signed by `team_id` at `wage`, if it cannot. Pure.
pub fn hire_blocker(game: &Game, team_id: &str, staff_id: &str, wage: u32) -> Option<&'static str> {
    let Some(staff) = game.staff.iter().find(|s| s.id == staff_id) else {
        return Some(ERR_NOT_FOUND);
    };
    if staff.team_id.is_some() {
        return Some(ERR_EMPLOYED);
    }
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return Some(ERR_TEAM);
    };
    let count = game
        .staff
        .iter()
        .filter(|s| s.team_id.as_deref() == Some(team_id) && s.post() == staff.post())
        .count();
    if count >= staff.post().limit() {
        return Some(ERR_POST_FULL);
    }
    if i64::from(wage) > staff_headroom(game, team_id) {
        return Some(ERR_BUDGET);
    }
    if team.finance < i64::from(wage) * SIGNING_FEE_WEEKS {
        return Some(ERR_CASH);
    }
    None
}

/// Signs a free-agent member at `wage` for `years`, paying the signing fee. Checks everything
/// before changing anything.
pub fn hire(
    game: &mut Game,
    team_id: &str,
    staff_id: &str,
    wage: u32,
    years: u8,
) -> Result<(), String> {
    if years == 0 || years > MAX_YEARS {
        return Err(ERR_YEARS.into());
    }
    if let Some(blocker) = hire_blocker(game, team_id, staff_id, wage) {
        return Err(blocker.into());
    }
    let date = today(game);
    journal::post(
        game,
        team_id,
        -(i64::from(wage) * SIGNING_FEE_WEEKS),
        CashKind::StaffWages,
        date,
    )?;
    let member = game
        .staff
        .iter_mut()
        .find(|s| s.id == staff_id)
        .ok_or(ERR_NOT_FOUND)?;
    member.team_id = Some(team_id.to_string());
    member.wage = wage;
    member.contract_end = Some(contract_end_for(date, years));
    game.available_staff_market_last_activity_date = Some(date.format("%Y-%m-%d").to_string());
    Ok(())
}

/// Offers `wage` for `years` to a free agent for `team_id`: signs if accepted.
pub fn negotiate_hire(
    game: &mut Game,
    team_id: &str,
    staff_id: &str,
    wage: u32,
    years: u8,
) -> Result<OfferResponse, String> {
    let reputation = club_reputation(game, team_id).ok_or(ERR_TEAM)?;
    let staff = game
        .staff
        .iter()
        .find(|s| s.id == staff_id)
        .ok_or(ERR_NOT_FOUND)?;
    if staff.team_id.is_some() {
        return Err(ERR_EMPLOYED.into());
    }
    let response = answer_offer(staff, reputation, wage, years);
    if response == OfferResponse::Accepted {
        hire(game, team_id, staff_id, wage, years)?;
    }
    Ok(response)
}

/// Days left on a member's contract; `None` for an open-ended one.
pub fn days_left(game: &Game, staff: &Staff) -> Option<i64> {
    let end = NaiveDate::parse_from_str(staff.contract_end.as_deref()?, "%Y-%m-%d").ok()?;
    Some((end - today(game)).num_days())
}

/// What releasing `staff` costs today.
pub fn severance(game: &Game, staff: &Staff) -> i64 {
    let wage = i64::from(staff.wage);
    let weeks_left = days_left(game, staff).map_or(0, |d| (d.max(0) + 6) / 7);
    (wage * weeks_left.min(104) * SEVERANCE_PCT / 100).max(wage * SEVERANCE_MIN_WEEKS)
}

/// Releases a member of `team_id`, paying the severance. They go back to the market.
pub fn release(game: &mut Game, team_id: &str, staff_id: &str) -> Result<i64, String> {
    let staff = game
        .staff
        .iter()
        .find(|s| s.id == staff_id)
        .ok_or(ERR_NOT_FOUND)?;
    if staff.team_id.as_deref() != Some(team_id) {
        return Err(ERR_NOT_YOURS.into());
    }
    if !game.teams.iter().any(|t| t.id == team_id) {
        return Err(ERR_TEAM.into());
    }
    let cost = severance(game, staff);
    let date = today(game);
    journal::post(game, team_id, -cost, CashKind::ContractTermination, date)?;
    let member = game
        .staff
        .iter_mut()
        .find(|s| s.id == staff_id)
        .ok_or(ERR_NOT_FOUND)?;
    member.team_id = None;
    member.contract_end = None;
    Ok(cost)
}

/// The wage a member asks to stay.
pub fn renewal_ask(staff: &Staff, club_reputation: u32) -> u32 {
    let current = i64::from(staff.wage);
    let market = i64::from(asking_wage(staff, club_reputation));
    u32::try_from(market.max(current * 105 / 100)).unwrap_or(u32::MAX)
}

/// Offers a renewal to a member of `team_id`: extends the contract if accepted.
pub fn negotiate_renewal(
    game: &mut Game,
    team_id: &str,
    staff_id: &str,
    wage: u32,
    years: u8,
) -> Result<OfferResponse, String> {
    if years == 0 || years > MAX_YEARS {
        return Err(ERR_YEARS.into());
    }
    let reputation = club_reputation(game, team_id).ok_or(ERR_TEAM)?;
    let staff = game
        .staff
        .iter()
        .find(|s| s.id == staff_id)
        .ok_or(ERR_NOT_FOUND)?;
    if staff.team_id.as_deref() != Some(team_id) {
        return Err(ERR_NOT_YOURS.into());
    }
    let ask = renewal_ask(staff, reputation);
    let offer = i64::from(wage);
    let response = if wage >= ask {
        OfferResponse::Accepted
    } else if offer * 100 >= i64::from(ask) * (100 - COUNTER_BAND_PCT) {
        OfferResponse::Counter { wage: ask }
    } else {
        OfferResponse::Refused
    };
    if response == OfferResponse::Accepted {
        let raise = i64::from(wage) - i64::from(staff.wage);
        if raise > staff_headroom(game, team_id) {
            return Err(ERR_BUDGET.into());
        }
        let date = today(game);
        let member = game
            .staff
            .iter_mut()
            .find(|s| s.id == staff_id)
            .ok_or(ERR_NOT_FOUND)?;
        member.wage = wage;
        member.contract_end = Some(contract_end_for(date, years));
    } else if response == OfferResponse::Refused {
        // keep the error code reachable for callers that want a hard failure
        let _ = ERR_REFUSED;
    }
    Ok(response)
}

/// The club's weekly commitments for the dashboard: players + staff.
pub fn total_weekly_wages(game: &Game, team_id: &str) -> i64 {
    calc_wages(game, team_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::staff::{StaffAttributes, StaffRole};
    use domain::team::Team;

    fn staff(id: &str, post: StaffPost, level: u8, wage: u32, team: Option<&str>) -> Staff {
        let mut s = Staff::new(
            id.into(),
            "A".into(),
            id.into(),
            "1980-01-01".into(),
            post.role(),
            StaffAttributes {
                coaching: level,
                judging_ability: level,
                judging_potential: level,
                physiotherapy: level,
            },
        );
        s.specialization = post.specialization();
        s.post = Some(post);
        s.wage = wage;
        s.team_id = team.map(str::to_string);
        s
    }

    fn game(members: Vec<Staff>) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut manager = Manager::new("m".into(), "T".into(), "M".into(), "1980-01-01".into(), "England".into());
        manager.hire("t1".into());
        let mut team = Team::new("t1".into(), "T".into(), "T".into(), "England".into(), "L".into(), "S".into(), 20_000);
        team.finance = 1_000_000;
        team.wage_budget = 100_000;
        team.reputation = 700;
        Game::new(clock, manager, vec![team], vec![], members, vec![])
    }

    #[test]
    fn the_structure_counts_each_post_and_flags_vacancies() {
        let g = game(vec![staff("a", StaffPost::AssistantManager, 60, 1000, Some("t1"))]);
        let s = structure(&g, "t1");
        let assistant = s.iter().find(|p| p.post == StaffPost::AssistantManager).unwrap();
        let physio = s.iter().find(|p| p.post == StaffPost::Physio).unwrap();
        assert!(!assistant.is_vacant());
        assert!(physio.is_vacant());
    }

    #[test]
    fn an_offer_is_accepted_countered_or_refused_by_how_close_it_is_to_the_ask() {
        let member = staff("p", StaffPost::Physio, 50, 1000, None);
        assert_eq!(answer_offer(&member, 700, 1000, 2), OfferResponse::Accepted);
        assert_eq!(answer_offer(&member, 700, 950, 2), OfferResponse::Counter { wage: 1000 });
        assert_eq!(answer_offer(&member, 700, 500, 2), OfferResponse::Refused);
    }

    #[test]
    fn a_strong_member_asks_more_of_a_small_club() {
        let star = staff("s", StaffPost::Physio, 90, 1000, None);
        assert!(asking_wage(&star, 300) > asking_wage(&star, 700));
        let weak = staff("w", StaffPost::Physio, 30, 1000, None);
        assert_eq!(asking_wage(&weak, 100), 1000);
    }

    #[test]
    fn hiring_signs_the_member_pays_the_fee_and_sets_the_contract() {
        let mut g = game(vec![staff("p", StaffPost::Physio, 70, 2000, None)]);
        hire(&mut g, "t1", "p", 2000, 3).unwrap();
        let p = &g.staff[0];
        assert_eq!(p.team_id.as_deref(), Some("t1"));
        assert_eq!(p.contract_end.as_deref(), Some("2029-08-01"));
        assert_eq!(g.teams[0].finance, 998_000);
        assert_eq!(g.teams[0].season_expenses, 2000);
    }

    #[test]
    fn a_hire_that_breaks_the_budget_the_limit_or_the_cash_is_refused_untouched() {
        // Budget: 15 % of 100 000 = 15 000 a week.
        let mut g = game(vec![staff("p", StaffPost::Physio, 70, 20_000, None)]);
        assert_eq!(hire(&mut g, "t1", "p", 20_000, 2), Err(ERR_BUDGET.into()));
        assert!(g.staff[0].team_id.is_none());

        let mut full: Vec<Staff> = (0..3)
            .map(|i| staff(&format!("x{i}"), StaffPost::Physio, 50, 100, Some("t1")))
            .collect();
        full.push(staff("p", StaffPost::Physio, 70, 100, None));
        let mut g = game(full);
        assert_eq!(hire(&mut g, "t1", "p", 100, 2), Err(ERR_POST_FULL.into()));

        let mut g = game(vec![staff("p", StaffPost::Physio, 70, 1000, None)]);
        g.teams[0].finance = 10;
        assert_eq!(hire(&mut g, "t1", "p", 1000, 2), Err(ERR_CASH.into()));
        assert_eq!(g.teams[0].finance, 10);
    }

    #[test]
    fn releasing_pays_half_of_what_is_left_on_the_contract() {
        let mut member = staff("p", StaffPost::Physio, 70, 1000, Some("t1"));
        member.contract_end = Some("2027-08-01".into());
        let mut g = game(vec![member]);
        let cost = release(&mut g, "t1", "p").unwrap();
        assert_eq!(cost, 1000 * 53 / 2);
        assert!(g.staff[0].team_id.is_none());
        assert_eq!(g.teams[0].finance, 1_000_000 - cost);
    }

    #[test]
    fn an_open_ended_contract_costs_the_minimum_to_end() {
        let g = game(vec![staff("p", StaffPost::Physio, 70, 1000, Some("t1"))]);
        assert_eq!(severance(&g, &g.staff[0]), 4000);
    }

    #[test]
    fn a_renewal_extends_the_contract_at_the_new_wage() {
        let mut member = staff("p", StaffPost::Physio, 50, 1000, Some("t1"));
        member.contract_end = Some("2026-09-01".into());
        let mut g = game(vec![member]);
        assert_eq!(renewal_ask(&g.staff[0], 700), 1050);
        let r = negotiate_renewal(&mut g, "t1", "p", 1050, 2).unwrap();
        assert_eq!(r, OfferResponse::Accepted);
        assert_eq!(g.staff[0].wage, 1050);
        assert_eq!(g.staff[0].contract_end.as_deref(), Some("2028-08-01"));
        assert_eq!(StaffRole::Physio, g.staff[0].role);
    }
}
