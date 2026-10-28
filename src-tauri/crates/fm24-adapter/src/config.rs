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
    pub potential: PotentialCfg,
    pub positions: PositionsCfg,
    pub squad: SquadCfg,
    pub finance: FinanceCfg,
    pub free_agents: FreeAgentsCfg,
    #[serde(default)]
    pub competitions: CompetitionsCfg,
}

/// How the competitions are built.
#[derive(Debug, Clone, Deserialize)]
pub struct CompetitionsCfg {
    /// Authored sporting rules (tiebreakers, promotion play-offs, reserve sides) and the six
    /// national cups. `false` rebuilds the plain league-only world, the baseline performance and
    /// behaviour are compared with.
    pub authored_rules: bool,
}

impl Default for CompetitionsCfg {
    fn default() -> Self {
        Self {
            authored_rules: true,
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

#[derive(Debug, Clone, Deserialize)]
pub struct PotentialCfg {
    pub ovr_points_per_ca: f64,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct FinanceCfg {
    pub restore_balance: bool,
    pub restore_transfer_budget: bool,
    pub restore_wage_budget: bool,
    pub restore_recurring_income: bool,
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
        toml::from_str(text).map_err(|e| format!("adapter.toml: {e}"))
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
