//! What a club's reputation is worth in money and in players (Run F5.1).
//!
//! Before this module a generated world was economically flat: a stadium's size was drawn at
//! random whatever the club's standing, every gate sold at the same price, no AI club had a
//! sponsor, and a squad's wages and quality were independent of how big the club was. The result
//! measured over 30 seasons (see `docs/APFM_RUN_F5_1_INTELLIGENT_WORLD.md`): the strongest clubs by
//! reputation were no stronger than the weakest, and the clubs with a small ground paid wages their
//! gate could not cover, so a quarter of the world sat in debt that only grew.
//!
//! The rule is one dial, [`income_scale`], and everything that earns or spends is read from it:
//!
//! - the size of the ground a generated club opens with and the price of its tickets;
//! - the sponsor an AI club signs (and re-signs every year);
//! - the wage bill a generated squad is paid, fitted to what the club can expect to earn;
//! - how good a generated squad is, a few rating points either side of the world's mean.
//!
//! Nothing here names a club. A big club is big because its reputation is high, and a reputation
//! moves with results (`reputation::update_team_reputation`), so the advantage is earned and can
//! be lost; it is also structural, because a rich club can pay what a poor one cannot.

use chrono::Datelike;
use domain::team::Sponsorship;

use crate::game::Game;

/// Reputation at and below which a club earns the least a club earns. Generated reputations run
/// from about 400 (the bottom of the pyramid) to about 780 (the top of the strongest nation).
const REPUTATION_FLOOR: f64 = 400.0;
const REPUTATION_CEILING: f64 = 780.0;

/// How much a club earns, relative to a club of middling reputation (about 1.0). The bottom club of
/// a pyramid earns about 0.4 of that, the top one about 1.6: a fourfold spread in revenue, which
/// is narrower than football's but wide enough that a bigger club can outbid a smaller one.
pub fn income_scale(reputation: u32) -> f64 {
    0.4 + 1.2 * stature(reputation).powf(1.3)
}

/// Where a reputation sits between the bottom (0) and the top (1) of a generated pyramid.
pub fn stature(reputation: u32) -> f64 {
    ((f64::from(reputation) - REPUTATION_FLOOR) / (REPUTATION_CEILING - REPUTATION_FLOOR))
        .clamp(0.0, 1.0)
}

/// Seats in the ground of a generated club of this reputation. `noise` (0 to 1) spreads clubs of
/// one standing over a range of ground sizes, ±15%, so two equals are not identical.
pub fn stadium_capacity(reputation: u32, noise: f64) -> u32 {
    let spread = 0.85 + 0.30 * noise.clamp(0.0, 1.0);
    let seats = 45_000.0 * income_scale(reputation) * spread;
    ((seats / 500.0).round() * 500.0).clamp(8_000.0, 90_000.0) as u32
}

/// What a ticket costs at a club of this reputation, relative to the world's mean price: 0.8 at
/// the bottom of the pyramid, 1.2 at the top.
pub fn ticket_price_factor(reputation: u32) -> f64 {
    0.8 + 0.4 * stature(reputation)
}

/// What an AI club's sponsor pays it a week. About a fifth of a middling club's income, in line
/// with its gate.
pub fn ai_sponsorship_weekly(reputation: u32) -> i64 {
    (65_000.0 * income_scale(reputation)).round() as i64
}

const AI_SPONSORSHIP_WEEKS: u32 = 52;

/// Weekly income a generated club can expect: the gate its ground earns over a league season's
/// home matches at the planning attendance and price, plus its sponsor.
pub fn expected_weekly_income(reputation: u32, capacity: u32) -> i64 {
    let gate = f64::from(capacity) * 0.76 * 20.0 * ticket_price_factor(reputation) * 19.0 / 52.0;
    gate.round() as i64 + ai_sponsorship_weekly(reputation)
}

/// How many rating points better or worse than the world's mean a generated club's squad is:
/// -3 at the bottom of the pyramid, +3 at the top.
pub fn quality_shift(reputation: u32) -> i32 {
    ((stature(reputation) - 0.5) * 6.0).round() as i32
}

