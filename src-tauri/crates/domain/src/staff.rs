use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Staff {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub date_of_birth: String,
    pub nationality: String,
    #[serde(default)]
    pub football_nation: String,
    #[serde(default)]
    pub birth_country: Option<String>,
    pub role: StaffRole,

    // Attributes 0-100
    pub attributes: StaffAttributes,
    pub team_id: Option<String>,

    // Coaching specialization — boosts one training focus area
    #[serde(default)]
    pub specialization: Option<CoachingSpecialization>,

    // Contract & finances
    #[serde(default)]
    pub wage: u32,
    #[serde(default)]
    pub contract_end: Option<String>,

    /// What they are called when it differs from their name ("Pep" for Josep Guardiola i Sala).
    #[serde(default)]
    pub known_as: Option<String>,
    /// The one name to show for them: `known_as` if set, else first and last name. Kept up to
    /// date by [`Staff::new`], [`Staff::set_known_as`] and the loader, so a reader renders it
    /// as it is and never decides between the names itself.
    #[serde(default)]
    pub display_name: String,

    /// The post they fill when it is finer than `role` + `specialization` can say (head of
    /// youth, analyst...). `None` means "derive it": see [`Staff::post`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<StaffPost>,
    /// The FM24 training attributes that the four OFM attributes were folded from. Present only
    /// for staff imported from a save; generated staff have none and the four attributes stand
    /// in for them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<StaffDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StaffRole {
    AssistantManager,
    Coach,
    Scout,
    Physio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CoachingSpecialization {
    Fitness,     // Boosts Physical training
    Technique,   // Boosts Technical training
    Tactics,     // Boosts Tactical training
    Defending,   // Boosts Defending training
    Attacking,   // Boosts Attacking training
    GoalKeeping, // Boosts GK-specific development
    Youth,       // Boosts young player development
}

#[derive(Debug, Clone, Serialize, Deserialize)]
// camelCase matches the package convention (see SCHEMA_REFERENCE) and the World
// Editor frontend, so authored staff attributes round-trip both ways. The
// snake_case aliases keep existing saves (serialized before this change) and
// snake_case-authored packages loading.
#[serde(rename_all = "camelCase")]
pub struct StaffAttributes {
    pub coaching: u8,
    #[serde(alias = "judging_ability")]
    pub judging_ability: u8,
    #[serde(alias = "judging_potential")]
    pub judging_potential: u8,
    pub physiotherapy: u8,
}

/// A seat on a club's staff. `StaffRole` says what kind of person this is; the post says which
/// seat they fill, and a club has a limit and a minimum for each seat.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StaffPost {
    AssistantManager,
    FirstTeamCoach,
    FitnessCoach,
    GoalkeepingCoach,
    YouthCoach,
    HeadOfYouth,
    Scout,
    HeadOfScouting,
    Analyst,
    Physio,
    HeadOfMedical,
}

impl StaffPost {
    pub const ALL: [StaffPost; 11] = [
        StaffPost::AssistantManager,
        StaffPost::FirstTeamCoach,
        StaffPost::FitnessCoach,
        StaffPost::GoalkeepingCoach,
        StaffPost::YouthCoach,
        StaffPost::HeadOfYouth,
        StaffPost::Scout,
        StaffPost::HeadOfScouting,
        StaffPost::Analyst,
        StaffPost::Physio,
        StaffPost::HeadOfMedical,
    ];

    /// The most a club can employ in this seat.
    pub fn limit(self) -> usize {
        match self {
            StaffPost::AssistantManager | StaffPost::HeadOfYouth => 1,
            StaffPost::HeadOfScouting | StaffPost::HeadOfMedical => 1,
            StaffPost::FitnessCoach | StaffPost::GoalkeepingCoach => 2,
            StaffPost::YouthCoach | StaffPost::Analyst => 2,
            StaffPost::FirstTeamCoach | StaffPost::Physio => 3,
            StaffPost::Scout => 6,
        }
    }

    /// The fewest a club needs before the seat counts as vacant.
    pub fn required(self) -> usize {
        match self {
            StaffPost::AssistantManager
            | StaffPost::FirstTeamCoach
            | StaffPost::FitnessCoach
            | StaffPost::GoalkeepingCoach
            | StaffPost::Physio
            | StaffPost::Scout => 1,
            _ => 0,
        }
    }

