//! Answering a transfer offer straight from its inbox message.
//!
//! The message offers `accept` / `reject` on the offer it describes; this runs the same
//! [`respond_to_offer`] the Transfers tab does, so every rule (squad floor, buyer's wage policy,
//! cash, registration date) applies identically, and nothing is decided twice.

use super::*;
use crate::messages::TRANSFER_OFFER_ACTION_PREFIX;
use std::collections::HashMap;

/// What the player is told about an answer given from the inbox.
pub struct TransferOfferResponseEffect {
    pub message: String,
    pub i18n_key: String,
    pub i18n_params: HashMap<String, String>,
}

fn effect(key: &str, player: &str) -> TransferOfferResponseEffect {
    TransferOfferResponseEffect {
        message: String::new(),
        i18n_key: key.to_string(),
        i18n_params: HashMap::from([("player".to_string(), player.to_string())]),
    }
}

/// Applies an inbox answer to a transfer offer. `None` when the action is not one of ours, the
/// option is unknown, or it was already answered (so a replayed request does nothing). An offer
/// that has since gone — answered in the Transfers tab, withdrawn, expired — closes the action
/// with an explanation. Any other refusal leaves the action open, with the reason, so the manager
/// can still answer it from the Transfers tab.
pub fn apply_transfer_offer_response(
    game: &mut Game,
    message_id: &str,
    action_id: &str,
    option_id: &str,
) -> Option<TransferOfferResponseEffect> {
    let offer_id = action_id
        .strip_prefix(TRANSFER_OFFER_ACTION_PREFIX)?
        .to_string();
    let accept = match option_id {
        "accept" => true,
        "reject" => false,
        _ => return None,
    };
    let message_index = game.messages.iter().position(|m| m.id == message_id)?;
    let action_index = game.messages[message_index]
        .actions
        .iter()
        .position(|action| action.id == action_id)?;
    if game.messages[message_index].actions[action_index].resolved {
        return None;
    }
    let player_id = game.messages[message_index].context.player_id.clone()?;
    let player_name = game
        .players
        .iter()
        .find(|player| player.id == player_id)
        .map(|player| player.full_name.clone())?;

    let (resolved, key) = match respond_to_offer(game, &player_id, &offer_id, accept) {
        Ok(()) if accept => (true, "be.msg.transferInterest.effect.accepted".to_string()),
        Ok(()) => (true, "be.msg.transferInterest.effect.rejected".to_string()),
        Err(error) if error == ERR_OFFER_NOT_PENDING => {
            (true, "be.msg.transferInterest.effect.expired".to_string())
        }
        Err(error) => (false, error),
    };
    if resolved {
        game.messages[message_index].actions[action_index].resolved = true;
    }
    Some(effect(&key, &player_name))
}
