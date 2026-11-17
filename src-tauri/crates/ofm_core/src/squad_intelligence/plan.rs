//! The squad plan: what a club is short of, how pressing it is, and why.
//!
//! [`super::needs`] answers *where* the side is weaker than the club's own standard, in expected
//! rating points. A club's staff ask a second question before they spend: *which of those places
//! matters now, which matters next season, and which can wait?* This module turns the needs into a
//! ranked plan:
//!
//! * **Urgent** — the side the club fields is clearly worse for it, a first choice is out for
//!   long, or a first choice's contract is about to end with nobody ready behind him;
//! * **Important** — a real weakness, a thin cover, or a succession the club should arrange;
//! * **Optional** — ordinary variation, worth a cheap improvement and not a fee.
//!
//! The urgency is not a fixed threshold per kind of need. The same gap is more pressing to a club
//! that counts the present (a contender) than to one that plans two years ahead (a developing
//! club), and less pressing to a club that asks a lot of strength for its money. The tilt comes
//! from the club's own [`Appetite`], derived from its strategy profile, so clubs in different
//! situations read the same squad differently for reasons that are in the game state.
//!
//! Nothing here is stored: the plan is a pure function of the squad, its formation and the date,
//! rebuilt whenever one of them changes. It decides nothing by itself; the market, the free-agent
//! review and the sales review read it and apply their own rules.

use domain::player::Position;

use super::appetite::Appetite;
use super::model::{Evaluation, Member, SLOTS, SquadModel, YearView, group_of};
use super::needs::{Need, NeedKind};

/// Days left on a first choice's contract that make the club plan his place. The renewal review's
/// own horizon: a player still unsigned this close to the end may leave.
const CONTRACT_ALERT_DAYS: i64 = crate::ai_contracts::RENEWAL_HORIZON_DAYS;

/// At this many days or fewer the contract is urgent: the window and the season are nearly over.
const CONTRACT_URGENT_DAYS: i64 = 60;

/// Expected rating points a weak starting slot must cost before it is urgent, before the club's
/// own horizon tilt is applied.
const URGENT_STARTER: f32 = 8.0;
/// The same for a weak cover.
const URGENT_COVER: f32 = 7.0;
/// The same for a succession: a first choice who will be gone or clearly worse.
const URGENT_SUCCESSION: f32 = 3.5;
/// A need under this share of the urgent bar is ordinary variation, worth a cheap improvement only.
const IMPORTANT_SHARE: f32 = 0.4;
/// The floor under a long-term absence: a first choice out for weeks is a need even when his deputy
/// is rated close to him.
const LONG_ABSENCE_SEVERITY: f32 = 4.0;
/// A first choice's contract running out is a planning need even when the model expects no decline
/// yet: the place will have to be filled.
const CONTRACT_SEVERITY: f32 = 4.0;

/// How pressing a need is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Urgency {
    Optional,
    Important,
    Urgent,
}

impl Urgency {
    /// Whether the club should spend a fee on this place, not only a cheap improvement.
    pub(crate) fn is_pressing(self) -> bool {
        self >= Self::Important
    }
}

/// Why the plan holds a need: the football reason, not only the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NeedReason {
    /// A first choice clearly below the level of the rest of the eleven.
    WeakStarter,
    /// A first choice fielded outside his natural position group.
    OutOfPosition,
    /// The replacement when the first choice is absent is far below the standard.
    WeakCover,
    /// A first choice is out for long enough that the club cannot field him now.
    LongTermInjury,
    /// A first choice is ageing and will be worse or gone within the horizon.
    AgeingStarter,
    /// A first choice's contract ends within the renewal horizon and no successor is ready.
    ContractExpiry,
}

impl NeedReason {
    /// The reason as a test or a report names it.
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::WeakStarter => "weak_starter",
            Self::OutOfPosition => "out_of_position",
            Self::WeakCover => "weak_cover",
            Self::LongTermInjury => "long_term_injury",
            Self::AgeingStarter => "ageing_starter",
            Self::ContractExpiry => "contract_expiry",
        }
    }
}

