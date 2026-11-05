//! What one player changes: the marginal sporting value of a signing, and what a sale costs.
//!
//! A signing is judged by comparing the squad's expected strength with and without him (the model
//! in [`super::model`]), horizon year by year. That covers, without a rule for each, the cases a
//! club cares about: he starts and improves the eleven; he fills a slot nobody covers; he is the
//! second keeper nobody needed; he displaces a better-suited player; he will be needed when a
//! veteran retires. The price of the money is set against the gain by the club's appetite, so two
//! players who add the same strength are told apart by what they cost.
//!
//! The gain does not depend on the price, so it is worked out once per player ([`Marginal`]) and
//! priced as often as a negotiation needs ([`Marginal::priced`]).

use super::appetite::Appetite;
use super::model::{Evaluation, Member, SLOTS, SquadModel};

/// The smallest strength gain, in mean rating points of the eleven, worth a transfer at all: 1.3
/// rating points added across the side. Under it a signing is churn.
pub(crate) const MIN_GAIN: f32 = 0.12;

/// A sale costing at least this many strength points makes a player a starter.
pub(crate) const STARTER_LOSS: f32 = 0.12;

/// The money has to buy this many times what a sale costs, in the same points, for the sale to be
/// worth making to fund it.
pub(crate) const REINVESTMENT_RATIO: f32 = 3.0;

/// The most a sale may cost before a club calls the player critical, in the same points: a loss
/// this big means no replacement of the same standard is in the squad or on the cheap market.
pub(crate) const CRITICAL_LOSS: f32 = 0.35;

/// A sale costing under this much is of a player the club does not need: surplus.
pub(crate) const SURPLUS_LOSS: f32 = 0.03;

/// The standard of the replacement a club can sign cheaply when a player leaves, in rating points
/// below the club's own average starter: a free agent or a youth, never a star. A player who is
/// worse than this is a player the club can replace for nothing, and one who is better has to be
/// replaced by someone worse.
const CHEAP_REPLACEMENT_BELOW_STANDARD: f32 = 12.0;

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

/// What a player would add to a squad, before the price.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Marginal {
    /// Change in the squad's weighted expected strength.
    pub gain: f32,
    /// The part of it that is the better starting eleven.
    pub starter_gain: f32,
    /// The slot he would start in today, if he would start.
    pub starts_at: Option<usize>,
    /// The gain in slot-rating points (the gain across all eleven slots), on the market's own
    /// rating-point scale.
    pub points: i32,
    pub reason: Reason,
}

/// A [`Marginal`] set against what it costs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SigningCase {
    pub marginal: Marginal,
    /// The gain less the price of the money.
    pub utility: f32,
}

impl SigningCase {
    pub(crate) fn gain(&self) -> f32 {
        self.marginal.gain
    }

    /// Whether the signing is worth making: a real gain that the money does not eat.
    pub(crate) fn is_worth_it(&self) -> bool {
        self.marginal.gain >= MIN_GAIN && self.utility > 0.0
    }

    /// The utility in slot-rating points, for ranking against the market's own scale.
    pub(crate) fn utility_points(&self) -> i32 {
        (self.utility * SLOTS as f32).round() as i32
    }
}

impl Marginal {
    /// Set against `price`, for a club with this `budget` and `appetite`.
    pub(crate) fn priced(&self, price: Price, budget: Budget, appetite: &Appetite) -> SigningCase {
        let fee_share = share(price.fee as f64, budget.cash as f64);
        let wage_share = share(
            f64::from(price.weekly_wage),
            budget.weekly_wage_budget as f64,
        );
        SigningCase {
            marginal: *self,
            utility: self.gain - appetite.price * (fee_share + wage_share),
        }
    }
}

/// The marginal value of adding `candidate` to the squad.
pub(crate) fn assess_signing(
    model: &SquadModel,
    base: &Evaluation,
    candidate: &Member,
) -> Marginal {
    let with = model.evaluate_with(candidate);
    let gain = with.value - base.value;
    let starter_gain: f32 = (0..model.weights.len())
        .map(|year| model.weights[year] * (with.years[year].xi - base.years[year].xi))
        .sum();
    // He is appended after the squad, so his index is the squad's length.
    let starts_at =
        (0..SLOTS).find(|&slot| with.years[0].starter_index[slot] == Some(model.members.len()));
    let now_gain = model.weights[0] * (with.years[0].expected - base.years[0].expected);
    let reason = if gain > 0.0 && now_gain < gain * 0.25 {
        Reason::Succession
    } else if starts_at.is_some() {
        Reason::Starter
    } else {
        Reason::Depth
    };
    Marginal {
        gain,
        starter_gain,
        starts_at,
        points: (gain * SLOTS as f32).round() as i32,
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
    /// Strength lost if he goes and nobody comes, in the same weighted points a signing gains.
    pub loss: f32,
    /// Strength lost once the club has signed the cheapest replacement it could: what the sale
    /// really costs, as a club can always buy a body.
    pub net_loss: f32,
    /// He is a first choice whom nobody of the same standard replaces.
    pub critical: bool,
    /// The club hardly misses him.
    pub surplus: bool,
}

/// The cost of letting `id` go.
pub(crate) fn assess_sale(model: &SquadModel, base: &Evaluation, id: &str) -> SaleCase {
    let without = model.evaluate_without(id);
    let loss = (base.value - without.value).max(0.0);
    let net_loss = model
        .members
        .iter()
        .find(|member| member.id == id)
        .map_or(loss, |member| {
            let standard = base.years[0].starter.iter().sum::<f32>() / SLOTS as f32;
            let cheap = cheap_replacement(member, standard - CHEAP_REPLACEMENT_BELOW_STANDARD);
            let replaced = model.evaluate_with_instead_of(id, &cheap);
            (base.value - replaced.value).max(0.0)
        });
    SaleCase {
        loss,
        net_loss,
        critical: net_loss >= CRITICAL_LOSS,
        surplus: net_loss < SURPLUS_LOSS,
    }
}

/// The player who would come in his place for next to nothing: the same kind of player, no better
/// than `cap` in any slot.
fn cheap_replacement(member: &Member, cap: f32) -> Member {
    let mut replacement = member.clone();
    replacement.id = format!("{}~replacement", member.id);
    for fit in &mut replacement.fit {
        *fit = fit.min(cap);
    }
    // A free agent is not a prospect and he is not going to retire either: held where he is.
    replacement.shift = [Some(0.0); super::horizon::HORIZON];
    replacement
}
