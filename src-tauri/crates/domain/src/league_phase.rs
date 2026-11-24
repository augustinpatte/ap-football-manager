//! Data of a UEFA-style league phase followed by two-legged knockout rounds.
//!
//! A league phase is *not* a round-robin and not a set of groups: 36 clubs share one ranking but
//! each plays only six or eight of the others, chosen by a draw out of seeding pots. This module
//! holds only the data (rules authored on the competition, state the competition accumulates) and
//! its pure value semantics. Everything that decides an outcome (the draw, the ranking, who
//! advances) lives in `ofm_core::league_phase`.
//!
//! Ownership: [`LeaguePhaseRules`] is authored and never changes during an edition;
//! [`LeaguePhaseState`] is owned by the competition's flow and rewritten only through it. Fixtures
//! stay in `League::fixtures` and rounds in `League::knockout_rounds`, so the calendar, the
//! completion proof and the screens read the same lists they read for every other format.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How a club's home and away matches are balanced against the pots it draws from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum HomeAwayRule {
    /// Each pot's two opponents are met once at home and once away (four pots of nine).
    #[default]
    PerPot,
    /// Pots are taken in consecutive pairs (1+2, 3+4, 5+6); of a club's opponents from the two
    /// pots of a pair, one is met at home and the other away (six pots of six).
    PairedPots,
}

/// The shape of the league phase. Plain data: two profiles ship ([`Self::four_pots_of_nine`] and
/// [`Self::six_pots_of_six`]) and an authored competition may carry its own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LeaguePhaseRules {
    pub pot_count: u8,
    /// Opponents each club draws from *every* pot (its own included).
    pub opponents_per_pot: u8,
    pub home_away: HomeAwayRule,
    /// Ranks that go straight to the round of 16.
    pub direct_places: u8,
    /// Ranks after those that go to the two-legged knockout play-offs.
    pub playoff_places: u8,
    /// Most opponents a club may draw from one *other* association.
    pub max_opponents_per_association: u8,
    /// Whether two clubs of one association may be drawn together. The regulations say "in
    /// principle" not; relaxing it is an explicit, separate decision of the authored rules.
    pub allow_same_association: bool,
    /// Most clubs one association may send to an edition, or `0` for no whole-edition limit.
    ///
    /// This is **not a UEFA rule**: UEFA limits who qualifies through each association's access
    /// list, not how many clubs of one association an edition may hold (2026/27 has five English
    /// and five Spanish Champions League clubs). It exists only as a game-balance stand-in for
    /// worlds that infer a field from reputation, where a strength-ordered field could hold more
    /// clubs of one association than any draw can place. A non-zero value is at most half a pot
    /// ([`Self::validate`]), which keeps every such field drawable. A field with a real access
    /// list carries `0`; whether it can be drawn is then decided by the draw itself, pot by pot.
    pub max_clubs_per_association: u8,
    /// The season's matchday and knockout dates.
    pub calendar: LeaguePhaseCalendar,
    /// Configurable prize and participation amounts (game balance, not UEFA's).
    pub prize: ContinentalPrize,
}

impl Default for LeaguePhaseRules {
    fn default() -> Self {
        Self::four_pots_of_nine()
    }
}

impl LeaguePhaseRules {
    /// Champions League and Europa League: 4 pots, 2 opponents from each, 8 matches.
    pub fn four_pots_of_nine() -> Self {
        Self {
            pot_count: 4,
            opponents_per_pot: 2,
            home_away: HomeAwayRule::PerPot,
            direct_places: 8,
            playoff_places: 16,
            max_opponents_per_association: 2,
            allow_same_association: false,
            max_clubs_per_association: 4,
            calendar: LeaguePhaseCalendar::default(),
            prize: ContinentalPrize::default(),
        }
    }

    /// Conference League: 6 pots, 1 opponent from each, 6 matches.
    pub fn six_pots_of_six() -> Self {
        Self {
            pot_count: 6,
            opponents_per_pot: 1,
            home_away: HomeAwayRule::PairedPots,
            max_clubs_per_association: 3,
            ..Self::four_pots_of_nine()
        }
    }

    /// Matches each club plays in the league phase.
    pub fn matches_per_club(&self) -> usize {
        usize::from(self.pot_count) * usize::from(self.opponents_per_pot)
    }

    /// Clubs in the competition: the places that move on plus the ones that go out.
    pub fn field_size(&self) -> usize {
        usize::from(self.pot_count) * self.pot_size()
    }

