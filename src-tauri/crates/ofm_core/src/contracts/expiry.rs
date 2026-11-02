//! Contract expiry: the daily sweep that warns on approaching expiries and
//! releases players whose deals have run out, plus the shared release routine
//! that both expiry and manager-termination funnel through. Byte-faithful
//! extraction from the original contracts.rs.

use super::*;

pub fn contract_warning_stage(
    contract_end: Option<&str>,
    current_date: NaiveDate,
) -> Option<ContractWarningStage> {
    let days_remaining = contract_days_remaining(contract_end, current_date)?;

    if days_remaining <= 0 {
        return None;
    }

    if days_remaining <= 30 {
        return Some(ContractWarningStage::FinalWeeks);
    }

    if days_remaining <= 90 {
        return Some(ContractWarningStage::ThreeMonths);
    }

    if days_remaining <= 180 {
        return Some(ContractWarningStage::SixMonths);
    }

    if days_remaining <= 365 {
        return Some(ContractWarningStage::TwelveMonths);
    }

    None
}

/// The key of the warning a player's contract is at: the contract's end and the stage, so the
/// next stage and the next contract are different warnings.
fn warning_key(player: &Player, stage: ContractWarningStage) -> Option<String> {
    Some(format!(
        "{}|{}",
        player.contract_end()?,
        stage.message_suffix()
    ))
}

/// Whether the player's expiring contract is something to stop the manager for today: inside the
/// last six months (a year out is the player's concern, not an alert, and sends no message),
/// not already given up on (let-expire), and not already dealt with at this stage.
///
/// The same warning used to be raised every day until the contract was renewed or released:
/// answering the player's message, opening renewal talks, or choosing to carry on changed
/// nothing, so each day of a fast-forward stopped on it again.
pub fn contract_needs_attention(player: &Player, current_date: NaiveDate) -> bool {
    let Some(stage) = contract_warning_stage(player.contract_end(), current_date) else {
        return false;
    };
    if stage == ContractWarningStage::TwelveMonths || has_let_expire_intent(player) {
        return false;
    }
    let handled = player
        .morale_core
        .renewal_state
        .as_ref()
        .and_then(|state| state.warning_handled.as_deref());
    handled != warning_key(player, stage).as_deref()
}

/// Records that the manager dealt with the warning the player's contract is at today.
pub(crate) fn mark_contract_warning_handled(player: &mut Player, current_date: NaiveDate) {
    let Some(stage) = contract_warning_stage(player.contract_end(), current_date) else {
        return;
    };
    let Some(key) = warning_key(player, stage) else {
        return;
    };
    player
        .morale_core
        .renewal_state
        .get_or_insert_with(ContractRenewalState::default)
        .warning_handled = Some(key);
}

/// The manager chose to carry on past the contract warnings showing now: they are dealt with
/// until the next stage. Returns how many players' warnings that was.
pub fn acknowledge_user_team_contract_warnings(game: &mut Game) -> usize {
    let Some(team_id) = game.manager.team_id.clone() else {
        return 0;
    };
    let today = game.clock.current_date.date_naive();
    let mut acknowledged = 0;
    for player in game
        .players
        .iter_mut()
        .filter(|player| player.team_id.as_deref() == Some(team_id.as_str()))
    {
        if contract_needs_attention(player, today) {
            mark_contract_warning_handled(player, today);
            acknowledged += 1;
        }
    }
    acknowledged
}

/// The warning stage of each requested player's contract on the game's date: the one the daily
/// warnings, the morale pressure and the renewal urgency use, so a screen shows the same stage.
/// A player with no stage (beyond a year, or already over) maps to `None`; an unknown id is left out.
pub fn contract_warning_stages(
    game: &Game,
    player_ids: &[String],
) -> std::collections::HashMap<String, Option<ContractWarningStage>> {
    let today = game.clock.current_date.date_naive();
    player_ids
        .iter()
        .filter_map(|id| {
            let player = game.players.iter().find(|player| &player.id == id)?;
            Some((
                id.clone(),
                contract_warning_stage(player.contract_end(), today),
            ))
        })
        .collect()
}

