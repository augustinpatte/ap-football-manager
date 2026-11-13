//! What a permanent transfer can carry beyond its fee, and the debts it leaves behind.
//!
//! [`TransferTerms`] ride on the offer while it is negotiated (so they survive a deal that waits
//! for the window to open). Once a deal completes, what is still owed — the later instalments of
//! the fee, a clause on the next sale — becomes a [`TransferObligation`], kept on the game and
//! persisted in its own table so it outlives the offer, which is pruned.

use serde::{Deserialize, Serialize};

/// Most instalments a fee may be split into: one now, the rest a year apart.
pub const MAX_INSTALMENTS: u8 = 4;
/// Largest share of a later sale a seller may keep, per cent.
pub const MAX_SELL_ON_PCT: u8 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TransferTerms {
    /// Equal payments of the fee: the first on completion, each next one a year later. 1 is a
    /// single payment.
    pub instalments: u8,
    /// Per cent of the fee the buyer later gets for the player that the seller is owed.
    pub sell_on_pct: u8,
    /// Paid to the player on completion, on top of the fee.
    pub signing_bonus: u64,
    /// The buyer promises not to obstruct a suitable future move; see `SteppingStonePromise`.
    pub promise_stepping_stone: bool,
    /// The weekly wage the buyer offers the player for his new contract. Zero: the buyer's
    /// standard terms (what he would ask a club of that stature), the only terms before the
    /// player had a say.
    pub player_weekly_wage: u32,
    /// Length of that contract in years. Zero: the standard length for his age.
    pub player_contract_years: u8,
}

impl Default for TransferTerms {
    fn default() -> Self {
        Self {
            instalments: 1,
            sell_on_pct: 0,
            signing_bonus: 0,
            promise_stepping_stone: false,
            player_weekly_wage: 0,
            player_contract_years: 0,
        }
    }
}

impl TransferTerms {
    /// Whether the manager has put a contract to the player: a wage or a length of his own,
    /// rather than leaving the buyer's standard ones to be assumed.
    pub fn names_player_terms(&self) -> bool {
        self.player_weekly_wage > 0 || self.player_contract_years > 0
    }

    /// Nothing but a fee paid at once: what every offer was before terms existed.
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransferObligation {
    /// One later payment of an agreed fee. Settled once: `paid_on` is set in the same step that
    /// moves the money.
    Instalment {
        id: String,
        player_id: String,
        payer_team_id: String,
        payee_team_id: String,
        amount: u64,
        /// `YYYY-MM-DD`.
        due: String,
        /// 1-based position among the instalments, the first being paid on completion.
        number: u8,
        of: u8,
        #[serde(default)]
        paid_on: Option<String>,
    },
    /// A share of the next fee the holder gets for the player. Consumed by that sale.
    SellOn {
        id: String,
        player_id: String,
        /// The club that owes it when it sells.
        holder_team_id: String,
        beneficiary_team_id: String,
        pct: u8,
        #[serde(default)]
        settled_on: Option<String>,
    },
}

impl TransferObligation {
    pub fn id(&self) -> &str {
        match self {
            Self::Instalment { id, .. } | Self::SellOn { id, .. } => id,
        }
    }
}
