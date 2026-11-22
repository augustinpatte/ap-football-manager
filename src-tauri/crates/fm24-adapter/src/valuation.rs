//! The estimated market value of an imported player: what a profile shows as his value.
//!
//! Three different amounts exist in the game and this module only produces the first:
//!
//! 1. the **estimated value** (`Player::market_value`): what the game thinks he is worth;
//! 2. the **asking price**, which his club sets from that value, his status, his contract and the
//!    window (`ofm_core::transfers::realism::asking_fee`);
//! 3. the **fee** actually agreed after the negotiation (`agreed_fee`).
//!
//! # Why the source value is not simply converted
//!
//! The FM save stores a value for every player. It is a real FM field, but it is noisy as a
//! measure of the player: among players of the same ability, age band and position, the saved
//! values of one league's comparable group span a factor of about eight (CA 157-165, age 26-31:
//! 29 M to 229 M GBP). One consequence the owner saw was a 28-year-old of CA 161 priced above a
//! 27-year-old of CA 191 (Rashford against Mbappé), another a 29-year-old of CA 147 at 51 M GBP
//! next to comparable players at 5 M.
//!
//! # The estimate
//!
//! A log-linear model of the value, fitted at import time on the world's own `save` values (so the
//! comparison group is the world itself, and nothing about a particular player is written into the
//! code): ability and its curve, age, the gap between ability and potential for a young player, years
//! left on the contract, the reputation of the club and the position group.
//!
//! # How far the source value is trusted
//!
//! The source value is kept **exactly** while it lies within a band of the estimate measured in median
//! absolute deviations of the fit's own residuals (the noise is measured, not assumed). Beyond the
//! band it is pulled to its edge: the source still pushes the value up or down, by no more than the
//! data's own noise. The band is narrower above the estimate than below it: an inflated value is what
//! produces an asking price nobody can meet, and the estimate is least reliable at the very top and
//! for prospects, exactly where it would otherwise raise a deflated value. A value the save generated from its own regression (`générée`) is trusted less, and a
//! missing value takes the estimate. Finally no value exceeds what the richest club of the world could
//! pay: a price no buyer can meet is not a market value.
//!
//! Everything is deterministic: rows are fitted in the order they are given, the solve is a fixed
//! elimination, and a value the model decides is rounded to three significant figures so that a
//! last-bit difference of a platform's `ln`/`exp` cannot change the world file.

use crate::config::{Config, ValuationCfg};
use crate::source::{ClubRow, PlayerRow};
use chrono::NaiveDate;

/// The position group a value is modelled on. A goalkeeper's market is not an outfielder's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Goalkeeper,
    Defender,
    Midfielder,
    Forward,
}

impl Group {
    /// The group of an FM position code (`GK`, `DC`, `AMC`, `STC`...). An unknown code is a midfielder,
    /// the group the model treats as its baseline after the forwards.
    pub fn from_fm_code(code: &str) -> Self {
        match code {
            "GK" => Self::Goalkeeper,
            "DC" | "DL" | "DR" | "WBL" | "WBR" | "SW" => Self::Defender,
            "AMC" | "AML" | "AMR" | "STC" => Self::Forward,
            _ => Self::Midfielder,
        }
    }
}

/// What the source says the player is worth, in euros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The value the FM save stores.
    Save(u64),
    /// A value the source importer produced from its own regression (`générée`).
    Estimated(u64),
    /// No value at all.
    Missing,
}

/// Everything the estimate depends on. Ability and potential are FM's (1-200), not APFM's rating.
#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    pub ca: i64,
    pub pa: i64,
    /// Age in years at the start date.
    pub age: f64,
    /// Contract years left at the start date; 0 for a player without a contract.
    pub years_left: f64,
    /// The owner club's reputation on the source's 0-10 000 scale.
    pub club_reputation: i64,
    pub group: Group,
    pub source: Source,
}

