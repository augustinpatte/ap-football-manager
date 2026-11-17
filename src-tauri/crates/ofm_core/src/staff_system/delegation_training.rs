//! Delegated training: the plan a staff member sets for the club's week.
//!
//! The base plan is the one every AI club runs (`ai_training::plan_for_team`), so the manager's
//! club and the others live by the same condition bands and nobody has a second, drifting
//! planner. What the owner of the duty adds, from what the club's staff really are:
//!
//! * **Fixtures.** Two or more matches inside the coming week, and the stored plan is recovery
//!   work, which is what `training` will run that week anyway: the manager sees the session he
//!   is going to get, not one the taper silently overrides.
//! * **Coaching strengths.** One session a week (the Wednesday slot) is the one the coaching
//!   staff teach best, by the very figure `training` applies to the gains
//!   (`coaching_mult x specialization_mult`), if it beats the style's own session by 5 %. The
//!   other four sessions keep the club's style, so a staff that is brilliant at one thing does
//!   not turn the squad one-dimensional. An owner below [`READS_COACHES_FROM`] does not look.
//! * **Caution.** An owner below [`CAUTIOUS_BELOW`] never asks for a High session: a weak
//!   planner develops the squad less, he does not wear it out.
//!
//! Easing the load is always done at once. Raising it is the decision that costs fitness, so
//! under "ask me" it is a proposal (and a refused one is not repeated for a week).

use domain::delegation::{Duty, ProposalAction};
use domain::team::{TrainingFocus, TrainingIntensity};

use super::assistant::next_match_and_load;
use super::delegation::{date, log, param, stamp, user_team};
use super::delegation_control as control;
use super::delegation_owner as owner;
use super::profile::team_staff;
use crate::game::Game;

/// An owner rated below this does not read what the coaches are good at.
pub const READS_COACHES_FROM: f64 = 40.0;
/// An owner rated below this never asks for a High session.
pub const CAUTIOUS_BELOW: f64 = 35.0;
/// The weekday slot (`weekday % 5`) given to the coaching staff's best session.
const TAILORED_SLOT: usize = 2;
/// How much better a session must be taught than the style's own to replace it.
const TAILOR_MARGIN: f64 = 1.05;
/// Days a raised load is not asked about again after the manager answered.
const ASK_AGAIN_AFTER_DAYS: i64 = 7;
const LOAD_PROPOSAL_KEY: &str = "delegation.proposal.trainingLoad";

/// The sessions a coaching staff can be asked to lean on: the four outfield skills. Physical is
/// the club's base work and Recovery is a medical decision, so neither is "tailored".
const TAILORABLE: [TrainingFocus; 4] = [
    TrainingFocus::Technical,
    TrainingFocus::Tactical,
    TrainingFocus::Defending,
    TrainingFocus::Attacking,
];

fn load_rank(intensity: &TrainingIntensity) -> u8 {
    match intensity {
        TrainingIntensity::Low => 0,
        TrainingIntensity::Medium => 1,
        TrainingIntensity::High => 2,
    }
}

/// How well the club's coaching staff teach `focus`, as `training` will count it.
pub fn teaching_effect(game: &Game, team_id: &str, focus: &TrainingFocus) -> f64 {
    let staff = team_staff(game, team_id);
    let bonus = crate::training::compute_coaching_bonus(&staff, focus);
    bonus.coaching_mult * bonus.specialization_mult
}

/// The tailorable session the coaching staff teach best, with how well. Ties go to the earlier
/// in [`TAILORABLE`], so the answer never depends on iteration order.
pub fn best_taught(game: &Game, team_id: &str) -> (TrainingFocus, f64) {
    let mut best = (
        TAILORABLE[0].clone(),
        teaching_effect(game, team_id, &TAILORABLE[0]),
    );
    for focus in &TAILORABLE[1..] {
        let effect = teaching_effect(game, team_id, focus);
        if effect > best.1 {
            best = (focus.clone(), effect);
        }
    }
    best
}

