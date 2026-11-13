//! Who is in control of a delegated duty.
//!
//! A duty the manager has handed over is the assistant's until the manager takes a decision of his
//! own in it: the training he sets, the eleven he names, the tactical plan he picks. The assistant
//! does not compare notes with the manager; he remembers what he last wrote, and if the club's
//! setting is no longer that, the manager has changed it. The duty then stands back for a few
//! days, the history says so, and nothing the manager decided is ever overwritten. This works the
//! same for every door the manager uses (the screens, the MCP tools), and across a reload,
//! because the memory is saved with the delegation.
//!
//! This module also holds the tactical-preparation duty, which is new: before this the assistant
//! could only advise on tactics.

use domain::delegation::{DecisionOutcome, Duty};
use domain::team::Team;

use super::delegation::{log, param, record, stamp};
use crate::game::Game;
use crate::stable_hash::stable_hash;

/// An assistant below this rating does not read the squad when he plans a match: he plans from
/// the club's style and results alone, so he may ask a tired side to press. Not a dice roll: the
/// same assistant gives the same plan in the same state.
const SQUAD_READING_RATING: f64 = 45.0;
const PRINT_SEED: u64 = 0x636f_6e74_726f_6c00; // "control\0"

/// What the club's training is set to, as the assistant remembers it.
pub(super) fn training_print(team: &Team) -> String {
    format!("{:?}|{:?}", team.training_focus, team.training_intensity)
}

/// The eleven, in order, as the assistant remembers it.
pub(super) fn lineup_print(ids: &[String]) -> String {
    ids.join(",")
}

/// The tactical plan, fingerprinted: nine dials are too many to keep readable.
pub(super) fn tactics_print(team: &Team) -> String {
    let text = serde_json::to_string(&team.tactics_phase).unwrap_or_default();
    format!("{:016x}", stable_hash(text.as_bytes(), PRINT_SEED))
}

/// Whether the manager is in control of `duty` today, and so the assistant must leave it alone.
///
/// `changed_since_applied` is the caller's reading of "the club's setting is not what I last
/// wrote". It is evaluated by the caller because what counts as the same differs by duty (a
/// departed player must not look like a manager's decision). The first time it is true the
/// override is recorded in the history; afterwards the duty stands back until its date.
pub(super) fn manager_is_in_control(
    game: &mut Game,
    duty: Duty,
    changed_since_applied: bool,
) -> bool {
    let today = stamp(game);
    if game.delegation.is_standing_back(duty, &today) {
        return true;
    }
    if !changed_since_applied {
        return false;
    }
    let until = (super::delegation::date(game) + chrono::Duration::days(duty.stand_back_days()))
        .format("%Y-%m-%d")
        .to_string();
    game.delegation.stand_back(duty, &until);
    game.delegation.forget_applied(duty);
    record(
        game,
        duty,
        "delegation.log.managerOverride",
        vec![param("until", until)],
        DecisionOutcome::LeftToManager,
    );
    true
}

/// Whether the eleven the manager has now differs from the one the assistant wrote, ignoring
/// players who are no longer at the club.
pub(super) fn eleven_was_changed(game: &Game, team_id: &str, applied: Option<&str>) -> bool {
    let Some(applied) = applied else {
        return false;
    };
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return false;
    };
    let at_club = |id: &str| {
        game.players
            .iter()
            .any(|p| p.id == id && p.team_id.as_deref() == Some(team_id))
    };
    let was: Vec<&str> = applied.split(',').filter(|id| at_club(id)).collect();
    let is: Vec<&str> = team
        .starting_xi_ids
        .iter()
        .map(String::as_str)
        .filter(|id| at_club(id))
        .collect();
    was != is
}

/// Tactical preparation: the day before a match, the plan the club's style, squad and results
/// call for. Written only if it differs from what the club has.
pub(super) fn tactics(game: &mut Game, team_id: &str, assistant_rating: f64) {
    if !super::delegation::matches_within(game, team_id, 1) {
        return;
    }
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return;
    };
    let current = tactics_print(team);
    let changed = game
        .delegation
        .applied(Duty::Tactics)
        .is_some_and(|applied| applied != current);
    if manager_is_in_control(game, Duty::Tactics, changed) {
        return;
    }
    let plan =
        crate::ai_tactics::plan_for_club(game, team_id, assistant_rating >= SQUAD_READING_RATING);
    let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) else {
        return;
    };
    let style = format!("{:?}", team.play_style);
    if team.tactics_phase != plan {
        team.tactics_phase = plan;
        let print = tactics_print(team);
        game.delegation.record_applied(Duty::Tactics, &print);
        log(
            game,
            Duty::Tactics,
            "delegation.log.tacticsSet",
            vec![param("style", style)],
        );
    } else {
        game.delegation.record_applied(Duty::Tactics, &current);
    }
}

