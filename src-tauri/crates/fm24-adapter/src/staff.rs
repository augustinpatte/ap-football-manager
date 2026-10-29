//! FM staff -> OFM staff (4 roles, 4 attributes) and head coaches -> OFM managers.
//!
//! OFM models an assistant manager, coaches (with a specialisation), scouts and physios. FM roles
//! with no OFM counterpart (directors, analysts, owners...) are not imported -- they stay in
//! world.db and are counted in the report. No new staff system is created here.

use crate::config::Config;
use crate::ids;
use crate::journal::Journal;
use crate::refs::Refs;
use crate::source::StaffRow;
use domain::staff::{CoachingSpecialization, StaffAttributes, StaffDetail, StaffPost, StaffRole};
use ofm_core::generator::StaffDef;

/// What an FM role becomes in OFM.
#[derive(Debug, Clone, PartialEq)]
pub enum Mapping {
    Staff(StaffRole, Option<Spec>),
    /// Head coach: becomes an OFM manager, not a staff member.
    Manager,
    /// No OFM counterpart: kept in world.db, not imported.
    Excluded,
}

/// Coaching specialisation, or "the dominant training attribute" for a generic coach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spec {
    Fixed(SpecializationKind),
    Dominant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecializationKind {
    Fitness,
    Tactics,
    GoalKeeping,
    Youth,
}

pub fn map_role(role_code: &str) -> Mapping {
    use SpecializationKind as K;
    match role_code {
        "entraineur_principal" => Mapping::Manager,
        "adjoint" => Mapping::Staff(StaffRole::AssistantManager, None),
        "entraineur" => Mapping::Staff(StaffRole::Coach, Some(Spec::Dominant)),
        "prep_physique" => Mapping::Staff(StaffRole::Coach, Some(Spec::Fixed(K::Fitness))),
        "entraineur_gardiens" => {
            Mapping::Staff(StaffRole::Coach, Some(Spec::Fixed(K::GoalKeeping)))
        }
        "entraineur_jeunes" => Mapping::Staff(StaffRole::Coach, Some(Spec::Fixed(K::Youth))),
        // Set-piece coach: closest OFM focus is tactics (documented approximation).
        "entraineur_cpa" => Mapping::Staff(StaffRole::Coach, Some(Spec::Fixed(K::Tactics))),
        "recruteur" | "chef_recruteur" => Mapping::Staff(StaffRole::Scout, None),
        // Analysts and the head of youth fill their own seats; `post_for` names them.
        "analyste_recrutement"
        | "resp_analyse_recrutement"
        | "analyste_performance"
        | "resp_analyse_performance" => Mapping::Staff(StaffRole::Scout, None),
        "resp_formation" => Mapping::Staff(StaffRole::Coach, Some(Spec::Fixed(K::Youth))),
        "kine" | "kine_chef" | "medecin" | "resp_medical" => {
            Mapping::Staff(StaffRole::Physio, None)
        }
        _ => Mapping::Excluded,
    }
}

/// The seat an FM role fills when it is finer than the OFM role and specialisation say.
pub fn post_for(role_code: &str) -> Option<StaffPost> {
    match role_code {
        "chef_recruteur" => Some(StaffPost::HeadOfScouting),
        "kine_chef" | "resp_medical" => Some(StaffPost::HeadOfMedical),
        "resp_formation" => Some(StaffPost::HeadOfYouth),
        "analyste_recrutement"
        | "resp_analyse_recrutement"
        | "analyste_performance"
        | "resp_analyse_performance" => Some(StaffPost::Analyst),
        _ => None,
    }
}

/// The FM24 training attributes the save gave, kept beside the four OFM attributes they were
/// folded into. A value the save did not give stays 0 ("unknown"), never a made-up skill.
pub fn detail(row: &StaffRow) -> StaffDetail {
    let get = |key: &str| clamp(raw(row, key)).min(99) * u8::from(row.attrs.contains_key(key));
    StaffDetail {
        attacking: get("attacking"),
        defending: get("defending"),
        technical: get("technical"),
        tactical: get("tactical"),
        mental: get("mental"),
        fitness: get("fitness"),
        goalkeeping: {
            let keys = ["gk_handling", "gk_shot_stopping", "gk_distribution"];
            if keys.iter().all(|k| row.attrs.contains_key(*k)) {
                clamp(keys.iter().map(|k| raw(row, k)).sum::<f64>() / 3.0)
            } else {
                0
            }
        },
        determination: get("determination"),
        motivating: get("motivating"),
        discipline: get("level_of_discipline"),
        analysing: get("analysing_data"),
        sports_science: get("sports_science"),
    }
}

