//! The 2026/27 European club competitions: who plays in them, and which `world.db` club each
//! participant is.
//!
//! The participants are UEFA's (the league-phase lists published on uefa.com), recorded in
//! `config/europe_2026_27.csv` with the source club each one is. Matching is by internal id with
//! the source's own name kept beside it as evidence: a row whose id carries another name is
//! refused, so the mapping cannot drift when the source changes. A participant with no source club
//! is listed as such and becomes a generated club with its UEFA identity; no source club is ever
//! guessed from a loose name.
//!
//! The source's three continental competitions hold no participants, so nothing about the fields
//! comes from `world.db` but the clubs themselves.

use crate::Result;
use crate::ids;
use crate::source::ClubRow;
use domain::league::{Berth, BerthRule};
use std::collections::{BTreeMap, BTreeSet};

const EUROPE_CSV: &str = include_str!("../config/europe_2026_27.csv");

/// Clubs per European competition's league phase (the engine's own constant).
pub const FIELD_SIZE: usize = domain::league_phase::LEAGUE_PHASE_FIELD_SIZE;

/// The three competitions, strongest first: `(csv key, short key of the OFM id, display name)`.
pub const COMPETITIONS: [(&str, &str, &str); 3] = [
    ("UCL", "ucl", "UEFA Champions League"),
    ("UEL", "uel", "UEFA Europa League"),
    ("UECL", "uecl", "UEFA Conference League"),
];

/// The translation key of each competition's name, in the order of [`COMPETITIONS`]: a player
/// reads the name in their own language, the `name` above is only the fallback.
pub const NAME_KEYS: [&str; 3] = [
    "tournaments.competitions.championsLeague",
    "tournaments.competitions.europaLeague",
    "tournaments.competitions.conferenceLeague",
];

/// One participant: a club UEFA names, and the source club it is when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub competition: String,
    pub uefa_name: String,
    /// OFM nation code of the UEFA association.
    pub association: String,
    pub fm_club_id: Option<i64>,
    /// The club's name in `world.db` when the mapping was made.
    pub fm_name: String,
    pub city: String,
    pub evidence: String,
    /// World.db reputation scale; only for a club with no source club.
    pub assumed_reputation: Option<i64>,
}