#[cfg(test)]
mod tests {
    use super::super::assistant::tests::game;
    use super::super::delegation::{run_after_day, run_training};
    use super::*;
    use domain::delegation::DelegationMode;
    use domain::team::{PlayStyle, TrainingFocus, TrainingIntensity};

    /// Match on the 3rd, so the 2nd is the eve and the 3rd is match day.
    fn eve_of_a_match() -> Game {
        let mut g = game(Some(80), 2);
        g.clock.current_date += chrono::Duration::days(1);
        g.teams[0].play_style = PlayStyle::Counter;
        g
    }

    fn next_day(g: &mut Game) {
        g.clock.current_date += chrono::Duration::days(1);
    }

    fn entries(g: &Game, duty: Duty) -> Vec<&domain::delegation::DelegationLogEntry> {
        g.delegation.log.iter().filter(|e| e.duty == duty).collect()
    }

    fn injure(g: &mut Game, id: &str) {
        let player = g.players.iter_mut().find(|p| p.id == id).unwrap();
        player.injury = Some(domain::player::Injury {
            name: "Hamstring".into(),
            days_remaining: 20,
        });
    }

    /// Given a manager who has delegated nothing, with a tactical plan to make and a starter hurt
    /// When the eve of a match is played
    /// Then nothing is changed and nothing is written to the history.
    #[test]
    fn in_manual_mode_the_assistant_changes_nothing_and_records_nothing() {
        let mut g = eve_of_a_match();
        injure(&mut g, "s3");
        let (xi, tactics) = (
            g.teams[0].starting_xi_ids.clone(),
            g.teams[0].tactics_phase.clone(),
        );

        run_after_day(&mut g);

        assert_eq!(g.teams[0].starting_xi_ids, xi);
        assert_eq!(g.teams[0].tactics_phase, tactics);
        assert!(g.delegation.log.is_empty());
    }

    /// Given full delegation, a counter-attacking club and an injured starter
    /// When the eve of a match is played
    /// Then the eleven drops the injured player and the tactical plan is the club's style, each
    /// with a history entry; and a day with no match writes nothing.
    #[test]
    fn in_full_mode_the_assistant_names_the_eleven_and_sets_the_tactics() {
        let mut g = eve_of_a_match();
        g.delegation.apply_mode(DelegationMode::Full);
        injure(&mut g, "s3");

        run_after_day(&mut g);

        assert!(!g.teams[0].starting_xi_ids.contains(&"s3".to_string()));
        assert_eq!(
            g.teams[0].tactics_phase,
            crate::ai_tactics::plan_for_club(&g, "team1", true),
            "the plan is the one the club's style, squad and results call for"
        );
        assert_ne!(
            g.teams[0].tactics_phase,
            domain::team::TacticsPhaseSettings::default(),
            "a counter-attacking club does not play the neutral plan"
        );
        assert_eq!(entries(&g, Duty::Lineup).len(), 1);
        let set = entries(&g, Duty::Tactics);
        assert_eq!(set.len(), 1);
        assert_eq!(set[0].title_key, "delegation.log.tacticsSet");
        assert_eq!(set[0].outcome, DecisionOutcome::Executed);

        // Days with no match act on nothing and write nothing.
        let before = g.delegation.log.len();
        let mut quiet = game(Some(80), 20);
        quiet.delegation.apply_mode(DelegationMode::Full);
        run_after_day(&mut quiet);
        assert!(quiet.delegation.log.iter().all(|e| !matches!(
            e.duty,
            Duty::Lineup | Duty::Tactics | Duty::MatchPreparation
        )));
        assert!(before > 0);
    }

    /// Given only tactical preparation delegated
    /// When the eve of a match is played with a starter hurt
    /// Then the tactics are set and the eleven is the manager's to name.
    #[test]
    fn in_custom_mode_only_the_chosen_duties_act() {
        let mut g = eve_of_a_match();
        g.delegation.set(Duty::Tactics, true, false);
        injure(&mut g, "s3");
        let xi = g.teams[0].starting_xi_ids.clone();

        run_after_day(&mut g);

        assert_eq!(g.delegation.mode(), DelegationMode::Custom);
        assert_eq!(g.teams[0].starting_xi_ids, xi);
        assert!(entries(&g, Duty::Lineup).is_empty());
        assert_eq!(entries(&g, Duty::Tactics).len(), 1);
    }

