//! The manager's academy review: where each academy player stands, in terms the manager can act
//! on — ready for the senior squad, in need of more development, worth a loan, or unlikely to
//! progress — with the facts behind the verdict.
//!
//! The verdict is advice drawn from the club's own squad model and from a player's own numbers;
//! it changes no player and no rating. Readiness is [`youth_pathway::pathway_of`], the same
//! question the AI clubs ask when they promote, so the manager's screen and an AI club's academy
//! agree about what "ready" means. The rest (loan, unlikely) are plain comparisons of the player's
//! rating and ceiling with the cover the senior squad already has in his position, and each is
//! given as a set of reasons the screen can show. Potential is a ceiling, not a promise: the
//! review shows the rating the development model expects a year on beside it.

use crate::game::Game;
use crate::manager_identity::ManagerIdentity;
use crate::squad_floor::group_index;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::youth_pathway::{Pathway, cover_levels, pathway_of};
use domain::player::{Player, Position, SquadRole};
use serde::Serialize;

/// A youngster from this age has had time to show what he is; younger, the review does not call
/// him a loan case or a lost cause.
const JUDGED_FROM_AGE: i32 = 18;

/// How many rating points below the squad's cover a youngster may be and still gain from regular
/// senior-level football somewhere else rather than from another year of academy football.
const LOAN_REACH: i32 = 8;

/// Points of ceiling a loan has to be able to unlock: a player at his ceiling has nothing left
/// to develop, wherever he plays.
const LOAN_MIN_HEADROOM: i32 = 5;