impl Inputs {
    /// The inputs of a source row. `natural_code` is the FM code of the position the converter chose,
    /// `contract_end` the end date the world will carry (a generated one counts: it is what the
    /// player will have), and `on` the date the world starts. A row without a birth date is read as
    /// 27, the age at which the model asks for neither a youth premium nor a decline.
    pub fn from_row(
        row: &PlayerRow,
        natural_code: &str,
        contract_end: Option<&str>,
        on: NaiveDate,
        cfg: &Config,
    ) -> Self {
        let age = row
            .birth_date
            .as_deref()
            .and_then(|dob| NaiveDate::parse_from_str(dob, "%Y-%m-%d").ok())
            .map_or(AGE_DECLINE, |dob| (on - dob).num_days() as f64 / 365.25);
        let years_left = contract_end
            .and_then(|end| NaiveDate::parse_from_str(end, "%Y-%m-%d").ok())
            .map_or(0.0, |end| ((end - on).num_days() as f64 / 365.25).max(0.0));
        let source = match (row.value_gbp, row.value_source.as_deref()) {
            (Some(value), Some("save")) if value > 0 => {
                Source::Save(cfg.to_eur(value).max(0) as u64)
            }
            (Some(value), _) if value > 0 => Source::Estimated(cfg.to_eur(value).max(0) as u64),
            _ => Source::Missing,
        };
        Self {
            ca: row.ca,
            pa: row.pa,
            age,
            years_left,
            club_reputation: row.club_reputation,
            group: Group::from_fm_code(natural_code),
            source,
        }
    }
}

/// The contract end the world carries for a source row: the save's own when it is after the start,
/// otherwise the configured fallback. One place, because the imported world and the external
/// registry must feed the valuation the same years left.
pub fn contract_end_for(row: &PlayerRow, cfg: &Config) -> String {
    match &row.end_date {
        Some(end)
            if row
                .start_date
                .as_deref()
                .is_none_or(|start| end.as_str() > start) =>
        {
            end.clone()
        }
        _ => cfg.contracts.missing_end.clone(),
    }
}

/// What the richest club of the world could pay for one player: its largest transfer budget (GBP,
/// as the save states it) in euros, times `cfg.valuation.affordability_cap_share`. `None` when no
/// club has a budget.
pub fn affordability_cap_from_budget(richest_gbp: Option<i64>, cfg: &Config) -> Option<u64> {
    let richest = cfg.to_eur(richest_gbp?).max(0);
    (richest > 0).then_some((richest as f64 * cfg.valuation.affordability_cap_share) as u64)
}

/// [`affordability_cap_from_budget`] for the clubs already read from the source.
pub fn affordability_cap(clubs: &[ClubRow], cfg: &Config) -> Option<u64> {
    let richest = clubs
        .iter()
        .filter_map(|club| club.finance.as_ref())
        .map(|finance| finance.transfer_budget_gbp)
        .max();
    affordability_cap_from_budget(richest, cfg)
}

/// Where a value came from, for the import report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// The source value, unchanged: it agrees with the estimate within the trusted band.
    Source,
    /// The source value moved to the edge of the trusted band.
    PulledToEstimate,
    /// No usable source value: the estimate.
    Estimate,
    /// Reduced to what the richest club of the world could pay.
    Capped,
}