/// One place the club should strengthen, and how pressing it is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanNeed {
    pub kind: NeedKind,
    pub urgency: Urgency,
    pub reason: NeedReason,
    pub slot: usize,
    pub position: Position,
    /// Broad group, 0 goalkeeper to 3 forward.
    pub group: usize,
    /// Expected rating points the side loses for it.
    pub severity: f32,
    /// The rating a signing for this place has to beat to be an improvement: the first choice for a
    /// weak starter, the deputy for weak cover. `None` when no one holds the place.
    pub incumbent: Option<u8>,
    /// The player who holds the place today: the weak first choice, the deputy behind an absent
    /// one, or the expiring first choice. The renewal and sales reviews read it so they do not let
    /// the answer to a pressing place leave. `None` when the place is empty.
    pub incumbent_id: Option<String>,
    /// The need matters to the side the club fields this season, not only in a later horizon.
    pub short_term: bool,
}

impl PlanNeed {
    /// Whether the club must keep the player who holds this place: the best deputy behind a first
    /// choice it cannot field, when the gap is urgent. An ordinary cover is left to the club's own
    /// renewal policy; a weak first choice is not protected at all — the plan wants him replaced,
    /// not renewed.
    pub(crate) fn protects_the_incumbent(&self) -> bool {
        self.urgency == Urgency::Urgent
            && matches!(
                self.reason,
                NeedReason::WeakCover | NeedReason::LongTermInjury
            )
    }

    /// Whether `player_id` is the player who holds this place today.
    pub(crate) fn holds_the_place(&self, player_id: &str) -> bool {
        self.incumbent_id.as_deref() == Some(player_id)
    }
}

/// The club's ranked plan, most pressing first.
pub(crate) fn plan(
    model: &SquadModel,
    evaluation: &Evaluation,
    appetite: &Appetite,
) -> Vec<PlanNeed> {
    let now = &evaluation.years[0];
    let mut planned: Vec<PlanNeed> = Vec::new();
    add_model_needs(&mut planned, model, evaluation, appetite);
    // Tactical: a first choice fielded outside his own position is classified in `reason_of`; there
    // is deliberately no separate pass, so one shortfall is never counted twice.
    add_long_absence_needs(&mut planned, model, now, appetite);
    add_contract_expiry_needs(&mut planned, model, now, appetite);
    rank(&mut planned);
    planned
}

/// The plan's first source: every place `super::needs` reports weaker than the club's standard,
/// with the reason and the bar a signing has to clear.
fn add_model_needs(
    planned: &mut Vec<PlanNeed>,
    model: &SquadModel,
    evaluation: &Evaluation,
    appetite: &Appetite,
) {
    for need in super::needs::needs(model, evaluation) {
        let member = member_in_slot(model, evaluation, &need);
        let (reason, severity) = reason_of(&need, member);
        let urgency = urgency_for(need.kind, severity, appetite);
        let incumbent = incumbent_of(model, evaluation, &need);
        let incumbent_id = member.map(|member| member.id.clone());
        merge(
            planned,
            PlanNeed {
                kind: need.kind,
                urgency,
                reason,
                slot: need.slot,
                position: need.position,
                group: need.group,
                severity,
                incumbent,
                incumbent_id,
                short_term: need.kind != NeedKind::Succession,
            },
        );
    }
}

/// Availability: a first choice out for long is a place the side has to cover now, even when the
/// deputy is rated close to him.
fn add_long_absence_needs(
    planned: &mut Vec<PlanNeed>,
    model: &SquadModel,
    now: &YearView,
    appetite: &Appetite,
) {
    for member in &model.members {
        if !member.long_injured || member.loaned_in {
            continue;
        }
        let Some((slot, level)) = best_slot_after_the_absence(member) else {
            continue;
        };
        let gap = (level - now.starter[slot]).max(0.0);
        let severity = LONG_ABSENCE_SEVERITY + gap;
        let urgency = urgency_for(NeedKind::Cover, severity, appetite);
        let cover = now.cover_index[slot].and_then(|index| model.members.get(index));
        merge(
            planned,
            PlanNeed {
                kind: NeedKind::Cover,
                urgency,
                reason: NeedReason::LongTermInjury,
                slot,
                position: model.slots[slot].clone(),
                group: group_of(&model.slots[slot]),
                severity,
                incumbent: cover.map(|member| member.ovr),
                incumbent_id: cover.map(|member| member.id.clone()),
                short_term: true,
            },
        );
    }
}

