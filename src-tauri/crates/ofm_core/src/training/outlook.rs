//! What a club's training will really do today and this week, for the Training screen.
//!
//! The stored plan is the manager's; the session that runs is that plan after the taper, the
//! fatigue guard and the coaching staff (see the parent module). The screen used to show only
//! the stored plan, so a manager could not tell that a High setting would run as Medium before a
//! match, or that a congested week was recovery work. This reads those same rules, it does not
//! restate them: the taper comes from `tapering_teams`, the coaching figures from
//! `compute_coaching_bonus`.

use serde::Serialize;

use super::{Taper, compute_coaching_bonus, downgrade_intensity, tapering_teams};
use crate::game::Game;
use domain::team::{TrainingFocus, TrainingIntensity};

/// Today's session and the figures behind it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrainingOutlook {
    /// Whether the schedule has a session today.
    pub is_training_day: bool,
    /// Why the session runs lighter than the stored plan: `nearMatch` or `congested`.
    pub taper: Option<&'static str>,
    /// The intensity today's session actually runs at.
    pub session_intensity: TrainingIntensity,
    /// Fixtures in the coming week, in every competition.
    pub fixtures_this_week: usize,
    /// How well the coaching staff teach the stored focus (1.0 = a staff of 50 and no specialist).
    pub coaching_effect: f64,
    /// Condition recovery from the medical department and the fitness coaches (1.0 = none).
    pub recovery_effect: f64,
    /// Average condition of the eleven likely to play, the reading the assistant plans from.
    pub starters_condition: f64,
    /// The staff member who sets the plan when training is handed over; `None` when the manager
    /// keeps it.
    pub delegated_to: Option<OutlookOwner>,
    /// A proposed change of the plan is waiting for the manager's answer.
    pub proposal_waiting: bool,
}

/// The member of staff running the club's training.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OutlookOwner {
    pub name: String,
    pub post: domain::staff::StaffPost,
    pub overall: u8,
}

/// The outlook for `team_id`, or `None` if there is no such club.
pub fn training_outlook(game: &Game, team_id: &str) -> Option<TrainingOutlook> {
    let team = game.teams.iter().find(|t| t.id == team_id)?;
    let weekday = chrono::Datelike::weekday(&game.clock.current_date).num_days_from_monday();
    let is_training_day = team.training_schedule.is_training_day(weekday);
    let taper = tapering_teams(game).get(team_id).copied();
    let session_intensity = match taper {
        Some(Taper::Congested) => TrainingIntensity::Low,
        Some(Taper::NearMatch) => downgrade_intensity(&team.training_intensity),
        None => team.training_intensity.clone(),
    };
    let staff = crate::staff_system::profile::team_staff(game, team_id);
    let bonus = compute_coaching_bonus(&staff, &team.training_focus);
    let by_club = crate::match_day_eligibility::players_by_club(&game.players);
    let club = by_club.get(team_id).map_or(&[][..], Vec::as_slice);
    let (_, fixtures_this_week) =
        crate::staff_system::assistant::next_match_and_load(game, team_id);
    Some(TrainingOutlook {
        is_training_day,
        taper: taper.map(|t| match t {
            Taper::NearMatch => "nearMatch",
            Taper::Congested => "congested",
        }),
        session_intensity,
        fixtures_this_week,
        coaching_effect: if team.training_focus == TrainingFocus::Recovery {
            1.0
        } else {
            bonus.coaching_mult * bonus.specialization_mult
        },
        recovery_effect: bonus.physio_mult,
        starters_condition: crate::ai_training::likely_starters_condition(&team.formation, club),
        delegated_to: if game
            .delegation
            .is_delegated(domain::delegation::Duty::Training)
        {
            crate::staff_system::delegation_owner::resolve(
                game,
                team_id,
                domain::delegation::Duty::Training,
            )
            .map(|o| OutlookOwner {
                name: o.name,
                post: o.post,
                overall: o.rating.round() as u8,
            })
        } else {
            None
        },
        proposal_waiting: game
            .delegation
            .proposals
            .iter()
            .any(|p| p.duty == domain::delegation::Duty::Training),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staff_system::assistant::tests::game;

    /// Given a club that trains at High with a match two days away
    /// When the outlook is read
    /// Then the session runs one step lighter (Medium) and says why; with the match six days
    ///      away it runs as set.
    #[test]
    fn a_match_two_days_away_lightens_the_session_the_outlook_reports() {
        let mut near = game(Some(60), 2);
        near.teams[0].training_intensity = TrainingIntensity::High;
        let outlook = training_outlook(&near, "team1").expect("a club");
        assert_eq!(outlook.taper, Some("nearMatch"));
        assert_eq!(outlook.session_intensity, TrainingIntensity::Medium);

        let mut far = game(Some(60), 6);
        far.teams[0].training_intensity = TrainingIntensity::High;
        let outlook = training_outlook(&far, "team1").expect("a club");
        assert_eq!(outlook.taper, None);
        assert_eq!(outlook.session_intensity, TrainingIntensity::High);
    }

    /// Given a second fixture inside the week
    /// When the outlook is read
    /// Then the week is congested and the session is Low, and the fixtures are counted.
    #[test]
    fn a_congested_week_is_reported_as_low_work() {
        let mut g = game(Some(60), 4);
        let date = (g.clock.current_date + chrono::Duration::days(6))
            .format("%Y-%m-%d")
            .to_string();
        g.competitions[0].fixtures.push(domain::league::Fixture {
            id: "x".into(),
            competition_id: "l".into(),
            date,
            home_team_id: "team2".into(),
            away_team_id: "team1".into(),
            ..Default::default()
        });
        g.teams[0].training_intensity = TrainingIntensity::High;
        let outlook = training_outlook(&g, "team1").expect("a club");
        assert_eq!(outlook.taper, Some("congested"));
        assert_eq!(outlook.session_intensity, TrainingIntensity::Low);
        assert_eq!(outlook.fixtures_this_week, 2);
    }

    /// Given training handed to the assistant, then taken back
    /// When the outlook is read
    /// Then it names who runs the plan while handed over, and nobody once the manager keeps it.
    #[test]
    fn the_outlook_names_who_runs_a_delegated_plan() {
        let mut g = game(Some(60), 6);
        assert_eq!(
            training_outlook(&g, "team1").expect("club").delegated_to,
            None
        );
        g.delegation
            .set(domain::delegation::Duty::Training, true, true);
        let owner = training_outlook(&g, "team1")
            .expect("club")
            .delegated_to
            .expect("the assistant runs it");
        assert_eq!(owner.post, domain::staff::StaffPost::AssistantManager);
        assert_eq!(owner.overall, 60);
        g.delegation
            .set(domain::delegation::Duty::Training, false, true);
        assert_eq!(
            training_outlook(&g, "team1").expect("club").delegated_to,
            None
        );
    }

    /// Given a better and a worse coaching staff
    /// When the outlook is read
    /// Then the coaching effect is higher for the better staff, and a missing club is `None`.
    #[test]
    fn better_coaches_show_a_higher_coaching_effect() {
        let strong = game(Some(90), 6);
        let weak = game(Some(20), 6);
        let (s, w) = (
            training_outlook(&strong, "team1").expect("club"),
            training_outlook(&weak, "team1").expect("club"),
        );
        assert!(s.coaching_effect > w.coaching_effect);
        assert!(training_outlook(&strong, "nowhere").is_none());
    }
}
