//! Who runs a delegated duty.
//!
//! The manager can put a particular member of staff in charge of a duty (the head of youth runs
//! the academy, a fitness coach runs training) or leave it with the assistant manager, which is
//! what every duty did before duties had owners. A duty has exactly one owner: the setting holds
//! one id, so two members can never both think a duty is theirs.
//!
//! Only posts that could really do the job are offered: what a duty does is decided by the
//! same functions the manager uses, and the owner's rating is what the duty is judged with
//! (how well he reads a squad, a scout's report, a youngster). A duty whose owner has left falls
//! back to the assistant and the history says so once.

use domain::delegation::{DecisionOutcome, Duty};
use domain::staff::{Staff, StaffPost};

use super::delegation::{param, record};
use super::profile::team_staff;
use crate::game::Game;

/// A member of staff in charge of a duty today.
#[derive(Debug, Clone, PartialEq)]
pub struct Owner {
    pub id: String,
    pub post: StaffPost,
    /// The member's overall, 0-100: the competence the duty is carried out with.
    pub rating: f64,
}

/// The posts that may be put in charge of `duty`. The assistant manager is always among them and
/// is the default; a duty that lists only him cannot be given to anyone else, because nobody
/// else holds the information it needs (the whole squad, the club's finances, the board).
pub fn eligible_posts(duty: Duty) -> &'static [StaffPost] {
    match duty {
        Duty::Training => &[
            StaffPost::AssistantManager,
            StaffPost::FirstTeamCoach,
            StaffPost::FitnessCoach,
        ],
        Duty::MatchPreparation => &[StaffPost::AssistantManager, StaffPost::FirstTeamCoach],
        Duty::Scouting => &[StaffPost::AssistantManager, StaffPost::HeadOfScouting],
        Duty::Youth => &[
            StaffPost::AssistantManager,
            StaffPost::HeadOfYouth,
            StaffPost::YouthCoach,
        ],
        Duty::StaffRecruitment
        | Duty::StaffRenewals
        | Duty::Lineup
        | Duty::PlayerRenewals
        | Duty::PlayerRecruitment
        | Duty::Tactics
        | Duty::Offers => &[StaffPost::AssistantManager],
    }
}

fn owner_of(member: &Staff) -> Owner {
    Owner {
        id: member.id.clone(),
        post: member.post(),
        rating: f64::from(member.overall()),
    }
}

/// Who is in charge of `duty` for `team_id`: the member the manager named if he is still there
/// and still fits the duty, else the assistant manager, else nobody.
pub fn resolve(game: &Game, team_id: &str, duty: Duty) -> Option<Owner> {
    let staff = team_staff(game, team_id);
    let named = game.delegation.owner(duty).and_then(|id| {
        staff
            .iter()
            .find(|s| s.id == id && eligible_posts(duty).contains(&s.post()))
    });
    named
        .or_else(|| {
            staff
                .iter()
                .find(|s| s.post() == StaffPost::AssistantManager)
        })
        .map(|member| owner_of(member))
}