/// Contracts: a first choice whose deal ends before the renewal horizon and who has not been
/// renewed is a place the club will have to fill, whether or not he is declining yet.
fn add_contract_expiry_needs(
    planned: &mut Vec<PlanNeed>,
    model: &SquadModel,
    now: &YearView,
    appetite: &Appetite,
) {
    for slot in 0..SLOTS {
        let Some(index) = now.starter_index[slot] else {
            continue;
        };
        let Some(member) = model.members.get(index) else {
            continue;
        };
        if member.loaned_in {
            continue;
        }
        let Some(days) = member.contract_days else {
            continue;
        };
        if days > CONTRACT_ALERT_DAYS {
            continue;
        }
        let days_extra = if days <= CONTRACT_URGENT_DAYS {
            2.0
        } else {
            1.0
        };
        let severity = CONTRACT_SEVERITY + days_extra;
        let mut urgency = urgency_for(NeedKind::Succession, severity, appetite);
        if days <= CONTRACT_URGENT_DAYS {
            urgency = urgency.max(Urgency::Important);
        }
        merge(
            planned,
            PlanNeed {
                kind: NeedKind::Succession,
                urgency,
                reason: NeedReason::ContractExpiry,
                slot,
                position: model.slots[slot].clone(),
                group: group_of(&model.slots[slot]),
                severity,
                incumbent: None,
                incumbent_id: Some(member.id.clone()),
                short_term: false,
            },
        );
    }
}

/// Most pressing first; ties broken on severity, then slot, then kind, so the plan is deterministic.
fn rank(planned: &mut [PlanNeed]) {
    planned.sort_by(|a, b| {
        b.urgency
            .cmp(&a.urgency)
            .then(b.severity.total_cmp(&a.severity))
            .then(a.slot.cmp(&b.slot))
            .then((a.kind as u8).cmp(&(b.kind as u8)))
    });
}

/// How pressing a need of `kind` at `severity` is, for a club that counts its horizon years this
/// way and asks this much strength for its money.
fn urgency_for(kind: NeedKind, severity: f32, appetite: &Appetite) -> Urgency {
    let present = appetite.weights[0].clamp(0.05, 0.95);
    let future = (1.0 - present).max(0.05);
    let urgent = match kind {
        NeedKind::Starter => URGENT_STARTER / (0.5 + present),
        NeedKind::Cover => URGENT_COVER / (0.5 + present),
        NeedKind::Succession => URGENT_SUCCESSION / future,
    };
    // A club that asks a lot of strength for its money is slower to call a gap pressing: the same
    // shortfall is a cheap club's problem before it is a careful club's.
    let urgent = urgent * appetite.price.clamp(0.3, 3.0).sqrt();
    let important = urgent * IMPORTANT_SHARE;
    if severity >= urgent {
        Urgency::Urgent
    } else if severity >= important {
        Urgency::Important
    } else {
        Urgency::Optional
    }
}

/// The member a need is about: the first choice for a starter or succession, the cover for a
/// cover need.
fn member_in_slot<'a>(
    model: &'a SquadModel,
    evaluation: &Evaluation,
    need: &Need,
) -> Option<&'a Member> {
    let now = &evaluation.years[0];
    let index = match need.kind {
        NeedKind::Starter | NeedKind::Succession => now.starter_index[need.slot],
        NeedKind::Cover => now.cover_index[need.slot].or(now.starter_index[need.slot]),
    }?;
    model.members.get(index)
}

