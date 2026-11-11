//! Where a player of the save plays, and for whom.
//!
//! World.db keeps two facts per player: the club that holds his **contract** (`contract.club_id`)
//! and the **team he is registered with** (`player.registered_team_id`: a club and one of its
//! teams, slot 0 being the first team and higher slots the reserve and youth sides). They differ
//! for 4 842 players of the save: loans, and players of an affiliated side (Bayern II, Real Madrid
//! Castilla) who play for another club under their owner's contract. The first importer read the
//! contract only, so every loanee stayed in his owner's first team and every reserve and youth
//! player of the owner was listed as a senior.
//!
//! The rules (documented in `docs/APFM_FM24_ROSTER_FIXES.md`, checked independently in
//! `tools/roster_audit/oracle.py`):
//!
//! * registered with his contract club (or not registered): he belongs to that club; the first
//!   team (slot 0) is the senior squad, a reserve or youth side is the academy, except that a
//!   player past the academy's age limit is senior (OFM graduates him at the next turn anyway);
//! * registered with another club of the perimeter: loan between two clubs of the world;
//! * registered with a club outside the perimeter that is an **affiliated side** of his owner (a B
//!   team: seven or more players of the owner are registered there, or its name contains the
//!   owner's): at most the academy's age, he is the owner's academy player (the owner calls him up
//!   at will); older, he plays for that side and is imported as a loan to it;
//! * registered with any other club outside the perimeter: loan to a club the world does not contain;
//! * contract outside the perimeter, registered with a club of the perimeter: he plays for that
//!   club and is imported as its player (his owner is not in the world, so no loan can return him).

use crate::source::PlayerRow;
use chrono::NaiveDate;
use domain::player::SquadRole;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Plays for the club that holds his contract.
    Own,
    /// Owner and borrower are both clubs of the world.
    LoanInsideWorld,
    /// The borrower is a club the world does not contain.
    LoanToOutsideClub,
    /// The owner is outside the world; imported as a player of the club he plays for.
    LoanedInFromOutside,
    /// Registered with an affiliated side outside the world, within the academy's age: the owner's
    /// academy player.
    AffiliateAcademy,
    /// Registered with an affiliated side outside the world, past the academy's age: on loan there.
    AffiliateLoan,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Own => "propre",
            Kind::LoanInsideWorld => "prêt interne",
            Kind::LoanToOutsideClub => "prêt externe",
            Kind::LoanedInFromOutside => "prêt entrant (propriétaire hors périmètre)",
            Kind::AffiliateAcademy => "équipe B (académie du propriétaire)",
            Kind::AffiliateLoan => "équipe B (joue là-bas)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub kind: Kind,
    /// The club he plays for (a club outside the perimeter for `LoanToOutsideClub`).
    pub playing_club: i64,
    /// His owner when he is on loan.
    pub owner_club: Option<i64>,
    pub role: SquadRole,
    /// Name of the borrower, kept for loans to clubs the world does not contain.
    pub borrower_name: Option<String>,
}

impl Placement {
    pub fn is_loan(&self) -> bool {
        matches!(
            self.kind,
            Kind::LoanInsideWorld | Kind::LoanToOutsideClub | Kind::AffiliateLoan
        )
    }
}

/// Minimum number of an owner's players registered with one club outside the world for that club
/// to count as the owner's affiliated side.
pub const AFFILIATE_MIN_PLAYERS: usize = 7;

/// Words that do not identify a club (legal forms, "club", "real", "united", numbers of foundation...).
const GENERIC: &[&str] = &[
    "fc",
    "cf",
    "ac",
    "sc",
    "sv",
    "ud",
    "cd",
    "rc",
    "rcd",
    "club",
    "de",
    "del",
    "la",
    "le",
    "los",
    "las",
    "do",
    "da",
    "di",
    "dos",
    "united",
    "city",
    "town",
    "real",
    "sporting",
    "athletic",
    "atletico",
    "deportivo",
    "futebol",
    "football",
    "calcio",
    "the",
    "b",
    "c",
    "ii",
    "iii",
    "a",
    "u23",
    "u21",
    "u19",
    "s",
    "ss",
    "afc",
    "as",
    "sd",
    "kv",
    "fk",
    "ca",
    "clube",
    "sociedad",
    "associazione",
    "team",
    "y",
    "n",
];

