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
    pub potential: PotentialCfg,
    pub positions: PositionsCfg,
    pub squad: SquadCfg,
    pub finance: FinanceCfg,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct FinanceCfg {
    pub restore_balance: bool,
    pub restore_transfer_budget: bool,
    pub restore_wage_budget: bool,
    pub restore_recurring_income: bool,
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