/// The level a signing for `need` has to beat at that slot: the first choice's rating where the
/// side actually fields him for a weak starter, the deputy's for weak or absent cover. Measured at
/// the slot, not in the player's own position: a good centre-back at left-back is a left-back of
/// his left-back rating. A succession has no one to beat today; it is judged a year on.
fn incumbent_of(model: &SquadModel, evaluation: &Evaluation, need: &Need) -> Option<u8> {
    let now = &evaluation.years[0];
    let index = match need.kind {
        NeedKind::Starter => now.starter_index[need.slot],
        NeedKind::Cover => now.cover_index[need.slot],
        NeedKind::Succession => None,
    }?;
    model
        .members
        .get(index)
        .map(|member| member.fit[need.slot].round().clamp(0.0, 99.0) as u8)
}

/// The football reason for a need, and its severity after the reason's own weight.
fn reason_of(need: &Need, member: Option<&Member>) -> (NeedReason, f32) {
    match need.kind {
        NeedKind::Starter => {
            // The game already prices the positional penalty into the slot's rating; the plan only
            // names the cause. A shift inside the player's own group (a right-back at left-back)
            // is a weakness of the place, not a player out of position.
            let misplaced = member.is_some_and(|member| member.natural_group != need.group);
            let reason = if misplaced {
                NeedReason::OutOfPosition
            } else {
                NeedReason::WeakStarter
            };
            (reason, need.severity)
        }
        NeedKind::Cover => (NeedReason::WeakCover, need.severity),
        NeedKind::Succession => {
            let expiry = member.is_some_and(|member| {
                member
                    .contract_days
                    .is_some_and(|days| days <= CONTRACT_ALERT_DAYS)
                    && member.shift[1].is_none()
                    && member.age > crate::squad_outlook::peak_age_limit(member.natural_group)
            });
            if expiry {
                let days = member.and_then(|member| member.contract_days).unwrap_or(0);
                let extra = if days <= CONTRACT_URGENT_DAYS {
                    2.0
                } else {
                    1.0
                };
                (
                    NeedReason::ContractExpiry,
                    need.severity.max(CONTRACT_SEVERITY) + extra,
                )
            } else {
                (NeedReason::AgeingStarter, need.severity)
            }
        }
    }
}

/// Where a long-injured player would play, and how good he is there a year on: the place his
/// absence leaves open, judged after the club's own growth and decline rules.
fn best_slot_after_the_absence(member: &Member) -> Option<(usize, f32)> {
    (0..SLOTS)
        .filter_map(|slot| Some((slot, member.level(slot, 1)?)))
        .max_by(|(left_slot, left), (right_slot, right)| {
            left.total_cmp(right)
                .then_with(|| right_slot.cmp(left_slot))
        })
}

/// Put `need` into the plan, keeping the most pressing answer for the same slot and kind.
fn merge(planned: &mut Vec<PlanNeed>, need: PlanNeed) {
    if let Some(existing) = planned
        .iter_mut()
        .find(|existing| existing.slot == need.slot && existing.kind == need.kind)
    {
        if need.severity > existing.severity || need.urgency > existing.urgency {
            let severity = existing.severity.max(need.severity);
            *existing = PlanNeed { severity, ..need };
        }
        return;
    }
    planned.push(need);
}

#[cfg(test)]
mod tests {
    use domain::player::{Injury, Position};

    use super::*;
    use crate::squad_intelligence::appetite::Appetite;
    use crate::squad_intelligence::test_squads::Squad;

    fn neutral() -> Appetite {
        Appetite::NEUTRAL
    }

    fn appetite(weights: [f32; 3], price: f32) -> Appetite {
        Appetite { weights, price }
    }

    fn plan_of(squad: &Squad, appetite: Appetite) -> Vec<PlanNeed> {
        let club = squad.club("4-4-2", 30_000_000, appetite);
        club.strategic_plan()
    }

    fn need_at(plan: &[PlanNeed], slot: usize) -> Option<&PlanNeed> {
        plan.iter().find(|need| need.slot == slot)
    }