impl Provenance {
    pub fn label(self) -> &'static str {
        match self {
            Provenance::Source => "save",
            Provenance::PulledToEstimate => "save (ramenée à l'estimation)",
            Provenance::Estimate => "estimée (modèle)",
            Provenance::Capped => "plafonnée (capacité d'achat)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Valuation {
    pub value_eur: u64,
    /// The model's own estimate before the source value was weighed in.
    pub estimate_eur: u64,
    pub provenance: Provenance,
}

/// Ability above which the value curve changes slope (FM CA).
const CA_KNOTS: [f64; 3] = [100.0, 130.0, 160.0];
/// Age hinges: the youth premium ends at 22, decline starts at 27 and steepens at 31.
const AGE_YOUNG: f64 = 22.0;
const AGE_DECLINE: f64 = 27.0;
const AGE_LATE: f64 = 31.0;
/// Age windows `[from, to)` for the weight of the ability-to-potential gap: a teenager's ceiling is
/// valued more than a twenty-five-year-old's.
const GAP_WINDOWS: [(f64, f64); 3] = [(0.0, 20.0), (20.0, 23.0), (23.0, 27.0)];
const REPUTATION_KNEE: f64 = 7_000.0;
/// Contract years that still add value; past that the length is not worth more.
const YEARS_LEFT_MAX: f64 = 5.0;
const FEATURES: usize = 2 + CA_KNOTS.len() + 3 + GAP_WINDOWS.len() + 2 + 2 + 3;

type Row = [f64; FEATURES];

fn features(inputs: &Inputs) -> Row {
    let ca = inputs.ca as f64;
    let gap = (inputs.pa - inputs.ca).max(0) as f64 / 100.0;
    let age = inputs.age;
    let reputation = inputs.club_reputation as f64;
    let mut row = [0.0; FEATURES];
    let mut next = 0;
    let mut push = |value: f64| {
        row[next] = value;
        next += 1;
    };
    push(1.0);
    push(ca / 100.0);
    for knot in CA_KNOTS {
        push((ca - knot).max(0.0) / 100.0);
    }
    push((AGE_YOUNG - age).max(0.0));
    push((age - AGE_DECLINE).max(0.0));
    push((age - AGE_LATE).max(0.0));
    for (from, to) in GAP_WINDOWS {
        push(if age >= from && age < to { gap } else { 0.0 });
    }
    push(inputs.years_left.clamp(0.0, YEARS_LEFT_MAX));
    push(if inputs.years_left <= 1.0 { 1.0 } else { 0.0 });
    push(reputation / 1_000.0);
    push((reputation - REPUTATION_KNEE).max(0.0) / 1_000.0);
    push(f64::from(inputs.group == Group::Goalkeeper));
    push(f64::from(inputs.group == Group::Defender));
    push(f64::from(inputs.group == Group::Midfielder));
    row
}

/// Solves the normal equations `(XᵀX + ridge·I) b = Xᵀy` by Gauss-Jordan elimination with partial
/// pivoting. `None` when the system is singular. The order of operations is fixed, which is what
/// makes the fit reproducible.
fn least_squares(rows: &[Row], targets: &[f64], ridge: f64) -> Option<[f64; FEATURES]> {
    let mut system = [[0.0_f64; FEATURES + 1]; FEATURES];
    for (row, target) in rows.iter().zip(targets) {
        for i in 0..FEATURES {
            for j in 0..FEATURES {
                system[i][j] += row[i] * row[j];
            }
            system[i][FEATURES] += row[i] * target;
        }
    }
    for (i, line) in system.iter_mut().enumerate() {
        line[i] += ridge;
    }
    for column in 0..FEATURES {
        let pivot_row = (column..FEATURES).max_by(|a, b| {
            system[*a][column]
                .abs()
                .total_cmp(&system[*b][column].abs())
        })?;
        if system[pivot_row][column].abs() < 1e-12 {
            return None;
        }
        system.swap(column, pivot_row);
        let pivot = system[column][column];
        for cell in &mut system[column][column..] {
            *cell /= pivot;
        }
        let pivot_line = system[column];
        for (r, line) in system.iter_mut().enumerate() {
            if r != column && line[column] != 0.0 {
                let factor = line[column];
                for c in column..=FEATURES {
                    line[c] -= factor * pivot_line[c];
                }
            }
        }
    }
    let mut solution = [0.0; FEATURES];
    for (i, line) in system.iter().enumerate() {
        solution[i] = line[FEATURES];
    }
    Some(solution)
}

fn dot(coefficients: &[f64; FEATURES], row: &Row) -> f64 {
    coefficients.iter().zip(row).map(|(c, x)| c * x).sum()
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or_default()
}

/// Rounds to three significant figures.
fn round_significant(value: f64) -> f64 {
    if value < 1.0 {
        return 0.0;
    }
    let magnitude = 10f64.powi(value.log10().floor() as i32 - 2);
    (value / magnitude).round() * magnitude
}

/// Keeps `value` below `cap` without making the best players tie at it: unchanged up to the knee
/// (`knee_share` of the cap), then bent towards the cap, which it only approaches. The bend is
/// rational (`x / (1 + x)`), not exponential: it saturates slowly enough that a value twice the
/// ceiling and one three times it still differ. Strictly increasing, so it never reorders players.
fn squeeze_below(value: u64, cap: u64, knee_share: f64) -> u64 {
    let knee = (cap as f64 * knee_share).floor();
    let value = value as f64;
    if value <= knee {
        return value as u64;
    }
    let room = cap as f64 - knee;
    let excess = (value - knee) / room;
    (knee + room * excess / (1.0 + excess))
        .floor()
        .min(cap as f64) as u64
}

/// A fitted valuation model.
#[derive(Debug, Clone)]
pub struct Model {
    coefficients: [f64; FEATURES],
    /// Half-widths of the trusted band, in natural-log units, for a value the save stores: above the
    /// estimate (an inflated value) and below it (a deflated one).
    band_save_above: f64,
    band_save_below: f64,
    band_estimated: f64,
    cap_eur: Option<u64>,
    cap_knee_share: f64,
    minimum_value_eur: u64,
    /// How many rows the fit used and the median absolute residual of the save values: the
    /// reliability of the source, in log units.
    pub fitted_on: usize,
    pub residual_mad: f64,
    neutral_reputation: i64,
}

impl Model {
    /// Fits the model on the rows that carry a `save` value. `None` when there are fewer than
    /// `cfg.min_samples` of them (or the fit is degenerate): the caller keeps the source values.
    /// `cap_eur` is the largest sum a club of the world could pay.
    pub fn fit(samples: &[Inputs], cap_eur: Option<u64>, cfg: &ValuationCfg) -> Option<Self> {
        let known: Vec<(Row, f64)> = samples
            .iter()
            .filter_map(|inputs| match inputs.source {
                Source::Save(value) if value > 0 => Some((features(inputs), (value as f64).ln())),
                _ => None,
            })
            .collect();
        if known.len() < cfg.min_samples {
            return None;
        }
        let rows: Vec<Row> = known.iter().map(|(row, _)| *row).collect();
        let targets: Vec<f64> = known.iter().map(|(_, target)| *target).collect();
        let first = least_squares(&rows, &targets, cfg.ridge)?;
        let residual =
            |coefficients: &[f64; FEATURES], i: usize| targets[i] - dot(coefficients, &rows[i]);
        let first_residuals: Vec<f64> = (0..rows.len()).map(|i| residual(&first, i)).collect();
        let centre = first_residuals.iter().sum::<f64>() / first_residuals.len() as f64;
        let spread = (first_residuals
            .iter()
            .map(|r| (r - centre).powi(2))
            .sum::<f64>()
            / first_residuals.len() as f64)
            .sqrt();
        // One trimming pass: the tails are the noise the model is meant to see through, and they
        // would otherwise bend the curve towards themselves.
        let kept: Vec<usize> = (0..rows.len())
            .filter(|&i| (first_residuals[i] - centre).abs() <= cfg.trim_sd * spread)
            .collect();
        let kept_rows: Vec<Row> = kept.iter().map(|&i| rows[i]).collect();
        let kept_targets: Vec<f64> = kept.iter().map(|&i| targets[i]).collect();
        let coefficients = least_squares(&kept_rows, &kept_targets, cfg.ridge)?;
        let residual_mad = median(
            (0..rows.len())
                .map(|i| residual(&coefficients, i).abs())
                .collect(),
        );
        let neutral_reputation = median(
            samples
                .iter()
                .map(|inputs| inputs.club_reputation as f64)
                .collect(),
        )
        .round() as i64;
        Some(Self {
            coefficients,
            band_save_above: cfg.trust_save * residual_mad,
            band_save_below: cfg.trust_save_below * residual_mad,
            band_estimated: cfg.trust_estimated * residual_mad,
            cap_eur,
            cap_knee_share: cfg.affordability_knee_share,
            minimum_value_eur: cfg.minimum_value_eur,
            fitted_on: rows.len(),
            residual_mad,
            neutral_reputation,
        })
    }

