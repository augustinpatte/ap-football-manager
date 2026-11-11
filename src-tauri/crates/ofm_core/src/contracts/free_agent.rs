//! Signing free agents: the affordability projection, the one function that puts a
//! contract-less player on a club's books, and the announcement message. The negotiation that
//! leads to a signing lives in [`super::free_agent_talks`].

use super::*;

pub fn project_free_agent_contract_impact(
    game: &Game,
    player_id: &str,
    offered_wage: u32,
) -> Result<RenewalFinancialProjection, String> {
    let manager_team_id = game
        .manager
        .team_id
        .clone()
        .ok_or(ERR_NO_TEAM_ASSIGNED.to_string())?;
    let team = game
        .teams
        .iter()
        .find(|candidate| candidate.id == manager_team_id)
        .ok_or(ERR_MANAGED_TEAM_NOT_FOUND.to_string())?;
    let player = game
        .players
        .iter()
        .find(|candidate| candidate.id == player_id)
        .ok_or(ERR_PLAYER_NOT_FOUND.to_string())?;

    if player.retired || player.team_id.is_some() {
        return Err(ERR_PLAYER_NOT_FREE_AGENT.to_string());
    }

    Ok(project_contract_offer_financial_impact(
        game,
        team,
        player,
        offered_wage,
    ))
}

