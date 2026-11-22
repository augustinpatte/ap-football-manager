//! Passes that stamp facts onto the plans and the built world: the estimated market value of every
//! imported player (`valuation`), and what the save can and cannot say about each of them (no
//! earlier career, an owner outside the world). Kept apart from `pipeline` so its `run` stays the
//! one place the order of the steps is written.

use crate::config::Config;
use crate::free_agents::FreeAgentPlan;
use crate::ids;
use crate::journal::Journal;
use crate::pipeline::PlayerPlan;
use crate::placement::Kind;
use crate::source::PlayerRow;
use ofm_core::generator::WorldData;
use std::collections::{BTreeMap, BTreeSet};

/// What the valuation pass did, for the report and the checks.
#[derive(Debug, Clone, Default)]
pub struct ValuationSummary {
    /// Whether a model was fitted. `false` leaves every source value as it was (a world too small to
    /// calibrate on, `valuation.min_samples`).
    pub fitted: bool,
    pub fitted_on: usize,
    pub residual_mad: f64,
    /// Players by provenance label.
    pub by_provenance: BTreeMap<&'static str, usize>,
}

/// Replaces the value the save stores with the estimated market value (`valuation`): the model is
/// fitted on the placed players' own saved values, then every imported player and every free agent of
/// the pool is valued by it. A world too small to calibrate on keeps the source values untouched.
pub(crate) fn apply_valuation(
    plans: &mut [PlayerPlan],
    free_agent_plans: &mut [FreeAgentPlan],
    rows: &[PlayerRow],
    cfg: &Config,
    on: chrono::NaiveDate,
    journal: &mut Journal,
) -> ValuationSummary {
    use crate::valuation::{Inputs, Model, Source};
    let by_person: BTreeMap<i64, &PlayerRow> = rows.iter().map(|r| (r.person_id, r)).collect();
    // Plans are in person-id order (the source query sorts by it), which fixes the fitting order.
    let inputs: Vec<Option<Inputs>> = plans
        .iter()
        .map(|plan| {
            by_person.get(&plan.person_id).map(|row| {
                Inputs::from_row(
                    row,
                    plan.natural_code,
                    plan.def.contract_end.as_deref(),
                    on,
                    cfg,
                )
            })
        })
        .collect();
    let samples: Vec<Inputs> = inputs.iter().flatten().copied().collect();
    let mut summary = ValuationSummary::default();
    let Some(model) = Model::fit(&samples, &cfg.valuation) else {
        journal.warn(
            "valuation.not_fitted",
            "world",
            format!(
                "fewer than {} saved values: no model fitted, the values of the save are kept as they are",
                cfg.valuation.min_samples
            ),
        );
        for plan in plans.iter() {
            *summary.by_provenance.entry(plan.value_source).or_default() += 1;
        }
        return summary;
    };
    summary.fitted = true;
    summary.fitted_on = model.fitted_on;
    summary.residual_mad = model.residual_mad;
    for (plan, input) in plans.iter_mut().zip(&inputs) {
        let Some(input) = input else { continue };
        let valued = model.value(input);
        plan.def.value = Some(valued.value_eur);
        plan.value_source = valued.provenance.label();
        plan.value_estimate_eur = Some(valued.estimate_eur);
        *summary.by_provenance.entry(plan.value_source).or_default() += 1;
    }
    // A free agent has no club and no contract: he is valued at the median club of the fit, with
    // nothing left on a contract. What the pool estimated for him stands in for a missing value.
    for fa in free_agent_plans.iter_mut() {
        let mut input = Inputs::from_row(&fa.source, fa.natural_code, None, on, cfg);
        input.club_reputation = model.neutral_reputation();
        input.source = match (fa.def.value, fa.value_source) {
            (Some(value), "save") => Source::Save(value),
            (Some(value), _) => Source::Estimated(value),
            (None, _) => Source::Missing,
        };
        let valued = model.value(&input);
        fa.def.value = Some(valued.value_eur);
        fa.value_source = valued.provenance.label();
    }
    journal.info(
        "valuation.fitted",
        "world",
        format!(
            "model fitted on {} saved values (median absolute residual {:.3} in log units); \
             {} players valued, by provenance: {:?}",
            model.fitted_on,
            model.residual_mad,
            plans.len() + free_agent_plans.len(),
            summary.by_provenance
        ),
    );
    summary
}

/// The world's value scale (`valuation::fit_value_scale`), fitted on the players this import has just
/// valued whose owner is a club of the world (a player loaned in from outside is left out: the
/// registry fits the same population, so the world and its external market share one scale).
/// `None` for a world too small to place anchors, which then keeps the native formula.
pub(crate) fn fit_world_value_scale(
    plans: &[PlayerPlan],
    cfg: &Config,
    on: chrono::NaiveDate,
) -> Option<ofm_core::value_scale::ValueScale> {
    use crate::valuation::{ScaleSample, fit_value_scale};
    let samples: Vec<ScaleSample> = plans
        .iter()
        .filter(|plan| plan.placement.kind != Kind::LoanedInFromOutside)
        .filter_map(|plan| {
            Some(ScaleSample {
                ovr: plan.ovr_expected,
                potential: plan.def.potential?,
                age: domain::age::age_on_or_unknown(on, plan.def.date_of_birth.as_deref()?),
                value_eur: plan.def.value?,
            })
        })
        .collect();
    fit_value_scale(&samples, &cfg.value_scale)
}

/// Records what the save can and cannot say about each imported player.
///
/// * The save carries no career records, so every imported player and every pool free agent has an
///   `Unavailable` earlier career: the game must not invent one for a real person.
/// * A player the save has owned by a club outside the world and playing for a club of it stays the
///   borrower's player (nothing could return him to a club the game does not contain), but the
///   owner's identity is kept as the save states it.
pub(crate) fn apply_provenance(
    world: &mut WorldData,
    plans: &[PlayerPlan],
    free_agent_plans: &[FreeAgentPlan],
    rows: &[PlayerRow],
    journal: &mut Journal,
) {
    use domain::player::{EarlierCareer, OutsideOwner};
    let by_person: BTreeMap<i64, &PlayerRow> = rows.iter().map(|r| (r.person_id, r)).collect();
    let outside_owner: BTreeMap<&str, OutsideOwner> = plans
        .iter()
        .filter(|plan| plan.placement.kind == Kind::LoanedInFromOutside)
        .filter_map(|plan| {
            let row = by_person.get(&plan.person_id)?;
            (row.club_id > 0 && !row.club_name.trim().is_empty()).then(|| {
                (
                    plan.def.id.as_str(),
                    OutsideOwner {
                        club_id: ids::club(row.club_id),
                        club_name: row.club_name.clone(),
                    },
                )
            })
        })
        .collect();
    let imported: BTreeSet<&str> = plans
        .iter()
        .map(|plan| plan.def.id.as_str())
        .chain(free_agent_plans.iter().map(|fa| fa.def.id.as_str()))
        .collect();
    let mut marked = 0;
    for player in world.players.iter_mut() {
        if !imported.contains(player.id.as_str()) {
            continue;
        }
        player.provenance.earlier_career = EarlierCareer::Unavailable;
        player.provenance.outside_owner = outside_owner.get(player.id.as_str()).cloned();
        marked += 1;
    }
    journal.info(
        "provenance.earlier_career_unavailable",
        "world",
        format!(
            "{marked} imported players have no earlier career: the save carries none and none is invented; {} of them are owned by a club outside the world",
            outside_owner.len()
        ),
    );
}
