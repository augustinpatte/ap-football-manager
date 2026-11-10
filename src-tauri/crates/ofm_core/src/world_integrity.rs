//! Integrity audit of a world's players and club memberships.
//!
//! It answers one question about a roster, whatever produced it (the procedural generator, a
//! `.ofm` package, the FM24 importer): is it internally consistent? It does **not** say whether
//! a player is real. A generated world passes this audit and is still entirely fictional; see
//! `docs/APFM_DATA_PROVENANCE.md` for what each source is.
//!
//! Pure and read-only: nothing here edits a player, so it is safe to run over a user's database.

use crate::player_rating::natural_ovr;
use chrono::{Datelike, NaiveDate};
use domain::league::League;
use domain::player::{Player, PlayerAttributes, SquadRole};
use domain::team::Team;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

/// The most the stored `ovr` may drift from the rating its own attributes produce. Generation
/// and import both derive `ovr` from the attributes, so a larger gap means one of the two was
/// edited without the other.
pub const MAX_OVR_ATTRIBUTE_DRIFT: f64 = 2.0;

/// A senior's plausible age band on the audit date; outside it the birth date is more likely
/// wrong than the player unusual.
const PLAUSIBLE_AGE: std::ops::RangeInclusive<i32> = 14..=45;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The data contradicts itself: something downstream will misbehave.
    Error,
    /// Unusual and worth a look, but a legitimate world can contain it.
    Warning,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IntegrityFinding {
    DuplicatePlayerId {
        id: String,
    },
    /// Two clubs share an id, so a player's membership no longer identifies one club.
    DuplicateTeamId {
        id: String,
    },
    /// Two or more players share name, birth date and nationality: indistinguishable people.
    DuplicateIdentity {
        full_name: String,
        date_of_birth: String,
        nationality: String,
        player_ids: Vec<String>,
    },
    RatingOutOfRange {
        player_id: String,
        field: &'static str,
        value: u8,
    },
    PotentialBelowOverall {
        player_id: String,
        ovr: u8,
        potential: u8,
    },
    AttributeOutOfRange {
        player_id: String,
        attribute: &'static str,
        value: u8,
    },
    OvrDisagreesWithAttributes {
        player_id: String,
        stored: u8,
        computed: f64,
    },
    /// A contract ends on or before the day it starts: the ledger's own rule
    /// (`LedgerError::EndNotAfterStart`) says that cannot happen.
    ContractEndsBeforeStart {
        player_id: String,
        start: String,
        end: String,
    },
    ContractDateInvalid {
        player_id: String,
        field: &'static str,
        value: String,
    },
    /// A contracted player's deal ended before the day being audited. Not always a bug
    /// (a save can be taken mid-negotiation), but it is worth surfacing.
    ContractExpiredAtAuditDate {
        player_id: String,
        end: String,
    },
    /// A competition that exists but has no participants at all.
    CompetitionWithoutParticipants {
        competition_id: String,
    },
    /// A competition names a team the world does not contain (club or national team).
    UnknownCompetitionParticipant {
        competition_id: String,
        team_id: String,
    },
    FixtureUnknownTeam {
        competition_id: String,
        fixture_id: String,
        team_id: String,
    },
    /// A fixture pairs a team with itself.
    FixtureSelfMatch {
        competition_id: String,
        fixture_id: String,
    },
    /// The same fixture id appears in more than one place.
    DuplicateFixtureId {
        fixture_id: String,
    },
    /// A fixture involves a team that is not in its competition's participant list.
    FixtureTeamNotParticipant {
        competition_id: String,
        fixture_id: String,
        team_id: String,
    },
    /// The exact same home/away pairing is scheduled twice in one competition. Normally a
    /// defect, though a cup replay is the legitimate exception, hence a warning.
    DuplicateFixturePair {
        competition_id: String,
        home_team_id: String,
        away_team_id: String,
    },
    InvalidBirthDate {
        player_id: String,
        value: String,
    },
    ImplausibleAge {
        player_id: String,
        age: i32,
    },
    /// `team_id` names a club the world does not contain (and the player is not on loan to a
    /// club declared as outside the world).
    UnknownClub {
        player_id: String,
        team_id: String,
    },
    UnknownParentClub {
        player_id: String,
        parent_team_id: String,
    },
    /// The player's `team_id` is not the club his loan record says he is at.
    LoanClubMismatch {
        player_id: String,
        team_id: String,
        loan_team_id: String,
    },
    RetiredPlayerAtClub {
        player_id: String,
        team_id: String,
    },
    /// A club's starting XI names a player who is not on that club's books.
    LineupPlayerNotAtClub {
        team_id: String,
        player_id: String,
    },
    SquadTooSmall {
        team_id: String,
        seniors: usize,
        minimum: usize,
    },
    SquadTooLarge {
        team_id: String,
        players: usize,
        maximum: usize,
    },
}

