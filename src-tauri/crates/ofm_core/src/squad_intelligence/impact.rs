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

impl Reason {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Starter => "Starter",
            Self::Depth => "Depth",
            Self::Succession => "Succession",
        }
    }
}

/// What a player would add to a squad, before the price.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Marginal {
    /// Change in the squad's weighted expected strength.
    pub gain: f32,
    /// Change in this season's expected strength alone: what he does for the side the club fields
    /// now, before any future year is counted. On the same scale as `MIN_GAIN` and as the loan
    /// path's `worth_borrowing`, not weighted by how much the club counts the present.
    pub now_gain: f32,
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
    #[cfg(test)]
    pub(crate) fn gain(&self) -> f32 {
        self.marginal.gain
    }

    /// Whether the signing is worth making: a real gain that the money does not eat.
    pub(crate) fn is_worth_it(&self) -> bool {
        self.adds_something() && self.utility > 0.0
    }

    /// Whether the club's own model can see any gain at all, before the price. The floor under
    /// every transfer: the loan path already asks it of a borrower (`worth_borrowing`), and a fee
    /// must not be a way round it.
    pub(crate) fn adds_something(&self) -> bool {
        self.marginal.gain >= MIN_GAIN
    }

    /// Whether he makes the side better this season, not only in a future one. A club short of
    /// bodies may waive the price for a player who helps it field a better eleven today; a player
    /// whose whole value is a later horizon has to be worth his fee on the club's own scale.
    pub(crate) fn helps_now(&self) -> bool {
        self.marginal.now_gain >= MIN_GAIN
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
    // He is appended after the squad, so his index is the squad's length.
    let starts_at =
        (0..SLOTS).find(|&slot| with.years[0].starter_index[slot] == Some(model.members.len()));
    let now_gain = with.years[0].expected - base.years[0].expected;
    let now_share = model.weights[0] * now_gain;
    let mut marginal = Marginal {
        gain,
        now_gain,
        starts_at,
        points: (gain * SLOTS as f32).round() as i32,
        reason: Reason::Depth,
    };
    marginal.reason = if gain > 0.0 && now_share < gain * 0.25 {
        Reason::Succession
    } else if marginal.starts_at.is_some() {
        Reason::Starter
    } else {
        Reason::Depth
    };
    marginal
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
///
/// A member who neither starts nor covers anyone in any horizon year costs nothing to lose, and is
/// not evaluated. A loss under the surplus line stays under it once a replacement comes (the
/// replacement is never better than the player, so it can only make the loss smaller): the one
/// evaluation is the answer.
pub(crate) fn assess_sale(model: &SquadModel, base: &Evaluation, id: &str) -> SaleCase {
    // A player the club has only borrowed is not the club's to sell: he costs it nothing to lose,
    // as a player it does not hold.
    let Some(index) = model
        .members
        .iter()
        .position(|member| member.id == id && !member.loaned_in)
    else {
        return SaleCase {
            loss: 0.0,
            net_loss: 0.0,
            critical: false,
            surplus: true,
        };
    };
    if !base.involves(index) {
        return SaleCase {
            loss: 0.0,
            net_loss: 0.0,
            critical: false,
            surplus: true,
        };
    }
    let without = model.evaluate_without(id);
    let loss = (base.value - without.value).max(0.0);
    let net_loss = if loss < SURPLUS_LOSS {
        loss
    } else {
        let member = &model.members[index];
        let standard = base.years[0].starter.iter().sum::<f32>() / SLOTS as f32;
        let cheap = cheap_replacement(member, standard - CHEAP_REPLACEMENT_BELOW_STANDARD);
        let replaced = model.evaluate_with_instead_of(id, &cheap);
        (base.value - replaced.value).max(0.0)
    };
    let case = SaleCase {
        loss,
        net_loss,
        critical: net_loss >= CRITICAL_LOSS,
        surplus: net_loss < SURPLUS_LOSS,
    };
    debug_assert!(case.loss >= 0.0 && case.net_loss >= 0.0);
    case
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
    replacement.refresh_levels();
    replacement
}