    /// Clubs per pot. The field is fixed at [`LEAGUE_PHASE_FIELD_SIZE`]; the pot size follows.
    pub fn pot_size(&self) -> usize {
        LEAGUE_PHASE_FIELD_SIZE / usize::from(self.pot_count.max(1))
    }

    /// Matches in the whole league phase: every fixture involves two clubs.
    pub fn league_fixture_count(&self) -> usize {
        LEAGUE_PHASE_FIELD_SIZE * self.matches_per_club() / 2
    }

    /// Clubs that enter the knockout phase (direct places and play-offs).
    pub fn knockout_entrants(&self) -> usize {
        usize::from(self.direct_places) + usize::from(self.playoff_places)
    }

    /// Whether the rules describe a field the draw can be asked to build. `Err` says why not, as
    /// a stable key an authoring tool can show.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.pot_count == 0
            || !LEAGUE_PHASE_FIELD_SIZE.is_multiple_of(usize::from(self.pot_count))
        {
            return Err("be.error.leaguePhase.rules.potCountMustDivideField");
        }
        if self.opponents_per_pot == 0 {
            return Err("be.error.leaguePhase.rules.opponentsPerPotMissing");
        }
        if self.home_away == HomeAwayRule::PerPot && self.opponents_per_pot != 2 {
            return Err("be.error.leaguePhase.rules.perPotNeedsTwoOpponents");
        }
        if self.home_away == HomeAwayRule::PairedPots
            && (self.opponents_per_pot != 1 || !self.pot_count.is_multiple_of(2))
        {
            return Err("be.error.leaguePhase.rules.pairedPotsNeedOneOpponentAndEvenPots");
        }
        // A pot must be able to hold the matches drawn inside it: with one opponent per pot a pot
        // needs an even size (a perfect matching); with two, at least three clubs (a cycle).
        let pot = self.pot_size();
        if (self.opponents_per_pot == 1 && !pot.is_multiple_of(2))
            || usize::from(self.opponents_per_pot) >= pot
        {
            return Err("be.error.leaguePhase.rules.potTooSmallForItsOwnDraw");
        }
        if self.knockout_entrants() != 24 || self.direct_places != 8 {
            return Err("be.error.leaguePhase.rules.knockoutShapeUnsupported");
        }
        if self.max_opponents_per_association == 0 {
            return Err("be.error.leaguePhase.rules.associationCapMissing");
        }
        // With the same-association rule on, one association's clubs in a pot need opponents from
        // the rest of that pot, which holds only `pot_size - theirs`: so no pot can take more than
        // half its places from one association. Bounding the whole edition's share by that makes
        // every field the limit admits pot-feasible, whatever its ranking does to the pots.
        if !self.allow_same_association
            && self.max_clubs_per_association != 0
            && usize::from(self.max_clubs_per_association) > self.pot_size() / 2
        {
            return Err("be.error.leaguePhase.rules.associationLimitExceedsPotCapacity");
        }
        Ok(())
    }
}

/// Clubs in a league phase. The ranking, the 8 / 16 / 12 split and the bracket are all defined
/// for exactly this many.
pub const LEAGUE_PHASE_FIELD_SIZE: usize = 36;

/// The dates of one edition, authored for a reference season. Later seasons shift the reference
/// dates by whole weeks, so a Tuesday stays a Tuesday.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct LeaguePhaseCalendar {
    /// Bumped when the authored dates change, so a save can tell which template it was built from.
    pub template_version: u32,
    /// The season (year it starts) the dates below are written for.
    pub reference_season: u32,
    /// Free text: where the dates come from and how far they can be trusted.
    pub provenance: String,
    /// For each league-phase matchday, the ISO dates its matches are spread across.
    pub league_matchdays: Vec<Vec<String>>,
    /// One window per knockout round, in order: play-offs, round of 16, quarter-finals,
    /// semi-finals, final.
    pub knockout_windows: Vec<KnockoutWindow>,
}

/// The dates of one knockout round. A tie's first leg falls on one of `first_leg`, its second on
/// one of `second_leg`; a single-match round (the final) has no second leg.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct KnockoutWindow {
    pub stage: KnockoutStage,
    pub first_leg: Vec<String>,
    pub second_leg: Vec<String>,
}

