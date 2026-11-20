//! What the club's youth staff can tell the manager about an academy player's ceiling.
//!
//! A player's `potential` is the development model's ceiling: the simulation reads it and the save
//! keeps it, and it stays exact. The *manager* is not told it. His youth department assesses each
//! youngster and hands over a band that always contains the ceiling, and how wide the band is
//! depends on how good that department is (`StaffProfile::youth`, the rating that already speeds
//! the academy's progression): a club with no youth staff learns little, a club with a strong head
//! of youth learns a good deal, and nobody learns the number.
//!
//! The assessment is a pure function of the ceiling, the player's current rating and the
//! department's rating. There is no random draw and nothing is stored, so it reads the same on
//! every render, after every save and reload, and from every screen. The bands sit on a fixed grid
//! (6, 12 or 24 points wide, each a multiple of the one before) rather than being centred on the
//! truth with an error: a centre with an error moves with the band's width, so a manager who
//! changed his staff and read the band again could solve for the ceiling. A grid band gives the
//! same facts at every width, and a better department only ever shows a smaller band inside the
//! larger one.
//!
//! Current ability is not assessed here: an academy player is the club's own, and the manager
//! reads his own players exactly (`rating_view`).

use crate::game::Game;
use crate::rating_view::RatingView;
use crate::scouting_dossier::{Confidence, Range};
use crate::staff_system::profile::{StaffProfile, team_staff};
use domain::player::Player;
use domain::staff::StaffPost;
use domain::team::TrainingFocus;
use serde::Serialize;

/// A department rated this well or better reads a ceiling to within a band of [`NARROW_BAND`].
const STRONG_DEPARTMENT_MIN_RATING: f64 = 70.0;
/// A department rated this well or better, but not strong, reads it to within [`MEDIUM_BAND`]. An
/// average (50) department sits exactly here; an empty one is rated worse (`VACANT_RATING`).
const AVERAGE_DEPARTMENT_MIN_RATING: f64 = 50.0;

const NARROW_BAND: u8 = 6;
const MEDIUM_BAND: u8 = 12;
const WIDE_BAND: u8 = 24;

/// How wide a band the department can give, and the trust that earns it.
fn band_of_department(youth_rating: f64) -> (u8, Confidence) {
    if youth_rating >= STRONG_DEPARTMENT_MIN_RATING {
        (NARROW_BAND, Confidence::High)
    } else if youth_rating >= AVERAGE_DEPARTMENT_MIN_RATING {
        (MEDIUM_BAND, Confidence::Moderate)
    } else {
        (WIDE_BAND, Confidence::Low)
    }
}

/// The band of the grid that holds `true_ceiling`, never below the player's own rating (a ceiling
/// cannot be under what he already is).
fn ceiling_band(true_ceiling: u8, ovr: u8, width: u8) -> Range {
    let ceiling = true_ceiling.max(ovr).clamp(1, 99);
    let start = (ceiling - 1) / width * width + 1;
    let end = (start + width - 1).min(99);
    Range {
        low: start.max(ovr),
        high: end,
    }
}

/// What the club's youth department is, in the terms the screen explains it: how well it reads a
/// ceiling, and how much it speeds the academy up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YouthCoaching {
    /// The department's rating, 0 to 100 (an empty department is rated below any hire).
    pub rating: u8,
    pub has_staff: bool,
    /// How far the manager can trust an assessment.
    pub confidence: Confidence,
    /// The width, in rating points, of the band the department reads a ceiling to.
    pub band_width: u8,
    /// The extra progression, in percent, that the academy's players (21 and under) get from the
    /// youth coaches; the training model's own figure, 0 with nobody in the seat.
    pub progression_bonus_pct: u8,
}

/// The assessment of the club's academy players, found once for a list: the department's rating
/// is read from the staff a single time.
pub(crate) struct AcademyLens<'a> {
    game: &'a Game,
    team_id: String,
    youth_rating: f64,
    width: u8,
    confidence: Confidence,
}

impl<'a> AcademyLens<'a> {
    /// `None` when the manager has no club.
    pub(crate) fn of(game: &'a Game) -> Option<Self> {
        let team_id = game.manager.team_id.clone()?;
        let youth_rating = StaffProfile::of(game, &team_id).youth;
        let (width, confidence) = band_of_department(youth_rating);
        Some(Self {
            game,
            team_id,
            youth_rating,
            width,
            confidence,
        })
    }

    /// The band his department reads for the player's ceiling; `None` for a player who is not the
    /// club's own (a stranger's ceiling is scouting's to estimate, not the academy's).
    pub(crate) fn ceiling_range_of(&self, player: &Player) -> Option<Range> {
        let own = player.team_id.as_deref() == Some(self.team_id.as_str())
            || player.contract_club_id() == Some(self.team_id.as_str());
        own.then(|| ceiling_band(player.potential, player.ovr, self.width))
    }

