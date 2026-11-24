//! Adapter parameters (`config/adapter.toml`), compiled in so a given binary always uses the
//! parameters it was tested with.

use crate::Result;
use serde::Deserialize;

pub const DEFAULT_CONFIG: &str = include_str!("../config/adapter.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub world: WorldCfg,
    pub money: MoneyCfg,
    pub start: StartCfg,
    pub contracts: ContractsCfg,
    pub loans: LoansCfg,
    pub ratings: RatingsCfg,
    pub positions: PositionsCfg,
    pub squad: SquadCfg,
    pub finance: FinanceCfg,
    pub free_agents: FreeAgentsCfg,
    pub valuation: ValuationCfg,
    pub value_scale: ValueScaleCfg,
    #[serde(default)]
    pub competitions: CompetitionsCfg,
    #[serde(default)]
    pub calendar: CalendarCfg,
}

/// How each league's rounds fall across the week. Generated, not the official fixture lists: the
/// shape of a football week in that country (which days, in what proportion, how many midweek
/// rounds), not any season's dates.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CalendarCfg {
    /// One entry per league that has a pattern; a league with none keeps the one-day-a-week cadence.
    #[serde(default)]
    pub league: Vec<LeagueCalendarCfg>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeagueCalendarCfg {
    /// The country's OFM code (`ENG`, `FR`, ...) and the division within it (1 = top).
    pub country: String,
    pub tier: i64,
    /// Relative weight of each weekday, Monday first, for an ordinary round.
    pub weekend: [u8; 7],
    /// The same for a midweek round.
    pub midweek: [u8; 7],
    /// Rounds played midweek at least; the scheduler adds the ones the window needs.
    pub midweek_rounds: u32,
    /// The month and day the regular season should be over by.
    pub season_end_month: u8,
    pub season_end_day: u8,
}

impl CalendarCfg {
    pub fn league(&self, country: &str, tier: i64) -> Option<&LeagueCalendarCfg> {
        self.league
            .iter()
            .find(|entry| entry.country == country && entry.tier == tier)
    }
}

/// How the world's value scale is fitted from its own players (`valuation::fit_value_scale`): the
/// curve every later price of a world's player (a newgen's, a renewed contract's) is read from.
#[derive(Debug, Clone, Deserialize)]
pub struct ValueScaleCfg {
    /// Ratings at which the prime-age value is read, from `first_anchor` to `last_anchor` every `anchor_step`.
    pub first_anchor: u8,
    pub last_anchor: u8,
    pub anchor_step: u8,
    /// Players within this many rating points of an anchor inform it.
    pub window: u8,
    /// An anchor needs this many prime-age players; a rating with fewer is skipped.
    pub min_per_anchor: usize,
    /// An age's factor needs this many players of that age; a lacking age takes its neighbour's.
    pub min_per_age: usize,
    /// Prime age, inclusive: the ages whose players fix the curve.
    pub prime_age_from: i32,
    pub prime_age_to: i32,
}

/// How an imported player's estimated market value is set (`valuation.rs`).
#[derive(Debug, Clone, Deserialize)]
pub struct ValuationCfg {
    /// Fewer `save` values than this and no model is fitted: the source values are kept as they are.
    pub min_samples: usize,
    /// Ridge added to the normal equations; only there to keep the solve well-posed.
    pub ridge: f64,
    /// Rows whose residual is further than this many standard deviations from the first fit are left
    /// out of the second.
    pub trim_sd: f64,
    /// The half-width of the band in which a stored (`save`) value is kept as it is, in median
    /// absolute deviations of the fit's residuals, for a value *above* the estimate.
    pub trust_save: f64,
    /// The same for a value the save stores *below* the estimate; wider, because the estimate is
    /// least reliable where it would raise a star or a prospect.
    pub trust_save_below: f64,
    /// The same for a value the save generated itself (`générée`).
    pub trust_estimated: f64,
    /// A player of the world is never valued below this (euros).
    pub minimum_value_eur: u64,
}

/// How the competitions are built.
#[derive(Debug, Clone, Deserialize)]
pub struct CompetitionsCfg {
    /// Authored sporting rules (tiebreakers, promotion play-offs, reserve sides) and the six
    /// national cups. `false` rebuilds the plain league-only world, the baseline performance and
    /// behaviour are compared with.
    pub authored_rules: bool,
    /// The three 2026/27 European competitions and the continental-only clubs that play them
    /// (`config/europe_2026_27.csv`). Needs `authored_rules`; `false` keeps the five-country world.
    #[serde(default = "yes")]
    pub european: bool,
}

fn yes() -> bool {
    true
}