impl Participant {
    /// The id the club carries in the world: its `fm-c-` identity, or a generated one.
    pub fn club_id(&self) -> String {
        match self.fm_club_id {
            Some(id) => ids::club(id),
            None => ids::generated_club(&self.association, &self.uefa_name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EuropeanFields {
    pub participants: Vec<Participant>,
}

impl EuropeanFields {
    pub fn embedded() -> Result<Self> {
        Self::from_csv(EUROPE_CSV)
    }

    pub fn from_csv(text: &str) -> Result<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .comment(Some(b'#'))
            .from_reader(text.as_bytes());
        let mut participants = Vec::new();
        for record in reader.records() {
            let record = record.map_err(|e| format!("europe_2026_27.csv: {e}"))?;
            let field = |index: usize| record.get(index).unwrap_or("").trim().to_string();
            let id_text = field(3);
            let fm_club_id = if id_text.is_empty() {
                None
            } else {
                Some(
                    id_text
                        .parse()
                        .map_err(|_| format!("europe_2026_27.csv: bad fm_club_id `{id_text}`"))?,
                )
            };
            let reputation_text = field(7);
            participants.push(Participant {
                competition: field(0),
                uefa_name: field(1),
                association: field(2),
                fm_club_id,
                fm_name: field(4),
                city: field(5),
                evidence: field(6),
                assumed_reputation: if reputation_text.is_empty() {
                    None
                } else {
                    Some(
                        reputation_text
                            .parse()
                            .map_err(|_| format!("bad assumed_reputation `{reputation_text}`"))?,
                    )
                },
            });
        }
        let fields = Self { participants };
        fields.validate()?;
        Ok(fields)
    }

    /// The structural rules of the mapping, independent of any source: three fields of 36, no club
    /// twice (across competitions too), and a row either matched to a source club by id and name
    /// or explicitly unmatched with its assumed reputation.
    pub fn validate(&self) -> Result<()> {
        for (key, _, _) in COMPETITIONS {
            let count = self.field_of(key).count();
            if count != FIELD_SIZE {
                return Err(format!("{key} lists {count} clubs, expected {FIELD_SIZE}"));
            }
        }
        if let Some(stray) = self
            .participants
            .iter()
            .find(|p| !COMPETITIONS.iter().any(|(key, _, _)| *key == p.competition))
        {
            return Err(format!(
                "{} is in unknown competition `{}`",
                stray.uefa_name, stray.competition
            ));
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for participant in &self.participants {
            if !seen.insert(participant.club_id()) {
                return Err(format!(
                    "{} appears twice in the European fields ({})",
                    participant.uefa_name,
                    participant.club_id()
                ));
            }
            match (participant.fm_club_id, participant.assumed_reputation) {
                (Some(_), None) if !participant.fm_name.is_empty() => {}
                (None, Some(_)) if participant.evidence.starts_with("unmatched") => {}
                _ => {
                    return Err(format!(
                        "{}: a matched club needs an id and the source name, an unmatched one an \
                         `unmatched` evidence and an assumed reputation",
                        participant.uefa_name
                    ));
                }
            }
            if participant.association.is_empty()
                || ofm_core::nations::nation_by_code(&participant.association).is_none()
            {
                return Err(format!(
                    "{}: `{}` is not an OFM nation code",
                    participant.uefa_name, participant.association
                ));
            }
        }
        Ok(())
    }

    pub fn field_of<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Participant> {
        self.participants
            .iter()
            .filter(move |p| p.competition == key)
    }

    /// Source clubs of the fields that no domestic league of the world contains, sorted: these
    /// become continental-only teams.
    pub fn continental_only_ids(&self, domestic: &BTreeSet<i64>) -> Vec<i64> {
        self.participants
            .iter()
            .filter_map(|p| p.fm_club_id)
            .filter(|id| !domestic.contains(id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Participants with no source club: generated, marked as such.
    pub fn generated(&self) -> impl Iterator<Item = &Participant> {
        self.participants.iter().filter(|p| p.fm_club_id.is_none())
    }

    /// The participant behind a source club id, if any.
    pub fn by_fm_id(&self, id: i64) -> Option<&Participant> {
        self.participants.iter().find(|p| p.fm_club_id == Some(id))
    }

    /// Checks every matched row against the source clubs read for it: the id exists, its name is
    /// the one recorded, and its nation (when the source has one) is the association. Returns every
    /// disagreement at once.
    pub fn check_against_source(
        &self,
        source_clubs: &[ClubRow],
        nation_code: impl Fn(&str) -> Option<String>,
    ) -> Vec<String> {
        let by_id: BTreeMap<i64, &ClubRow> = source_clubs.iter().map(|c| (c.id, c)).collect();
        let mut problems = Vec::new();
        for participant in &self.participants {
            let Some(id) = participant.fm_club_id else {
                continue;
            };
            let Some(club) = by_id.get(&id) else {
                problems.push(format!(
                    "{}: club {id} is not in world.db",
                    participant.uefa_name
                ));
                continue;
            };
            if club.name != participant.fm_name {
                problems.push(format!(
                    "{}: club {id} is `{}` in world.db, the mapping was made for `{}`",
                    participant.uefa_name, club.name, participant.fm_name
                ));
            }
            if let Some(nation) = club.nation.as_deref()
                && nation_code(nation).as_deref() != Some(participant.association.as_str())
            {
                problems.push(format!(
                    "{}: world.db places club {id} in {nation}, not in association {}",
                    participant.uefa_name, participant.association
                ));
            }
        }
        problems
    }
}

/// How one association's domestic results qualify clubs for Europe in the seasons after the first.
///
/// **APFM's modelled access list**, shaped like UEFA's published structure for 2026/27 (four
/// Champions League places for England, Spain, Germany and Italy, three for France; the next
/// places in the Europa League and the Conference League; the domestic cup winner in the Europa
/// League; the Champions League's European performance places) and chosen so that each association
/// sends the number of clubs it sends in the 2026/27 snapshot (England 5/3/1 and so on). It is
/// **not** UEFA's allocation for any later season: that depends on the coefficient ranking and on
/// qualifying rounds the game does not play.
pub struct AccessList {
    pub league: &'static str,
    /// League positions (inclusive) that earn a Champions League place.
    pub champions: (u32, u32),
    /// The position a European performance place adds to the Champions League.
    pub performance_position: u32,
    /// League positions (inclusive) that earn a Europa League place.
    pub europa: (u32, u32),
    /// League positions (inclusive) that earn a Conference League place.
    pub conference: (u32, u32),
    /// The domestic cup whose winner earns a Europa League place.
    pub cup: &'static str,
}

pub const ACCESS: [AccessList; 5] = [
    AccessList {
        league: "en_pl",
        champions: (1, 4),
        performance_position: 5,
        europa: (5, 6),
        conference: (7, 7),
        cup: "en_facup",
    },
    AccessList {
        league: "es_laliga",
        champions: (1, 4),
        performance_position: 5,
        europa: (5, 5),
        conference: (6, 6),
        cup: "es_copa",
    },
    AccessList {
        league: "de_bundesliga",
        champions: (1, 4),
        performance_position: 5,
        europa: (5, 5),
        conference: (6, 6),
        cup: "de_pokal",
    },
    AccessList {
        league: "it_seriea",
        champions: (1, 4),
        performance_position: 5,
        europa: (5, 5),
        conference: (6, 6),
        cup: "it_coppa",
    },
    AccessList {
        league: "fr_ligue1",
        champions: (1, 3),
        performance_position: 4,
        europa: (4, 5),
        conference: (6, 6),
        cup: "fr_coupe",
    },
];

/// The European performance places the Champions League adds, to the best associations of the
/// previous European season among those whose league the world simulates.
pub const PERFORMANCE_PLACES: u32 = 2;

fn berth(target: &str, rule: BerthRule) -> Berth {
    Berth {
        target: ids::european_competition(target),
        rule,
        fallback_to: None,
    }
}

/// The berths of a first division: positions into each competition, and the performance place.
pub fn league_berths(league_key: &str) -> Vec<Berth> {
    let Some(access) = ACCESS.iter().find(|access| access.league == league_key) else {
        return Vec::new();
    };
    let range = |(from, to): (u32, u32)| BerthRule::PositionRange { from, to };
    vec![
        berth("ucl", range(access.champions)),
        berth(
            "ucl",
            BerthRule::EuropeanPerformance {
                position: access.performance_position,
                places: PERFORMANCE_PLACES,
            },
        ),
        berth("uel", range(access.europa)),
        berth("uecl", range(access.conference)),
    ]
}

/// The berth of a national cup: its winner plays in the Europa League.
pub fn cup_berths(cup_key: &str) -> Vec<Berth> {
    if ACCESS.iter().any(|access| access.cup == cup_key) {
        vec![berth("uel", BerthRule::CupWinner)]
    } else {
        Vec::new()
    }
}

/// The berths of a European competition itself: the clubs whose league is not simulated keep their
/// place, the titleholder keeps (or takes) its place, and the winner of the competition below moves
/// up: the Europa League's champion to the Champions League, the Conference League's to the
/// Europa League.
pub fn european_berths(short: &str) -> Vec<Berth> {
    let mut berths = vec![berth(short, BerthRule::Incumbent)];
    match short {
        "ucl" => berths.push(berth("ucl", BerthRule::Titleholder)),
        "uel" => berths.push(berth("ucl", BerthRule::Titleholder)),
        _ => berths.push(berth("uel", BerthRule::Titleholder)),
    }
    berths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(competition: &str, name: &str, id: &str, fm: &str, evidence: &str, rep: &str) -> String {
        format!("{competition},{name},ENG,{id},{fm},City,{evidence},{rep}\n")
    }

    fn full_field(prefix: &str, first_id: i64) -> String {
        (0..FIELD_SIZE)
            .map(|i| {
                row(
                    prefix,
                    &format!("{prefix} club {i}"),
                    &(first_id + i as i64).to_string(),
                    &format!("FM {i}"),
                    "name",
                    "",
                )
            })
            .collect()
    }

    fn three_fields() -> String {
        let mut text = String::from(
            "competition,uefa_name,association,fm_club_id,fm_name,city,evidence,assumed_reputation\n",
        );
        text.push_str(&full_field("UCL", 100));
        text.push_str(&full_field("UEL", 200));
        text.push_str(&full_field("UECL", 300));
        text
    }

    /// Given three well-formed fields of 36
    /// When they are parsed
    /// Then they validate and every club has its source id as its world id.
    #[test]
    fn well_formed_fields_parse_and_carry_source_identities() {
        let fields = EuropeanFields::from_csv(&three_fields()).unwrap();
        assert_eq!(fields.participants.len(), 108);
        assert_eq!(fields.participants[0].club_id(), "fm-c-100");
    }

    /// Given a field of 35
    /// When it is parsed
    /// Then it is refused: a European competition is never short.
    #[test]
    fn a_short_field_is_refused() {
        let text = three_fields();
        let short: String = text.lines().filter(|l| !l.contains("UECL club 35")).fold(
            String::new(),
            |mut acc, line| {
                acc.push_str(line);
                acc.push('\n');
                acc
            },
        );
        let error = EuropeanFields::from_csv(&short).unwrap_err();
        assert!(error.contains("UECL lists 35"), "{error}");
    }

    /// Given one club listed in two competitions
    /// When the fields are parsed
    /// Then they are refused: the three fields are disjoint.
    #[test]
    fn a_club_in_two_fields_is_refused() {
        let text = three_fields().replace(",200,FM 0,", ",100,FM 0,");
        let error = EuropeanFields::from_csv(&text).unwrap_err();
        assert!(error.contains("appears twice"), "{error}");
    }

    /// Given a matched row without the source name, and an unmatched row without a reputation
    /// When they are parsed
    /// Then both are refused: a mapping needs its evidence.
    #[test]
    fn a_row_without_evidence_is_refused() {
        let nameless = three_fields().replace(",100,FM 0,", ",100,,");
        assert!(EuropeanFields::from_csv(&nameless).is_err());
        let unmatched = three_fields().replace(",100,FM 0,City,name,", ",,,City,unmatched: none,");
        assert!(EuropeanFields::from_csv(&unmatched).is_err());
    }

    /// Given a participant whose association is not an OFM nation code
    /// When the fields are parsed
    /// Then they are refused instead of guessing a nation.
    #[test]
    fn an_unknown_association_is_refused() {
        let text = three_fields().replace("UCL club 3,ENG", "UCL club 3,XXX");
        let error = EuropeanFields::from_csv(&text).unwrap_err();
        assert!(error.contains("not an OFM nation code"), "{error}");
    }

    fn club_row(id: i64, name: &str, nation: Option<&str>) -> ClubRow {
        ClubRow {
            id,
            fm_uid: None,
            name: name.to_string(),
            short_name: String::new(),
            nation: nation.map(str::to_string),
            reputation: 0,
            competition_id: 0,
            colours: None,
            finance: None,
        }
    }

    /// Given the mapping and a source whose club 100 has been renamed
    /// When the mapping is checked against the source
    /// Then the renamed club is reported, so an id is never re-pointed silently, while a matching
    ///      club passes.
    #[test]
    fn a_renamed_source_club_is_reported_not_followed() {
        let fields = EuropeanFields::from_csv(&three_fields()).unwrap();
        let clubs = vec![
            club_row(100, "Somebody Else", Some("England")),
            club_row(101, "FM 1", Some("England")),
        ];

        let problems =
            fields.check_against_source(&clubs, |n| (n == "England").then(|| "ENG".to_string()));

        assert!(
            problems.iter().any(|p| p.contains("`Somebody Else`")),
            "{problems:?}"
        );
        assert!(
            !problems.iter().any(|p| p.contains("club 101 is")),
            "{problems:?}"
        );
    }

    /// Given the embedded mapping
    /// When it is parsed
    /// Then it is the 108 participants UEFA lists, one of them unmatched with a generated id, and
    ///      its generated id names association and club (never a number).
    #[test]
    fn the_embedded_mapping_is_valid_and_names_its_one_unmatched_club() {
        let fields = EuropeanFields::embedded().unwrap();
        assert_eq!(fields.participants.len(), 108);
        let generated: Vec<&Participant> = fields.generated().collect();
        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].club_id(), "eu-gen-ge-iberia-tbilisi");
        assert!(
            fields
                .participants
                .iter()
                .any(|p| p.uefa_name == "Paris Saint-Germain" && p.club_id() == "fm-c-498")
        );
    }

    /// Given the embedded mapping and the perimeter of the five domestic countries
    /// When the continental-only clubs are listed
    /// Then no domestic-league club is among them, and the list is sorted and unique.
    #[test]
    fn continental_only_clubs_exclude_the_domestic_perimeter() {
        let fields = EuropeanFields::embedded().unwrap();
        let domestic: BTreeSet<i64> = [498, 321, 1113].into_iter().collect();
        let only = fields.continental_only_ids(&domestic);
        assert!(!only.contains(&498));
        assert!(only.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(only.len() > 60);
    }
}