    /// Given a club whose striker is 61 and whose other ten are 70 to 75
    /// When its plan is read
    /// Then the striker's slot is the first, urgent need, and it is named as a weak starter.
    #[test]
    fn a_clear_hole_in_the_eleven_is_the_first_urgent_need() {
        let squad = Squad::new()
            .add(Position::Goalkeeper, &[74, 66])
            .add(Position::LeftBack, &[72, 66])
            .add(Position::RightBack, &[72, 66])
            .add(Position::CenterBack, &[74, 72, 68, 66])
            .add(Position::LeftMidfielder, &[72, 66])
            .add(Position::RightMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[74, 72, 68, 66])
            .add(Position::Striker, &[61, 58]);
        let plan = plan_of(&squad, neutral());
        let first = plan.first().expect("a need");
        assert_eq!(first.group, 3, "{first:?}");
        assert_eq!(first.kind, NeedKind::Starter, "{first:?}");
        assert_eq!(first.reason, NeedReason::WeakStarter, "{first:?}");
        assert_eq!(first.urgency, Urgency::Urgent, "{first:?}");
    }

    /// Given a sound squad and a first choice whose contract has three months to run
    /// When the plan is read
    /// Then the place is planned as a contract expiry, not left to the ratings.
    #[test]
    fn an_expiring_first_choice_is_a_planning_need() {
        let mut squad = Squad::balanced();
        let striker = squad
            .players
            .iter_mut()
            .find(|player| player.natural_position == Position::Striker)
            .expect("a striker");
        striker.stage_contract_end(Some("2026-11-01".to_string()));
        let striker_id = striker.id.clone();
        let plan = plan_of(&squad, neutral());
        let expiry = plan
            .iter()
            .find(|need| need.reason == NeedReason::ContractExpiry)
            .expect("a contract-expiry need");
        assert_eq!(expiry.group, 3, "{expiry:?}");
        assert!(expiry.urgency >= Urgency::Important, "{expiry:?}");
        assert!(!expiry.short_term);
        assert!(expiry.holds_the_place(&striker_id), "{expiry:?}");
    }

    /// Given the same squad with the striker under a long contract
    /// When the plan is read
    /// Then there is no contract-expiry need: the case above is about the contract.
    #[test]
    fn a_first_choice_under_a_long_contract_is_not_a_contract_need() {
        let squad = Squad::balanced();
        let plan = plan_of(&squad, neutral());
        assert!(
            plan.iter()
                .all(|need| need.reason != NeedReason::ContractExpiry),
            "{plan:?}"
        );
    }

    /// Given a squad whose 35-year-old striker is its first choice and whose young deputy is far
    /// behind
    /// When the plan is read
    /// Then the striker's succession is in it, named as an ageing starter.
    #[test]
    fn an_ageing_first_choice_creates_a_succession_need() {
        let squad = Squad::new()
            .add(Position::Goalkeeper, &[74, 66])
            .add(Position::LeftBack, &[72, 66])
            .add(Position::RightBack, &[72, 66])
            .add(Position::CenterBack, &[74, 72, 68, 66])
            .add(Position::LeftMidfielder, &[72, 66])
            .add(Position::RightMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[74, 72, 68, 66])
            .add_aged(Position::Striker, &[(80, 35), (58, 20)]);
        let plan = plan_of(&squad, neutral());
        let need = plan
            .iter()
            .find(|need| need.kind == NeedKind::Succession && need.group == 3)
            .expect("a succession need");
        assert!(
            matches!(
                need.reason,
                NeedReason::AgeingStarter | NeedReason::ContractExpiry
            ),
            "{need:?}"
        );
    }