    /// The ceiling as the screens read it, in the shape the rating projection uses for an estimate.
    pub(crate) fn ceiling_of(&self, player: &Player) -> Option<RatingView> {
        let range = self.ceiling_range_of(player)?;
        Some(RatingView::Estimate {
            low: range.low,
            high: range.high,
            confidence: self.confidence,
            // Youth staff watch the academy every day: an assessment is never an old report.
            current: true,
            scouted_on: self.game.clock.current_date.date_naive().to_string(),
        })
    }

    /// The player as the manager's screens may reason about him: his true rating, and for ceiling
    /// the middle of the band. Everything the academy review decides, it decides on this copy, so
    /// no verdict, reason or ordering can depend on a figure the manager was not given.
    pub(crate) fn believed(&self, player: &Player) -> Player {
        let mut believed = player.clone();
        // Fails closed: a player who is not the club's has no assessment, so the copy carries no
        // ceiling beyond his rating rather than the figure the model holds.
        believed.potential = self
            .ceiling_range_of(player)
            .map_or(player.ovr, |range| range.mid());
        believed
    }

    pub(crate) fn coaching(&self) -> YouthCoaching {
        let staff = team_staff(self.game, &self.team_id);
        let youth_mult =
            crate::training::compute_coaching_bonus(&staff, &TrainingFocus::Technical).youth_mult;
        YouthCoaching {
            rating: self.youth_rating.round().clamp(0.0, 100.0) as u8,
            has_staff: staff
                .iter()
                .any(|s| matches!(s.post(), StaffPost::YouthCoach | StaffPost::HeadOfYouth)),
            confidence: self.confidence,
            band_width: self.width,
            progression_bonus_pct: ((youth_mult - 1.0) * 100.0).round().clamp(0.0, 100.0) as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given every ceiling and current rating a player can have, at every width the staff give
    /// When the band is drawn
    /// Then it holds the ceiling, stays in 1..=99, and never starts below his rating.
    #[test]
    fn the_band_always_holds_the_true_ceiling() {
        for width in [NARROW_BAND, MEDIUM_BAND, WIDE_BAND] {
            for ovr in 1..=99u8 {
                for potential in 1..=99u8 {
                    let band = ceiling_band(potential, ovr, width);
                    let ceiling = potential.max(ovr);
                    assert!(
                        band.contains(ceiling),
                        "ceiling {ceiling} outside {band:?} at width {width}"
                    );
                    assert!(band.low >= ovr && band.low >= 1 && band.high <= 99);
                    assert!(band.high - band.low < width);
                }
            }
        }
    }

    /// Given three departments of different quality
    /// When the same player is assessed
    /// Then the better the department, the narrower and the more trusted the band.
    #[test]
    fn a_better_youth_department_reads_a_narrower_band() {
        let (none, none_trust) = band_of_department(20.0);
        let (average, average_trust) = band_of_department(50.0);
        let (strong, strong_trust) = band_of_department(85.0);
        assert!(none > average && average > strong);
        // The cut-offs themselves: 70 is a strong department, a hair under it is not; 50 is
        // average, a hair under it is not.
        assert_eq!(band_of_department(70.0).0, NARROW_BAND);
        assert_eq!(band_of_department(69.9).0, MEDIUM_BAND);
        assert_eq!(band_of_department(50.0).0, MEDIUM_BAND);
        assert_eq!(band_of_department(49.9).0, WIDE_BAND);
        assert_eq!(
            (none_trust, average_trust, strong_trust),
            (Confidence::Low, Confidence::Moderate, Confidence::High)
        );
    }

    /// Given two players whose true ceilings differ but fall in one band of the grid
    /// When they are assessed
    /// Then the two readings are identical: the band says nothing about where inside it he is.
    #[test]
    fn two_ceilings_in_one_band_read_the_same() {
        assert_eq!(
            ceiling_band(62, 50, MEDIUM_BAND),
            ceiling_band(71, 50, MEDIUM_BAND)
        );
        assert_ne!(
            ceiling_band(71, 50, MEDIUM_BAND),
            ceiling_band(73, 50, MEDIUM_BAND)
        );
    }

    /// Given a department that improves
    /// When the same player is read at each width
    /// Then each narrower band sits inside the wider one, so changing staff and reading again
    /// teaches nothing a strong department would not have shown directly.
    #[test]
    fn a_narrower_band_is_nested_in_the_wider_one() {
        for ceiling in 1..=99u8 {
            let wide = ceiling_band(ceiling, 1, WIDE_BAND);
            let medium = ceiling_band(ceiling, 1, MEDIUM_BAND);
            let narrow = ceiling_band(ceiling, 1, NARROW_BAND);
            assert!(wide.low <= medium.low && medium.high <= wide.high);
            assert!(medium.low <= narrow.low && narrow.high <= medium.high);
        }
    }

    /// Given a youngster already at or above the bottom of his ceiling's band
    /// When he is assessed
    /// Then the band starts at his own rating, and one at his ceiling is not shown as exact.
    #[test]
    fn the_band_never_starts_below_the_players_own_rating() {
        let band = ceiling_band(64, 63, NARROW_BAND);
        assert_eq!(band, Range { low: 63, high: 66 });
        assert!(
            band.low < band.high,
            "at his ceiling he is still a band, not a number"
        );
    }
}
