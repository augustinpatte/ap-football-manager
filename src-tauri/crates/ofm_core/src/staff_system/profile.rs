//! What a club's staff is worth, department by department, and what that changes.
//!
//! Every number the simulation reads from the staff comes through here, so the size of every
//! staff effect is in one place and can be audited against the validation report. All ratings
//! are 0-100; a multiplier is `1.0` for an average (50) department and moves by a bounded
//! amount either way. A department with nobody in it is rated [`VACANT_RATING`], worse than any
//! real hire.

use crate::game::Game;
use domain::staff::{Staff, StaffDetail, StaffPost};
use domain::team::TrainingFocus;

/// The rating a department gets when nobody works in it.
pub const VACANT_RATING: f64 = 20.0;

/// Largest swing, in either direction, of each staff effect around an average department.
pub const YOUTH_PROGRESSION_SWING: f64 = 0.08;
pub const GOALKEEPER_PROGRESSION_SWING: f64 = 0.12;
/// Share of match injuries a perfect medical department prevents.
pub const INJURY_PREVENTION_MAX: f64 = 0.12;
/// Injury length multiplier: a rating of 0 lengthens by this, a rating of 100 shortens by it.
pub const INJURY_DURATION_SPAN: (f64, f64) = (1.25, 0.80);

/// The staff that work for `team_id`.
pub fn team_staff<'a>(game: &'a Game, team_id: &str) -> Vec<&'a Staff> {
    game.staff
        .iter()
        .filter(|s| s.team_id.as_deref() == Some(team_id))
        .collect()
}

/// Rates one department from the people in it: with a head, the head counts for half and the
/// rest for the other half; without one, everyone counts equally. `None` for an empty one.
fn department(members: &[(&Staff, f64)]) -> Option<f64> {
    if members.is_empty() {
        return None;
    }
    let (heads, others): (Vec<_>, Vec<_>) = members.iter().partition(|(s, _)| s.post().is_head());
    let mean = |list: &[&(&Staff, f64)]| -> Option<f64> {
        (!list.is_empty()).then(|| list.iter().map(|(_, r)| *r).sum::<f64>() / list.len() as f64)
    };
    let heads = mean(&heads.iter().collect::<Vec<_>>());
    let others = mean(&others.iter().collect::<Vec<_>>());
    Some(match (heads, others) {
        (Some(h), Some(o)) => 0.5 * h + 0.5 * o,
        (Some(h), None) => h,
        (None, Some(o)) => o,
        (None, None) => return None,
    })
}

fn detail_or(staff: &Staff, pick: impl Fn(&StaffDetail) -> u8, fallback: u8) -> f64 {
    staff
        .detail
        .as_ref()
        .and_then(|d| StaffDetail::known(pick(d)))
        .unwrap_or(fallback) as f64
}

/// How good a coach is at teaching `focus`: the FM24 attribute for that kind of training when
/// the save gave it, else their general coaching.
pub fn focus_rating(staff: &Staff, focus: &TrainingFocus) -> f64 {
    let general = staff.attributes.coaching;
    match focus {
        TrainingFocus::Physical => detail_or(staff, |d| d.fitness, general),
        TrainingFocus::Technical => detail_or(staff, |d| d.technical, general),
        TrainingFocus::Tactical => detail_or(staff, |d| d.tactical, general),
        TrainingFocus::Defending => detail_or(staff, |d| d.defending, general),
        TrainingFocus::Attacking => detail_or(staff, |d| d.attacking, general),
        TrainingFocus::Recovery => {
            0.5 * detail_or(staff, |d| d.fitness, general)
                + 0.5 * detail_or(staff, |d| d.sports_science, general)
        }
    }
}

/// The ratings of a club's departments.
#[derive(Debug, Clone, PartialEq)]
pub struct StaffProfile {
    /// The assistant manager alone.
    pub assistant: f64,
    /// Everyone who coaches the first team (coaches and the assistant).
    pub coaching: f64,
    /// Fitness coaches.
    pub fitness: f64,
    /// Goalkeeping coaches.
    pub goalkeeping: f64,
    /// Youth coaches and the head of youth.
    pub youth: f64,
    /// Physios and the head of medical.
    pub medical: f64,
    /// Scouts and the head of scouting.
    pub scouting: f64,
    /// Analysts.
    pub analysis: f64,
}

impl StaffProfile {
    pub fn of(game: &Game, team_id: &str) -> Self {
        Self::from_staff(&team_staff(game, team_id))
    }

    pub fn from_staff(staff: &[&Staff]) -> Self {
        let pick = |posts: &[StaffPost]| -> Vec<(&Staff, f64)> {
            staff
                .iter()
                .filter(|s| posts.contains(&s.post()))
                .map(|s| (*s, f64::from(s.overall())))
                .collect()
        };
        let rating = |posts: &[StaffPost]| department(&pick(posts)).unwrap_or(VACANT_RATING);
        Self {
            assistant: rating(&[StaffPost::AssistantManager]),
            coaching: rating(&[StaffPost::AssistantManager, StaffPost::FirstTeamCoach]),
            fitness: rating(&[StaffPost::FitnessCoach]),
            goalkeeping: rating(&[StaffPost::GoalkeepingCoach]),
            youth: rating(&[StaffPost::YouthCoach, StaffPost::HeadOfYouth]),
            medical: rating(&[StaffPost::Physio, StaffPost::HeadOfMedical]),
            scouting: rating(&[StaffPost::Scout, StaffPost::HeadOfScouting]),
            analysis: rating(&[StaffPost::Analyst]),
        }
    }

