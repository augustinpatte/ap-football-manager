//! The squad of next season and of the one after: who will have grown, declined or gone.
//!
//! These are the planning rules `squad_outlook` already applies to a year ahead, read once more for
//! a second year. They are heuristics a club could hold, not a forecast of the development model: a
//! club does not know a prospect's dice.

use crate::squad_outlook::{PlannedPlayer, expected_ovr_in_a_year, peak_age_limit};

/// Horizon years the model looks at: this season, the next, the one after.
pub(crate) const HORIZON: usize = 3;

/// A retirement this likely at a season's end counts as a departure for planning (the line
/// `squad_outlook::leaves_within_a_year` draws).
const LIKELY_RETIREMENT_PCT: u32 = 40;

/// Whether the club should plan without him `years` seasons from now: a likely retirement, or a
/// veteran past his group's renewal age whose contract runs out inside the horizon (the review
/// will not extend him for being good). For one year this is exactly
/// `squad_outlook::leaves_within_a_year`.
pub(crate) fn leaves_within(player: &PlannedPlayer, years: i32) -> bool {
    if crate::aging::retirement_base_chance(player.age + years) >= LIKELY_RETIREMENT_PCT {
        return true;
    }
    let renewal_window =
        crate::ai_contracts::RENEWAL_HORIZON_DAYS + 365 * i64::from((years - 1).max(0));
    player.age + years - 1 > peak_age_limit(player.group)
        && player
            .contract_days
            .is_some_and(|days| days <= renewal_window)
}

/// How his rating changes by each horizon year, `None` once he has gone. Year 0 is today.
pub(crate) fn projected_shifts(player: &PlannedPlayer) -> [Option<f32>; HORIZON] {
    let mut shifts = [None; HORIZON];
    shifts[0] = Some(0.0);
    let mut rating = player.ovr;
    for (year, shift) in shifts.iter_mut().enumerate().skip(1) {
        let years = year as i32;
        if leaves_within(player, years) {
            // Once gone, gone: a later year never brings him back.
            break;
        }
        rating = expected_ovr_in_a_year(
            rating,
            player.potential,
            player.age + years - 1,
            player.group,
        );
        *shift = Some(f32::from(rating) - f32::from(player.ovr));
    }
    shifts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned(age: i32, ovr: u8, potential: u8, group: usize, days: Option<i64>) -> PlannedPlayer {
        PlannedPlayer {
            ovr,
            potential,
            age,
            group,
            contract_days: days,
        }
    }

    /// Given a 19-year-old rated 60 with a ceiling of 80
    /// When his next two seasons are projected
    /// Then he is expected to grow in both, by more in the first.
    #[test]
    fn a_prospect_grows_in_both_years_and_most_in_the_first() {
        let shifts = projected_shifts(&planned(19, 60, 80, 2, Some(900)));
        let (first, second) = (shifts[1].unwrap(), shifts[2].unwrap());
        assert!(first >= 7.0, "{first}");
        assert!(second > first, "{second} should keep climbing past {first}");
        assert!(
            second - first < first,
            "the second year adds less than the first"
        );
    }

    /// Given a 33-year-old outfielder rated 80 on a long contract
    /// When his next two seasons are projected
    /// Then he declines in the first year and has retired in the second (a 35-year-old is a coin
    /// toss to retire, which planning treats as gone).
    #[test]
    fn a_veteran_declines_and_then_leaves() {
        let shifts = projected_shifts(&planned(33, 80, 80, 2, Some(1_000)));
        assert!(shifts[1].unwrap() < 0.0);
        assert!(shifts[2].is_none());
    }

    /// Given the same veteran at one year, and an outfielder of 33 on an expiring contract
    /// When one year is asked
    /// Then the answer is the one `squad_outlook` gives (the two must never disagree).
    #[test]
    fn one_year_is_what_the_squad_outlook_says() {
        for age in 25..=38 {
            for days in [Some(60), Some(400), None] {
                for group in 0..4 {
                    let p = planned(age, 70, 70, group, days);
                    assert_eq!(
                        leaves_within(&p, 1),
                        crate::squad_outlook::leaves_within_a_year(&p),
                        "age {age} days {days:?} group {group}"
                    );
                }
            }
        }
    }

    /// Given a player who is gone in year one
    /// When the second year is projected
    /// Then he is still gone.
    #[test]
    fn a_player_who_has_gone_does_not_come_back() {
        let shifts = projected_shifts(&planned(36, 70, 70, 2, Some(900)));
        assert!(shifts[1].is_none() && shifts[2].is_none());
    }
}
