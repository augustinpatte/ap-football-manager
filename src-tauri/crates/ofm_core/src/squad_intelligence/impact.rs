//! What one player changes: the marginal sporting value of a signing, and what a sale costs.
//!
//! A signing is judged by comparing the squad's expected strength with and without him (the model
//! in [`super::model`]), horizon year by year. That covers, without a rule for each, the cases a
//! club cares about: he starts and improves the eleven; he fills a slot nobody covers; he is the
//! second keeper nobody needed; he displaces a better-suited player; he will be needed when a
//! veteran retires. The price of the money is set against the gain by the club's appetite, so two
//! players who add the same strength are told apart by what they cost.

use super::appetite::Appetite;
use super::model::{Evaluation, Member, SLOTS, SquadModel};

/// The smallest strength gain, in mean rating points of the eleven, worth a transfer at all: 1.3
/// rating points added across the side. Under it a signing is churn.
pub(crate) const MIN_GAIN: f32 = 0.12;

/// The most a sale may cost before a club calls the player critical, in the same points: a loss
/// this big means no replacement of the same standard is in the squad.
const CRITICAL_LOSS: f32 = 0.35;

/// A sale costing under this much is of a player the club does not need: surplus.
const SURPLUS_LOSS: f32 = 0.03;

/// What the club has to spend.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Budget {
    /// Cash for a fee: the lesser of the transfer budget and the balance.
    pub cash: i64,
    /// The weekly wage budget.
    pub weekly_wage_budget: i64,
}

/// What a signing costs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Price {
    pub fee: u64,
    pub weekly_wage: u32,
}

/// Where most of a signing's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// He would start and improve the eleven.
    Starter,
    /// He would be cover where the squad has none, or a better one.
    Depth,
    /// He is not needed now but will be.
    Succession,
}

/// The sporting value of a signing, and what it is worth after its price.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SigningCase {
    /// Change in the squad's weighted expected strength.
    pub gain: f32,
    /// The part of it that is the better starting eleven.
    pub starter_gain: f32,
    /// The slot he would start in today, if he would start.
    pub starts_at: Option<usize>,
    /// The gain in slot-rating points (the gain across all eleven slots), for pricing and ranking
    /// against the market's own rating-point scale.
    pub points: i32,
    /// The gain less the price of the money.
    pub utility: f32,
    pub reason: Reason,
}

impl SigningCase {
    /// Whether the signing is worth making: a real gain that the money does not eat.
    pub(crate) fn is_worth_it(&self) -> bool {
        self.gain >= MIN_GAIN && self.utility > 0.0
    }
}

/// The marginal value of adding `candidate` to the squad, priced.
pub(crate) fn assess_signing(
    model: &SquadModel,
    base: &Evaluation,
    candidate: &Member,
    price: Price,
    budget: Budget,
    appetite: &Appetite,
) -> SigningCase {
    let with = model.evaluate_with(candidate);
    let gain = with.value - base.value;
    let starter_gain: f32 = (0..model.weights.len())
        .map(|year| model.weights[year] * (with.years[year].xi - base.years[year].xi))
        .sum();
    // He is appended after the squad, so his index is the squad's length.
    let starts_at =
        (0..SLOTS).find(|&slot| with.years[0].starter_index[slot] == Some(model.members.len()));
    let fee_share = share(price.fee as f64, budget.cash as f64);
    let wage_share = share(
        f64::from(price.weekly_wage),
        budget.weekly_wage_budget as f64,
    );
    let utility = gain - appetite.price * (fee_share + wage_share);
    let now_gain = model.weights[0] * (with.years[0].expected - base.years[0].expected);
    let reason = if gain > 0.0 && now_gain < gain * 0.25 {
        Reason::Succession
    } else if starts_at.is_some() {
        Reason::Starter
    } else {
        Reason::Depth
    };
    SigningCase {
        gain,
        starter_gain,
        starts_at,
        points: (gain * SLOTS as f32).round() as i32,
        utility,
        reason,
    }
}

fn share(amount: f64, of: f64) -> f32 {
    if amount <= 0.0 {
        return 0.0;
    }
    // A club with nothing to spend prices every pound at the full price: the sum is not finite
    // but the cap keeps the comparison meaningful.
    (amount / of.max(1.0)).min(4.0) as f32
}

/// What selling a player does to the squad.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SaleCase {
    /// Strength lost, in the same weighted points a signing gains.
    pub loss: f32,
    /// He is a first choice whom nobody of the same standard replaces.
    pub critical: bool,
    /// The club hardly misses him.
    pub surplus: bool,
}

/// The cost of letting `id` go.
pub(crate) fn assess_sale(model: &SquadModel, base: &Evaluation, id: &str) -> SaleCase {
    let without = model.evaluate_without(id);
    let loss = (base.value - without.value).max(0.0);
    SaleCase {
        loss,
        critical: loss >= CRITICAL_LOSS,
        surplus: loss < SURPLUS_LOSS,
    }
}
