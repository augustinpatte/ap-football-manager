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
//! Everything is deterministic for a given toolchain and platform: rows are fitted in the order they
//! are given, the solve is a fixed elimination, and a value the model decides is rounded to three
//! significant figures. `ln` and `exp` come from the platform's libm, so a different operating system
//! could in principle differ in the last bit; the rounding makes that vanishingly unlikely to change a
//! value (a flip at a band or trim boundary is the residual hazard), but the guarantee is "same
//! source, configuration and build give the same world file", not more.

use crate::config::{Config, ValuationCfg};
use crate::source::PlayerRow;
use chrono::NaiveDate;
use ofm_core::value_scale::{FIRST_AGE, VALUE_SCALE_VERSION, ValueScale, quality};

/// The position group a value is modelled on. A goalkeeper's market is not an outfielder's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Goalkeeper,
    Defender,
    Midfielder,
    Forward,
}

impl Group {
    /// The group of an FM position code (`GK`, `DC`, `AMC`, `STC`...). This is the valuation's own
    /// projection, not `Position::to_group_position`: the market prices an attacking midfielder with
    /// the forwards (the fit separates AMC from DM/MC by a clear margin), where the domain groups him
    /// with the midfielders. An unknown code is a midfielder.
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

/// One player the value scale is fitted on: what he is and what the import valued him at.
#[derive(Debug, Clone, Copy)]
pub struct ScaleSample {
    pub ovr: u8,
    pub potential: u8,
    pub age: i32,
    pub value_eur: u64,
}

fn median_ln(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// Fits the world's value scale from the players the import has valued: the prime-age value at each
/// anchor rating (the median of the logarithms of the values within the window, made
/// non-decreasing), and, for each age, the median of each player's value over the curve's value
/// for him. `None` when there are too few prime-age players to place four anchors, in which case the
/// world keeps the native formula. Deterministic: medians of sorted values, in a fixed order.
pub fn fit_value_scale(
    samples: &[ScaleSample],
    cfg: &crate::config::ValueScaleCfg,
) -> Option<ValueScale> {
    let prime: Vec<&ScaleSample> = samples
        .iter()
        .filter(|s| s.value_eur > 0 && (cfg.prime_age_from..=cfg.prime_age_to).contains(&s.age))
        .collect();
    let mut anchors: Vec<(u8, f64)> = Vec::new();
    let mut rating = cfg.first_anchor;
    while rating <= cfg.last_anchor {
        let near: Vec<f64> = prime
            .iter()
            .filter(|s| s.ovr.abs_diff(rating) <= cfg.window)
            .map(|s| (s.value_eur as f64).ln())
            .collect();
        if near.len() >= cfg.min_per_anchor {
            let floor = anchors.last().map_or(f64::MIN, |(_, ln)| *ln);
            anchors.push((rating, median_ln(near).max(floor)));
        }
        rating = rating.saturating_add(cfg.anchor_step.max(1));
        if rating == u8::MAX {
            break;
        }
    }
    if anchors.len() < 4 {
        return None;
    }
    let curve_only = ValueScale {
        version: VALUE_SCALE_VERSION,
        anchors: anchors
            .iter()
            .map(|(rating, ln)| (*rating, round_three_figures(ln.exp())))
            .collect(),
        age_factors: vec![1000],
    };
    // Each age's factor: the median of value / curve over the players of that age.
    let ages = (FIRST_AGE..=FIRST_AGE + 24).collect::<Vec<i32>>();
    let mut factors: Vec<Option<f64>> = Vec::with_capacity(ages.len());
    for age in &ages {
        let ratios: Vec<f64> = samples
            .iter()
            .filter(|s| s.value_eur > 0 && s.age == *age)
            .filter_map(|s| {
                let priced = curve_only.prime_value(quality(s.ovr, s.potential, s.age));
                (priced > 0.0).then(|| (s.value_eur as f64 / priced).ln())
            })
            .collect();
        factors.push((ratios.len() >= cfg.min_per_age).then(|| median_ln(ratios).exp()));
    }
    // An age with too few players takes the nearest age that has enough.
    let known: Vec<(usize, f64)> = factors
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.map(|f| (i, f)))
        .collect();
    if known.is_empty() {
        return None;
    }
    let age_factors = (0..ages.len())
        .map(|i| {
            let nearest = known
                .iter()
                .min_by_key(|(j, _)| j.abs_diff(i))
                .map_or(1.0, |(_, f)| *f);
            (nearest * 1000.0).round().clamp(20.0, 3000.0) as u32
        })
        .collect();
    Some(ValueScale {
        age_factors,
        ..curve_only
    })
}

fn round_three_figures(value: f64) -> u64 {
    if value < 1.0 {
        return 0;
    }
    let magnitude = 10f64.powi(value.log10().floor() as i32 - 2);
    ((value / magnitude).round() * magnitude) as u64
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
}

impl Provenance {
    pub fn label(self) -> &'static str {
        match self {
            Provenance::Source => "save",
            Provenance::PulledToEstimate => "save (ramenée à l'estimation)",
            Provenance::Estimate => "estimée (modèle)",
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

/// A fitted valuation model.
#[derive(Debug, Clone)]
pub struct Model {
    coefficients: [f64; FEATURES],
    /// Half-widths of the trusted band, in natural-log units, for a value the save stores: above the
    /// estimate (an inflated value) and below it (a deflated one).
    band_save_above: f64,
    band_save_below: f64,
    band_estimated: f64,
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
    pub fn fit(samples: &[Inputs], cfg: &ValuationCfg) -> Option<Self> {
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
            minimum_value_eur: cfg.minimum_value_eur,
            fitted_on: rows.len(),
            residual_mad,
            neutral_reputation,
        })
    }