    /// Given the assistant has named the eleven
    /// When the manager names another before the match and the day comes
    /// Then the manager's eleven stands, the history says the assistant stood back, and the
    /// duty comes back to him once the days have passed.
    #[test]
    fn the_assistant_does_not_overwrite_the_managers_eleven() {
        let mut g = eve_of_a_match();
        g.delegation.set(Duty::Lineup, true, false);
        injure(&mut g, "s3");
        run_after_day(&mut g);
        let assistants = g.teams[0].starting_xi_ids.clone();
        assert!(!assistants.contains(&"s3".to_string()));

        let managers: Vec<String> = (5..16).map(|i| format!("s{i}")).collect();
        g.teams[0].starting_xi_ids = managers.clone();
        next_day(&mut g); // match day: still "imminent"
        run_after_day(&mut g);

        assert_eq!(g.teams[0].starting_xi_ids, managers);
        let over = entries(&g, Duty::Lineup)
            .into_iter()
            .filter(|e| e.outcome == DecisionOutcome::LeftToManager)
            .count();
        assert_eq!(over, 1);

        // Three days on, with a new match, the duty is the assistant's again.
        for _ in 0..3 {
            next_day(&mut g);
        }
        let mut league = g.competitions[0].clone();
        league.fixtures[0].status = domain::league::FixtureStatus::Completed;
        league.fixtures.push(domain::league::Fixture {
            id: "f2".into(),
            competition_id: "l".into(),
            date: (g.clock.current_date + chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
            home_team_id: "team1".into(),
            away_team_id: "team2".into(),
            ..Default::default()
        });
        g.competitions = vec![league];
        run_after_day(&mut g);
        assert_ne!(g.teams[0].starting_xi_ids, managers);
    }

    /// Given a delegated training plan
    /// When the manager sets the training himself
    /// Then the assistant leaves it for a week and the history says so.
    #[test]
    fn the_assistant_does_not_overwrite_the_managers_training() {
        let mut g = game(Some(80), 20);
        g.delegation.set(Duty::Training, true, false);
        run_training(&mut g, 1);
        let by_assistant = (
            g.teams[0].training_focus.clone(),
            g.teams[0].training_intensity.clone(),
        );
        assert!(g.delegation.applied(Duty::Training).is_some());

        g.teams[0].training_focus = TrainingFocus::Defending;
        g.teams[0].training_intensity = TrainingIntensity::High;
        // Make the assistant's own plan differ from what the manager chose.
        for p in g
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
        {
            p.condition = 20;
        }
        next_day(&mut g);
        run_training(&mut g, 2);

        assert_eq!(g.teams[0].training_focus, TrainingFocus::Defending);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::High);
        assert!(
            entries(&g, Duty::Training)
                .iter()
                .any(|e| e.outcome == DecisionOutcome::LeftToManager)
        );

        for _ in 0..7 {
            next_day(&mut g);
        }
        run_training(&mut g, 3);
        assert_ne!(
            (
                g.teams[0].training_focus.clone(),
                g.teams[0].training_intensity.clone()
            ),
            (TrainingFocus::Defending, TrainingIntensity::High),
            "after the week the assistant plans again (assistant earlier: {by_assistant:?})"
        );
    }

    /// Given a tired squad of a pressing club
    /// When a good assistant and a poor one plan the next match
    /// Then the good one asks for less pressing and the poor one does not read the squad.
    #[test]
    fn a_poor_assistant_plans_without_reading_the_squad() {
        let plan_by = |rating: u8| {
            let mut g = game(Some(rating), 1);
            g.teams[0].play_style = PlayStyle::HighPress;
            for p in g
                .players
                .iter_mut()
                .filter(|p| p.team_id.as_deref() == Some("team1"))
            {
                p.attributes.stamina = 30;
            }
            g.delegation.set(Duty::Tactics, true, false);
            run_after_day(&mut g);
            g.teams[0].tactics_phase.pressing_intensity.clone()
        };

        assert_ne!(plan_by(80), plan_by(30));
        assert_eq!(
            plan_by(30),
            plan_by(30),
            "the same assistant plans the same way"
        );
    }

    /// Given a duty the assistant stood back from
    /// When the manager hands it over again
    /// Then the assistant acts at once.
    #[test]
    fn handing_a_duty_over_again_takes_it_back_from_the_manager() {
        let mut g = game(Some(80), 20);
        g.delegation.set(Duty::Training, true, false);
        g.delegation.stand_back(Duty::Training, "2099-01-01");
        g.delegation.set(Duty::Training, false, false);
        g.delegation.set(Duty::Training, true, false);
        g.teams[0].training_focus = TrainingFocus::Defending;
        for p in g
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
        {
            p.condition = 20;
        }

        run_training(&mut g, 1);

        assert_eq!(g.teams[0].training_focus, TrainingFocus::Recovery);
    }
}