fn raw(row: &StaffRow, key: &str) -> f64 {
    row.attrs.get(key).copied().unwrap_or(0) as f64
}

fn clamp(value: f64) -> u8 {
    value.round().clamp(1.0, 99.0) as u8
}

const TRAINING: [&str; 5] = ["attacking", "defending", "technical", "tactical", "fitness"];

fn dominant_specialization(row: &StaffRow) -> CoachingSpecialization {
    // Ties go to the first in this fixed order.
    let mut best = ("attacking", -1.0);
    for key in TRAINING {
        let value = raw(row, key);
        if value > best.1 {
            best = (key, value);
        }
    }
    match best.0 {
        "attacking" => CoachingSpecialization::Attacking,
        "defending" => CoachingSpecialization::Defending,
        "technical" => CoachingSpecialization::Technique,
        "tactical" => CoachingSpecialization::Tactics,
        _ => CoachingSpecialization::Fitness,
    }
}

fn specialization(row: &StaffRow, spec: Spec) -> CoachingSpecialization {
    match spec {
        Spec::Dominant => dominant_specialization(row),
        Spec::Fixed(SpecializationKind::Fitness) => CoachingSpecialization::Fitness,
        Spec::Fixed(SpecializationKind::Tactics) => CoachingSpecialization::Tactics,
        Spec::Fixed(SpecializationKind::GoalKeeping) => CoachingSpecialization::GoalKeeping,
        Spec::Fixed(SpecializationKind::Youth) => CoachingSpecialization::Youth,
    }
}

/// The four OFM staff attributes from FM's staff attributes (0-100 raw, same scale).
pub fn attributes(
    row: &StaffRow,
    role: &StaffRole,
    spec: Option<CoachingSpecialization>,
) -> StaffAttributes {
    let general = (raw(row, "attacking")
        + raw(row, "defending")
        + raw(row, "fitness")
        + raw(row, "mental")
        + raw(row, "tactical")
        + raw(row, "technical"))
        / 6.0;
    let coaching = match (role, spec) {
        (StaffRole::Coach, Some(CoachingSpecialization::GoalKeeping)) => {
            (raw(row, "gk_handling") + raw(row, "gk_shot_stopping") + raw(row, "gk_distribution"))
                / 3.0
        }
        (StaffRole::Coach, Some(CoachingSpecialization::Fitness)) => {
            0.7 * raw(row, "fitness") + 0.3 * raw(row, "sports_science")
        }
        _ => general,
    };
    let physiotherapy = 0.7 * raw(row, "physiotherapy") + 0.3 * raw(row, "sports_science");
    StaffAttributes {
        coaching: clamp(coaching),
        judging_ability: clamp(raw(row, "judging_player_ability")),
        judging_potential: clamp(raw(row, "judging_player_potential")),
        physiotherapy: clamp(physiotherapy),
    }
}

/// Builds the OFM staff definitions. Returns the staff and the head-coach candidates per club
/// (all `entraineur_principal` rows; selection happens in [`pick_managers`]).
pub fn build(
    rows: &[StaffRow],
    club_country: &dyn Fn(i64) -> Option<String>,
    refs: &Refs,
    journal: &mut Journal,
) -> Vec<(StaffDef, i64)> {
    let mut staff = Vec::new();
    let mut excluded: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for row in rows {
        match map_role(&row.role_code) {
            Mapping::Manager => {}
            Mapping::Excluded => {
                *excluded.entry(row.role_code.as_str()).or_default() += 1;
            }
            Mapping::Staff(role, spec) => {
                let specialization = spec.map(|s| specialization(row, s));
                let id = ids::staff(row.person_id);
                let nationality = match row.nation.as_deref().and_then(|n| refs.nation_code(n)) {
                    Some(code) => code,
                    None => {
                        journal.warn(
                            "staff.nation_fallback",
                            &id,
                            "no usable nationality in the save; the club's country is used",
                        );
                        club_country(row.club_id).unwrap_or_default()
                    }
                };
                if row.birth_date.is_none() {
                    journal.warn(
                        "staff.birth_missing",
                        &id,
                        "no birth date in the save; OFM draws an age",
                    );
                }
                let known_as = row.common_name.clone().filter(|name| {
                    !name.trim().is_empty()
                        && *name != format!("{} {}", row.first_name, row.last_name)
                });
                staff.push((
                    StaffDef {
                        id,
                        first_name: row.first_name.clone(),
                        last_name: row.last_name.clone(),
                        club: ids::club(row.club_id),
                        nationality,
                        role: role.clone(),
                        attributes: Some(attributes(row, &role, specialization.clone())),
                        specialization,
                        date_of_birth: row.birth_date.clone(),
                        age: None,
                        known_as,
                        post: post_for(&row.role_code),
                        detail: Some(detail(row)),
                    },
                    row.person_id,
                ));
            }
        }
    }
    for (code, count) in excluded {
        journal.info(
            "staff.role_not_imported",
            code,
            format!("{count} contract(s) with FM role `{code}` have no OFM counterpart; kept in world.db only"),
        );
    }
    staff
}