/// Everyone who could be put in charge of `duty` today, best first (rating, then id, so the list
/// is stable).
pub fn candidates(game: &Game, team_id: &str, duty: Duty) -> Vec<Owner> {
    let mut list: Vec<Owner> = team_staff(game, team_id)
        .into_iter()
        .filter(|s| eligible_posts(duty).contains(&s.post()))
        .map(owner_of)
        .collect();
    list.sort_by(|a, b| {
        b.rating
            .partial_cmp(&a.rating)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    list
}

/// Puts `staff_id` in charge of `duty`, or hands it back to the assistant with `None`.
///
/// Refused if the member is not on the club's staff or his post does not fit the duty.
pub fn assign(
    game: &mut Game,
    team_id: &str,
    duty: Duty,
    staff_id: Option<&str>,
) -> Result<(), String> {
    let Some(id) = staff_id else {
        game.delegation.set_owner(duty, None);
        return Ok(());
    };
    let member = team_staff(game, team_id)
        .into_iter()
        .find(|s| s.id == id)
        .map(Staff::post)
        .ok_or("be.error.delegation.ownerNotFound")?;
    if !eligible_posts(duty).contains(&member) {
        return Err("be.error.delegation.ownerNotEligible".to_string());
    }
    // The assistant is the default, not an override: storing him would let a later change of
    // assistant leave the duty pointing at someone who has gone.
    let owner = (member != StaffPost::AssistantManager).then(|| id.to_string());
    game.delegation.set_owner(duty, owner);
    Ok(())
}

/// Drops every named owner who has left the club or no longer fits, handing the duty back to
/// the assistant and saying so once in the history.
pub(super) fn reconcile(game: &mut Game, team_id: &str) {
    let stale: Vec<(Duty, String)> = Duty::ALL
        .into_iter()
        .filter_map(|duty| {
            let id = game.delegation.owner(duty)?.to_string();
            let still_fits = team_staff(game, team_id)
                .iter()
                .any(|s| s.id == id && eligible_posts(duty).contains(&s.post()));
            (!still_fits).then_some((duty, id))
        })
        .collect();
    for (duty, id) in stale {
        game.delegation.set_owner(duty, None);
        record(
            game,
            duty,
            "delegation.log.ownerLeft",
            vec![param("staff", id)],
            DecisionOutcome::Executed,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staff_system::assistant::tests::{game, staff_member};

    /// A club with an assistant of 60 and the extra members given, each as `(id, post, level)`.
    fn club(extra: &[(&str, StaffPost, u8)]) -> Game {
        let mut g = game(Some(60), 6);
        for (id, post, level) in extra {
            g.staff.push(staff_member(id, *post, *level, "team1"));
        }
        g
    }

    /// Given a club with an assistant and no named owner
    /// When the owner of training is resolved
    /// Then it is the assistant, with his own rating.
    #[test]
    fn a_duty_without_a_named_owner_belongs_to_the_assistant() {
        let g = club(&[("coach", StaffPost::FirstTeamCoach, 80)]);
        let owner = resolve(&g, "team1", Duty::Training).expect("an assistant is in post");
        assert_eq!(owner.id, "assist");
        assert_eq!(owner.rating, 60.0);
    }

    /// Given a fitness coach of 85 named as the owner of training
    /// When the owner is resolved
    /// Then the duty is run with his rating, not the assistant's.
    #[test]
    fn a_named_owner_runs_the_duty_with_his_own_rating() {
        let mut g = club(&[("fit", StaffPost::FitnessCoach, 85)]);
        assign(&mut g, "team1", Duty::Training, Some("fit")).expect("a fitness coach may train");
        let owner = resolve(&g, "team1", Duty::Training).expect("owner");
        assert_eq!((owner.id.as_str(), owner.rating), ("fit", 85.0));
        assert_eq!(g.delegation.owner(Duty::Training), Some("fit"));
    }

    /// Given a physio of 90
    /// When he is named to run the lineup or training
    /// Then both are refused, and an unknown member is refused too; nothing is stored.
    #[test]
    fn a_post_that_cannot_do_the_job_is_refused() {
        let mut g = club(&[("physio-x", StaffPost::Physio, 90)]);
        let err = assign(&mut g, "team1", Duty::Lineup, Some("physio-x")).unwrap_err();
        assert_eq!(err, "be.error.delegation.ownerNotEligible");
        let err = assign(&mut g, "team1", Duty::Training, Some("physio-x")).unwrap_err();
        assert_eq!(err, "be.error.delegation.ownerNotEligible");
        let err = assign(&mut g, "team1", Duty::Training, Some("nobody")).unwrap_err();
        assert_eq!(err, "be.error.delegation.ownerNotFound");
        assert_eq!(g.delegation.owner(Duty::Training), None);
    }

    /// Given training owned by one coach
    /// When another is named, and then the assistant
    /// Then each replaces the last: a duty has one owner, and the assistant is the default, not
    ///      a stored override.
    #[test]
    fn a_duty_has_one_owner_and_reassigning_replaces_him() {
        let mut g = club(&[
            ("c1", StaffPost::FirstTeamCoach, 70),
            ("c2", StaffPost::FitnessCoach, 75),
        ]);
        assign(&mut g, "team1", Duty::Training, Some("c1")).expect("c1");
        assign(&mut g, "team1", Duty::Training, Some("c2")).expect("c2");
        assert_eq!(g.delegation.owner(Duty::Training), Some("c2"));
        assign(&mut g, "team1", Duty::Training, Some("assist")).expect("back to the default");
        assert_eq!(g.delegation.owner(Duty::Training), None);
    }

    /// Given an owner who then leaves the club
    /// When the duties are reconciled twice
    /// Then the duty is the assistant's again and the history says so once.
    #[test]
    fn an_owner_who_leaves_hands_the_duty_back_once() {
        let mut g = club(&[("c1", StaffPost::FirstTeamCoach, 70)]);
        assign(&mut g, "team1", Duty::Training, Some("c1")).expect("c1");
        g.staff.retain(|s| s.id != "c1");
        reconcile(&mut g, "team1");
        reconcile(&mut g, "team1");
        assert_eq!(g.delegation.owner(Duty::Training), None);
        let notes = g
            .delegation
            .log
            .iter()
            .filter(|e| e.title_key == "delegation.log.ownerLeft")
            .count();
        assert_eq!(notes, 1, "said once, not on every run");
        let owner = resolve(&g, "team1", Duty::Training).expect("assistant");
        assert_eq!(owner.id, "assist");
    }

    /// Given a club with two coaches, the assistant and a scout
    /// When the candidates for training are listed
    /// Then they are the three who can coach, best first, and the scout is not among them.
    #[test]
    fn candidates_are_listed_best_first() {
        let mut g = club(&[
            ("c1", StaffPost::FirstTeamCoach, 70),
            ("c2", StaffPost::FitnessCoach, 75),
            ("scout-x", StaffPost::Scout, 99),
        ]);
        g.staff.retain(|s| !s.id.starts_with("have-"));
        let ids: Vec<_> = candidates(&g, "team1", Duty::Training)
            .into_iter()
            .map(|o| o.id)
            .collect();
        assert_eq!(ids, ["c2", "c1", "assist"]);
    }
}
