//! Free-agent contract talks: an offer to a player without a club is a negotiation that takes a
//! few days of game time and ends in a signing, a refusal, a counter-offer, a withdrawal or a
//! rival's signing.
//!
//! * [`appeal`] — how a player weighs an offer: one explainable score.
//! * [`workflow`] — the talk itself, and the single place a signing is completed.
//! * [`rivals`] — AI clubs competing for the attractive free agents.
//!
//! Interfaces for the rest of the game: [`process_free_agent_talks`] is the daily step (the clock
//! calls it once per day); [`submit_free_agent_offer`], [`accept_free_agent_counter`] and
//! [`withdraw_free_agent_offer`] are the manager's actions, in the same `&mut Game` terms an
//! assistant acting for the manager would use; [`free_agent_talks_awaiting_manager`] counts the
//! answers the manager still owes.

use super::*;

mod appeal;
mod consts;
mod rivals;
mod workflow;

pub use appeal::{Appeal, OfferTerms, PlaceInSide, Verdict};
pub(crate) use rivals::declines_on_standing;
pub(crate) use workflow::settle_talks_on_signing;
pub use workflow::{
    FreeAgentInterest, FreeAgentOfferDecision, FreeAgentOfferOutcome, InterestLevel, RivalInterest,
    accept_free_agent_counter, assess_free_agent_interest, free_agent_talks_awaiting_manager,
    process_free_agent_talks, submit_free_agent_offer, withdraw_free_agent_offer,
};