    /// Given a squad whose only good centre-back is out for two months, and a deputy who is not
    /// close to him
    /// When the plan is read
    /// Then the absence is a short-term cover need, named as such.
    #[test]
    fn a_long_term_absence_is_read_as_a_cover_need() {
        let mut squad = Squad::balanced();
        let centre_back_id = {
            let centre_back = squad
                .players
                .iter_mut()
                .find(|player| player.natural_position == Position::CenterBack)
                .expect("a centre-back");
            centre_back.ovr = 84;
            centre_back.injury = Some(Injury {
                name: "Hamstring".to_string(),
                days_remaining: crate::transfers::LONG_INJURY_DAYS + 5,
            });
            centre_back.id.clone()
        };
        let plan = plan_of(&squad, neutral());
        let absence = plan
            .iter()
            .find(|need| need.reason == NeedReason::LongTermInjury)
            .expect("an absence need");
        assert_eq!(absence.kind, NeedKind::Cover, "{absence:?}");
        assert!(absence.short_term, "{absence:?}");
        assert!(absence.urgency >= Urgency::Important, "{absence:?}");
        assert!(absence.protects_the_incumbent(), "{absence:?}");
        assert!(
            absence
                .incumbent_id
                .as_deref()
                .is_some_and(|id| id != centre_back_id),
            "the deputy holds the place, not the man out: {absence:?}"
        );
    }

    /// Given the same gap in the same squad
    /// When it is read by a club that counts the present and by one that counts the future
    /// Then the present-minded club finds the cover more pressing: the urgency follows the club,
    /// not a fixed bar.
    #[test]
    fn urgency_follows_the_club_not_only_the_gap() {
        let weights_now = [0.70, 0.22, 0.08];
        let weights_later = [0.45, 0.33, 0.22];
        assert_eq!(
            urgency_for(NeedKind::Cover, 6.0, &appetite(weights_now, 1.0)),
            Urgency::Urgent
        );
        assert_eq!(
            urgency_for(NeedKind::Cover, 6.0, &appetite(weights_later, 1.0)),
            Urgency::Important
        );
    }

    /// Given the same gap and the same money at two clubs, one eager to spend and one careful
    /// When each reads its plan
    /// Then the eager club finds it urgent and the careful club only important: urgency is a
    /// judgement about the club, not a league-wide bar.
    #[test]
    fn urgency_follows_how_readily_the_club_spends() {
        let weights = [0.55, 0.30, 0.15];
        assert_eq!(
            urgency_for(NeedKind::Cover, 5.5, &appetite(weights, 0.6)),
            Urgency::Urgent
        );
        assert_eq!(
            urgency_for(NeedKind::Cover, 5.5, &appetite(weights, 1.8)),
            Urgency::Important
        );
    }

    /// Given the same weak cover at a wealthy contender and a constrained small club
    /// When each club reads its own plan
    /// Then the same gap is urgent to the one and not to the other: the club's means and ambition,
    /// not a league-wide bar, decide what is worth spending on.
    #[test]
    fn the_same_gap_is_read_differently_by_clubs_with_different_means() {
        use crate::transfers::{Ambition, ClubProfile, Means};

        let contender = Appetite::of(&ClubProfile {
            ambition: Ambition::Contender,
            means: Means::Wealthy,
            youth_focus: 0.0,
        });
        let small = Appetite::of(&ClubProfile {
            ambition: Ambition::Fighter,
            means: Means::Constrained,
            youth_focus: 1.0,
        });
        assert_eq!(
            urgency_for(NeedKind::Cover, 6.0, &contender),
            Urgency::Urgent
        );
        assert!(urgency_for(NeedKind::Cover, 6.0, &small) < Urgency::Urgent);
    }

    /// Given every reason a plan can hold
    /// When they are named
    /// Then each has its own name, so a report can tell them apart.
    #[test]
    fn every_reason_has_its_own_name() {
        let reasons = [
            NeedReason::WeakStarter,
            NeedReason::OutOfPosition,
            NeedReason::WeakCover,
            NeedReason::LongTermInjury,
            NeedReason::AgeingStarter,
            NeedReason::ContractExpiry,
        ];
        for (index, left) in reasons.iter().enumerate() {
            for right in &reasons[index + 1..] {
                assert_ne!(left.name(), right.name());
            }
        }
    }