    /// The club reputation a player without a club is estimated at: the median of the fitted rows.
    pub fn neutral_reputation(&self) -> i64 {
        self.neutral_reputation
    }

    /// The model's estimate alone, in euros.
    pub fn estimate(&self, inputs: &Inputs) -> u64 {
        round_significant(dot(&self.coefficients, &features(inputs)).exp()) as u64
    }

    /// The value to import for `inputs`.
    pub fn value(&self, inputs: &Inputs) -> Valuation {
        let log_estimate = dot(&self.coefficients, &features(inputs));
        let estimate_eur = round_significant(log_estimate.exp()) as u64;
        let (mut value_eur, mut provenance) = match inputs.source {
            Source::Missing | Source::Save(0) | Source::Estimated(0) => {
                (estimate_eur, Provenance::Estimate)
            }
            Source::Save(source) => self.within_band(
                source,
                log_estimate,
                self.band_save_above,
                self.band_save_below,
            ),
            Source::Estimated(source) => self.within_band(
                source,
                log_estimate,
                self.band_estimated,
                self.band_estimated,
            ),
        };
        value_eur = value_eur.max(self.minimum_value_eur);
        if let Some(cap) = self.cap_eur {
            let squeezed = squeeze_below(value_eur, cap, self.cap_knee_share);
            if squeezed < value_eur {
                value_eur = squeezed;
                provenance = Provenance::Capped;
            }
        }
        Valuation {
            value_eur,
            estimate_eur,
            provenance,
        }
    }