/// The words of a club name that identify it: lower case, accents removed, generic words and
/// numbers dropped.
fn identifying_words(name: &str) -> BTreeSet<String> {
    let folded: String = name
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => {
                'a'
            }
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' => {
                'o'
            }
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            'ñ' | 'Ñ' => 'n',
            other => other.to_ascii_lowercase(),
        })
        .collect();
    folded
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() > 1 && !w.chars().all(|c| c.is_ascii_digit()) && !GENERIC.contains(w))
        .map(str::to_string)
        .collect()
}

/// True when `side`'s name contains every identifying word of `owner`'s (and the owner has some):
/// "FC Bayern München II" is Bayern's, "Esporte Clube Vitória" is not "Criciúma Esporte Clube"'s.
pub fn name_suggests_affiliate(owner: &str, side: &str) -> bool {
    let owner = identifying_words(owner);
    !owner.is_empty() && owner.is_subset(&identifying_words(side))
}

/// The (owner, club outside the world) pairs that are affiliated sides: the owner has
/// [`AFFILIATE_MIN_PLAYERS`] or more players registered there, or the names say so.
pub fn affiliated_sides(rows: &[PlayerRow], perimeter: &BTreeSet<i64>) -> BTreeSet<(i64, i64)> {
    let mut counts: std::collections::BTreeMap<(i64, i64), usize> = Default::default();
    let mut names: std::collections::BTreeMap<(i64, i64), (&str, &str)> = Default::default();
    for r in rows {
        if let Some(side) = r.registered_club_id
            && side != r.club_id
            && perimeter.contains(&r.club_id)
            && !perimeter.contains(&side)
        {
            *counts.entry((r.club_id, side)).or_default() += 1;
            names.insert(
                (r.club_id, side),
                (
                    r.club_name.as_str(),
                    r.registered_club_name.as_deref().unwrap_or(""),
                ),
            );
        }
    }
    counts
        .into_iter()
        .filter(|(pair, n)| {
            *n >= AFFILIATE_MIN_PLAYERS || {
                let (owner, side) = names[pair];
                name_suggests_affiliate(owner, side)
            }
        })
        .map(|(pair, _)| pair)
        .collect()
}