impl IntegrityFinding {
    pub fn severity(&self) -> Severity {
        match self {
            Self::ImplausibleAge { .. }
            | Self::SquadTooLarge { .. }
            | Self::ContractExpiredAtAuditDate { .. }
            | Self::CompetitionWithoutParticipants { .. }
            | Self::DuplicateFixturePair { .. } => Severity::Warning,
            Self::DuplicatePlayerId { .. }
            | Self::DuplicateTeamId { .. }
            | Self::DuplicateIdentity { .. }
            | Self::RatingOutOfRange { .. }
            | Self::PotentialBelowOverall { .. }
            | Self::AttributeOutOfRange { .. }
            | Self::OvrDisagreesWithAttributes { .. }
            | Self::ContractEndsBeforeStart { .. }
            | Self::ContractDateInvalid { .. }
            | Self::UnknownCompetitionParticipant { .. }
            | Self::FixtureUnknownTeam { .. }
            | Self::FixtureSelfMatch { .. }
            | Self::DuplicateFixtureId { .. }
            | Self::FixtureTeamNotParticipant { .. }
            | Self::InvalidBirthDate { .. }
            | Self::UnknownClub { .. }
            | Self::UnknownParentClub { .. }
            | Self::LoanClubMismatch { .. }
            | Self::RetiredPlayerAtClub { .. }
            | Self::LineupPlayerNotAtClub { .. }
            | Self::SquadTooSmall { .. } => Severity::Error,
        }
    }

    /// A stable machine-readable name, for reports and assertions.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DuplicatePlayerId { .. } => "duplicate_player_id",
            Self::DuplicateTeamId { .. } => "duplicate_team_id",
            Self::DuplicateIdentity { .. } => "duplicate_identity",
            Self::RatingOutOfRange { .. } => "rating_out_of_range",
            Self::PotentialBelowOverall { .. } => "potential_below_overall",
            Self::AttributeOutOfRange { .. } => "attribute_out_of_range",
            Self::OvrDisagreesWithAttributes { .. } => "ovr_disagrees_with_attributes",
            Self::ContractEndsBeforeStart { .. } => "contract_ends_before_start",
            Self::ContractDateInvalid { .. } => "contract_date_invalid",
            Self::ContractExpiredAtAuditDate { .. } => "contract_expired_at_audit_date",
            Self::CompetitionWithoutParticipants { .. } => "competition_without_participants",
            Self::UnknownCompetitionParticipant { .. } => "unknown_competition_participant",
            Self::FixtureUnknownTeam { .. } => "fixture_unknown_team",
            Self::FixtureSelfMatch { .. } => "fixture_self_match",
            Self::DuplicateFixtureId { .. } => "duplicate_fixture_id",
            Self::FixtureTeamNotParticipant { .. } => "fixture_team_not_participant",
            Self::DuplicateFixturePair { .. } => "duplicate_fixture_pair",
            Self::InvalidBirthDate { .. } => "invalid_birth_date",
            Self::ImplausibleAge { .. } => "implausible_age",
            Self::UnknownClub { .. } => "unknown_club",
            Self::UnknownParentClub { .. } => "unknown_parent_club",
            Self::LoanClubMismatch { .. } => "loan_club_mismatch",
            Self::RetiredPlayerAtClub { .. } => "retired_player_at_club",
            Self::LineupPlayerNotAtClub { .. } => "lineup_player_not_at_club",
            Self::SquadTooSmall { .. } => "squad_too_small",
            Self::SquadTooLarge { .. } => "squad_too_large",
        }
    }
}