impl Default for CompetitionsCfg {
    fn default() -> Self {
        Self {
            authored_rules: true,
            european: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorldCfg {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub base_year: i32,
    pub seed: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoneyCfg {
    pub gbp_to_eur: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartCfg {
    pub date: String,
    pub phase: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContractsCfg {
    pub missing_end: String,
}

/// Terms of the loans the save implies (it records where a player plays, not the terms).
#[derive(Debug, Clone, Deserialize)]
pub struct LoansCfg {
    pub enabled: bool,
    pub end_date: String,
    pub wage_contribution_pct: u8,
}

/// How FM's Current Ability sets a player's level on OFM's scale (see `ratings.rs`).
#[derive(Debug, Clone, Deserialize)]
pub struct RatingsCfg {
    /// `false` keeps the rating the converted attributes give, unanchored (the baseline the
    /// anchored one is compared with).
    pub anchor_to_ca: bool,
    /// `[CA, OFM rating]` knots, CA strictly increasing, rating never decreasing; interpolated
    /// linearly, constant beyond the ends. Potential is read off the same curve at PA.
    pub ca_to_ovr: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PositionsCfg {
    pub alternate_min_rating: i64,
    pub natural_within: i64,
    pub both_feet_min: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SquadCfg {
    pub trim_generated_to_floor: bool,
    pub calibrate_generated_to_division: bool,
    pub promote_academy_before_generating: bool,
    /// FM squad status codes read as youth statuses (`placement::RoleEvidence`).
    pub youth_status_codes: Vec<i64>,
    /// Lets a non-youth squad status at first-team level lift a reserve-registered player of academy
    /// age to the senior squad. `false` leaves registration the only signal.
    pub senior_status_promotion: bool,
    /// The floor of a club's first team is this percentile of its first-team registrations' ability.
    pub senior_floor_percentile: usize,
    /// A club with fewer first-team registrations than this has no floor, so no status promotion.
    pub senior_floor_min_players: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FinanceCfg {
    pub restore_balance: bool,
    pub restore_transfer_budget: bool,
    pub restore_wage_budget: bool,
    pub restore_recurring_income: bool,
    /// The most of a club's income (per cent) its day-one commitments may take before its recurring
    /// income is raised to carry them; 0 leaves the source's income as it is.
    #[serde(default = "default_commitment_share")]
    pub max_commitment_share_of_income_pct: i64,
}

/// The board's own soft cap on a wage bill (110 % of the budget, `contract_wage_policy`): a club
/// is let carry commitments up to that share of its income, and no further. 90 % was measured
/// first and rejected: it gave 31 of 240 clubs invented income (twelve of them never in debt),
/// and the 19 that needed it spent five times what they did without it. At 110 % the same 19 go
/// from 26 club-seasons in debt to 2 and spend what they did before.
fn default_commitment_share() -> i64 {
    110
}

/// Which of the save's unattached players enter the world as free agents, and what they ask.
#[derive(Debug, Clone, Deserialize)]
pub struct FreeAgentsCfg {
    pub enabled: bool,
    /// Ages are taken on this date (the configured start date).
    pub age_on: String,
    pub min_age: i64,
    pub max_age: i64,
    /// A player below this current ability is not worth a professional club's roster spot.
    pub min_ca: i64,
    /// A young player below `min_ca` is kept when his potential is at least `prospect_min_pa`
    /// (and his ability `prospect_min_ca`): an academy graduate released at 19 is a real free agent.
    pub prospect_max_age: i64,
    pub prospect_min_pa: i64,
    pub prospect_min_ca: i64,
    /// Size of the active pool: the eligible players are ranked by a seeded hash of their id and
    /// the first `pool_size` enter. The rest stay in world.db.
    pub pool_size: usize,
    /// The asked wage is this quantile of what contracted players of the same ability and age earn.
    pub wage_quantile: f64,
    /// Ability window (+-) used to find those contracted players, widened until enough are found.
    pub ca_window: i64,
}

impl Config {
    pub fn default_config() -> Self {
        Self::from_toml(DEFAULT_CONFIG).expect("the compiled-in adapter.toml parses")
    }

    pub fn from_toml(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).map_err(|e| format!("adapter.toml: {e}"))?;
        crate::ratings::validate(&config.ratings).map_err(|e| format!("adapter.toml: {e}"))?;
        Ok(config)
    }

    /// £ -> € for a weekly or total amount, rounded to the nearest unit.
    pub fn to_eur(&self, gbp: i64) -> i64 {
        (gbp as f64 * self.money.gbp_to_eur).round() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compiled_in_config_parses() {
        let cfg = Config::default_config();
        assert_eq!(cfg.world.id, "apfm-fm24-2026");
        assert_eq!(cfg.world.base_year, 2026);
    }

    #[test]
    fn money_conversion_rounds_to_the_nearest_euro() {
        let cfg = Config::default_config();
        assert_eq!(cfg.to_eur(1_000), 1_170);
        assert_eq!(cfg.to_eur(0), 0);
        assert_eq!(cfg.to_eur(3), 4); // 3.51
    }
}