    /// The club reputation a player without a club is estimated at: the median club reputation of all
    /// the samples the model was given (before the fit keeps only its stored values and trims).
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
        let (mut value_eur, provenance) = match inputs.source {
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
        // The floor is for values the model decides; a stored value kept as it is stays exact.
        if provenance != Provenance::Source {
            value_eur = value_eur.max(self.minimum_value_eur);
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

        let first = Model::fit(&rows, &cfg()).expect("fits");
        let second = Model::fit(&rows, &cfg()).expect("fits");

        for row in rows.iter().take(200) {
            assert_eq!(first.value(row), second.value(row));
        }
    }

    /// Given fewer rows than the configured minimum,
    /// When fitting,
    /// Then there is no model and the caller keeps the source values.
    #[test]
    fn too_few_rows_give_no_model() {
        assert!(Model::fit(&population(20), &cfg()).is_none());
    }

    /// Given a fitted model,
    /// When an otherwise identical player is stronger by ten ability points,
    /// Then his estimate is never lower, whatever his ability (the estimate has no inversions).
    #[test]
    fn the_estimate_never_falls_as_ability_rises() {
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");

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
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");
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
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");
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
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");
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
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");
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
        let model = Model::fit(&population(4_000), &cfg()).expect("fits");
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

    /// Given a stored value under the minimum that agrees with the estimate,
    /// When valuing,
    /// Then it is kept to the euro: the minimum only applies to values the model decides.
    #[test]
    fn the_minimum_never_overrides_a_stored_value_that_is_kept() {
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");
        let weak = player(1, Source::Missing);
        let estimate = model.estimate(&weak);
        assert!(
            estimate > 0 && estimate < cfg().minimum_value_eur,
            "the premise: the estimate itself is under the minimum ({estimate})"
        );

        let kept = model.value(&Inputs {
            source: Source::Save(estimate),
            ..weak
        });
        let decided = model.value(&weak);

        assert_eq!(
            (kept.value_eur, kept.provenance),
            (estimate, Provenance::Source)
        );
        assert_eq!(decided.value_eur, cfg().minimum_value_eur);
    }

    /// Given a player with no source value,
    /// When valuing,
    /// Then he gets the estimate, labelled as one.
    #[test]
    fn a_missing_value_takes_the_estimate() {
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");

        let valuation = model.value(&player(120, Source::Missing));

        assert_eq!(valuation.provenance, Provenance::Estimate);
        // The oracle is the estimate asked for directly, not the value's own field.
        assert_eq!(
            valuation.value_eur,
            model
                .estimate(&player(120, Source::Missing))
                .max(cfg().minimum_value_eur)
        );
    }

    /// Given a value the save generated itself,
    /// When it disagrees with the estimate by the same amount as a stored one,
    /// Then it is pulled in harder: it is trusted less.
    #[test]
    fn a_generated_source_value_is_trusted_less_than_a_stored_one() {
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");
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
        let model = Model::fit(&rows, &cfg()).expect("fits");
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

    /// Given FM position codes,
    /// When mapped to the valuation's groups,
    /// Then goalkeepers, defenders (wing-backs included), midfielders and forwards (attacking
    /// midfielders included) are told apart.
    #[test]
    fn position_groups_follow_the_fm_codes() {
        assert_eq!(Group::from_fm_code("GK"), Group::Goalkeeper);
        assert_eq!(Group::from_fm_code("WBL"), Group::Defender);
        assert_eq!(Group::from_fm_code("DM"), Group::Midfielder);
        assert_eq!(Group::from_fm_code("AML"), Group::Forward);
        // Deliberately not the domain's grouping (Midfielder): see `from_fm_code`.
        assert_eq!(Group::from_fm_code("AMC"), Group::Forward);
        assert_eq!(Group::from_fm_code("STC"), Group::Forward);
    }

    /// Given values of different magnitudes,
    /// When rounded for the world file,
    /// Then they keep three significant figures, and anything under one euro is zero.
    #[test]
    fn three_significant_figures() {
        assert_eq!(round_significant(123_456.0), 123_000.0);
        assert_eq!(round_significant(98_765_432.0), 98_800_000.0);
        assert_eq!(round_significant(0.4), 0.0);
    }

    fn source_row(value_gbp: Option<i64>, source: Option<&str>) -> PlayerRow {
        let mut row = crate::source::sample_row(150, 170, "MC", &[]);
        row.birth_date = Some("2000-06-01".into());
        row.club_reputation = 8_500;
        row.value_gbp = value_gbp;
        row.value_source = source.map(str::to_string);
        row
    }

    fn on() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()
    }

    /// Given source rows with a stored value, a generated value and none,
    /// When their inputs are read,
    /// Then each is the matching kind, converted to euros: a generated value is never read as stored.
    #[test]
    fn the_kind_of_a_source_value_follows_its_label_and_is_converted_to_euros() {
        let cfg = crate::config::Config::default_config();
        let read = |value, label| {
            Inputs::from_row(
                &source_row(value, label),
                "MC",
                Some("2029-06-30"),
                on(),
                &cfg,
            )
            .source
        };

        assert_eq!(read(Some(1_000), Some("save")), Source::Save(1_170));
        assert_eq!(read(Some(1_000), Some("générée")), Source::Estimated(1_170));
        assert_eq!(read(None, None), Source::Missing);
        assert_eq!(read(Some(0), Some("save")), Source::Missing);
    }

    /// Given a row with no birth date,
    /// When its inputs are read,
    /// Then he is 27, the age at which the model asks for neither a youth premium nor a decline; and
    /// the years left and age of a dated row are measured on the start date.
    #[test]
    fn age_and_years_left_are_measured_on_the_start_date() {
        let cfg = crate::config::Config::default_config();
        let mut undated = source_row(Some(1), Some("save"));
        undated.birth_date = None;

        let unknown = Inputs::from_row(&undated, "MC", Some("2027-06-01"), on(), &cfg);
        let known = Inputs::from_row(&source_row(Some(1), Some("save")), "MC", None, on(), &cfg);

        assert_eq!(unknown.age, AGE_DECLINE);
        assert!(
            (unknown.years_left - 1.0).abs() < 0.01,
            "{}",
            unknown.years_left
        );
        assert!((known.age - 26.0).abs() < 0.01, "{}", known.age);
        assert_eq!(known.years_left, 0.0, "no contract, nothing left on it");
    }

    /// Given source rows whose contract end is missing, before its start, or fine,
    /// When the contract end the world carries is read,
    /// Then only a usable end is kept; otherwise the configured fallback stands in.
    #[test]
    fn an_unusable_contract_end_is_replaced_by_the_configured_one() {
        let cfg = crate::config::Config::default_config();
        let with = |start: Option<&str>, end: Option<&str>| {
            let mut row = source_row(None, None);
            row.start_date = start.map(str::to_string);
            row.end_date = end.map(str::to_string);
            contract_end_for(&row, &cfg)
        };

        assert_eq!(with(Some("2025-07-01"), Some("2029-06-30")), "2029-06-30");
        assert_eq!(with(None, Some("2029-06-30")), "2029-06-30");
        assert_eq!(with(Some("2025-07-01"), None), cfg.contracts.missing_end);
        assert_eq!(
            with(Some("2029-07-01"), Some("2025-06-30")),
            cfg.contracts.missing_end
        );
    }

    /// Given a stored value and a generated value that deviate from the estimate by the same
    /// amount, inside the stored band but outside the generated one,
    /// When valued,
    /// Then the stored value is kept and the generated one is pulled in: a generated value is
    /// trusted strictly less.
    #[test]
    fn a_deviation_inside_the_stored_band_but_outside_the_generated_one_is_pulled_only_when_generated()
     {
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");
        let estimate = model.estimate(&player(140, Source::Missing)) as f64;
        // Between the two bands, above the estimate: wider than the generated band, narrower than the stored one.
        let between = (model.band_estimated + model.band_save_above) / 2.0;
        let source = (estimate * between.exp()) as u64;

        let stored = model.value(&player(140, Source::Save(source)));
        let generated = model.value(&player(140, Source::Estimated(source)));

        assert_eq!(stored.provenance, Provenance::Source);
        assert_eq!(generated.provenance, Provenance::PulledToEstimate);
        assert!(generated.value_eur < stored.value_eur);
    }

    /// Given a stored value of zero (the save's "no value"),
    /// When valued,
    /// Then it is treated as missing and takes the estimate.
    #[test]
    fn a_stored_zero_is_a_missing_value() {
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");

        assert_eq!(
            model.value(&player(120, Source::Save(0))),
            model.value(&player(120, Source::Missing))
        );
        assert_eq!(
            model.value(&player(120, Source::Save(0))).provenance,
            Provenance::Estimate
        );
    }

    /// Given the fitted model,
    /// When two otherwise identical players differ by 120 ability points,
    /// Then the stronger is worth strictly more: the estimate is not flat.
    #[test]
    fn the_estimate_rises_strictly_with_ability() {
        let model = Model::fit(&population(3_000), &cfg()).expect("fits");

        assert!(
            model.estimate(&player(190, Source::Missing))
                > 10 * model.estimate(&player(70, Source::Missing))
        );
    }

    fn scale_cfg() -> crate::config::ValueScaleCfg {
        crate::config::Config::default_config().value_scale
    }

    /// Players whose value follows a known law: e^(8 + 0.12 * rating) at prime age, halved at 33, a
    /// third at 19 for a player with no ceiling. Deterministic, with a little multiplicative scatter.
    fn scale_population() -> Vec<ScaleSample> {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as f64 / (1u64 << 31) as f64
        };
        let mut out = Vec::new();
        for _ in 0..30_000 {
            let ovr = 35 + (next() * 60.0) as u8;
            let age = 17 + (next() * 20.0) as i32;
            let law = (8.0 + 0.12 * f64::from(ovr)).exp()
                * match age {
                    ..=19 => 0.33,
                    20..=25 => 0.8,
                    26..=29 => 1.0,
                    30..=32 => 0.7,
                    _ => 0.5,
                };
            let scatter = ((next() - 0.5) * 0.3).exp();
            out.push(ScaleSample {
                ovr,
                potential: ovr,
                age,
                value_eur: (law * scatter) as u64,
            });
        }
        out
    }

    /// Given players valued by a known law,
    /// When the scale is fitted,
    /// Then each anchor recovers the law's prime-age value within the scatter, anchors rise with
    /// rating, and the age factors fall for the very young and for veterans.
    #[test]
    fn the_fitted_scale_recovers_the_law_it_was_given() {
        let scale = fit_value_scale(&scale_population(), &scale_cfg()).expect("fits");

        assert!(scale.is_calibrated());
        // (The anchors on the population's edges see a window of one side only.)
        for (rating, value) in scale
            .anchors
            .iter()
            .filter(|(rating, _)| (40..=90).contains(rating))
        {
            let law = (8.0 + 0.12 * f64::from(*rating)).exp();
            assert!(
                (*value as f64 / law - 1.0).abs() < 0.12,
                "{rating}: {value} vs {law}"
            );
        }
        assert!(scale.anchors.windows(2).all(|w| w[0].1 <= w[1].1));
        let factor = |age: i32| f64::from(scale.age_factors[(age - FIRST_AGE) as usize]) / 1000.0;
        assert!((factor(27) - 1.0).abs() < 0.1, "{}", factor(27));
        assert!(factor(18) < 0.5, "{}", factor(18));
        assert!(factor(35) < 0.65, "{}", factor(35));
    }

    /// Given the same samples in any order,
    /// When the scale is fitted twice,
    /// Then it is identical: a world file and a registry built apart share one scale.
    #[test]
    fn the_fit_does_not_depend_on_the_order_of_the_samples() {
        let samples = scale_population();
        let mut reversed = samples.clone();
        reversed.reverse();

        assert_eq!(
            fit_value_scale(&samples, &scale_cfg()),
            fit_value_scale(&reversed, &scale_cfg())
        );
    }

    /// Given too few prime-age players to place four anchors,
    /// When the scale is fitted,
    /// Then there is none and the world keeps the native formula.
    #[test]
    fn too_few_players_give_no_scale() {
        let few: Vec<ScaleSample> = scale_population().into_iter().take(40).collect();

        assert_eq!(fit_value_scale(&few, &scale_cfg()), None);
    }

    /// Given the fitted scale and a player of the law,
    /// When he is valued on it,
    /// Then the value is close to the law's: the scale reproduces the world it was fitted on.
    #[test]
    fn the_scale_values_a_player_like_the_world_it_came_from() {
        let scale = fit_value_scale(&scale_population(), &scale_cfg()).expect("fits");

        let value = scale.value(ofm_core::value_scale::Standing {
            ovr: 80,
            potential: 80,
            age: 27,
        });
        let law = (8.0 + 0.12 * 80.0_f64).exp();

        assert!((value as f64 / law - 1.0).abs() < 0.15, "{value} vs {law}");
    }
}