/// The session an owner rated `owner_rating` would set today, or `None` on a rest day.
pub fn plan(
    game: &Game,
    team_id: &str,
    weekday_num: u32,
    owner_rating: f64,
) -> Option<(TrainingFocus, TrainingIntensity)> {
    let by_club = crate::match_day_eligibility::players_by_club(&game.players);
    let club = by_club.get(team_id).map_or(&[][..], Vec::as_slice);
    let (mut focus, mut intensity) =
        crate::ai_training::plan_for_team(&game.teams, club, team_id, weekday_num)?;
    drop(by_club);

    let (_, week_load) = next_match_and_load(game, team_id);
    if week_load >= crate::training::CONGESTION_FIXTURE_THRESHOLD {
        return Some((TrainingFocus::Recovery, TrainingIntensity::Low));
    }
    if focus != TrainingFocus::Recovery
        && weekday_num as usize % 5 == TAILORED_SLOT
        && owner_rating >= READS_COACHES_FROM
    {
        let (best, effect) = best_taught(game, team_id);
        if effect >= teaching_effect(game, team_id, &focus) * TAILOR_MARGIN {
            focus = best;
        }
    }
    if owner_rating < CAUTIOUS_BELOW && intensity == TrainingIntensity::High {
        intensity = TrainingIntensity::Medium;
    }
    Some((focus, intensity))
}