/// One manager per club: the first-team head coach (team slot 0, or no team) with the highest
/// reputation; ties go to the lower person id. Clubs with none are returned in `missing`.
pub fn pick_managers<'a>(rows: &'a [StaffRow], club_ids: &[i64]) -> (Vec<&'a StaffRow>, Vec<i64>) {
    let mut chosen = Vec::new();
    let mut missing = Vec::new();
    for &club in club_ids {
        let best = rows
            .iter()
            .filter(|r| r.club_id == club && r.role_code == "entraineur_principal")
            .filter(|r| r.team_slot.is_none_or(|slot| slot == 0))
            .max_by(|a, b| {
                a.rep_current
                    .unwrap_or(0)
                    .cmp(&b.rep_current.unwrap_or(0))
                    .then(b.person_id.cmp(&a.person_id))
            });
        match best {
            Some(row) => chosen.push(row),
            None => missing.push(club),
        }
    }
    (chosen, missing)
}

/// OFM manager reputation (their scale is 0-1000, generated managers sit in 200-900) from FM's 0-10000.
pub fn manager_reputation(rep_current: Option<i64>) -> u32 {
    (rep_current.unwrap_or(2_000) / 10).clamp(50, 950) as u32
}

/// Weekly wage in euros and contract end, applied after the world is built (`StaffDef` has neither).
pub fn contract(row: &StaffRow, cfg: &Config) -> (u32, Option<String>) {
    (
        u32::try_from(cfg.to_eur(row.wage_gbp_week).max(0)).unwrap_or(u32::MAX),
        row.end_date.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn row(role: &str, attrs: &[(&str, i64)]) -> StaffRow {
        StaffRow {
            person_id: 1,
            fm_uid: None,
            fm_uid_status: "unique".into(),
            first_name: "A".into(),
            last_name: "B".into(),
            common_name: None,
            birth_date: Some("1980-01-01".into()),
            nation: Some("France".into()),
            club_id: 10,
            team_slot: Some(0),
            role_code: role.into(),
            wage_gbp_week: 1000,
            end_date: Some("2028-06-30".into()),
            ca: None,
            pa: None,
            rep_current: Some(5000),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn roles_map_to_the_four_ofm_roles() {
        assert_eq!(
            map_role("adjoint"),
            Mapping::Staff(StaffRole::AssistantManager, None)
        );
        assert!(matches!(
            map_role("recruteur"),
            Mapping::Staff(StaffRole::Scout, None)
        ));
        assert!(matches!(
            map_role("kine"),
            Mapping::Staff(StaffRole::Physio, None)
        ));
        assert!(matches!(
            map_role("entraineur_gardiens"),
            Mapping::Staff(StaffRole::Coach, Some(_))
        ));
        assert_eq!(map_role("entraineur_principal"), Mapping::Manager);
        assert_eq!(map_role("president"), Mapping::Excluded);
        // Analysts and the head of youth now fill their own seats.
        assert_eq!(
            map_role("analyste_performance"),
            Mapping::Staff(StaffRole::Scout, None)
        );
        assert_eq!(
            post_for("analyste_performance"),
            Some(StaffPost::Analyst)
        );
        assert_eq!(post_for("resp_formation"), Some(StaffPost::HeadOfYouth));
        assert_eq!(post_for("kine"), None);
    }

    #[test]
    fn detail_keeps_what_the_save_gave_and_leaves_the_rest_unknown_across_clubs() {
        // Three clubs, three profiles: full, partial, and none of the detailed attributes.
        let full = row(
            "entraineur",
            &[
                ("attacking", 70),
                ("defending", 55),
                ("technical", 62),
                ("tactical", 48),
                ("mental", 51),
                ("fitness", 80),
                ("gk_handling", 30),
                ("gk_shot_stopping", 60),
                ("gk_distribution", 90),
                ("determination", 66),
                ("motivating", 71),
                ("level_of_discipline", 44),
            ],
        );
        let d = detail(&full);
        assert_eq!((d.attacking, d.fitness, d.determination), (70, 80, 66));
        assert_eq!(d.goalkeeping, 60);
        // Not in the row: unknown (0), never a default skill.
        assert_eq!((d.analysing, d.sports_science), (0, 0));
        assert_eq!(StaffDetail::known(d.analysing), None);

        // Only one of the three goalkeeping attributes: the average is not guessed.
        let partial = row("kine", &[("sports_science", 77), ("gk_handling", 40)]);
        let d = detail(&partial);
        assert_eq!(d.sports_science, 77);
        assert_eq!(d.goalkeeping, 0);
        assert_eq!(d.attacking, 0);

        let none = row("resp_formation", &[]);
        assert_eq!(detail(&none), StaffDetail::default());

        // Seats: the analysts and the head of youth keep their own, the rest derive theirs.
        for (club, code, post) in [
            (1, "resp_formation", Some(StaffPost::HeadOfYouth)),
            (2, "analyste_recrutement", Some(StaffPost::Analyst)),
            (3, "chef_recruteur", Some(StaffPost::HeadOfScouting)),
            (3, "kine_chef", Some(StaffPost::HeadOfMedical)),
            (3, "adjoint", None),
        ] {
            let mut r = row(code, &[]);
            r.club_id = club;
            assert_eq!(post_for(&r.role_code), post, "club {club} role {code}");
        }
    }

    #[test]
    fn a_generic_coach_specialises_in_his_best_training_attribute() {
        let r = row(
            "entraineur",
            &[
                ("attacking", 40),
                ("defending", 90),
                ("technical", 60),
                ("tactical", 50),
                ("fitness", 30),
            ],
        );
        assert_eq!(
            dominant_specialization(&r),
            CoachingSpecialization::Defending
        );
    }

    #[test]
    fn a_goalkeeping_coach_is_rated_on_goalkeeping() {
        let r = row(
            "entraineur_gardiens",
            &[
                ("gk_handling", 90),
                ("gk_shot_stopping", 80),
                ("gk_distribution", 70),
                ("attacking", 10),
            ],
        );
        let a = attributes(
            &r,
            &StaffRole::Coach,
            Some(CoachingSpecialization::GoalKeeping),
        );
        assert_eq!(a.coaching, 80);
    }

    #[test]
    fn attributes_are_bounded() {
        let r = row(
            "kine",
            &[("physiotherapy", 0), ("judging_player_ability", 100)],
        );
        let a = attributes(&r, &StaffRole::Physio, None);
        assert_eq!(a.physiotherapy, 1);
        assert_eq!(a.judging_ability, 99);
    }

    #[test]
    fn the_first_team_head_coach_with_the_highest_reputation_becomes_the_manager() {
        let mut first = row("entraineur_principal", &[]);
        first.person_id = 5;
        first.rep_current = Some(8000);
        let mut youth = row("entraineur_principal", &[]);
        youth.person_id = 6;
        youth.team_slot = Some(2);
        youth.rep_current = Some(9000);
        let rows = vec![first, youth];
        let (chosen, missing) = pick_managers(&rows, &[10, 11]);
        assert_eq!(chosen.len(), 1);
        assert_eq!(
            chosen[0].person_id, 5,
            "a youth-team head coach is never the club's manager"
        );
        assert_eq!(missing, vec![11]);
    }

    #[test]
    fn manager_reputation_is_on_ofms_scale() {
        assert_eq!(manager_reputation(Some(8598)), 859);
        assert_eq!(manager_reputation(Some(10)), 50);
        assert_eq!(manager_reputation(None), 200);
    }
}