/// Prize and participation amounts the game posts for a continental edition, whole currency
/// units. These are **game-balance assumptions, not UEFA's published distribution**.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ContinentalPrize {
    pub participation: i64,
    pub per_win: i64,
    pub per_draw: i64,
    /// Paid once to each club that reaches the stage, in [`KnockoutStage`] order from the
    /// play-offs to the final; one entry per stage. The champion also gets `champion`.
    pub per_stage_reached: Vec<i64>,
    pub champion: i64,
}

impl ContinentalPrize {
    pub fn is_authored(&self) -> bool {
        self.participation != 0
            || self.per_win != 0
            || self.per_draw != 0
            || self.champion != 0
            || self.per_stage_reached.iter().any(|amount| *amount != 0)
    }
}

/// A round of the knockout phase, in the order they are played.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum KnockoutStage {
    #[default]
    Playoffs,
    RoundOf16,
    QuarterFinals,
    SemiFinals,
    Final,
}

impl KnockoutStage {
    pub const ALL: [Self; 5] = [
        Self::Playoffs,
        Self::RoundOf16,
        Self::QuarterFinals,
        Self::SemiFinals,
        Self::Final,
    ];

    /// Index in [`Self::ALL`].
    pub fn index(self) -> usize {
        match self {
            Self::Playoffs => 0,
            Self::RoundOf16 => 1,
            Self::QuarterFinals => 2,
            Self::SemiFinals => 3,
            Self::Final => 4,
        }
    }

    /// Ties the round holds.
    pub fn tie_count(self) -> usize {
        match self {
            Self::Playoffs | Self::RoundOf16 => 8,
            Self::QuarterFinals => 4,
            Self::SemiFinals => 2,
            Self::Final => 1,
        }
    }

    /// Matches in each tie: every round but the final is two-legged.
    pub fn legs(self) -> usize {
        if self == Self::Final { 1 } else { 2 }
    }

    pub fn next(self) -> Option<Self> {
        Self::ALL.get(self.index() + 1).copied()
    }

    /// The competition stage that is current while this round is played.
    pub fn competition_stage(self) -> CompetitionStage {
        match self {
            Self::Playoffs => CompetitionStage::KnockoutPlayoffs,
            Self::RoundOf16 => CompetitionStage::RoundOf16,
            Self::QuarterFinals => CompetitionStage::QuarterFinals,
            Self::SemiFinals => CompetitionStage::SemiFinals,
            Self::Final => CompetitionStage::Final,
        }
    }
}

/// Where an edition is in its life. Persisted, so progress is never inferred from fixture counts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CompetitionStage {
    /// Built, the draw not yet made (or not makeable: see `draw_error`).
    #[default]
    Created,
    /// Draw made, fixtures dated, nothing played.
    LeaguePhaseScheduled,
    LeaguePhaseInProgress,
    /// Every league-phase match is played and the final ranking is fixed; the play-offs are not
    /// drawn yet (a state a retry leaves again).
    LeaguePhaseCompleted,
    KnockoutPlayoffs,
    RoundOf16,
    QuarterFinals,
    SemiFinals,
    Final,
    Completed,
}

impl CompetitionStage {
    /// The knockout round being played, if the competition is in one.
    pub fn knockout_stage(self) -> Option<KnockoutStage> {
        match self {
            Self::KnockoutPlayoffs => Some(KnockoutStage::Playoffs),
            Self::RoundOf16 => Some(KnockoutStage::RoundOf16),
            Self::QuarterFinals => Some(KnockoutStage::QuarterFinals),
            Self::SemiFinals => Some(KnockoutStage::SemiFinals),
            Self::Final => Some(KnockoutStage::Final),
            _ => None,
        }
    }

    pub fn is_league_phase(self) -> bool {
        matches!(
            self,
            Self::LeaguePhaseScheduled | Self::LeaguePhaseInProgress
        )
    }
}

/// One club's row of the league-phase table. Everything here is *derived* from the completed
/// league-phase fixtures and rewritten from them on every result, never incremented: finalising a
/// result twice cannot count it twice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct LeaguePhaseRow {
    pub team_id: String,
    /// 1-based position under the regulations' order at the time of the last result.
    pub position: u32,
    pub played: u32,
    pub won: u32,
    pub drawn: u32,
    pub lost: u32,
    pub goals_for: u32,
    pub goals_against: u32,
    pub away_goals_for: u32,
    pub away_wins: u32,
    pub points: u32,
    /// Collective points, goal difference and goals scored of this club's league-phase opponents.
    pub opponents_points: u32,
    pub opponents_goal_difference: i32,
    pub opponents_goals_for: u32,
    /// Yellow 1, red 3 from the match reports. Matches settled by scoreline carry no cards.
    pub disciplinary_points: u32,
    /// The coefficient supplied for this club, when one was. Never invented.
    pub coefficient: Option<u32>,
}