pub fn process_contract_expiries(game: &mut Game) {
    let current_date = game.clock.current_date.date_naive();

    let expired_player_indices: Vec<usize> = game
        .players
        .iter()
        .enumerate()
        .filter_map(|(index, player)| {
            let days_remaining = contract_days_remaining(player.contract_end(), current_date)?;
            if player.team_id.is_some() && days_remaining <= 0 {
                Some(index)
            } else {
                None
            }
        })
        .collect();

    for player_index in expired_player_indices {
        release_player_contract(game, player_index, ContractReleaseReason::Expired);
    }
}

pub(crate) fn remove_player_from_team_references(team: &mut Team, player_id: &str) {
    team.remove_player_references(player_id);
}

pub(crate) fn release_player_contract(
    game: &mut Game,
    player_index: usize,
    reason: ContractReleaseReason,
) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let player_id = game.players[player_index].id.clone();
    let player_name = game.players[player_index].match_name.clone();
    let team_id = game.players[player_index]
        .contract_club_id()
        .map(str::to_string);

    let Some(team_id) = team_id.as_deref() else {
        return;
    };
    let Some(team_name) = game
        .teams
        .iter()
        .find(|candidate| candidate.id == team_id)
        .map(|team| team.name.clone())
    else {
        return;
    };
    // Expiry releases every club's players, so whether the user hears about it is decided per
    // player: the contract is theirs, or the player is in their squad on loan and leaves it.
    let user_team_id = game.manager.team_id.as_deref();
    let leaves_user_squad = user_team_id.is_some_and(|user_team_id| {
        user_team_id == team_id
            || game.players[player_index].team_id.as_deref() == Some(user_team_id)
    });

    for team in &mut game.teams {
        remove_player_from_team_references(team, &player_id);
    }

    let player = &mut game.players[player_index];
    player.team_id = None;
    player.active_loan = None;
    player.transfer_listed = false;
    player.loan_listed = false;
    player.transfer_offers.clear();
    player.loan_offers.clear();
    player.morale_core.renewal_state = None;
    // Recording the release is what ends the contract: the dates and the wage follow it.
    record_movement(
        player,
        PlayerMovementEntry {
            from_team_id: Some(team_id.to_string()),
            from_team_name: Some(team_name.clone()),
            release_reason: Some(match reason {
                ContractReleaseReason::Expired => ReleaseReason::Expired,
                ContractReleaseReason::ManagerTermination { .. } => ReleaseReason::Terminated,
            }),
            ..PlayerMovementEntry::new(today.clone(), PlayerMovementKind::Released)
        },
    );

    if !leaves_user_squad {
        return;
    }
    // Each release appends one entry to the append-only movement history, so its position names
    // this release and no other: a player can be released, re-signed and released again, even
    // on the same day, and a key that named only the player or the day would let the ledger
    // silence the second release.
    let release_number = game.players[player_index].movement_history.len();
    let release_id = format!("{player_id}_{today}_{release_number}");
    let message = match reason {
        ContractReleaseReason::Expired => {
            contract_expired_message(&release_id, &player_name, &team_name, &today)
        }
        ContractReleaseReason::ManagerTermination { severance_cost } => {
            contract_terminated_message(
                &release_id,
                &player_name,
                &team_name,
                severance_cost,
                &today,
            )
        }
    };

    crate::inbox::emit(game, message);
}

pub(crate) fn contract_expired_message(
    release_id: &str,
    player_name: &str,
    team_name: &str,
    date: &str,
) -> InboxMessage {
    let mut i18n_params = std::collections::HashMap::new();
    i18n_params.insert("player".to_string(), player_name.to_string());
    i18n_params.insert("team".to_string(), team_name.to_string());

    InboxMessage::new(
        format!("contract_expired_{release_id}"),
        String::new(),
        String::new(),
        String::new(),
        date.to_string(),
    )
    .with_category(MessageCategory::Contract)
    .with_priority(MessagePriority::Urgent)
    .with_sender_role("")
    .with_i18n(
        "be.msg.contractExpired.subject",
        "be.msg.contractExpired.body",
        i18n_params,
    )
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}