pub(crate) fn free_agent_signed_message(
    player_id: &str,
    player_name: &str,
    team_name: &str,
    contract_years: u32,
    date: &str,
) -> InboxMessage {
    let mut i18n_params = std::collections::HashMap::new();
    i18n_params.insert("player".to_string(), player_name.to_string());
    i18n_params.insert("team".to_string(), team_name.to_string());
    i18n_params.insert("years".to_string(), contract_years.to_string());

    InboxMessage::new(
        format!("free_agent_signed_{}", player_id),
        String::new(),
        String::new(),
        String::new(),
        date.to_string(),
    )
    .with_category(MessageCategory::Contract)
    .with_priority(MessagePriority::Urgent)
    .with_sender_role("")
    .with_i18n(
        "be.msg.freeAgentSigned.subject",
        "be.msg.freeAgentSigned.body",
        i18n_params,
    )
    .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

/// Sign a free agent to `team` on the given terms: the club, the contract, a
/// shirt number, the movement record and the morale of a player who has found
/// a club. The one way a free agent joins a club — a negotiated offer, a rival's bid and
/// the squad-floor top-up all come through here, so a signing means the same
/// thing whoever makes it. Whether the deal should happen at all (the wage
/// policy, the player's demands) is the caller's question, not this one's; but a player who
/// already has a club cannot be signed twice, and every other club's open talk with him ends.
pub(crate) fn sign_free_agent(
    game: &mut Game,
    player_index: usize,
    team: &Team,
    weekly_wage: u32,
    contract_years: u32,
    current_date: NaiveDate,
) -> Result<(), String> {
    if game.players[player_index].team_id.is_some() || game.players[player_index].retired {
        return Err(ERR_PLAYER_NOT_FREE_AGENT.to_string());
    }
    let new_contract_end = current_date
        .checked_add_months(Months::new(contract_years * 12))
        .ok_or(ERR_UNABLE_TO_CALCULATE_CONTRACT_END_DATE.to_string())?;
    let resolved_jersey_number =
        crate::roster::resolve_jersey_for(game, &game.players[player_index], team);

    let player = &mut game.players[player_index];
    player.team_id = Some(team.id.clone());
    player.jersey_number = resolved_jersey_number;
    player.transfer_listed = false;
    player.loan_listed = false;
    player.transfer_offers.clear();
    record_movement(
        player,
        contract_entry(
            PlayerMovementKind::FreeAgentSigning,
            current_date,
            team,
            contract_record(
                current_date,
                new_contract_end,
                weekly_wage,
                ContractSource::FreeAgent,
            ),
        ),
    );
    if matches!(
        player
            .morale_core
            .unresolved_issue
            .as_ref()
            .map(|issue| &issue.category),
        Some(domain::player::PlayerIssueCategory::Contract)
    ) {
        player.morale_core.unresolved_issue = None;
    }
    player.morale_core.renewal_state = None;
    player.morale = (i16::from(player.morale) + 6).clamp(0, 100) as u8;
    player.morale_core.manager_trust = player.morale_core.manager_trust.max(55);
    settle_talks_on_signing(game, player_index, team, current_date);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::test_support::uniform_attributes;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::Position;

    fn game_with_player(team_id: Option<&str>) -> Game {
        let mut team = Team::new(
            "club".to_string(),
            "Club".to_string(),
            "CLB".to_string(),
            "England".to_string(),
            "Town".to_string(),
            "Ground".to_string(),
            10_000,
        );
        team.manager_id = Some("m".to_string());
        let mut manager = Manager::new(
            "m".to_string(),
            "A".to_string(),
            "B".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("club".to_string());
        let mut player = Player::new(
            "p".to_string(),
            "P".to_string(),
            "P".to_string(),
            "1998-01-01".to_string(),
            "England".to_string(),
            Position::Midfielder,
            uniform_attributes(50),
        );
        player.team_id = team_id.map(str::to_string);
        Game::new(
            GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()),
            manager,
            vec![team],
            vec![player],
            vec![],
            vec![],
        )
    }

    /// Given a player who already belongs to a club
    /// When any caller tries to sign him as a free agent
    /// Then the signing is refused and nothing about him changes.
    #[test]
    fn a_player_with_a_club_cannot_be_signed_again() {
        let mut game = game_with_player(Some("other"));
        let team = game.teams[0].clone();
        let today = game.clock.current_date.date_naive();

        let refused = sign_free_agent(&mut game, 0, &team, 1_000, 2, today);

        assert_eq!(refused, Err(ERR_PLAYER_NOT_FREE_AGENT.to_string()));
        assert_eq!(game.players[0].team_id.as_deref(), Some("other"));
        assert!(game.players[0].movement_history.is_empty());
    }

    /// Given a free agent with the manager's offer open and a rival club's offer open
    /// When the squad-floor path signs him for the rival directly, without a talk
    /// Then the rival's talk is closed as signed, the manager's as lost, and he is told who got him.
    #[test]
    fn a_signing_through_any_path_closes_every_open_talk() {
        use domain::free_agent_talk::{FreeAgentTalk, FreeAgentTalkStatus};
        let mut game = game_with_player(None);
        let mine = FreeAgentTalk {
            id: "mine".to_string(),
            club_id: "club".to_string(),
            made_by_manager: true,
            ..FreeAgentTalk::default()
        };
        let theirs = FreeAgentTalk {
            id: "theirs".to_string(),
            club_id: "rival".to_string(),
            ..FreeAgentTalk::default()
        };
        game.players[0].morale_core.free_agent_talks = vec![mine, theirs];
        let rival = Team::new(
            "rival".to_string(),
            "Rival FC".to_string(),
            "RIV".to_string(),
            "England".to_string(),
            "City".to_string(),
            "Ground".to_string(),
            10_000,
        );
        let today = game.clock.current_date.date_naive();

        sign_free_agent(&mut game, 0, &rival, 1_000, 2, today).expect("he is free");

        let talks = &game.players[0].morale_core.free_agent_talks;
        assert_eq!(talks[0].status, FreeAgentTalkStatus::LostToRival);
        assert_eq!(talks[1].status, FreeAgentTalkStatus::Signed);
        let told = game
            .messages
            .iter()
            .find(|message| {
                message.subject_key.as_deref() == Some("be.msg.freeAgentTalk.lost.subject")
            })
            .expect("the manager is told");
        assert_eq!(
            told.i18n_params.get("team").map(String::as_str),
            Some("Rival FC")
        );
    }
}