impl fmt::Display for IntegrityFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::DuplicatePlayerId { id } => write!(f, "{code}: player id {id} is used twice"),
            Self::DuplicateTeamId { id } => write!(f, "{code}: club id {id} is used twice"),
            Self::DuplicateIdentity {
                full_name,
                date_of_birth,
                nationality,
                player_ids,
            } => write!(
                f,
                "{code}: {full_name} ({nationality}, born {date_of_birth}) is {} players: {}",
                player_ids.len(),
                player_ids.join(", ")
            ),
            Self::RatingOutOfRange {
                player_id,
                field,
                value,
            } => write!(f, "{code}: {player_id} has {field} {value} (allowed 1-99)"),
            Self::PotentialBelowOverall {
                player_id,
                ovr,
                potential,
            } => write!(f, "{code}: {player_id} potential {potential} < ovr {ovr}"),
            Self::AttributeOutOfRange {
                player_id,
                attribute,
                value,
            } => write!(
                f,
                "{code}: {player_id} has {attribute} {value} (allowed 0-100)"
            ),
            Self::OvrDisagreesWithAttributes {
                player_id,
                stored,
                computed,
            } => write!(
                f,
                "{code}: {player_id} stores ovr {stored}, his attributes give {computed:.1}"
            ),
            Self::ContractEndsBeforeStart {
                player_id,
                start,
                end,
            } => write!(
                f,
                "{code}: {player_id} has a contract {start}..{end}, which does not end after it starts"
            ),
            Self::ContractDateInvalid {
                player_id,
                field,
                value,
            } => write!(f, "{code}: {player_id} has contract {field} {value:?}"),
            Self::ContractExpiredAtAuditDate { player_id, end } => {
                write!(f, "{code}: {player_id}'s contract ended {end}")
            }
            Self::CompetitionWithoutParticipants { competition_id } => {
                write!(f, "{code}: {competition_id} has no participants")
            }
            Self::UnknownCompetitionParticipant {
                competition_id,
                team_id,
            } => write!(f, "{code}: {competition_id} names unknown team {team_id}"),
            Self::FixtureUnknownTeam {
                competition_id,
                fixture_id,
                team_id,
            } => write!(
                f,
                "{code}: {competition_id} fixture {fixture_id} names unknown team {team_id}"
            ),
            Self::FixtureSelfMatch {
                competition_id,
                fixture_id,
            } => write!(
                f,
                "{code}: {competition_id} fixture {fixture_id} pairs a team with itself"
            ),
            Self::DuplicateFixtureId { fixture_id } => {
                write!(f, "{code}: fixture id {fixture_id} is used twice")
            }
            Self::FixtureTeamNotParticipant {
                competition_id,
                fixture_id,
                team_id,
            } => write!(
                f,
                "{code}: {competition_id} fixture {fixture_id} uses {team_id}, who is not in the competition"
            ),
            Self::DuplicateFixturePair {
                competition_id,
                home_team_id,
                away_team_id,
            } => write!(
                f,
                "{code}: {competition_id} schedules {home_team_id} v {away_team_id} twice"
            ),
            Self::InvalidBirthDate { player_id, value } => {
                write!(f, "{code}: {player_id} has birth date {value:?}")
            }
            Self::ImplausibleAge { player_id, age } => {
                write!(f, "{code}: {player_id} is {age} years old")
            }
            Self::UnknownClub { player_id, team_id } => {
                write!(f, "{code}: {player_id} belongs to unknown club {team_id}")
            }
            Self::UnknownParentClub {
                player_id,
                parent_team_id,
            } => write!(
                f,
                "{code}: {player_id} is on loan from unknown club {parent_team_id}"
            ),
            Self::LoanClubMismatch {
                player_id,
                team_id,
                loan_team_id,
            } => write!(
                f,
                "{code}: {player_id} is at {team_id} but his loan says {loan_team_id}"
            ),
            Self::RetiredPlayerAtClub { player_id, team_id } => {
                write!(
                    f,
                    "{code}: retired {player_id} is still on {team_id}'s books"
                )
            }
            Self::LineupPlayerNotAtClub { team_id, player_id } => write!(
                f,
                "{code}: {team_id}'s starting XI names {player_id}, who is not at the club"
            ),
            Self::SquadTooSmall {
                team_id,
                seniors,
                minimum,
            } => write!(
                f,
                "{code}: {team_id} has {seniors} seniors (minimum {minimum})"
            ),
            Self::SquadTooLarge {
                team_id,
                players,
                maximum,
            } => write!(
                f,
                "{code}: {team_id} has {players} players (plausible maximum {maximum})"
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntegrityLimits {
    /// Seniors a club must have; the game's own playing floor by default.
    pub min_seniors: usize,
    /// Players on a club's books above which the roster is flagged as bloated. A heuristic,
    /// not a rule of the game: top clubs register a first team plus an academy, not 60 people.
    pub max_roster: usize,
    /// The date ages are measured on.
    pub on: NaiveDate,
}

impl IntegrityLimits {
    pub fn new(on: NaiveDate) -> Self {
        Self {
            min_seniors: crate::squad_floor::MIN_SENIOR_PLAYERS,
            max_roster: 45,
            on,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct IntegrityReport {
    pub findings: Vec<IntegrityFinding>,
}

impl IntegrityReport {
    pub fn errors(&self) -> impl Iterator<Item = &IntegrityFinding> {
        self.findings
            .iter()
            .filter(|finding| finding.severity() == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &IntegrityFinding> {
        self.findings
            .iter()
            .filter(|finding| finding.severity() == Severity::Warning)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }

    /// Findings per code, for a compact summary.
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for finding in &self.findings {
            *counts.entry(finding.code()).or_default() += 1;
        }
        counts
    }
}

fn attribute_values(attributes: &PlayerAttributes) -> [(&'static str, u8); 19] {
    [
        ("pace", attributes.pace),
        ("stamina", attributes.stamina),
        ("strength", attributes.strength),
        ("agility", attributes.agility),
        ("passing", attributes.passing),
        ("shooting", attributes.shooting),
        ("tackling", attributes.tackling),
        ("dribbling", attributes.dribbling),
        ("defending", attributes.defending),
        ("positioning", attributes.positioning),
        ("vision", attributes.vision),
        ("decisions", attributes.decisions),
        ("composure", attributes.composure),
        ("aggression", attributes.aggression),
        ("teamwork", attributes.teamwork),
        ("leadership", attributes.leadership),
        ("handling", attributes.handling),
        ("reflexes", attributes.reflexes),
        ("aerial", attributes.aerial),
    ]
}

fn audit_ratings(player: &Player, findings: &mut Vec<IntegrityFinding>) {
    for (field, value) in [("ovr", player.ovr), ("potential", player.potential)] {
        if !(1..=99).contains(&value) {
            findings.push(IntegrityFinding::RatingOutOfRange {
                player_id: player.id.clone(),
                field,
                value,
            });
        }
    }
    if player.potential < player.ovr {
        findings.push(IntegrityFinding::PotentialBelowOverall {
            player_id: player.id.clone(),
            ovr: player.ovr,
            potential: player.potential,
        });
    }
    for (attribute, value) in attribute_values(&player.attributes) {
        if value > 100 {
            findings.push(IntegrityFinding::AttributeOutOfRange {
                player_id: player.id.clone(),
                attribute,
                value,
            });
        }
    }
    let computed = natural_ovr(player);
    if (computed - f64::from(player.ovr)).abs() > MAX_OVR_ATTRIBUTE_DRIFT {
        findings.push(IntegrityFinding::OvrDisagreesWithAttributes {
            player_id: player.id.clone(),
            stored: player.ovr,
            computed,
        });
    }
}

fn audit_age(player: &Player, on: NaiveDate, findings: &mut Vec<IntegrityFinding>) {
    let Ok(born) = NaiveDate::parse_from_str(&player.date_of_birth, "%Y-%m-%d") else {
        findings.push(IntegrityFinding::InvalidBirthDate {
            player_id: player.id.clone(),
            value: player.date_of_birth.clone(),
        });
        return;
    };
    let mut age = on.year() - born.year();
    if (on.month(), on.day()) < (born.month(), born.day()) {
        age -= 1;
    }
    if !PLAUSIBLE_AGE.contains(&age) {
        findings.push(IntegrityFinding::ImplausibleAge {
            player_id: player.id.clone(),
            age,
        });
    }
}

fn audit_contracts(player: &Player, on: NaiveDate, findings: &mut Vec<IntegrityFinding>) {
    let Some(start) = player.contract_start() else {
        return;
    };
    let Some(end) = player.contract_end() else {
        return;
    };

    let parsed_start = NaiveDate::parse_from_str(start, "%Y-%m-%d");
    let parsed_end = NaiveDate::parse_from_str(end, "%Y-%m-%d");
    if parsed_start.is_err() {
        findings.push(IntegrityFinding::ContractDateInvalid {
            player_id: player.id.clone(),
            field: "start",
            value: start.to_string(),
        });
    }
    if parsed_end.is_err() {
        findings.push(IntegrityFinding::ContractDateInvalid {
            player_id: player.id.clone(),
            field: "end",
            value: end.to_string(),
        });
    }
    let (Ok(start_date), Ok(end_date)) = (parsed_start, parsed_end) else {
        return;
    };

    if end_date <= start_date {
        findings.push(IntegrityFinding::ContractEndsBeforeStart {
            player_id: player.id.clone(),
            start: start.to_string(),
            end: end.to_string(),
        });
    } else if end_date < on && player.team_id.is_some() {
        findings.push(IntegrityFinding::ContractExpiredAtAuditDate {
            player_id: player.id.clone(),
            end: end.to_string(),
        });
    }
}

fn audit_identities(players: &[Player], findings: &mut Vec<IntegrityFinding>) {
    let mut ids: HashSet<&str> = HashSet::new();
    let mut reported_ids: HashSet<&str> = HashSet::new();
    let mut identities: BTreeMap<(String, &str, &str), Vec<&str>> = BTreeMap::new();
    for player in players {
        if !ids.insert(player.id.as_str()) && reported_ids.insert(player.id.as_str()) {
            findings.push(IntegrityFinding::DuplicatePlayerId {
                id: player.id.clone(),
            });
        }
        identities
            .entry((
                player.full_name.trim().to_lowercase(),
                player.date_of_birth.as_str(),
                player.nationality.as_str(),
            ))
            .or_default()
            .push(player.id.as_str());
    }
    for ((_, date_of_birth, nationality), player_ids) in identities {
        // The same id twice is already reported above; two *different* ids are the defect here.
        let distinct: HashSet<&str> = player_ids.iter().copied().collect();
        if distinct.len() < 2 {
            continue;
        }
        let first = players.iter().find(|p| p.id == player_ids[0]);
        findings.push(IntegrityFinding::DuplicateIdentity {
            full_name: first.map(|p| p.full_name.clone()).unwrap_or_default(),
            date_of_birth: date_of_birth.to_string(),
            nationality: nationality.to_string(),
            player_ids: player_ids.iter().map(|id| (*id).to_string()).collect(),
        });
    }
}

fn audit_membership(
    teams: &[Team],
    players: &[Player],
    limits: &IntegrityLimits,
    findings: &mut Vec<IntegrityFinding>,
) {
    let mut seen_team_ids: HashSet<&str> = HashSet::new();
    let mut reported_team_ids: HashSet<&str> = HashSet::new();
    for team in teams {
        if !seen_team_ids.insert(team.id.as_str()) && reported_team_ids.insert(team.id.as_str()) {
            findings.push(IntegrityFinding::DuplicateTeamId {
                id: team.id.clone(),
            });
        }
    }
    let team_ids: HashSet<&str> = teams.iter().map(|team| team.id.as_str()).collect();
    let mut by_club: HashMap<&str, Vec<&Player>> = HashMap::new();
    for player in players {
        if let Some(loan) = &player.active_loan {
            if !team_ids.contains(loan.parent_team_id.as_str()) {
                findings.push(IntegrityFinding::UnknownParentClub {
                    player_id: player.id.clone(),
                    parent_team_id: loan.parent_team_id.clone(),
                });
            }
            if player
                .team_id
                .as_deref()
                .is_some_and(|team_id| team_id != loan.loan_team_id)
            {
                findings.push(IntegrityFinding::LoanClubMismatch {
                    player_id: player.id.clone(),
                    team_id: player.team_id.clone().unwrap_or_default(),
                    loan_team_id: loan.loan_team_id.clone(),
                });
            }
        }
        let Some(team_id) = player.team_id.as_deref() else {
            continue;
        };
        let borrower_outside_world = player
            .active_loan
            .as_ref()
            .is_some_and(|loan| loan.loan_team_name.is_some());
        if !team_ids.contains(team_id) {
            if !borrower_outside_world {
                findings.push(IntegrityFinding::UnknownClub {
                    player_id: player.id.clone(),
                    team_id: team_id.to_string(),
                });
            }
            continue;
        }
        if player.retired {
            findings.push(IntegrityFinding::RetiredPlayerAtClub {
                player_id: player.id.clone(),
                team_id: team_id.to_string(),
            });
        }
        by_club.entry(team_id).or_default().push(player);
    }
    for team in teams {
        let members = by_club.get(team.id.as_str()).map_or(&[][..], Vec::as_slice);
        let seniors = members
            .iter()
            .filter(|player| player.squad_role == SquadRole::Senior)
            .count();
        if seniors < limits.min_seniors {
            findings.push(IntegrityFinding::SquadTooSmall {
                team_id: team.id.clone(),
                seniors,
                minimum: limits.min_seniors,
            });
        }
        if members.len() > limits.max_roster {
            findings.push(IntegrityFinding::SquadTooLarge {
                team_id: team.id.clone(),
                players: members.len(),
                maximum: limits.max_roster,
            });
        }
        let at_club: HashSet<&str> = members.iter().map(|player| player.id.as_str()).collect();
        for player_id in team
            .starting_xi_ids
            .iter()
            .filter(|id| !at_club.contains(id.as_str()))
        {
            findings.push(IntegrityFinding::LineupPlayerNotAtClub {
                team_id: team.id.clone(),
                player_id: player_id.clone(),
            });
        }
    }
}

/// Audits `players` against `teams`. Never mutates; the order of findings is stable for a
/// given input.
pub fn audit_players(
    teams: &[Team],
    players: &[Player],
    limits: &IntegrityLimits,
) -> IntegrityReport {
    let mut findings = Vec::new();
    audit_identities(players, &mut findings);
    for player in players {
        audit_ratings(player, &mut findings);
        audit_age(player, limits.on, &mut findings);
        audit_contracts(player, limits.on, &mut findings);
    }
    audit_membership(teams, players, limits, &mut findings);
    IntegrityReport { findings }
}

/// Audits competition membership and fixtures against the teams the world contains.
///
/// `known_team_ids` is the union of club ids and national-team ids: a World Cup names
/// national teams, which are not in `teams`, and rejecting those would be a false
/// positive. Like [`audit_players`], this never mutates and the finding order is stable.
pub fn audit_competitions(
    competitions: &[League],
    known_team_ids: &HashSet<&str>,
) -> IntegrityReport {
    let mut findings = Vec::new();
    let mut fixture_ids: HashSet<&str> = HashSet::new();

    for competition in competitions {
        if competition.participant_ids.is_empty() {
            findings.push(IntegrityFinding::CompetitionWithoutParticipants {
                competition_id: competition.id.clone(),
            });
        }
        let participants: HashSet<&str> = competition
            .participant_ids
            .iter()
            .map(String::as_str)
            .collect();
        for team_id in &competition.participant_ids {
            if !known_team_ids.contains(team_id.as_str()) {
                findings.push(IntegrityFinding::UnknownCompetitionParticipant {
                    competition_id: competition.id.clone(),
                    team_id: team_id.clone(),
                });
            }
        }

        let mut pairs: HashSet<(&str, &str)> = HashSet::new();
        for fixture in &competition.fixtures {
            if !fixture_ids.insert(fixture.id.as_str()) {
                findings.push(IntegrityFinding::DuplicateFixtureId {
                    fixture_id: fixture.id.clone(),
                });
            }
            if fixture.home_team_id == fixture.away_team_id {
                findings.push(IntegrityFinding::FixtureSelfMatch {
                    competition_id: competition.id.clone(),
                    fixture_id: fixture.id.clone(),
                });
            } else if !pairs.insert((fixture.home_team_id.as_str(), fixture.away_team_id.as_str()))
            {
                findings.push(IntegrityFinding::DuplicateFixturePair {
                    competition_id: competition.id.clone(),
                    home_team_id: fixture.home_team_id.clone(),
                    away_team_id: fixture.away_team_id.clone(),
                });
            }
            for team_id in [&fixture.home_team_id, &fixture.away_team_id] {
                if !known_team_ids.contains(team_id.as_str()) {
                    findings.push(IntegrityFinding::FixtureUnknownTeam {
                        competition_id: competition.id.clone(),
                        fixture_id: fixture.id.clone(),
                        team_id: team_id.clone(),
                    });
                }
                if !participants.contains(team_id.as_str()) {
                    findings.push(IntegrityFinding::FixtureTeamNotParticipant {
                        competition_id: competition.id.clone(),
                        fixture_id: fixture.id.clone(),
                        team_id: team_id.clone(),
                    });
                }
            }
        }
    }

    IntegrityReport { findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::{ActiveLoan, Position};

    fn on() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 1).expect("valid date")
    }

    fn limits() -> IntegrityLimits {
        IntegrityLimits {
            min_seniors: 2,
            max_roster: 4,
            on: on(),
        }
    }

    fn team(id: &str) -> Team {
        Team::new(
            id.into(),
            id.into(),
            "TST".into(),
            "England".into(),
            "Town".into(),
            "Ground".into(),
            10_000,
        )
    }

    fn player(id: &str, team_id: Option<&str>) -> Player {
        let attributes = PlayerAttributes {
            pace: 60,
            stamina: 60,
            strength: 60,
            agility: 60,
            passing: 60,
            shooting: 60,
            tackling: 60,
            dribbling: 60,
            defending: 60,
            positioning: 60,
            vision: 60,
            decisions: 60,
            composure: 60,
            aggression: 60,
            teamwork: 60,
            leadership: 60,
            handling: 20,
            reflexes: 20,
            aerial: 60,
        };
        let mut player = Player::new(
            id.into(),
            id.into(),
            format!("Name {id}"),
            "2000-01-01".into(),
            "England".into(),
            Position::Midfielder,
            attributes,
        );
        player.team_id = team_id.map(str::to_string);
        player.ovr = natural_ovr(&player).round() as u8;
        player.potential = player.ovr;
        player.squad_role = SquadRole::Senior;
        player
    }

    fn loan(parent: &str, borrower: &str, borrower_name: Option<&str>) -> ActiveLoan {
        ActiveLoan {
            parent_team_id: parent.into(),
            loan_team_id: borrower.into(),
            loan_team_name: borrower_name.map(str::to_string),
            start_date: "2026-01-01".into(),
            end_date: "2026-12-31".into(),
            wage_contribution_pct: 0,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
        }
    }

    fn codes(report: &IntegrityReport) -> Vec<&'static str> {
        report.findings.iter().map(IntegrityFinding::code).collect()
    }

    /// Given a consistent two-club world, when audited, then nothing is reported.
    #[test]
    fn a_consistent_world_has_no_findings() {
        let teams = [team("a")];
        let players = [player("p1", Some("a")), player("p2", Some("a"))];
        let report = audit_players(&teams, &players, &limits());
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    /// Given two clubs sharing an id, when audited, then the id is reported once.
    #[test]
    fn a_repeated_team_id_is_reported_once() {
        let teams = [team("a"), team("a")];
        let players = [player("p1", Some("a")), player("p2", Some("a"))];
        let report = audit_players(&teams, &players, &limits());
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|f| f.code() == "duplicate_team_id")
                .count(),
            1
        );
    }

    /// Given the same id twice, when audited, then the id is reported once and is not also an identity clash.
    #[test]
    fn a_repeated_id_is_reported_once() {
        let teams = [team("a")];
        let players = [
            player("p1", Some("a")),
            player("p1", Some("a")),
            player("p2", Some("a")),
        ];
        let report = audit_players(&teams, &players, &limits());
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|f| f.code() == "duplicate_player_id")
                .count(),
            1
        );
        assert!(!codes(&report).contains(&"duplicate_identity"));
    }

    /// Given two players with the same name, birth date and nationality but different ids, when audited, then they are one identity clash.
    #[test]
    fn two_ids_for_one_identity_are_a_clash() {
        let teams = [team("a"), team("b")];
        let mut twin = player("p3", Some("b"));
        twin.full_name = "Name p1".into();
        let players = [
            player("p1", Some("a")),
            player("p2", Some("a")),
            twin,
            player("p4", Some("b")),
        ];
        let report = audit_players(&teams, &players, &limits());
        let clash = report
            .findings
            .iter()
            .find(|f| f.code() == "duplicate_identity")
            .expect("clash");
        assert!(
            matches!(clash, IntegrityFinding::DuplicateIdentity { player_ids, .. } if player_ids.len() == 2)
        );
    }

    /// Given impossible ratings, when audited, then each is reported with its player.
    #[test]
    fn impossible_ratings_are_reported() {
        let teams = [team("a")];
        let mut zero = player("p1", Some("a"));
        zero.ovr = 0;
        let mut below = player("p2", Some("a"));
        below.potential = below.ovr - 5;
        let mut overflow = player("p3", Some("a"));
        overflow.attributes.pace = 140;
        let report = audit_players(&teams, &[zero, below, overflow], &limits());
        let found = codes(&report);
        assert!(found.contains(&"rating_out_of_range"));
        assert!(found.contains(&"potential_below_overall"));
        assert!(found.contains(&"attribute_out_of_range"));
    }

    /// Given an ovr edited without its attributes, when audited, then the drift is reported; a small one is tolerated.
    #[test]
    fn ovr_drift_beyond_tolerance_is_reported() {
        let teams = [team("a")];
        let mut edited = player("p1", Some("a"));
        edited.ovr = 90;
        edited.potential = 95;
        let mut rounded = player("p2", Some("a"));
        rounded.ovr += 1;
        rounded.potential = rounded.ovr;
        let report = audit_players(&teams, &[edited, rounded], &limits());
        let drift: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.code() == "ovr_disagrees_with_attributes")
            .collect();
        assert_eq!(drift.len(), 1, "{:?}", report.findings);
    }

    /// Given players of an unknown club, a mismatched loan and a retiree, when audited, then each membership defect is named.
    #[test]
    fn inconsistent_memberships_are_reported() {
        let teams = [team("a"), team("b")];
        let ghost = player("p1", Some("nowhere"));
        let mut loanee = player("p2", Some("a"));
        loanee.active_loan = Some(loan("missing", "b", None));
        let mut retired = player("p3", Some("a"));
        retired.retired = true;
        let report = audit_players(&teams, &[ghost, loanee, retired], &limits());
        let found = codes(&report);
        for expected in [
            "unknown_club",
            "unknown_parent_club",
            "loan_club_mismatch",
            "retired_player_at_club",
        ] {
            assert!(found.contains(&expected), "{expected} in {found:?}");
        }
    }

    /// Given a loan to a club the world does not contain, when audited, then it is not an unknown-club error.
    #[test]
    fn a_loan_to_a_declared_outside_club_is_allowed() {
        let teams = [team("a")];
        let mut loanee = player("p1", Some("outside"));
        loanee.active_loan = Some(loan("a", "outside", Some("Elsewhere FC")));
        let report = audit_players(&teams, &[loanee, player("p2", Some("a"))], &limits());
        assert!(
            !codes(&report).contains(&"unknown_club"),
            "{:?}",
            report.findings
        );
    }

    /// Given a starved club, a bloated club and a lineup naming a stranger, when audited, then roster sizes and the lineup are flagged.
    #[test]
    fn roster_size_and_lineup_defects_are_reported() {
        let mut thin = team("thin");
        thin.starting_xi_ids = vec!["stranger".into()];
        let teams = [thin, team("fat")];
        let mut players = vec![player("t1", Some("thin"))];
        players.extend((0..5).map(|n| player(&format!("f{n}"), Some("fat"))));
        let report = audit_players(&teams, &players, &limits());
        let found = codes(&report);
        assert!(found.contains(&"squad_too_small"));
        assert!(found.contains(&"squad_too_large"));
        assert!(found.contains(&"lineup_player_not_at_club"));
        assert_eq!(
            report
                .warnings()
                .filter(|f| f.code() == "squad_too_large")
                .count(),
            1
        );
    }

    /// Given an unparsable and an implausible birth date, when audited, then both are reported with the right severity.
    #[test]
    fn bad_birth_dates_are_reported() {
        let teams = [team("a")];
        let mut garbled = player("p1", Some("a"));
        garbled.date_of_birth = "01/02/2000".into();
        let mut ancient = player("p2", Some("a"));
        ancient.date_of_birth = "1950-01-01".into();
        let report = audit_players(&teams, &[garbled, ancient], &limits());
        assert!(report.errors().any(|f| f.code() == "invalid_birth_date"));
        assert!(report.warnings().any(|f| f.code() == "implausible_age"));
    }

    /// Given a contract that ends before it starts, one with a bad date and one that has
    /// expired, when audited, then the first two are errors and expiry is a warning.
    #[test]
    fn contract_defects_are_reported() {
        let teams = [team("a")];
        let mut reversed = player("p1", Some("a"));
        reversed.stage_contract(Some("2027-06-30".into()), Some("2026-07-01".into()), 1_000);
        let mut invalid = player("p2", Some("a"));
        invalid.stage_contract(Some("not-a-date".into()), Some("2027-06-30".into()), 1_000);
        let mut expired = player("p3", Some("a"));
        expired.stage_contract(Some("2025-07-01".into()), Some("2026-01-01".into()), 1_000);

        let report = audit_players(&teams, &[reversed, invalid, expired], &limits());

        let errors: Vec<&'static str> = report.errors().map(IntegrityFinding::code).collect();
        assert!(errors.contains(&"contract_ends_before_start"), "{errors:?}");
        assert!(errors.contains(&"contract_date_invalid"), "{errors:?}");
        assert!(
            report
                .warnings()
                .any(|f| f.code() == "contract_expired_at_audit_date")
        );
    }

    /// Given a player on a valid contract, a free agent with no contract, and a player with no
    /// dates at all, when audited, then none is reported: a missing contract is the legitimate
    /// pre-career and free-agent state, not corruption.
    #[test]
    fn legitimate_contract_states_are_not_reported() {
        let teams = [team("a")];
        let mut contracted = player("p1", Some("a"));
        contracted.stage_contract(Some("2026-07-01".into()), Some("2027-06-30".into()), 1_000);
        let free_agent = player("p2", None);
        // The generated world before a career opens: a staged end but no start yet.
        let mut pre_career = player("p3", Some("a"));
        pre_career.stage_contract(None, Some("2027-06-30".into()), 1_000);

        let report = audit_players(&teams, &[contracted, free_agent, pre_career], &limits());

        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code().starts_with("contract_")),
            "{:?}",
            report.findings
        );
    }

    fn competition(
        id: &str,
        participants: &[&str],
        fixtures: Vec<domain::league::Fixture>,
    ) -> League {
        League {
            id: id.into(),
            participant_ids: participants.iter().map(|id| (*id).to_string()).collect(),
            fixtures,
            ..League::default()
        }
    }

    fn fixture(id: &str, home: &str, away: &str) -> domain::league::Fixture {
        domain::league::Fixture {
            id: id.into(),
            competition_id: "cup".into(),
            matchday: 1,
            date: "2026-08-01".into(),
            home_team_id: home.into(),
            away_team_id: away.into(),
            competition: domain::league::FixtureCompetition::Cup,
            status: domain::league::FixtureStatus::Scheduled,
            result: None,
            postponement: None,
        }
    }

    /// Given a consistent competition, when audited, then nothing is reported.
    #[test]
    fn a_consistent_competition_has_no_findings() {
        let known: HashSet<&str> = ["a", "b"].into_iter().collect();
        let clean = competition("cup", &["a", "b"], vec![fixture("f1", "a", "b")]);
        let report = audit_competitions(&[clean], &known);
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    /// Given a competition naming a stranger, fixtures with an unknown team, a self-match,
    /// a repeated id and a repeated pairing, when audited, then each defect is named and the
    /// repeated pairing is only a warning.
    #[test]
    fn competition_and_fixture_defects_are_reported() {
        let known: HashSet<&str> = ["a", "b"].into_iter().collect();
        let broken = competition(
            "broken",
            &["a", "ghost"],
            vec![
                fixture("dup", "a", "b"),
                fixture("dup", "b", "a"),
                fixture("self", "a", "a"),
                fixture("stranger", "a", "nowhere"),
                fixture("pair", "a", "b"),
            ],
        );
        let empty = competition("empty", &[], vec![]);

        let report = audit_competitions(&[broken, empty], &known);
        let found = codes(&report);
        for expected in [
            "unknown_competition_participant",
            "fixture_unknown_team",
            "fixture_self_match",
            "duplicate_fixture_id",
            "fixture_team_not_participant",
            "competition_without_participants",
        ] {
            assert!(found.contains(&expected), "{expected} in {found:?}");
        }
        assert!(found.contains(&"duplicate_fixture_pair"), "{found:?}");
        assert!(
            report
                .warnings()
                .any(|f| f.code() == "duplicate_fixture_pair"),
            "a replay is a warning, not an error"
        );
    }
}