    /// Progression multiplier for players of 21 and under.
    pub fn youth_progression_mult(&self) -> f64 {
        centered(self.youth, YOUTH_PROGRESSION_SWING)
    }

    /// Progression multiplier for goalkeepers.
    pub fn goalkeeper_progression_mult(&self) -> f64 {
        centered(self.goalkeeping, GOALKEEPER_PROGRESSION_SWING)
    }

    /// Chance that a match injury is caught early and does not happen.
    pub fn injury_prevention(&self) -> f64 {
        INJURY_PREVENTION_MAX * self.medical / 100.0
    }

    /// Multiplier on the days a new injury keeps a player out.
    pub fn injury_duration_mult(&self) -> f64 {
        let (worst, best) = INJURY_DURATION_SPAN;
        worst + (best - worst) * self.medical / 100.0
    }

    /// Extra recovery from the fitness coaches, on top of the physios' (up to +8 %).
    pub fn fitness_recovery_bonus(&self) -> f64 {
        1.0 + 0.08 * self.fitness / 100.0
    }
}

/// A scout's judging as the club's scouting department lets them use it: their own attributes
/// plus up to 8 points from the club's analysts (who hand the scouts the data) and up to 4 from
/// the head of scouting. Reports are as accurate as this, not as the bare attributes.
pub fn effective_judging(game: &Game, scout: &Staff) -> (u8, u8) {
    let Some(team_id) = scout.team_id.as_deref() else {
        return (scout.attributes.judging_ability, scout.attributes.judging_potential);
    };
    let staff = team_staff(game, team_id);
    let analysts: Vec<f64> = staff
        .iter()
        .filter(|s| s.post() == StaffPost::Analyst)
        .map(|s| f64::from(s.overall()))
        .collect();
    let analyst_bonus = if analysts.is_empty() {
        0.0
    } else {
        8.0 * analysts.iter().sum::<f64>() / analysts.len() as f64 / 100.0
    };
    let head_bonus = staff
        .iter()
        .filter(|s| s.post() == StaffPost::HeadOfScouting && s.id != scout.id)
        .map(|s| 4.0 * f64::from(s.overall()) / 100.0)
        .fold(0.0, f64::max);
    let lift = |base: u8| (f64::from(base) + analyst_bonus + head_bonus).round().min(100.0) as u8;
    (lift(scout.attributes.judging_ability), lift(scout.attributes.judging_potential))
}

/// `1.0` at a rating of 50, `1 ± swing` at 100 and 0.
pub fn centered(rating: f64, swing: f64) -> f64 {
    1.0 + (rating - 50.0) / 50.0 * swing
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::staff::{StaffAttributes, StaffRole};

    fn member(post: StaffPost, level: u8) -> Staff {
        let mut staff = Staff::new(
            format!("{post:?}-{level}"),
            "A".into(),
            "B".into(),
            "1980-01-01".into(),
            post.role(),
            StaffAttributes {
                coaching: level,
                judging_ability: level,
                judging_potential: level,
                physiotherapy: level,
            },
        );
        staff.specialization = post.specialization();
        staff.post = Some(post);
        staff
    }

    #[test]
    fn an_empty_department_is_rated_below_a_weak_one() {
        let weak = member(StaffPost::Physio, 30);
        let empty = StaffProfile::from_staff(&[]);
        let weak = StaffProfile::from_staff(&[&weak]);
        assert!(empty.medical < weak.medical);
        assert_eq!(empty.medical, VACANT_RATING);
    }

    #[test]
    fn a_head_counts_for_half_of_the_department() {
        let head = member(StaffPost::HeadOfMedical, 90);
        let physio = member(StaffPost::Physio, 30);
        let profile = StaffProfile::from_staff(&[&head, &physio]);
        assert!((profile.medical - 60.0).abs() < 1e-9);
    }

    #[test]
    fn effects_are_neutral_at_fifty_and_bounded_at_the_extremes() {
        let average = StaffProfile {
            assistant: 50.0, coaching: 50.0, fitness: 50.0, goalkeeping: 50.0,
            youth: 50.0, medical: 50.0, scouting: 50.0, analysis: 50.0,
        };
        assert!((average.youth_progression_mult() - 1.0).abs() < 1e-9);
        assert!((average.goalkeeper_progression_mult() - 1.0).abs() < 1e-9);
        let best = StaffProfile { youth: 100.0, medical: 100.0, goalkeeping: 100.0, ..average.clone() };
        assert!((best.youth_progression_mult() - 1.08).abs() < 1e-9);
        assert!((best.injury_duration_mult() - 0.80).abs() < 1e-9);
        assert!(best.injury_prevention() <= INJURY_PREVENTION_MAX + 1e-12);
        let worst = StaffProfile { medical: 0.0, ..average };
        assert!((worst.injury_duration_mult() - 1.25).abs() < 1e-9);
        assert_eq!(worst.injury_prevention(), 0.0);
    }

    #[test]
    fn a_coach_is_rated_on_the_attribute_of_the_session() {
        let mut coach = member(StaffPost::FirstTeamCoach, 50);
        coach.detail = Some(StaffDetail { technical: 90, fitness: 20, ..StaffDetail::default() });
        assert_eq!(focus_rating(&coach, &TrainingFocus::Technical), 90.0);
        assert_eq!(focus_rating(&coach, &TrainingFocus::Physical), 20.0);
        // No detail for tactics: the general coaching stands in.
        assert_eq!(focus_rating(&coach, &TrainingFocus::Tactical), 50.0);
        assert_eq!(coach.role, StaffRole::Coach);
    }
}