/// A player is "unlikely" when even his ceiling leaves him this far under the squad's cover.
const UNLIKELY_MARGIN: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    ReadyForSenior,
    KeepDeveloping,
    LoanCandidate,
    UnlikelyToProgress,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcademyRow {
    pub player_id: String,
    pub full_name: String,
    pub age: i32,
    pub natural_position: Position,
    pub ovr: u8,
    /// The ceiling, which the development model may or may not let him reach.
    pub potential: u8,
    /// What the development model expects him to be rated a year on.
    pub expected_ovr_next_year: u8,
    pub verdict: Verdict,
    /// Translation-key suffixes under `academy.reason.`.
    pub reasons: Vec<String>,
    /// The rating of the senior squad's cover in his position group: the bar he is measured on.
    pub squad_cover: u8,
    pub seasons_left_in_academy: i32,
    pub injured: bool,
    pub on_loan: bool,
    pub loan_listed: bool,
    pub contract_end: Option<String>,
    pub weekly_wage: u32,
    /// Whether a senior place is free for him today; when not, the key says why.
    pub promotion_blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcademyReview {
    pub rows: Vec<AcademyRow>,
    pub seniors: usize,
    pub senior_limit: usize,
    /// Academy players the club holds per position group, `[GK, DEF, MID, FWD]`, against the
    /// academy it aims for.
    pub by_group: [usize; 4],
    pub target_by_group: [usize; 4],
    /// Youngsters the next season-end intake will bring in, by the same plan the intake uses.
    pub next_intake: usize,
}

/// The refusal (a translation key) when `team_id` has no senior place for a promoted youngster:
/// the same rule every signing route answers to, including the places promised to deals waiting
/// to register and to seniors out on loan. `None` when he can be registered.
pub fn promotion_place_refusal(game: &Game, team_id: &str) -> Option<String> {
    crate::transfers::signing_place_refusal(game, team_id, "")
}

/// The review of the manager's own academy; `None` when the manager has no club.
pub fn academy_review(game: &Game) -> Option<AcademyReview> {
    let team_id = game.manager.team_id.as_deref()?;
    let team = game.teams.iter().find(|t| t.id == team_id)?;
    let today = game.clock.current_date.date_naive();
    let club: Vec<usize> = game
        .players
        .iter()
        .enumerate()
        .filter(|(_, p)| p.team_id.as_deref() == Some(team_id) || p.contract_club_id() == Some(team_id))
        .map(|(i, _)| i)
        .collect();
    let covers = cover_levels(game, &club);
    let squad = ClubSquad::of_for_planning(game, team_id, Appetite::NEUTRAL);
    let youth_trust = ManagerIdentity::for_club(game, Some(team)).youth_trust;
    let place = crate::transfers::signing_place_refusal(game, team_id, "");
    let capacity = crate::contracts::squad_capacity(game, team_id);

    let academy: Vec<&Player> = club
        .iter()
        .map(|&i| &game.players[i])
        .filter(|p| p.squad_role == SquadRole::Youth && !p.retired && p.contract_club_id() == Some(team_id))
        .collect();
    let mut rows: Vec<AcademyRow> = academy
        .iter()
        .map(|p| row_of(p, today, &covers, squad.as_ref(), youth_trust, place.as_deref()))
        .collect();
    rows.sort_by(|a, b| {
        verdict_rank(a.verdict)
            .cmp(&verdict_rank(b.verdict))
            .then(b.potential.cmp(&a.potential))
            .then(a.player_id.cmp(&b.player_id))
    });

    let mut by_group = [0usize; 4];
    for p in &academy {
        by_group[group_index(&p.position)] += 1;
    }
    Some(AcademyReview {
        rows,
        seniors: capacity.registered_seniors,
        senior_limit: capacity.senior_limit,
        by_group,
        target_by_group: crate::youth_intake::ACADEMY_TARGET_PER_GROUP,
        next_intake: crate::youth_intake::plan_for(academy.iter().copied()).groups.len(),
    })
}

fn verdict_rank(verdict: Verdict) -> u8 {
    match verdict {
        Verdict::ReadyForSenior => 0,
        Verdict::LoanCandidate => 1,
        Verdict::KeepDeveloping => 2,
        Verdict::UnlikelyToProgress => 3,
    }
}

fn row_of(
    player: &Player,
    today: chrono::NaiveDate,
    covers: &[u8; 4],
    squad: Option<&ClubSquad>,
    youth_trust: f64,
    place_refusal: Option<&str>,
) -> AcademyRow {
    let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
    let group = group_index(&player.position);
    let cover = covers[group];
    let expected = crate::squad_outlook::expected_ovr_for(player, today);
    let pathway = squad.and_then(|squad| {
        let mut senior = player.clone();
        senior.squad_role = SquadRole::Senior;
        squad
            .signing(&senior, 0.0)
            .map(|marginal| pathway_of(&marginal, age, youth_trust))
    });
    let (verdict, reasons) = judge(player, age, cover, expected, pathway);
    AcademyRow {
        player_id: player.id.clone(),
        full_name: player.full_name.clone(),
        age,
        natural_position: player.natural_position.clone(),
        ovr: player.ovr,
        potential: player.potential,
        expected_ovr_next_year: expected,
        verdict,
        reasons,
        squad_cover: cover,
        seasons_left_in_academy: (crate::roster::YOUTH_ACADEMY_MAX_AGE - age + 1).max(0),
        injured: player.injury.is_some(),
        on_loan: player.active_loan.is_some(),
        loan_listed: player.loan_listed,
        contract_end: player.contract_end().map(str::to_string),
        weekly_wage: player.wage(),
        promotion_blocked: place_refusal.map(str::to_string),
    }
}

/// The verdict and the reasons behind it. Ordered: a player the squad can use now is ready
/// whatever his age; then the cases that need a judgement of his future, which only a player old
/// enough to have shown it gets.
fn judge(
    player: &Player,
    age: i32,
    cover: u8,
    expected: u8,
    pathway: Option<Pathway>,
) -> (Verdict, Vec<String>) {
    let ovr = i32::from(player.ovr);
    let ceiling = i32::from(player.potential.max(player.ovr));
    let cover = i32::from(cover);
    let mut reasons = Vec::new();

    match pathway {
        Some(Pathway::Starter) => {
            reasons.push("wouldStart".to_string());
            return (Verdict::ReadyForSenior, reasons);
        }
        Some(Pathway::Cover) => {
            reasons.push("addsCover".to_string());
            return (Verdict::ReadyForSenior, reasons);
        }
        Some(Pathway::Succession) => reasons.push("succession".to_string()),
        Some(Pathway::NotYet) | None => {}
    }
    if player.injury.is_some() {
        reasons.push("injured".to_string());
    }
    if age < JUDGED_FROM_AGE {
        reasons.push("tooYoungToJudge".to_string());
        return (Verdict::KeepDeveloping, reasons);
    }
    if ceiling + UNLIKELY_MARGIN < cover {
        reasons.push("ceilingBelowCover".to_string());
        return (Verdict::UnlikelyToProgress, reasons);
    }
    let headroom = ceiling - ovr;
    if player.active_loan.is_none()
        && player.injury.is_none()
        && headroom >= LOAN_MIN_HEADROOM
        && ovr + LOAN_REACH >= cover
    {
        reasons.push("closeToSeniorLevel".to_string());
        reasons.push("needsMatchMinutes".to_string());
        return (Verdict::LoanCandidate, reasons);
    }
    if i32::from(expected) > ovr {
        reasons.push("stillImproving".to_string());
    } else {
        reasons.push("nearCeiling".to_string());
    }
    (Verdict::KeepDeveloping, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::PlayerAttributes;

    fn youth(ovr: u8, potential: u8) -> Player {
        let attrs: PlayerAttributes = serde_json::from_value(serde_json::json!({
            "pace": 50, "stamina": 50, "strength": 50, "agility": 50, "passing": 50,
            "shooting": 50, "tackling": 50, "dribbling": 50, "defending": 50,
            "positioning": 50, "vision": 50, "decisions": 50, "composure": 50,
            "aggression": 50, "teamwork": 50, "leadership": 50,
            "handling": 50, "reflexes": 50, "aerial": 50
        }))
        .unwrap();
        let mut p = Player::new(
            "y".into(),
            "Y".into(),
            "Y Y".into(),
            "2006-01-01".into(),
            "GB".into(),
            Position::Midfielder,
            attrs,
        );
        p.squad_role = SquadRole::Youth;
        p.ovr = ovr;
        p.potential = potential;
        p
    }

    /// Given the squad model says a youngster would start, or add cover
    /// When he is judged
    /// Then he is ready, whatever his age, and the reason says which.
    #[test]
    fn a_player_the_squad_can_use_is_ready_at_any_age() {
        let (verdict, reasons) = judge(&youth(70, 75), 16, 65, 71, Some(Pathway::Starter));
        assert_eq!(verdict, Verdict::ReadyForSenior);
        assert_eq!(reasons, vec!["wouldStart"]);
        assert_eq!(
            judge(&youth(60, 70), 17, 62, 62, Some(Pathway::Cover)).0,
            Verdict::ReadyForSenior
        );
    }

    /// Given a sixteen-year-old far below the cover
    /// When he is judged
    /// Then the review does not write him off: too young to judge.
    #[test]
    fn a_child_is_never_called_a_lost_cause() {
        let (verdict, reasons) = judge(&youth(35, 45), 16, 70, 38, Some(Pathway::NotYet));
        assert_eq!(verdict, Verdict::KeepDeveloping);
        assert!(reasons.contains(&"tooYoungToJudge".to_string()));
    }

    /// Given a nineteen-year-old whose ceiling is below the cover he would have to beat
    /// When he is judged
    /// Then he is unlikely to progress, with the reason; and the same player with a ceiling at the
    ///      cover is not.
    #[test]
    fn a_ceiling_below_the_cover_is_unlikely_to_progress() {
        let (verdict, reasons) = judge(&youth(52, 60), 19, 68, 55, Some(Pathway::NotYet));
        assert_eq!(verdict, Verdict::UnlikelyToProgress);
        assert!(reasons.contains(&"ceilingBelowCover".to_string()));
        assert_ne!(
            judge(&youth(52, 68), 19, 68, 55, Some(Pathway::NotYet)).0,
            Verdict::UnlikelyToProgress
        );
    }

    /// Given a nineteen-year-old close to the cover with room to grow
    /// When he is judged
    /// Then he is a loan candidate; injured, or already out on loan, or at his ceiling, he is not.
    #[test]
    fn a_player_close_to_senior_level_with_room_to_grow_is_a_loan_candidate() {
        assert_eq!(
            judge(&youth(62, 74), 19, 66, 65, Some(Pathway::NotYet)).0,
            Verdict::LoanCandidate
        );
        let mut hurt = youth(62, 74);
        hurt.injury = Some(domain::player::Injury {
            name: "x".into(),
            days_remaining: 10,
        });
        assert_eq!(judge(&hurt, 19, 66, 65, None).0, Verdict::KeepDeveloping);
        assert_eq!(
            judge(&youth(62, 64), 19, 66, 63, None).0,
            Verdict::KeepDeveloping,
            "no headroom: a loan unlocks nothing"
        );
    }

    /// Given a youngster far below the cover but with a high ceiling
    /// When he is judged
    /// Then he keeps developing in the academy: potential alone does not make a loan case.
    #[test]
    fn a_distant_prospect_stays_in_the_academy() {
        let (verdict, reasons) = judge(&youth(45, 80), 18, 70, 52, Some(Pathway::NotYet));
        assert_eq!(verdict, Verdict::KeepDeveloping);
        assert!(reasons.contains(&"stillImproving".to_string()));
    }
}