/// Share of expected income a generated club plans to pay its players. Below the board's own
/// plan ([`crate::ai_budget_review::wage_share`]) so the first year leaves room for upkeep, the
/// staff and the transfers a club makes.
const OPENING_WAGE_SHARE_OF_PLAN: f64 = 0.9;

/// The weekly player wage bill a generated club of this reputation and ground should open with.
pub fn opening_wage_bill(reputation: u32, capacity: u32) -> i64 {
    let plan = crate::ai_budget_review::wage_share(reputation);
    (expected_weekly_income(reputation, capacity) as f64 * plan * OPENING_WAGE_SHARE_OF_PLAN)
        .round() as i64
}

/// On Mondays, every AI club without a sponsor signs one for a year, at the rate its reputation
/// commands. The player's club is left alone: its sponsors are the player's to negotiate.
pub(crate) fn renew_ai_sponsorships(game: &mut Game) {
    if game.clock.current_date.weekday().num_days_from_monday() != 0 {
        return;
    }
    let user_team = game.manager.team_id.clone();
    let date = game.clock.current_date;
    for team in game.teams.iter_mut() {
        if user_team.as_deref() == Some(team.id.as_str()) {
            continue;
        }
        let open = team.sponsorship.as_ref().is_none_or(|sponsorship| {
            sponsorship.remaining_weeks == 0 || sponsorship.base_value <= 0
        });
        if open {
            team.sponsorship = Some(Sponsorship {
                sponsor_name: crate::finances::sponsor_pitch_partner(&team.id, date),
                base_value: ai_sponsorship_weekly(team.reputation),
                remaining_weeks: AI_SPONSORSHIP_WEEKS,
                bonus_criteria: Vec::new(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given the bottom and the top of a pyramid, when income is scaled, then the top earns about
    /// four times the bottom and a middling club sits near 1.
    #[test]
    fn the_top_club_earns_about_four_times_the_bottom_club() {
        let (bottom, middle, top) = (income_scale(400), income_scale(600), income_scale(780));
        assert!((top / bottom - 4.0).abs() < 0.1, "{bottom} {top}");
        assert!((0.8..1.1).contains(&middle), "{middle}");
    }

    /// Given reputations beyond the generated range, when scaled, then income stays bounded.
    #[test]
    fn income_is_bounded_outside_the_generated_range() {
        assert_eq!(income_scale(0), income_scale(400));
        assert_eq!(income_scale(5000), income_scale(780));
    }

    /// Given two clubs of different standing and the same noise, when their grounds are drawn,
    /// then the bigger club has the bigger ground, and noise alone never reverses a wide gap.
    #[test]
    fn a_bigger_club_has_a_bigger_ground() {
        assert!(stadium_capacity(700, 0.0) > stadium_capacity(450, 1.0));
        assert!(stadium_capacity(780, 0.5) > stadium_capacity(400, 0.5) * 3);
    }

    /// Given a club's reputation, when its expected income and opening bill are read, then the
    /// bill is a share of the income below the board's own plan, for every standing.
    #[test]
    fn the_opening_wage_bill_is_below_what_the_club_earns() {
        for reputation in (400..=780).step_by(20) {
            let capacity = stadium_capacity(reputation, 0.5);
            let income = expected_weekly_income(reputation, capacity);
            let bill = opening_wage_bill(reputation, capacity);
            assert!(
                bill > 0 && bill < income * 8 / 10,
                "{reputation}: {bill} of {income}"
            );
        }
    }

    /// Given the bottom, the middle and the top, when quality is shifted, then it is symmetric
    /// around the world's mean and a few points wide.
    #[test]
    fn quality_shift_is_symmetric_and_modest() {
        assert_eq!(quality_shift(400), -3);
        assert_eq!(quality_shift(780), 3);
        assert_eq!(quality_shift(590), 0);
    }
}