/// Whether the owner may put a raised load to the manager today: nothing of the kind is waiting,
/// and he has not answered one this past week.
fn may_ask(game: &Game) -> bool {
    let since = (date(game) - chrono::Duration::days(ASK_AGAIN_AFTER_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    let waiting = game
        .delegation
        .proposals
        .iter()
        .any(|p| p.duty == Duty::Training);
    let answered = game.delegation.log.iter().any(|e| {
        e.duty == Duty::Training
            && e.title_key.starts_with(LOAD_PROPOSAL_KEY)
            && e.date.as_str() >= since.as_str()
    });
    !waiting && !answered
}

/// Before the day's training: a delegated training plan.
///
/// Written only if the manager has not set the training himself since (see
/// [`control::manager_is_in_control`]); the history records a change of intensity and a recovery
/// order, the decisions, not the daily rotation of the focus.
pub(super) fn run(game: &mut Game, weekday_num: u32) {
    let Some(team_id) = user_team(game) else {
        return;
    };
    if !game.delegation.is_delegated(Duty::Training) {
        return;
    }
    owner::reconcile(game, &team_id);
    let Some(in_charge) = owner::resolve(game, &team_id, Duty::Training) else {
        return;
    };
    let Some(current) = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(control::training_print)
    else {
        return;
    };
    let changed = game
        .delegation
        .applied(Duty::Training)
        .is_some_and(|applied| applied != current);
    if control::manager_is_in_control(game, Duty::Training, changed) {
        return;
    }
    let Some((focus, wanted)) = plan(game, &team_id, weekday_num, in_charge.rating) else {
        return;
    };
    let Some(standing) = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(|t| t.training_intensity.clone())
    else {
        return;
    };
    // Raising the load is the step that costs fitness: under "ask me" it waits for the manager,
    // and until he answers the session runs at the standing intensity.
    let mut intensity = wanted.clone();
    if load_rank(&wanted) > load_rank(&standing)
        && game.delegation.needs_confirmation(Duty::Training)
    {
        intensity = standing.clone();
        if may_ask(game) {
            let today = stamp(game);
            game.delegation.propose(
                Duty::Training,
                &today,
                ProposalAction::SetTraining {
                    focus: focus.clone(),
                    intensity: wanted.clone(),
                },
                &format!("{LOAD_PROPOSAL_KEY}.title"),
                vec![
                    param("focus", format!("{focus:?}")),
                    param("intensity", format!("{wanted:?}")),
                    param("currentIntensity", format!("{standing:?}")),
                ],
                &format!("{LOAD_PROPOSAL_KEY}.reason"),
            );
        }
    }
    let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) else {
        return;
    };
    let eased_to_recovery = focus == TrainingFocus::Recovery
        && intensity == TrainingIntensity::Low
        && team.training_focus != TrainingFocus::Recovery;
    let intensity_changed = team.training_intensity != intensity;
    team.training_focus = focus.clone();
    team.training_intensity = intensity.clone();
    let print = control::training_print(team);
    game.delegation.record_applied(Duty::Training, &print);
    if eased_to_recovery {
        log(
            game,
            Duty::Training,
            "delegation.log.recoveryOrdered",
            vec![],
        );
    } else if intensity_changed {
        log(
            game,
            Duty::Training,
            "delegation.log.trainingIntensity",
            vec![
                param("focus", format!("{focus:?}")),
                param("intensity", format!("{intensity:?}")),
            ],
        );
    }
}

/// A refused raise of the load is the manager's word: the duty keeps the standing load for a
/// week. Called by the proposal resolution.
pub(super) fn on_load_declined(game: &mut Game) {
    let until = (date(game) + chrono::Duration::days(ASK_AGAIN_AFTER_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    game.delegation.stand_back(Duty::Training, &until);
}

/// An approved raise is what the staff last wrote, so the next day does not read it as the
/// manager's own change.
pub(super) fn on_load_approved(game: &mut Game, team_id: &str) {
    if let Some(team) = game.teams.iter().find(|t| t.id == team_id) {
        let print = control::training_print(team);
        game.delegation.record_applied(Duty::Training, &print);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staff_system::assistant::tests::{game, staff_member};
    use crate::staff_system::delegation::resolve_proposal;
    use domain::delegation::DecisionOutcome;
    use domain::league::Fixture;
    use domain::staff::{CoachingSpecialization, StaffPost};
    use domain::team::TrainingSchedule;

    const WEDNESDAY: u32 = 2;

    /// A rested club in a six-day-away league match, training six days a week at Medium, whose
    /// training is handed to the assistant (rated `assistant`), who asks before raising the load
    /// when `confirm` is set.
    fn rested_club(assistant: u8, confirm: bool) -> Game {
        let mut g = game(Some(assistant), 6);
        g.teams[0].training_schedule = TrainingSchedule::Intense;
        g.teams[0].training_intensity = TrainingIntensity::Medium;
        g.teams[0].training_focus = TrainingFocus::Technical;
        for p in g
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
        {
            p.condition = 95;
        }
        g.delegation.set(Duty::Training, true, confirm);
        g
    }

    fn add_fixture_in(g: &mut Game, days: i64) {
        let date = (g.clock.current_date + chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string();
        g.competitions
            .first_mut()
            .expect("the fixture league")
            .fixtures
            .push(Fixture {
                id: format!("extra-{days}"),
                competition_id: "l".into(),
                date,
                home_team_id: "team2".into(),
                away_team_id: "team1".into(),
                ..Default::default()
            });
    }

    fn training_proposals(g: &Game) -> usize {
        g.delegation
            .proposals
            .iter()
            .filter(|p| p.duty == Duty::Training)
            .count()
    }

    /// Given a rested squad whose assistant asks before raising the load, standing at Medium
    /// When the plan is written
    /// Then the raise to High is a proposal, the session stays Medium, and a second day does not
    ///      queue a second proposal.
    #[test]
    fn raising_the_load_is_a_proposal_under_ask_me() {
        let mut g = rested_club(80, true);
        run(&mut g, WEDNESDAY);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::Medium);
        assert_eq!(training_proposals(&g), 1);
        run(&mut g, WEDNESDAY + 1);
        run(&mut g, WEDNESDAY + 2);
        assert_eq!(training_proposals(&g), 1, "asked once, not every day");
    }

    /// Given the same club with the assistant allowed to act
    /// When the plan is written
    /// Then the load is raised at once and nothing is queued.
    #[test]
    fn raising_the_load_is_automatic_when_not_asking() {
        let mut g = rested_club(80, false);
        run(&mut g, WEDNESDAY);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::High);
        assert_eq!(training_proposals(&g), 0);
    }

    /// Given a spent squad under "ask me"
    /// When the plan is written
    /// Then the easing to recovery is made at once: protecting the squad never waits.
    #[test]
    fn easing_the_load_never_waits_for_the_manager() {
        let mut g = rested_club(80, true);
        g.teams[0].training_intensity = TrainingIntensity::High;
        for p in g
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
        {
            p.condition = 5;
        }
        run(&mut g, WEDNESDAY);
        assert_eq!(g.teams[0].training_focus, TrainingFocus::Recovery);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::Low);
        assert_eq!(training_proposals(&g), 0);
    }

    /// Given a proposal to raise the load
    /// When the manager accepts, the next day is planned
    /// Then the club trains at High and the manager's own approval is not read as an override.
    #[test]
    fn an_approved_raise_is_not_mistaken_for_a_manager_override() {
        let mut g = rested_club(80, true);
        run(&mut g, WEDNESDAY);
        let id = g.delegation.proposals[0].id.clone();
        resolve_proposal(&mut g, &id, true).expect("the raise is accepted");
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::High);
        run(&mut g, WEDNESDAY + 1);
        assert!(
            !g.delegation
                .log
                .iter()
                .any(|e| e.title_key == "delegation.log.managerOverride")
        );
        assert!(
            g.delegation
                .log
                .iter()
                .any(|e| e.outcome == DecisionOutcome::Approved)
        );
        assert_eq!(training_proposals(&g), 0);
    }

    /// Given a refused raise
    /// When the next days are planned
    /// Then the duty stands back for the week: the standing plan is kept and nothing is asked
    ///      again.
    #[test]
    fn a_refused_raise_is_not_asked_again_for_a_week() {
        let mut g = rested_club(80, true);
        run(&mut g, WEDNESDAY);
        let id = g.delegation.proposals[0].id.clone();
        resolve_proposal(&mut g, &id, false).expect("the raise is declined");
        for day in 0..5 {
            run(&mut g, WEDNESDAY + day);
        }
        assert_eq!(training_proposals(&g), 0);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::Medium);
        assert!(
            g.delegation
                .log
                .iter()
                .any(|e| e.outcome == DecisionOutcome::Declined)
        );
    }

    /// Given two league matches inside the coming week
    /// When the plan is made
    /// Then it is recovery work at Low, the session training would run anyway.
    #[test]
    fn a_congested_week_is_planned_as_recovery() {
        let mut g = rested_club(80, false);
        add_fixture_in(&mut g, 3);
        run(&mut g, WEDNESDAY);
        assert_eq!(g.teams[0].training_focus, TrainingFocus::Recovery);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::Low);
    }

    /// A club whose coaches are all of 60, one of them a defending specialist, with the owner of
    /// training rated `owner` (the assistant is the owner unless a member is named).
    fn club_with_a_defending_specialist(assistant: u8) -> Game {
        let mut g = rested_club(assistant, false);
        g.staff.retain(|s| !s.id.starts_with("have-First"));
        let mut coach = staff_member("def-coach", StaffPost::FirstTeamCoach, 60, "team1");
        coach.specialization = Some(CoachingSpecialization::Defending);
        g.staff.push(coach);
        g
    }

    /// Given coaches who teach Defending best and an assistant who reads them
    /// When the Wednesday slot is planned
    /// Then that session is Defending rather than the style's Tactical, and the Monday slot keeps
    ///      the style's own session.
    #[test]
    fn the_midweek_session_leans_on_what_the_coaches_teach_best() {
        let mut g = club_with_a_defending_specialist(80);
        run(&mut g, WEDNESDAY);
        assert_eq!(g.teams[0].training_focus, TrainingFocus::Defending);
        let mut monday = club_with_a_defending_specialist(80);
        run(&mut monday, 0);
        assert_eq!(monday.teams[0].training_focus, TrainingFocus::Physical);
    }

    /// Given a weak assistant (30) who does not read the coaches
    /// When the same slot is planned
    /// Then the style's session stands; and if a strong fitness coach (80) is put in charge of
    ///      training, the slot is tailored again: the owner's rating is what counts.
    #[test]
    fn a_weak_owner_does_not_tailor_and_a_strong_named_owner_does() {
        let mut g = club_with_a_defending_specialist(30);
        run(&mut g, WEDNESDAY);
        assert_eq!(g.teams[0].training_focus, TrainingFocus::Tactical);

        let mut g = club_with_a_defending_specialist(30);
        g.staff
            .push(staff_member("fit-80", StaffPost::FitnessCoach, 80, "team1"));
        owner::assign(&mut g, "team1", Duty::Training, Some("fit-80")).expect("a fitness coach");
        run(&mut g, WEDNESDAY);
        assert_eq!(g.teams[0].training_focus, TrainingFocus::Defending);
    }

    /// Given a rested squad and a weak owner (30)
    /// When the plan is made
    /// Then it never reaches High, whereas an owner of 80 raises it: a weak planner develops the
    ///      squad less, he does not wear it out.
    #[test]
    fn a_weak_owner_never_asks_for_a_high_session() {
        assert_eq!(
            plan(&rested_club(80, false), "team1", WEDNESDAY, 80.0).map(|p| p.1),
            Some(TrainingIntensity::High)
        );
        assert_eq!(
            plan(&rested_club(30, false), "team1", WEDNESDAY, 30.0).map(|p| p.1),
            Some(TrainingIntensity::Medium)
        );
    }

    /// Given the same state twice
    /// When the plan is made
    /// Then it is the same plan.
    #[test]
    fn the_plan_is_deterministic() {
        let g = club_with_a_defending_specialist(80);
        assert_eq!(
            plan(&g, "team1", WEDNESDAY, 80.0),
            plan(&g, "team1", WEDNESDAY, 80.0)
        );
    }

    /// Given the manager sets the training himself after the assistant wrote it
    /// When the next day is planned
    /// Then the assistant stands back and the manager's setting survives.
    #[test]
    fn the_managers_own_setting_is_left_alone() {
        let mut g = rested_club(80, false);
        run(&mut g, WEDNESDAY);
        g.teams[0].training_focus = TrainingFocus::Attacking;
        g.teams[0].training_intensity = TrainingIntensity::Low;
        run(&mut g, WEDNESDAY + 1);
        assert_eq!(g.teams[0].training_focus, TrainingFocus::Attacking);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::Low);
    }
}