    fn within_band(
        &self,
        source: u64,
        log_estimate: f64,
        above: f64,
        below: f64,
    ) -> (u64, Provenance) {
        let deviation = (source as f64).ln() - log_estimate;
        let band = if deviation >= 0.0 { above } else { below };
        if deviation.abs() <= band {
            return (source, Provenance::Source);
        }
        let edge = log_estimate + band.copysign(deviation);
        (
            round_significant(edge.exp()) as u64,
            Provenance::PulledToEstimate,
        )
    }
}

/// A player as the inversion count sees him.
#[derive(Debug, Clone)]
pub struct Comparable {
    pub id: String,
    pub inputs: Inputs,
    pub value_eur: u64,
}

/// How often a clearly stronger player is valued below a clearly weaker one, among comparable pairs.
///
/// A pair is comparable when both are of the same position group, within two years of age and 1 500
/// points of club reputation of each other. The stronger one is clearly stronger when his FM current
/// ability is at least 15 points higher and his potential is not lower by more than 5. Candidates
/// are taken in id order, each against the next 40, so the count is bounded and reproducible.
/// Returns `(pairs, inversions)`.
pub fn strict_inversions(items: &mut [Comparable]) -> (usize, usize) {
    items.sort_by(|a, b| a.id.cmp(&b.id));
    let mut by_group: std::collections::BTreeMap<(u8, i64), Vec<&Comparable>> = Default::default();
    for item in items.iter() {
        by_group
            .entry((
                item.inputs.group as u8,
                (item.inputs.age / 3.0).floor() as i64,
            ))
            .or_default()
            .push(item);
    }
    let (mut pairs, mut inversions) = (0, 0);
    for members in by_group.values() {
        for (i, a) in members.iter().enumerate() {
            for b in members.iter().skip(i + 1).take(40) {
                if (a.inputs.age - b.inputs.age).abs() > 2.0
                    || (a.inputs.club_reputation - b.inputs.club_reputation).abs() > 1_500
                {
                    continue;
                }
                let (hi, lo) = if a.inputs.ca >= b.inputs.ca {
                    (a, b)
                } else {
                    (b, a)
                };
                if hi.inputs.ca - lo.inputs.ca < 15 || hi.inputs.pa < lo.inputs.pa - 5 {
                    continue;
                }
                pairs += 1;
                if hi.value_eur < lo.value_eur {
                    inversions += 1;
                }
            }
        }
    }
    (pairs, inversions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ValuationCfg {
        crate::config::Config::default_config().valuation
    }

    /// A population whose true value is a smooth function of ability, age and club reputation, with
    /// a deterministic multiplicative "noise" of up to a factor of about 2.5 either way: the shape
    /// of the real save, without any of its data.
    fn population(size: usize) -> Vec<Inputs> {
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as f64 / (1u64 << 31) as f64
        };
        (0..size)
            .map(|_| {
                let ca = 70.0 + next() * 120.0;
                let age = 18.0 + next() * 18.0;
                let reputation = 4_000.0 + next() * 5_500.0;
                let years_left = next() * 5.0;
                let pa = (ca + next() * 30.0 * f64::from(age < 24.0)).min(200.0);
                let truth =
                    (4.0 + 0.075 * ca - 0.05 * (age - 27.0).max(0.0) + 0.0002 * reputation).exp();
                let noise = ((next() - 0.5) * 1.8).exp();
                Inputs {
                    ca: ca as i64,
                    pa: pa as i64,
                    age,
                    years_left,
                    club_reputation: reputation as i64,
                    group: Group::Midfielder,
                    source: Source::Save((truth * noise) as u64),
                }
            })
            .collect()
    }

    fn player(ca: i64, source: Source) -> Inputs {
        Inputs {
            ca,
            pa: ca,
            age: 27.0,
            years_left: 3.0,
            club_reputation: 8_000,
            group: Group::Midfielder,
            source,
        }
    }

    /// Given a population of players,
    /// When the model is fitted twice from the same rows,
    /// Then both fits value every player identically (the world file must be reproducible).
    #[test]
    fn the_same_rows_give_the_same_values() {
        let rows = population(1_500);

        let first = Model::fit(&rows, None, &cfg()).expect("fits");
        let second = Model::fit(&rows, None, &cfg()).expect("fits");

        for row in rows.iter().take(200) {
            assert_eq!(first.value(row), second.value(row));
        }
    }

    /// Given fewer rows than the configured minimum,
    /// When fitting,
    /// Then there is no model and the caller keeps the source values.
    #[test]
    fn too_few_rows_give_no_model() {
        assert!(Model::fit(&population(20), None, &cfg()).is_none());
    }

    /// Given a fitted model,
    /// When an otherwise identical player is stronger by ten ability points,
    /// Then his estimate is never lower, whatever his ability (the estimate has no inversions).
    #[test]
    fn the_estimate_never_falls_as_ability_rises() {
        let model = Model::fit(&population(3_000), None, &cfg()).expect("fits");

        let mut previous = 0;
        for ca in (70..=190).step_by(10) {
            let estimate = model.estimate(&player(ca, Source::Missing));
            assert!(estimate >= previous, "CA {ca}: {estimate} < {previous}");
            previous = estimate;
        }
    }

    /// Given a source value close to the estimate,
    /// When valuing,
    /// Then the source value is returned exactly and labelled as such.
    #[test]
    fn a_source_value_near_the_estimate_is_kept_exactly() {
        let model = Model::fit(&population(3_000), None, &cfg()).expect("fits");
        let estimate = model.estimate(&player(140, Source::Missing));
        let near = estimate + estimate / 20;

        let valuation = model.value(&player(140, Source::Save(near)));

        assert_eq!(valuation.value_eur, near);
        assert_eq!(valuation.provenance, Provenance::Source);
    }

    /// Given a source value several times the estimate for a player of his ability,
    /// When valuing,
    /// Then he is valued above the estimate but well below the source, and the label says the source
    /// was pulled in.
    #[test]
    fn an_inflated_source_value_is_pulled_to_the_edge_of_the_trusted_band() {
        let model = Model::fit(&population(3_000), None, &cfg()).expect("fits");
        let estimate = model.estimate(&player(150, Source::Missing));
        let source = estimate * 5;

        let valuation = model.value(&player(150, Source::Save(source)));

        assert_eq!(valuation.provenance, Provenance::PulledToEstimate);
        assert!(valuation.value_eur > estimate && valuation.value_eur < source / 2);
    }

    /// Given a deflated source value,
    /// When valuing,
    /// Then the value is raised to the edge of the band, never past the estimate.
    #[test]
    fn a_deflated_source_value_is_raised_but_not_to_the_estimate() {
        let model = Model::fit(&population(3_000), None, &cfg()).expect("fits");
        let estimate = model.estimate(&player(150, Source::Missing));

        let valuation = model.value(&player(150, Source::Save(estimate / 8)));

        assert_eq!(valuation.provenance, Provenance::PulledToEstimate);
        assert!(valuation.value_eur > estimate / 8 && valuation.value_eur < estimate);
    }

    /// Given a source value somewhat below the estimate (as the model is least sure of the top and of
    /// prospects),
    /// When valuing,
    /// Then it is kept as it is: a deflated value is raised only when severely deflated.
    #[test]
    fn a_moderately_deflated_source_value_is_kept() {
        let model = Model::fit(&population(3_000), None, &cfg()).expect("fits");
        let estimate = model.estimate(&player(150, Source::Missing));
        let source = estimate * 6 / 10;

        let valuation = model.value(&player(150, Source::Save(source)));

        assert_eq!(valuation.value_eur, source);
        assert_eq!(valuation.provenance, Provenance::Source);
    }

    /// Given two players who differ only in ability, the weaker with a very inflated source value
    /// and the stronger with a deflated one,
    /// When both are valued,
    /// Then the stronger is worth more: the order of the source's noise does not survive.
    #[test]
    fn a_source_inversion_between_comparable_players_is_resolved_by_ability() {
        let model = Model::fit(&population(4_000), None, &cfg()).expect("fits");
        let weaker_estimate = model.estimate(&player(155, Source::Missing));
        let stronger_estimate = model.estimate(&player(190, Source::Missing));

        let weaker_source = weaker_estimate * 14;
        let stronger_source = stronger_estimate / 2;
        assert!(
            weaker_source > stronger_source,
            "the premise: the source order is inverted"
        );

        let weaker = model.value(&player(155, Source::Save(weaker_source)));
        let stronger = model.value(&player(190, Source::Save(stronger_source)));

        assert!(stronger.value_eur > weaker.value_eur);
    }

    /// Given a player with no source value,
    /// When valuing,
    /// Then he gets the estimate, labelled as one.
    #[test]
    fn a_missing_value_takes_the_estimate() {
        let model = Model::fit(&population(3_000), None, &cfg()).expect("fits");

        let valuation = model.value(&player(120, Source::Missing));

        assert_eq!(valuation.provenance, Provenance::Estimate);
        assert_eq!(
            valuation.value_eur,
            valuation.estimate_eur.max(cfg().minimum_value_eur)
        );
    }

    /// Given a cap and values above it,
    /// When valuing,
    /// Then every value stays under the cap, the label says so, and a better player is still worth
    /// more than a lesser one (the cap bends the top, it does not flatten it into a tie).
    #[test]
    fn no_value_exceeds_what_the_richest_club_can_pay_and_stars_do_not_tie() {
        let model = Model::fit(&population(3_000), Some(1_000_000), &cfg()).expect("fits");

        let lesser = model.value(&player(160, Source::Missing));
        let better = model.value(&player(190, Source::Missing));

        assert!(better.value_eur <= 1_000_000 && lesser.value_eur <= 1_000_000);
        assert!(better.value_eur > lesser.value_eur);
        assert_eq!(better.provenance, Provenance::Capped);
    }

    #[test]
    fn the_ceiling_is_unchanged_below_its_knee_and_strictly_increasing_above() {
        assert_eq!(squeeze_below(500_000, 1_000_000, 0.6), 500_000);
        let mut previous = 0;
        for value in (600_000..=20_000_000).step_by(400_000) {
            let squeezed = squeeze_below(value, 1_000_000, 0.6);
            assert!(squeezed > previous && squeezed <= 1_000_000, "{value}");
            previous = squeezed;
        }
        // Two and three times the ceiling still differ by a visible amount.
        let two = squeeze_below(2_000_000, 1_000_000, 0.6);
        let three = squeeze_below(3_000_000, 1_000_000, 0.6);
        assert!(three - two > 20_000, "{two} {three}");
    }

    /// Given a value the save generated itself,
    /// When it disagrees with the estimate by the same amount as a stored one,
    /// Then it is pulled in harder: it is trusted less.
    #[test]
    fn a_generated_source_value_is_trusted_less_than_a_stored_one() {
        let model = Model::fit(&population(3_000), None, &cfg()).expect("fits");
        let estimate = model.estimate(&player(140, Source::Missing));
        let source = estimate * 2;

        let stored = model.value(&player(140, Source::Save(source)));
        let generated = model.value(&player(140, Source::Estimated(source)));

        assert!(generated.value_eur <= stored.value_eur);
    }

    /// Given a noisy population (the source's values vary by a factor of about 2.5 around the truth),
    /// When every player is valued by a model fitted on it,
    /// Then clearly-ordered comparable pairs are inverted at least two and a half times less often than in
    /// the source values, and under three pairs in a hundred.
    #[test]
    fn valuation_removes_most_inversions_between_comparable_players() {
        let rows = population(6_000);
        let model = Model::fit(&rows, None, &cfg()).expect("fits");
        let as_source = |rows: &[Inputs]| -> Vec<Comparable> {
            rows.iter()
                .enumerate()
                .map(|(i, inputs)| Comparable {
                    id: format!("{i:06}"),
                    inputs: *inputs,
                    value_eur: match inputs.source {
                        Source::Save(value) => value,
                        _ => 0,
                    },
                })
                .collect()
        };
        let mut raw = as_source(&rows);
        let mut valued: Vec<Comparable> = raw
            .iter()
            .map(|c| Comparable {
                value_eur: model.value(&c.inputs).value_eur,
                ..c.clone()
            })
            .collect();

        let (raw_pairs, raw_inversions) = strict_inversions(&mut raw);
        let (pairs, inversions) = strict_inversions(&mut valued);

        assert_eq!(raw_pairs, pairs);
        assert!(
            raw_inversions > 100,
            "the noisy source must invert some: {raw_inversions}"
        );
        assert!(
            inversions * 5 < raw_inversions * 2,
            "{inversions} vs {raw_inversions}"
        );
        assert!(inversions * 100 < pairs * 3, "{inversions} of {pairs}");
    }

    #[test]
    fn position_groups_follow_the_fm_codes() {
        assert_eq!(Group::from_fm_code("GK"), Group::Goalkeeper);
        assert_eq!(Group::from_fm_code("WBL"), Group::Defender);
        assert_eq!(Group::from_fm_code("DM"), Group::Midfielder);
        assert_eq!(Group::from_fm_code("AML"), Group::Forward);
        assert_eq!(Group::from_fm_code("STC"), Group::Forward);
    }

    #[test]
    fn three_significant_figures() {
        assert_eq!(round_significant(123_456.0), 123_000.0);
        assert_eq!(round_significant(98_765_432.0), 98_800_000.0);
        assert_eq!(round_significant(0.4), 0.0);
    }
}