    /// Given a club whose striker is clearly its weakest place
    /// When a striker good enough to hold the place joins
    /// Then the place is no longer an urgent need: the plan is read from the squad as it is.
    #[test]
    fn a_signing_that_fills_the_place_removes_the_need() {
        let weak = Squad::new()
            .add(Position::Goalkeeper, &[74, 66])
            .add(Position::LeftBack, &[72, 66])
            .add(Position::RightBack, &[72, 66])
            .add(Position::CenterBack, &[74, 72, 68, 66])
            .add(Position::LeftMidfielder, &[72, 66])
            .add(Position::RightMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[74, 72, 68, 66])
            .add(Position::Striker, &[61, 58]);
        let before = plan_of(&weak, neutral());
        assert!(
            before
                .iter()
                .any(|need| need.group == 3 && need.kind == NeedKind::Starter),
            "{before:?}"
        );

        let filled = Squad::new()
            .add(Position::Goalkeeper, &[74, 66])
            .add(Position::LeftBack, &[72, 66])
            .add(Position::RightBack, &[72, 66])
            .add(Position::CenterBack, &[74, 72, 68, 66])
            .add(Position::LeftMidfielder, &[72, 66])
            .add(Position::RightMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[74, 72, 68, 66])
            .add(Position::Striker, &[78, 76]);
        let after = plan_of(&filled, neutral());
        let striker = after
            .iter()
            .find(|need| need.group == 3 && need.kind == NeedKind::Starter);
        assert!(
            striker.is_none_or(|need| need.urgency == Urgency::Optional),
            "{after:?}"
        );
    }

    /// Given the same moderate weak place
    /// When an eager club and a careful one read it
    /// Then it is the more pressing need at the eager club: two clubs in the same division plan
    /// differently from the same squad.
    #[test]
    fn two_clubs_read_the_same_squad_differently() {
        use crate::transfers::{Ambition, ClubProfile, Means};

        let squad = Squad::new()
            .add(Position::Goalkeeper, &[72, 66])
            .add(Position::LeftBack, &[72, 66])
            .add(Position::RightBack, &[72, 66])
            .add(Position::CenterBack, &[72, 70, 68, 66])
            .add(Position::LeftMidfielder, &[72, 66])
            .add(Position::RightMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[72, 70, 68, 66])
            .add(Position::Striker, &[68, 66]);
        let eager = Appetite::of(&ClubProfile {
            ambition: Ambition::Contender,
            means: Means::Wealthy,
            youth_focus: 0.0,
        });
        let careful = Appetite::of(&ClubProfile {
            ambition: Ambition::Fighter,
            means: Means::Constrained,
            youth_focus: 1.0,
        });
        let urgency = |appetite| {
            plan_of(&squad, appetite)
                .iter()
                .find(|need| need.group == 3 && need.kind == NeedKind::Starter)
                .map(|need| need.urgency)
                .expect("a striker need")
        };
        assert!(urgency(eager) > urgency(careful));
    }

    /// Given the same squad read twice
    /// When the plan is made both times
    /// Then it is identical: the plan is a pure function of the state.
    #[test]
    fn the_plan_is_deterministic() {
        let squad = Squad::balanced();
        assert_eq!(plan_of(&squad, neutral()), plan_of(&squad, neutral()));
    }

    /// Given a formation and a squad in which a natural winger is the best left-sided midfielder
    /// available
    /// When the plan is read
    /// Then the out-of-position starter is called out as a tactical need.
    #[test]
    fn a_starter_out_of_his_natural_group_is_a_tactical_need() {
        let squad = Squad::new()
            .add(Position::Goalkeeper, &[74, 66])
            .add(Position::RightBack, &[72, 66])
            .add(Position::CenterBack, &[74, 72, 68, 66])
            .add(Position::LeftWinger, &[88, 60])
            .add(Position::RightMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[64, 62, 60, 58])
            .add(Position::Striker, &[75, 73, 67]);
        let club = squad.club("4-4-2", 30_000_000, neutral());
        let plan = club.strategic_plan();
        assert!(
            plan.iter()
                .any(|need| need.reason == NeedReason::OutOfPosition),
            "{plan:?}"
        );
        // The need is at left midfield, where the winger has to play.
        let left_mid = need_at(&plan, 5).expect("a left-midfield need");
        assert_eq!(left_mid.reason, NeedReason::OutOfPosition, "{left_mid:?}");
    }
}