impl LeaguePhaseRow {
    pub fn goal_difference(&self) -> i32 {
        self.goals_for as i32 - self.goals_against as i32
    }

    /// The row as the plain table entry every other competition keeps, so what reads
    /// `League::standings` (the active-scope check, the history) sees the league phase too.
    pub fn standing(&self) -> crate::league::StandingEntry {
        crate::league::StandingEntry {
            team_id: self.team_id.clone(),
            played: self.played,
            won: self.won,
            drawn: self.drawn,
            lost: self.lost,
            goals_for: self.goals_for,
            goals_against: self.goals_against,
            points: self.points,
        }
    }
}

/// How the table ended: who goes where. Written once, when the league phase completes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct LeaguePhaseOutcome {
    /// All clubs, best first.
    pub ranking: Vec<String>,
    /// Ranks 1–8, in rank order.
    pub direct: Vec<String>,
    /// Ranks 9–24, in rank order.
    pub playoff: Vec<String>,
    /// Ranks 25–36, in rank order.
    pub eliminated: Vec<String>,
    /// Whether any tiebreaker had to fall back past the data available (club coefficient not
    /// supplied, so the seeding order stood in for it).
    pub coefficient_fallback_used: bool,
}

/// How a two-legged tie, or the final, was decided.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TieDecision {
    Aggregate,
    ExtraTime,
    Penalties,
}

/// A knockout tie's settled outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TieResolution {
    pub winner_id: String,
    /// Goals over the tie by each side, extra time included, shootouts not.
    pub seeded_goals: u16,
    pub unseeded_goals: u16,
    /// Shootout score `(seeded, unseeded)` when one decided it.
    pub penalties: Option<(u8, u8)>,
    pub decision: TieDecision,
}

/// One knockout tie: a fixed place in the bracket that clubs move through.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct KnockoutTie {
    pub id: String,
    pub stage: KnockoutStage,
    /// Place within the round, `0..stage.tie_count()`.
    pub slot: u8,
    /// The bracket path the tie lies on (`1..=8`, the round-of-16 seed it descends from; the
    /// lowest path hosts the return leg). The club that wins takes over this path.
    pub path: u8,
    /// Hosts the return leg (the higher seed, or the lower path).
    pub seeded_team_id: String,
    /// Hosts the first leg.
    pub unseeded_team_id: String,
    /// Fixture ids in the order played; one for the final.
    pub fixture_ids: Vec<String>,
    pub resolution: Option<TieResolution>,
}

impl KnockoutTie {
    pub fn winner_id(&self) -> Option<&str> {
        self.resolution
            .as_ref()
            .map(|resolution| resolution.winner_id.as_str())
    }
}

/// Everything a league-phase competition accumulates beyond its fixtures.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct LeaguePhaseState {
    pub stage: CompetitionStage,
    /// The calendar year the edition's dates were generated for (the year its first matchday
    /// falls in), so later rounds are dated from the same template shift.
    pub calendar_year: i32,
    /// Seed of the draw; the same seed and field give the same draw.
    pub draw_seed: u64,
    /// Clubs strongest first, as the draw was fed them. The pots are chunks of this order.
    pub seeding: Vec<String>,
    pub pots: Vec<Vec<String>>,
    /// Association of each club at the time of the draw, by explicit identity (never read from
    /// a name).
    pub associations: BTreeMap<String, String>,
    /// Club coefficients, only when supplied by the world; empty otherwise.
    pub coefficients: BTreeMap<String, u32>,
    pub table: Vec<LeaguePhaseRow>,
    pub outcome: Option<LeaguePhaseOutcome>,
    pub ties: Vec<KnockoutTie>,
    pub champion_id: Option<String>,
    /// Why the draw could not be made, as a stable key. Present only while it is unmade.
    pub draw_error: Option<String>,
    /// The field the next edition will be drawn from, recorded at a rollover that found this
    /// edition still being played. Empty when none is waiting.
    pub next_entrants: Vec<String>,
    /// Prize and participation postings already made this edition, by stable key, so a payment
    /// posts once and only once.
    pub payments_posted: Vec<String>,
    /// Clubs that traded places across a pot boundary because the pots as ranked clustered one
    /// association too tightly for any draw to satisfy the same-association rule. Empty when the
    /// pots stand as ranked, which is every normal edition. See `ofm_core::league_phase::pots`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pot_adjustments: Vec<PotAdjustment>,
}