/// `None` for a player who has nothing to do with the perimeter. `affiliates` comes from
/// [`affiliated_sides`].
pub fn place(
    row: &PlayerRow,
    perimeter: &BTreeSet<i64>,
    affiliates: &BTreeSet<(i64, i64)>,
    on: NaiveDate,
    academy_max_age: i32,
) -> Option<Placement> {
    let contract = row.club_id;
    let contract_in = perimeter.contains(&contract);
    let registered = row.registered_club_id;
    let registered_in = registered.is_some_and(|c| perimeter.contains(&c));
    let overage = || {
        row.birth_date
            .as_deref()
            .is_none_or(|dob| domain::age::age_on_or_unknown(on, dob) > academy_max_age)
    };
    let role_at = |slot: Option<i64>| {
        if slot.is_none_or(|s| s == 0) || overage() {
            SquadRole::Senior
        } else {
            SquadRole::Youth
        }
    };
    match registered {
        _ if contract_in && registered.is_none_or(|r| r == contract) => {
            // Registered with his own club: the registration's team decides; without registration the
            // contract's team does.
            let slot = if registered.is_some() {
                row.registered_slot
            } else {
                row.team_slot
            };
            Some(Placement {
                kind: Kind::Own,
                playing_club: contract,
                owner_club: None,
                role: role_at(slot),
                borrower_name: None,
            })
        }
        Some(r) if contract_in && registered_in => Some(Placement {
            kind: Kind::LoanInsideWorld,
            playing_club: r,
            owner_club: Some(contract),
            role: role_at(row.registered_slot),
            borrower_name: None,
        }),
        Some(r) if contract_in && affiliates.contains(&(contract, r)) && !overage() => {
            Some(Placement {
                kind: Kind::AffiliateAcademy,
                playing_club: contract,
                owner_club: None,
                role: SquadRole::Youth,
                borrower_name: None,
            })
        }
        Some(r) if contract_in && affiliates.contains(&(contract, r)) => Some(Placement {
            kind: Kind::AffiliateLoan,
            playing_club: r,
            owner_club: Some(contract),
            role: SquadRole::Senior,
            borrower_name: row.registered_club_name.clone(),
        }),
        Some(r) if contract_in => Some(Placement {
            kind: Kind::LoanToOutsideClub,
            playing_club: r,
            owner_club: Some(contract),
            role: role_at(row.registered_slot),
            borrower_name: row.registered_club_name.clone(),
        }),
        Some(r) if registered_in => Some(Placement {
            kind: Kind::LoanedInFromOutside,
            playing_club: r,
            owner_club: None,
            role: role_at(row.registered_slot),
            borrower_name: None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::PlayerRow;
    use std::collections::BTreeMap;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()
    }

    fn world() -> BTreeSet<i64> {
        [498, 1107, 490].into_iter().collect()
    }

    fn row(contract: i64, registered: Option<(i64, i64)>, born: &str) -> PlayerRow {
        PlayerRow {
            person_id: 1,
            fm_uid: None,
            fm_uid_status: "unique".into(),
            first_name: String::new(),
            last_name: String::new(),
            common_name: None,
            birth_date: Some(born.into()),
            nation: None,
            height_cm: None,
            club_id: contract,
            club_name: "Owner FC".into(),
            club_nation: None,
            club_reputation: 0,
            team_slot: Some(0),
            team_is_youth: false,
            registered_club_id: registered.map(|r| r.0),
            registered_club_name: registered.map(|r| format!("club {}", r.0)),
            registered_slot: registered.map(|r| r.1),
            registered_is_youth: registered.is_some_and(|r| r.1 > 0),
            wage_gbp_week: 0,
            wage_original: 0,
            wage_source: String::new(),
            start_date: None,
            end_date: None,
            contract_type_code: None,
            squad_status_code: None,
            ca: 100,
            pa: 100,
            rep_current: None,
            rep_home: None,
            rep_world: None,
            foot_left: 50,
            foot_right: 50,
            value_gbp: None,
            value_source: None,
            attrs: crate::source::FmAttrs(BTreeMap::new()),
            positions: BTreeMap::new(),
        }
    }

    /// Joao Neves (person 47802): contract and registration at Paris Saint-Germain, first team.
    #[test]
    fn a_first_team_player_is_a_senior_of_his_club() {
        let p = place(
            &row(498, Some((498, 0)), "2004-09-27"),
            &world(),
            &BTreeSet::new(),
            day(),
            21,
        )
        .unwrap();
        assert_eq!(
            (p.kind, p.playing_club, p.role),
            (Kind::Own, 498, SquadRole::Senior)
        );
        assert_eq!(p.owner_club, None);
    }

    /// Kais Anelka (person 77083), born 2008-06-24: registered with PSG's under-18 side (slot 2).
    #[test]
    fn a_youth_side_player_is_an_academy_player() {
        let p = place(
            &row(498, Some((498, 2)), "2008-06-24"),
            &world(),
            &BTreeSet::new(),
            day(),
            21,
        )
        .unwrap();
        assert_eq!((p.kind, p.role), (Kind::Own, SquadRole::Youth));
    }

    /// Pierre Gibaud (person 1054), born 1988, in PSG's reserve side: past the academy age, so senior.
    #[test]
    fn an_overage_reserve_player_is_senior() {
        let p = place(
            &row(498, Some((498, 1)), "1988-04-22"),
            &world(),
            &BTreeSet::new(),
            day(),
            21,
        )
        .unwrap();
        assert_eq!(p.role, SquadRole::Senior);
        assert_eq!(p.kind, Kind::Own);
    }

    /// Gabriel Moscardo (person 63609): contract PSG, registered with Espanyol (a club of the world).
    #[test]
    fn a_player_registered_with_another_club_of_the_world_is_on_loan_there() {
        let p = place(
            &row(498, Some((1107, 0)), "2005-09-28"),
            &world(),
            &BTreeSet::new(),
            day(),
            21,
        )
        .unwrap();
        assert_eq!(
            (p.kind, p.playing_club, p.owner_club),
            (Kind::LoanInsideWorld, 1107, Some(498))
        );
        assert_eq!(p.role, SquadRole::Senior);
    }

    /// Renato Marin (person 62608): contract PSG, registered with Nacional (not in the world).
    #[test]
    fn a_player_registered_outside_the_world_is_on_loan_to_that_club() {
        let p = place(
            &row(498, Some((1600, 0)), "2006-07-10"),
            &world(),
            &BTreeSet::new(),
            day(),
            21,
        )
        .unwrap();
        assert_eq!(
            (p.kind, p.playing_club, p.owner_club),
            (Kind::LoanToOutsideClub, 1600, Some(498))
        );
        assert_eq!(p.borrower_name.as_deref(), Some("club 1600"));
    }

    /// A player whose owner is outside the world and who plays for a club of the world.
    #[test]
    fn a_loanee_of_an_outside_owner_is_imported_as_the_borrower_s_player() {
        let p = place(
            &row(9999, Some((490, 0)), "1998-01-01"),
            &world(),
            &BTreeSet::new(),
            day(),
            21,
        )
        .unwrap();
        assert_eq!(
            (p.kind, p.playing_club, p.owner_club),
            (Kind::LoanedInFromOutside, 490, None)
        );
    }

    /// Tenerife's goalkeeper 70118 (born 2004-10-09, 21) and Gastesi (person 55808, born 2003-10-30,
    /// 22), both registered with CD Tenerife B, which is outside the world.
    #[test]
    fn an_affiliated_side_keeps_its_young_players_as_academy_and_lends_the_older_ones() {
        let affiliates: BTreeSet<(i64, i64)> = [(498, 8888)].into_iter().collect();
        let young = place(
            &row(498, Some((8888, 0)), "2004-10-09"),
            &world(),
            &affiliates,
            day(),
            21,
        )
        .unwrap();
        assert_eq!(
            (young.kind, young.playing_club, young.role),
            (Kind::AffiliateAcademy, 498, SquadRole::Youth)
        );
        assert!(!young.is_loan());
        let older = place(
            &row(498, Some((8888, 0)), "2003-10-30"),
            &world(),
            &affiliates,
            day(),
            21,
        )
        .unwrap();
        assert_eq!(
            (older.kind, older.playing_club, older.owner_club),
            (Kind::AffiliateLoan, 8888, Some(498))
        );
        assert!(older.is_loan());
    }

    #[test]
    fn a_side_is_affiliated_by_its_name_or_by_the_number_of_players_registered_there() {
        assert!(name_suggests_affiliate(
            "FC Bayern München",
            "FC Bayern München II"
        ));
        assert!(name_suggests_affiliate(
            "Real Madrid C.F.",
            "Real Madrid Castilla C.F."
        ));
        assert!(name_suggests_affiliate(
            "Granada C.F.",
            "Club Recreativo Granada"
        ));
        assert!(!name_suggests_affiliate(
            "Esporte Clube Vitória",
            "Criciúma Esporte Clube"
        ));
        assert!(!name_suggests_affiliate("Chelsea", "RC Strasbourg Alsace"));
        let mut rows: Vec<PlayerRow> = (0..7)
            .map(|_| row(498, Some((8888, 0)), "2005-01-01"))
            .collect();
        rows.push(row(498, Some((7777, 0)), "2005-01-01")); // one loan elsewhere
        let found = affiliated_sides(&rows, &world());
        assert_eq!(found, [(498, 8888)].into_iter().collect());
    }

    #[test]
    fn a_player_with_no_link_to_the_world_is_not_placed() {
        assert!(
            place(
                &row(9999, Some((8888, 0)), "1998-01-01"),
                &world(),
                &BTreeSet::new(),
                day(),
                21
            )
            .is_none()
        );
    }

    /// Contract with a club of the world and no registration: the contract's team decides.
    #[test]
    fn without_registration_the_contract_team_decides() {
        let mut r = row(498, None, "2000-01-01");
        r.team_slot = Some(1);
        let p = place(&r, &world(), &BTreeSet::new(), day(), 21).unwrap();
        assert_eq!((p.kind, p.role), (Kind::Own, SquadRole::Senior)); // 26 years old
        let mut young = row(498, None, "2007-01-01");
        young.team_slot = Some(1);
        assert_eq!(
            place(&young, &world(), &BTreeSet::new(), day(), 21)
                .unwrap()
                .role,
            SquadRole::Youth
        );
    }
}