    /// The `StaffRole` a person in this seat has.
    pub fn role(self) -> StaffRole {
        match self {
            StaffPost::AssistantManager => StaffRole::AssistantManager,
            StaffPost::FirstTeamCoach
            | StaffPost::FitnessCoach
            | StaffPost::GoalkeepingCoach
            | StaffPost::YouthCoach
            | StaffPost::HeadOfYouth => StaffRole::Coach,
            StaffPost::Scout | StaffPost::HeadOfScouting | StaffPost::Analyst => StaffRole::Scout,
            StaffPost::Physio | StaffPost::HeadOfMedical => StaffRole::Physio,
        }
    }

    /// The coaching specialisation that goes with the seat, if it has one.
    pub fn specialization(self) -> Option<CoachingSpecialization> {
        match self {
            StaffPost::FitnessCoach => Some(CoachingSpecialization::Fitness),
            StaffPost::GoalkeepingCoach => Some(CoachingSpecialization::GoalKeeping),
            StaffPost::YouthCoach | StaffPost::HeadOfYouth => Some(CoachingSpecialization::Youth),
            _ => None,
        }
    }

    /// Heads lead their department: they count for more in its effects.
    pub fn is_head(self) -> bool {
        matches!(
            self,
            StaffPost::HeadOfYouth | StaffPost::HeadOfScouting | StaffPost::HeadOfMedical
        )
    }
}

/// FM24 staff attributes (0-100) behind the four OFM attributes. `0` means the save did not give
/// the value; readers go through the accessors so they never treat a missing value as a skill.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct StaffDetail {
    pub attacking: u8,
    pub defending: u8,
    pub technical: u8,
    pub tactical: u8,
    pub mental: u8,
    pub fitness: u8,
    pub goalkeeping: u8,
    pub determination: u8,
    pub motivating: u8,
    pub discipline: u8,
    pub analysing: u8,
    pub sports_science: u8,
}

impl StaffDetail {
    /// The value, or `None` when the save did not give it.
    pub fn known(value: u8) -> Option<u8> {
        (value > 0).then_some(value)
    }
}

impl Staff {
    /// The seat this person fills: the one written on the record, else derived from role and
    /// specialisation.
    pub fn post(&self) -> StaffPost {
        if let Some(post) = self.post {
            return post;
        }
        match (&self.role, &self.specialization) {
            (StaffRole::AssistantManager, _) => StaffPost::AssistantManager,
            (StaffRole::Scout, _) => StaffPost::Scout,
            (StaffRole::Physio, _) => StaffPost::Physio,
            (StaffRole::Coach, Some(CoachingSpecialization::Fitness)) => StaffPost::FitnessCoach,
            (StaffRole::Coach, Some(CoachingSpecialization::GoalKeeping)) => {
                StaffPost::GoalkeepingCoach
            }
            (StaffRole::Coach, Some(CoachingSpecialization::Youth)) => StaffPost::YouthCoach,
            (StaffRole::Coach, _) => StaffPost::FirstTeamCoach,
        }
    }

    /// How good they are at the seat they fill, 0-100: the attributes that seat actually uses,
    /// weighted the way the simulation reads them.
    pub fn overall(&self) -> u8 {
        let a = &self.attributes;
        let value = match self.post() {
            StaffPost::AssistantManager => {
                (u32::from(a.coaching) * 4
                    + u32::from(a.judging_ability) * 3
                    + u32::from(a.judging_potential) * 3)
                    / 10
            }
            StaffPost::Scout | StaffPost::HeadOfScouting => {
                (u32::from(a.judging_ability) + u32::from(a.judging_potential)) / 2
            }
            StaffPost::Analyst => {
                let judging = (u32::from(a.judging_ability) + u32::from(a.judging_potential)) / 2;
                match self
                    .detail
                    .as_ref()
                    .and_then(|d| StaffDetail::known(d.analysing))
                {
                    Some(analysing) => (u32::from(analysing) * 6 + judging * 4) / 10,
                    None => judging,
                }
            }
            StaffPost::Physio | StaffPost::HeadOfMedical => u32::from(a.physiotherapy),
            _ => u32::from(a.coaching),
        };
        value.min(100) as u8
    }
}