/// One swap of two clubs between adjacent pots, recorded so the departure from the strict
/// ranking is visible and checkable, never silent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PotAdjustment {
    /// The club of the crowded association that moved to `to_pot`.
    pub moved_club_id: String,
    /// The club of another association that took its place in `from_pot`.
    pub exchanged_club_id: String,
    /// Zero-based pot the moved club left.
    pub from_pot: u8,
    pub to_pot: u8,
}

impl LeaguePhaseState {
    pub fn ties_of(&self, stage: KnockoutStage) -> impl Iterator<Item = &KnockoutTie> {
        self.ties.iter().filter(move |tie| tie.stage == stage)
    }

    /// The ids of the fixtures that belong to knockout ties; every other fixture of the
    /// competition is a league-phase match.
    pub fn knockout_fixture_ids(&self) -> std::collections::BTreeSet<&str> {
        self.ties
            .iter()
            .flat_map(|tie| tie.fixture_ids.iter().map(String::as_str))
            .collect()
    }

    /// The tie a fixture belongs to.
    pub fn tie_of_fixture(&self, fixture_id: &str) -> Option<&KnockoutTie> {
        self.ties
            .iter()
            .find(|tie| tie.fixture_ids.iter().any(|id| id == fixture_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given the two shipped profiles
    /// When their derived sizes are read
    /// Then 4×9 plays eight matches (144 fixtures) and 6×6 plays six (108), and both validate.
    #[test]
    fn the_shipped_profiles_have_the_regulation_shape() {
        let big = LeaguePhaseRules::four_pots_of_nine();
        let small = LeaguePhaseRules::six_pots_of_six();

        assert_eq!(
            (
                big.matches_per_club(),
                big.league_fixture_count(),
                big.pot_size()
            ),
            (8, 144, 9)
        );
        assert_eq!(
            (
                small.matches_per_club(),
                small.league_fixture_count(),
                small.pot_size()
            ),
            (6, 108, 6)
        );
        assert_eq!(big.validate(), Ok(()));
        assert_eq!(small.validate(), Ok(()));
        assert_eq!(big.knockout_entrants(), 24);
    }

    /// Given rules that no draw could satisfy
    /// When they are validated
    /// Then each is refused with its own key rather than defaulted.
    #[test]
    fn impossible_rules_are_refused_not_defaulted() {
        let mut rules = LeaguePhaseRules::four_pots_of_nine();
        rules.pot_count = 5;
        assert_eq!(
            rules.validate(),
            Err("be.error.leaguePhase.rules.potCountMustDivideField")
        );

        let mut rules = LeaguePhaseRules::six_pots_of_six();
        rules.opponents_per_pot = 2;
        assert_eq!(
            rules.validate(),
            Err("be.error.leaguePhase.rules.pairedPotsNeedOneOpponentAndEvenPots")
        );

        let mut rules = LeaguePhaseRules::four_pots_of_nine();
        rules.direct_places = 4;
        assert_eq!(
            rules.validate(),
            Err("be.error.leaguePhase.rules.knockoutShapeUnsupported")
        );
    }

    /// Given a state with a tie
    /// When it is serialised and read back
    /// Then every field survives, and an empty JSON object loads as the default state.
    #[test]
    fn state_round_trips_and_old_json_loads_as_default() {
        let mut state = LeaguePhaseState {
            stage: CompetitionStage::RoundOf16,
            draw_seed: 42,
            ..Default::default()
        };
        state.ties.push(KnockoutTie {
            id: "t".into(),
            stage: KnockoutStage::RoundOf16,
            slot: 3,
            path: 4,
            seeded_team_id: "a".into(),
            unseeded_team_id: "b".into(),
            fixture_ids: vec!["f1".into(), "f2".into()],
            resolution: Some(TieResolution {
                winner_id: "b".into(),
                seeded_goals: 1,
                unseeded_goals: 2,
                penalties: None,
                decision: TieDecision::Aggregate,
            }),
        });

        let back: LeaguePhaseState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();

        assert_eq!(back, state);
        assert_eq!(
            serde_json::from_str::<LeaguePhaseState>("{}").unwrap(),
            LeaguePhaseState::default()
        );
        assert_eq!(state.tie_of_fixture("f2").map(|t| t.slot), Some(3));
    }

    /// Given a state with every field populated
    /// When it is serialised and read back
    /// Then every field survives (the table, the outcome, the champion, the postings, the waiting field).
    #[test]
    fn a_fully_populated_state_survives_a_round_trip() {
        let state = LeaguePhaseState {
            stage: CompetitionStage::Completed,
            calendar_year: 2033,
            draw_seed: 9,
            seeding: vec!["a".into(), "b".into()],
            pots: vec![vec!["a".into()], vec!["b".into()]],
            associations: [("a".to_string(), "X".to_string())].into(),
            coefficients: [("a".to_string(), 80)].into(),
            table: vec![LeaguePhaseRow {
                team_id: "a".into(),
                position: 1,
                played: 8,
                won: 5,
                drawn: 2,
                lost: 1,
                goals_for: 15,
                goals_against: 7,
                away_goals_for: 6,
                away_wins: 2,
                points: 17,
                opponents_points: 40,
                opponents_goal_difference: -3,
                opponents_goals_for: 90,
                disciplinary_points: 4,
                coefficient: Some(80),
            }],
            outcome: Some(LeaguePhaseOutcome {
                ranking: vec!["a".into()],
                direct: vec!["a".into()],
                playoff: vec![],
                eliminated: vec![],
                coefficient_fallback_used: true,
            }),
            ties: vec![],
            champion_id: Some("a".into()),
            draw_error: Some("be.error.leaguePhase.draw.infeasible".into()),
            next_entrants: vec!["b".into(), "a".into()],
            payments_posted: vec!["champion:a".into()],
        };

        let back: LeaguePhaseState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();

        assert_eq!(back, state);
    }

    /// Given a competition saved before the league phase existed
    /// When it is loaded and saved again
    /// Then it carries no league-phase key either way, and a league-phase competition's table is the
    ///      order `sorted_standings` returns (Article 18, not points-goal difference-goals).
    #[test]
    fn old_competitions_load_without_league_phase_data_and_a_league_phase_table_keeps_its_order() {
        use crate::league::{CompetitionRules, League};
        let old = serde_json::json!({"id": "x", "name": "X", "season": 2030});
        let mut league: League = serde_json::from_value(old).unwrap();
        assert!(league.league_phase.is_none() && league.rules.league_phase.is_none());
        let written = serde_json::to_value(&league).unwrap();
        assert!(written.get("league_phase").is_none());
        assert!(
            serde_json::to_value(CompetitionRules::default())
                .unwrap()
                .get("league_phase")
                .is_none()
        );

        let row = |id: &str, points: u32, goals_for: u32| LeaguePhaseRow {
            team_id: id.into(),
            points,
            goals_for,
            ..Default::default()
        };
        // The first row has fewer goals: the generic order would put the second above it.
        league.league_phase = Some(LeaguePhaseState {
            table: vec![row("first", 10, 5), row("second", 10, 9)],
            ..Default::default()
        });

        let order: Vec<String> = league
            .sorted_standings()
            .into_iter()
            .map(|e| e.team_id)
            .collect();

        assert_eq!(order, vec!["first".to_string(), "second".to_string()]);
    }

    /// Given a state with ties of two rounds
    /// When the knockout fixture ids and a round's successor are asked
    /// Then they are the legs of every tie, and the rounds follow in order to the final.
    #[test]
    fn knockout_fixture_ids_and_stage_order_are_what_the_state_holds() {
        let tie = |stage, ids: &[&str]| KnockoutTie {
            stage,
            fixture_ids: ids.iter().map(|s| (*s).to_string()).collect(),
            ..Default::default()
        };
        let state = LeaguePhaseState {
            ties: vec![
                tie(KnockoutStage::Playoffs, &["p1", "p2"]),
                tie(KnockoutStage::Final, &["f"]),
            ],
            ..Default::default()
        };

        let ids: Vec<&str> = state.knockout_fixture_ids().into_iter().collect();

        assert_eq!(ids, vec!["f", "p1", "p2"]);
        assert_eq!(
            KnockoutStage::Playoffs.next(),
            Some(KnockoutStage::RoundOf16)
        );
        assert_eq!(KnockoutStage::Final.next(), None);
    }
}