impl Staff {
    pub fn new(
        id: String,
        first_name: String,
        last_name: String,
        date_of_birth: String,
        role: StaffRole,
        attributes: StaffAttributes,
    ) -> Self {
        Self {
            id,
            first_name,
            last_name,
            date_of_birth,
            nationality: String::new(),
            football_nation: String::new(),
            birth_country: None,
            role,
            attributes,
            team_id: None,
            specialization: None,
            wage: 0,
            contract_end: None,
            known_as: None,
            display_name: String::new(),
            post: None,
            detail: None,
        }
        .with_display_name_refreshed()
    }

    fn with_display_name_refreshed(mut self) -> Self {
        self.refresh_display_name();
        self
    }

    /// First and last name together.
    pub fn full_name(&self) -> String {
        crate::identity::display_name(None, &self.first_name, &self.last_name)
    }

    /// Sets what they are called; blank means nothing in particular.
    pub fn set_known_as(&mut self, known_as: Option<String>) {
        self.known_as = known_as;
        self.refresh_display_name();
    }

    /// Tidies `known_as` and recomputes `display_name` from the names it depends on. Every way a
    /// record enters the game (a setter, a world file, a save) ends here, so a blank known-as
    /// can never survive as a blank display name.
    pub fn refresh_display_name(&mut self) {
        self.known_as = crate::identity::tidy_known_as(self.known_as.take());
        self.display_name = crate::identity::display_name(
            self.known_as.as_deref(),
            &self.first_name,
            &self.last_name,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Staff, StaffAttributes, StaffRole};

    fn staff() -> Staff {
        Staff::new(
            "s1".into(),
            "Josep".into(),
            "Guardiola i Sala".into(),
            "1971-01-18".into(),
            StaffRole::Coach,
            StaffAttributes {
                coaching: 80,
                judging_ability: 50,
                judging_potential: 50,
                physiotherapy: 10,
            },
        )
    }

    /// Given a staff member with and without a known-as name (#557)
    /// When their display name is read
    /// Then it is the known-as name when set, else the full name, and a blank known-as is none.
    #[test]
    fn the_display_name_is_the_known_as_name_or_the_full_name() {
        let mut member = staff();
        assert_eq!(member.display_name, "Josep Guardiola i Sala");

        member.set_known_as(Some("Pep Guardiola".into()));
        assert_eq!(member.display_name, "Pep Guardiola");
        assert_eq!(member.full_name(), "Josep Guardiola i Sala");

        member.set_known_as(Some("   ".into()));
        assert_eq!(member.known_as, None);
        assert_eq!(member.display_name, "Josep Guardiola i Sala");
    }

    #[test]
    fn deserializes_camelcase_attributes() {
        let json =
            r#"{"coaching":70,"judgingAbility":65,"judgingPotential":60,"physiotherapy":40}"#;
        let attrs: StaffAttributes = serde_json::from_str(json).expect("camelCase should parse");
        assert_eq!(attrs.judging_ability, 65);
        assert_eq!(attrs.judging_potential, 60);
    }

    #[test]
    fn still_deserializes_snakecase_attributes_from_existing_saves() {
        let json =
            r#"{"coaching":70,"judging_ability":65,"judging_potential":60,"physiotherapy":40}"#;
        let attrs: StaffAttributes =
            serde_json::from_str(json).expect("snake_case alias should parse");
        assert_eq!(attrs.judging_ability, 65);
        assert_eq!(attrs.judging_potential, 60);
    }

    #[test]
    fn serializes_as_camelcase_so_the_editor_can_read_it_back() {
        let attrs = StaffAttributes {
            coaching: 70,
            judging_ability: 65,
            judging_potential: 60,
            physiotherapy: 40,
        };
        let json = serde_json::to_string(&attrs).expect("should serialize");
        assert!(
            json.contains("\"judgingAbility\":65"),
            "serialized as: {json}"
        );
        assert!(
            json.contains("\"judgingPotential\":60"),
            "serialized as: {json}"
        );
        // Round-trips back to the same struct.
        let back: StaffAttributes = serde_json::from_str(&json).expect("round-trip");
        assert_eq!(back.judging_ability, 65);
    }
}
