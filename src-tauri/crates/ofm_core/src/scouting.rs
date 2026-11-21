use crate::game::{
    Game, ScoutingAssignment, YouthScoutingAssignment, YouthScoutingObjective, YouthScoutingRegion,
};
use domain::contract_ledger::ContractSource;
use domain::message::*;
use domain::player::{Player, PlayerMovementKind, Position, SquadRole};
use domain::staff::StaffRole;
use rand::RngExt;
use std::collections::HashMap;

const ERR_SCOUT_NOT_FOUND: &str = "be.error.scouting.scoutNotFound";
const ERR_STAFF_MEMBER_NOT_SCOUT: &str = "be.error.scouting.staffMemberNotScout";
const ERR_SCOUT_NOT_IN_TEAM: &str = "be.error.scouting.scoutNotInTeam";
const ERR_CANNOT_SCOUT_OWN_PLAYER: &str = "be.error.scouting.cannotScoutOwnPlayer";
const ERR_PLAYER_ALREADY_SCOUTED: &str = "be.error.scouting.playerAlreadyScouted";
const ERR_YOUTH_SEARCH_ALREADY_ACTIVE: &str = "be.error.scouting.youthSearchAlreadyActive";
const ERR_ASSIGNMENT_NOT_FOUND: &str = "be.error.scouting.assignmentNotFound";
const ERR_YOUTH_ASSIGNMENT_NOT_FOUND: &str = "be.error.scouting.youthAssignmentNotFound";
const ERR_SCOUT_ALREADY_ASSIGNED_TO_SEARCH: &str = "be.error.scouting.scoutAlreadyAssignedToSearch";

fn scouting_error_with_params(key: &str, params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return key.to_string();
    }

    let query = params
        .iter()
        .map(|(name, value)| format!("{}={}", name, value))
        .collect::<Vec<_>>()
        .join("&");

    format!("{}?{}", key, query)
}

fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Scouts can only handle one assignment at a time across player and youth scouting.
pub fn scout_max_assignments(judging_ability: u8) -> usize {
    let _ = judging_ability;
    1
}

fn scout_assignment_count(game: &Game, scout_id: &str) -> usize {
    game.scouting_assignments
        .iter()
        .filter(|assignment| assignment.scout_id == scout_id)
        .count()
        + game
            .youth_scouting_assignments
            .iter()
            .filter(|assignment| assignment.scout_id == scout_id)
            .count()
}

/// Whether this person can take a scouting assignment: the Scout role, but not the Analyst post.
/// FM24 imports file recruitment analysts under `StaffRole::Scout`, so the role alone over-counts.
/// The frontend mirrors this in `src/lib/scoutEligibility.ts`; change both together.
pub fn is_field_scout(staff: &domain::staff::Staff) -> bool {
    staff.role == StaffRole::Scout && staff.post() != domain::staff::StaffPost::Analyst
}

fn resolve_user_scout<'a>(
    game: &'a Game,
    scout_id: &str,
) -> Result<&'a domain::staff::Staff, String> {
    let user_team_id = game
        .manager
        .team_id
        .as_ref()
        .ok_or("be.error.noTeamAssigned")?;

    let scout = game
        .staff
        .iter()
        .find(|staff_member| staff_member.id == scout_id)
        .ok_or(ERR_SCOUT_NOT_FOUND)?;
    if !is_field_scout(scout) {
        return Err(ERR_STAFF_MEMBER_NOT_SCOUT.to_string());
    }
    if scout.team_id.as_ref() != Some(user_team_id) {
        return Err(ERR_SCOUT_NOT_IN_TEAM.to_string());
    }

    Ok(scout)
}

/// Days a scout adds to a job on a player outside the manager's own division.
const DAYS_TO_TRAVEL: u32 = 2;

fn assignment_days_for_player_scouting(judging_ability: u8) -> u32 {
    if judging_ability >= 80 {
        2
    } else if judging_ability >= 60 {
        3
    } else if judging_ability >= 40 {
        4
    } else {
        5
    }
}

fn assignment_days_for_youth_scouting(
    judging_potential: u8,
    region: YouthScoutingRegion,
    objective: YouthScoutingObjective,
) -> u32 {
    let base = if judging_potential >= 80 {
        4
    } else if judging_potential >= 60 {
        5
    } else if judging_potential >= 40 {
        6
    } else {
        7
    };

    let region_modifier = match region {
        YouthScoutingRegion::Domestic => 0,
        YouthScoutingRegion::International => 1,
    };
    let objective_modifier = match objective {
        YouthScoutingObjective::Balanced => 0,
        YouthScoutingObjective::HighPotential => 1,
        YouthScoutingObjective::ReadySoon => 0,
    };

    base + region_modifier + objective_modifier
}

/// Send a scout to evaluate a player. Returns an error string if invalid.
pub fn send_scout(game: &mut Game, scout_id: &str, player_id: &str) -> Result<(), String> {
    let user_team_id = game
        .manager
        .team_id
        .as_ref()
        .ok_or("be.error.noTeamAssigned")?;
    let scout = resolve_user_scout(game, scout_id)?;

    // Validate player exists and is not on user's team
    let player = game
        .players
        .iter()
        .find(|p| p.id == player_id)
        .ok_or("be.error.playerNotFound")?;
    if player.team_id.as_deref() == Some(user_team_id.as_str()) {
        return Err(ERR_CANNOT_SCOUT_OWN_PLAYER.to_string());
    }

    // Check scout capacity across both player and youth scouting.
    let max_slots = scout_max_assignments(scout.attributes.judging_ability);
    let current_count = scout_assignment_count(game, scout_id);
    if current_count >= max_slots {
        return Err(scouting_error_with_params(
            "be.error.scouting.scoutAssignmentFull",
            &[
                ("currentCount", current_count.to_string()),
                ("maxSlots", max_slots.to_string()),
            ],
        ));
    }

    // Check if player is already being scouted
    if game
        .scouting_assignments
        .iter()
        .any(|a| a.player_id == player_id)
    {
        return Err(ERR_PLAYER_ALREADY_SCOUTED.to_string());
    }

    // 2-5 days by scout quality, two more for a player the club only hears about: watching a
    // rival in the manager's own division is part of the staff's week, travelling to see anyone
    // else is not.
    let known = matches!(
        crate::player_knowledge::player_knowledge(game, player),
        crate::player_knowledge::PlayerKnowledge::Own
            | crate::player_knowledge::PlayerKnowledge::Known
    );
    let days = assignment_days_for_player_scouting(
        crate::staff_system::profile::effective_judging(game, scout).0,
    ) + if known { 0 } else { DAYS_TO_TRAVEL };

    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.scouting_assignments.push(ScoutingAssignment {
        // Made from what the assignment is, so a replay of the save starts the same one.
        id: crate::seed::derived_id(&["scouting", scout_id, player_id, &today]),
        scout_id: scout_id.to_string(),
        player_id: player_id.to_string(),
        days_remaining: days,
    });

    Ok(())
}

pub fn start_youth_scouting(
    game: &mut Game,
    scout_id: &str,
    region: YouthScoutingRegion,
    objective: YouthScoutingObjective,
    target_position: Option<Position>,
) -> Result<(), String> {
    let scout = resolve_user_scout(game, scout_id)?;
    let max_slots = scout_max_assignments(scout.attributes.judging_ability);
    let current_count = scout_assignment_count(game, scout_id);
    if current_count >= max_slots {
        return Err(scouting_error_with_params(
            "be.error.scouting.scoutAssignmentFull",
            &[
                ("currentCount", current_count.to_string()),
                ("maxSlots", max_slots.to_string()),
            ],
        ));
    }

    let target_position = target_position.map(|position| position.to_group_position());
    if game.youth_scouting_assignments.iter().any(|assignment| {
        assignment.region == region
            && assignment.objective == objective
            && assignment.target_position == target_position
    }) {
        return Err(ERR_YOUTH_SEARCH_ALREADY_ACTIVE.to_string());
    }

    let days = assignment_days_for_youth_scouting(
        crate::staff_system::profile::effective_judging(game, scout).1,
        region,
        objective,
    );
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let search = format!("{region:?}/{objective:?}/{target_position:?}");
    game.youth_scouting_assignments
        .push(YouthScoutingAssignment {
            id: crate::seed::derived_id(&["youth-scouting", scout_id, &search, &today]),
            scout_id: scout_id.to_string(),
            region,
            objective,
            target_position,
            days_remaining: days,
        });

    Ok(())
}

pub fn cancel_youth_scouting(game: &mut Game, assignment_id: &str) -> Result<(), String> {
    let original_len = game.youth_scouting_assignments.len();
    game.youth_scouting_assignments
        .retain(|assignment| assignment.id != assignment_id);

    if game.youth_scouting_assignments.len() == original_len {
        return Err(ERR_YOUTH_ASSIGNMENT_NOT_FOUND.to_string());
    }

    Ok(())
}

pub fn reassign_youth_scouting(
    game: &mut Game,
    assignment_id: &str,
    scout_id: &str,
) -> Result<(), String> {
    let assignment_index = game
        .youth_scouting_assignments
        .iter()
        .position(|assignment| assignment.id == assignment_id)
        .ok_or(ERR_YOUTH_ASSIGNMENT_NOT_FOUND)?;
    let current_scout_id = game.youth_scouting_assignments[assignment_index]
        .scout_id
        .clone();
    if current_scout_id == scout_id {
        return Err(ERR_SCOUT_ALREADY_ASSIGNED_TO_SEARCH.to_string());
    }

    let scout = resolve_user_scout(game, scout_id)?;
    let max_slots = scout_max_assignments(scout.attributes.judging_ability);
    let current_count = scout_assignment_count(game, scout_id);
    if current_count >= max_slots {
        return Err(scouting_error_with_params(
            "be.error.scouting.scoutAssignmentFull",
            &[
                ("currentCount", current_count.to_string()),
                ("maxSlots", max_slots.to_string()),
            ],
        ));
    }

    game.youth_scouting_assignments[assignment_index].scout_id = scout_id.to_string();
    Ok(())
}

pub fn cancel_scouting(game: &mut Game, assignment_id: &str) -> Result<(), String> {
    let before = game.scouting_assignments.len();
    game.scouting_assignments
        .retain(|assignment| assignment.id != assignment_id);
    if game.scouting_assignments.len() == before {
        return Err(ERR_ASSIGNMENT_NOT_FOUND.to_string());
    }
    Ok(())
}

/// Hands a running job to another scout. The days already spent stay spent: the job is the
/// same, only who is on it changes.
pub fn reassign_scouting(
    game: &mut Game,
    assignment_id: &str,
    scout_id: &str,
) -> Result<(), String> {
    let index = game
        .scouting_assignments
        .iter()
        .position(|assignment| assignment.id == assignment_id)
        .ok_or(ERR_ASSIGNMENT_NOT_FOUND)?;
    if game.scouting_assignments[index].scout_id == scout_id {
        return Err(ERR_SCOUT_ALREADY_ASSIGNED_TO_SEARCH.to_string());
    }
    let scout = resolve_user_scout(game, scout_id)?;
    let max_slots = scout_max_assignments(scout.attributes.judging_ability);
    let current_count = scout_assignment_count(game, scout_id);
    if current_count >= max_slots {
        return Err(scouting_error_with_params(
            "be.error.scouting.scoutAssignmentFull",
            &[
                ("currentCount", current_count.to_string()),
                ("maxSlots", max_slots.to_string()),
            ],
        ));
    }
    game.scouting_assignments[index].scout_id = scout_id.to_string();
    Ok(())
}

/// Process scouting assignments daily. Called from process_day().
/// Decrements days, delivers reports when complete.
pub fn process_scouting(game: &mut Game) {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let mut completed: Vec<ScoutingAssignment> = Vec::new();
    let mut completed_youth: Vec<YouthScoutingAssignment> = Vec::new();

    for assignment in game.scouting_assignments.iter_mut() {
        if assignment.days_remaining > 0 {
            assignment.days_remaining -= 1;
        }
        if assignment.days_remaining == 0 {
            completed.push(assignment.clone());
        }
    }

    for assignment in game.youth_scouting_assignments.iter_mut() {
        if assignment.days_remaining > 0 {
            assignment.days_remaining -= 1;
        }
        if assignment.days_remaining == 0 {
            completed_youth.push(assignment.clone());
        }
    }

    // Remove completed assignments
    game.scouting_assignments.retain(|a| a.days_remaining > 0);
    game.youth_scouting_assignments
        .retain(|assignment| assignment.days_remaining > 0);

    // Generate reports for completed assignments
    for assignment in &completed {
        deliver_player_report(game, assignment, &today);
    }

    crate::scouting_dossier::prune_shortlist(game);

    for assignment in &completed_youth {
        complete_youth_scouting_assignment(game, assignment, &today);
    }
}

/// Writes the dossier entry for a finished job and sends the scout's report to the inbox. The
/// inbox message and the dossier show the same estimates.
fn deliver_player_report(game: &mut Game, assignment: &ScoutingAssignment, today: &str) {
    let Some((report, read)) = crate::scouting_dossier::record_report(
        game,
        &assignment.id,
        &assignment.scout_id,
        &assignment.player_id,
    ) else {
        return;
    };
    let Some(player) = game.players.iter().find(|p| p.id == assignment.player_id) else {
        return;
    };
    let team_name = player
        .team_id
        .as_ref()
        .and_then(|tid| game.teams.iter().find(|t| &t.id == tid))
        .map(|t| t.name.clone());
    let msg = build_scout_report(
        &assignment.id,
        player,
        &report,
        &read,
        team_name.as_deref(),
        today,
    );
    game.messages.push(msg);
}

fn complete_youth_scouting_assignment(
    game: &mut Game,
    assignment: &YouthScoutingAssignment,
    date: &str,
) {
    let Some(scout) = game
        .staff
        .iter()
        .find(|staff_member| staff_member.id == assignment.scout_id)
        .cloned()
    else {
        return;
    };
    let Some(user_team_id) = game.manager.team_id.clone() else {
        return;
    };
    let Some(team) = game
        .teams
        .iter()
        .find(|candidate| candidate.id == user_team_id)
        .cloned()
    else {
        return;
    };

    // Prospects are scouted mid-career, so they are aged against the running
    // clock rather than the year the world opened in.
    let current_year = chrono::Datelike::year(&game.clock.current_date) as u32;
    let mut rng = game.rng_today(&format!("youth-scouting/{}", assignment.id));
    let prospects = generate_youth_recruitment_candidates(
        &team,
        assignment.region,
        assignment.objective,
        assignment.target_position.as_ref(),
        current_year,
        &mut rng,
    );
    if prospects.is_empty() {
        return;
    }

    let scout_name = format!("{} {}", scout.first_name, scout.last_name);
    game.messages.push(build_youth_recruitment_report(
        &assignment.id,
        &scout_name,
        &team.id,
        &team.name,
        &prospects,
        assignment.region,
        assignment.objective,
        assignment.target_position.as_ref(),
        date,
    ));
}

#[allow(clippy::too_many_arguments)]
fn build_youth_recruitment_report(
    assignment_id: &str,
    scout_name: &str,
    team_id: &str,
    team_name: &str,
    prospects: &[Player],
    region: YouthScoutingRegion,
    objective: YouthScoutingObjective,
    target_position: Option<&Position>,
    date: &str,
) -> InboxMessage {
    let target_position = target_position.map(|position| position.to_group_position());
    let message = InboxMessage::new(
        format!("youth-scout-{}", assignment_id),
        String::new(),
        String::new(),
        scout_name.to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::ScoutReport)
    .with_sender_role("");

    let message = prospects.iter().fold(message, |message, prospect| {
        message.with_action(MessageAction {
            id: format!("prospect:{}", prospect.id),
            label: prospect.full_name.clone(),
            action_type: ActionType::ChooseOption {
                options: youth_prospect_options(),
            },
            resolved: false,
            label_key: None,
        })
    });

    let message = message.with_context(MessageContext {
        team_id: Some(team_id.to_string()),
        youth_target_position: target_position
            .as_ref()
            .map(|position| format!("{:?}", position)),
        youth_search_region: Some(format!("{:?}", region)),
        youth_search_objective: Some(format!("{:?}", objective)),
        youth_prospects: Some(prospects.to_vec()),
        ..MessageContext::default()
    });

    let mut i18n_params = params(&[
        ("scout", scout_name),
        ("count", &prospects.len().to_string()),
        ("team", team_name),
        ("regionLabel", region_i18n_key(region)),
        ("objectiveLabel", objective_i18n_key(objective)),
    ]);
    let body_key = if let Some(target_position) = target_position.as_ref() {
        i18n_params.insert(
            "targetLabel".to_string(),
            youth_target_position_i18n_key(target_position).to_string(),
        );
        "be.msg.youthRecruitmentReport.bodyTargeted"
    } else {
        "be.msg.youthRecruitmentReport.bodyAny"
    };

    let mut message = message.with_i18n(
        "be.msg.youthRecruitmentReport.subject",
        body_key,
        i18n_params,
    );
    message.sender_role_key = Some("be.role.scout".to_string());
    message
}

fn youth_prospect_options() -> Vec<ActionOption> {
    vec![
        ActionOption {
            id: "sign".to_string(),
            label: String::new(),
            description: String::new(),
            label_key: Some("be.msg.youthRecruitment.option.sign.label".to_string()),
            description_key: Some("be.msg.youthRecruitment.option.sign.description".to_string()),
        },
        ActionOption {
            id: "shortlist".to_string(),
            label: String::new(),
            description: String::new(),
            label_key: Some("be.msg.youthRecruitment.option.shortlist.label".to_string()),
            description_key: Some(
                "be.msg.youthRecruitment.option.shortlist.description".to_string(),
            ),
        },
        ActionOption {
            id: "discard".to_string(),
            label: String::new(),
            description: String::new(),
            label_key: Some("be.msg.youthRecruitment.option.discard.label".to_string()),
            description_key: Some("be.msg.youthRecruitment.option.discard.description".to_string()),
        },
    ]
}

fn prospect_score(player: &Player, objective: YouthScoutingObjective) -> (u8, u8) {
    match objective {
        YouthScoutingObjective::Balanced => (
            player.ovr.saturating_add(player.potential / 2),
            player.potential,
        ),
        YouthScoutingObjective::HighPotential => (player.potential, player.ovr),
        YouthScoutingObjective::ReadySoon => (player.ovr, player.potential),
    }
}

fn generate_youth_recruitment_candidates(
    team: &domain::team::Team,
    region: YouthScoutingRegion,
    objective: YouthScoutingObjective,
    target_position: Option<&Position>,
    current_year: u32,
    rng: &mut impl rand::Rng,
) -> Vec<Player> {
    let pool_size = match objective {
        YouthScoutingObjective::Balanced => 4,
        YouthScoutingObjective::HighPotential => 6,
        YouthScoutingObjective::ReadySoon => 6,
    };
    let domestic_nationality = if team.football_nation.is_empty() {
        Some(team.country.as_str())
    } else {
        Some(team.football_nation.as_str())
    };

    let mut prospects: Vec<Player> = (0..pool_size)
        .map(|_| {
            let mut prospect = crate::generator::generate_youth_academy_recruit_with_nationality(
                team,
                target_position,
                match region {
                    YouthScoutingRegion::Domestic => domestic_nationality,
                    YouthScoutingRegion::International => None,
                },
                current_year,
                &mut *rng,
            );
            prospect.team_id = None;
            prospect.squad_role = SquadRole::Youth;
            prospect
        })
        .collect();

    prospects.sort_by(|left, right| {
        prospect_score(right, objective).cmp(&prospect_score(left, objective))
    });
    prospects.truncate(3);
    prospects
}

pub struct YouthRecruitmentEffect {
    pub message: String,
    pub i18n_key: String,
    pub i18n_params: HashMap<String, String>,
}

pub fn apply_youth_recruitment_response(
    game: &mut Game,
    message_id: &str,
    action_id: &str,
    option_id: &str,
) -> Option<YouthRecruitmentEffect> {
    let message_index = game
        .messages
        .iter()
        .position(|message| message.id == message_id)?;
    let action_index = game.messages[message_index]
        .actions
        .iter()
        .position(|action| action.id == action_id)?;
    let prospect_id = action_id.strip_prefix("prospect:")?;

    let prospects = game.messages[message_index]
        .context
        .youth_prospects
        .clone()?;
    let prospect_index = prospects
        .iter()
        .position(|prospect| prospect.id == prospect_id)?;
    let prospect = prospects[prospect_index].clone();

    match option_id {
        "discard" => {
            let mut remaining = prospects;
            remaining.remove(prospect_index);
            let message = &mut game.messages[message_index];
            message.context.youth_prospects = Some(remaining);
            message.actions.remove(action_index);

            Some(YouthRecruitmentEffect {
                message: String::new(),
                i18n_key: "be.msg.youthRecruitment.effect.discard".to_string(),
                i18n_params: params(&[("player", &prospect.full_name)]),
            })
        }
        "sign" => {
            // A prospect who was signed and has since retired is in the archive, not the roster:
            // replaying his report must not make him a live player again.
            let already_resolved = game.messages[message_index].actions[action_index].resolved;
            let known_person = game.players.iter().any(|player| player.id == prospect.id)
                || game
                    .retired_players
                    .iter()
                    .any(|retired| retired.id == prospect.id);
            if already_resolved || known_person {
                return None;
            }

            let mut signed_player = prospect;
            signed_player.team_id = game.manager.team_id.clone();
            signed_player.squad_role = SquadRole::Youth;
            if let Some(team_id) = signed_player.team_id.clone()
                && let Some(team) = game.teams.iter().find(|team| team.id == team_id)
            {
                signed_player.jersey_number =
                    crate::roster::resolve_jersey_for(game, &signed_player, team);
            }
            // Signing him is a contract made mid-career, so it goes in his history. He
            // signs on the terms he was generated with when those are still in the
            // future, and on the club's standard terms when they are not.
            let today = game.clock.current_date.date_naive();
            if let Some(team) = game
                .teams
                .iter()
                .find(|team| Some(&team.id) == game.manager.team_id.as_ref())
            {
                let own_terms = signed_player
                    .contract_end()
                    .and_then(crate::contracts::parse_contract_date)
                    .filter(|end| *end > today)
                    .map(|end| (signed_player.wage(), end))
                    .filter(|(wage, _)| *wage > 0);
                if let Some((wage, end)) = own_terms.or_else(|| {
                    crate::contracts::standard_contract_terms(&signed_player, team, today, 0)
                }) {
                    crate::contracts::record_movement(
                        &mut signed_player,
                        crate::contracts::contract_entry(
                            PlayerMovementKind::FreeAgentSigning,
                            today,
                            team,
                            crate::contracts::contract_record(
                                today,
                                end,
                                wage,
                                ContractSource::FreeAgent,
                            ),
                        ),
                    );
                }
            }
            let player_id = signed_player.id.clone();
            let player_name = signed_player.full_name.clone();
            let signed_jersey_number = signed_player.jersey_number;
            game.players.push(signed_player);

            let message = &mut game.messages[message_index];
            message.context.player_id = Some(player_id);
            if let Some(prospects) = message.context.youth_prospects.as_mut()
                && let Some(updated_prospect) = prospects
                    .iter_mut()
                    .find(|candidate| candidate.id == prospect_id)
            {
                updated_prospect.team_id = game.manager.team_id.clone();
                updated_prospect.squad_role = SquadRole::Youth;
                updated_prospect.jersey_number = signed_jersey_number;
            }
            if let Some(action) = message.actions.get_mut(action_index) {
                action.resolved = true;
            }

            Some(YouthRecruitmentEffect {
                message: String::new(),
                i18n_key: "be.msg.youthRecruitment.effect.sign".to_string(),
                i18n_params: params(&[("player", &player_name)]),
            })
        }
        "shortlist" => {
            let sender = game.messages[message_index].sender.clone();
            let date = game.messages[message_index].date.clone();
            let team_id = game.messages[message_index].context.team_id.clone();
            let youth_target_position = game.messages[message_index]
                .context
                .youth_target_position
                .clone();
            let youth_search_region = game.messages[message_index]
                .context
                .youth_search_region
                .clone();
            let youth_search_objective = game.messages[message_index]
                .context
                .youth_search_objective
                .clone();
            let prospect_name = prospect.full_name.clone();
            let shortlist_options = youth_prospect_options();

            game.messages.push(
                InboxMessage::new(
                    format!("youth-shortlist-{}", prospect.id),
                    String::new(),
                    String::new(),
                    sender,
                    date,
                )
                .with_category(MessageCategory::ScoutReport)
                .with_sender_role("")
                .with_action(MessageAction {
                    id: format!("prospect:{}", prospect.id),
                    label: prospect.full_name.clone(),
                    action_type: ActionType::ChooseOption {
                        options: vec![shortlist_options[0].clone(), shortlist_options[2].clone()],
                    },
                    resolved: false,
                    label_key: None,
                })
                .with_context(MessageContext {
                    team_id,
                    youth_target_position,
                    youth_search_region,
                    youth_search_objective,
                    youth_prospects: Some(vec![prospect]),
                    ..MessageContext::default()
                })
                .with_i18n(
                    "be.msg.youthRecruitmentShortlist.subject",
                    "be.msg.youthRecruitmentShortlist.body",
                    params(&[("player", &prospect_name)]),
                ),
            );

            if let Some(shortlist_message) = game.messages.last_mut() {
                shortlist_message.sender_role_key = Some("be.role.scout".to_string());
            }

            let message = &mut game.messages[message_index];
            if let Some(remaining) = message.context.youth_prospects.as_mut() {
                remaining.remove(prospect_index);
            }
            message.actions.remove(action_index);

            Some(YouthRecruitmentEffect {
                message: String::new(),
                i18n_key: "be.msg.youthRecruitment.effect.shortlist".to_string(),
                i18n_params: params(&[("player", &prospect_name)]),
            })
        }
        _ => None,
    }
}

fn region_i18n_key(region: YouthScoutingRegion) -> &'static str {
    match region {
        YouthScoutingRegion::Domestic => "scouting.regionDomestic",
        YouthScoutingRegion::International => "scouting.regionInternational",
    }
}

fn objective_i18n_key(objective: YouthScoutingObjective) -> &'static str {
    match objective {
        YouthScoutingObjective::Balanced => "scouting.objectiveBalanced",
        YouthScoutingObjective::HighPotential => "scouting.objectiveHighPotential",
        YouthScoutingObjective::ReadySoon => "scouting.objectiveReadySoon",
    }
}

fn youth_target_position_i18n_key(position: &Position) -> &'static str {
    match position {
        Position::Goalkeeper => "common.positions.Goalkeeper",
        Position::Defender => "common.positions.Defender",
        Position::Midfielder => "common.positions.Midfielder",
        Position::Forward => "common.positions.Forward",
        _ => "scouting.youthAnyPosition",
    }
}

/// The graded words the inbox report uses, from the dossier's estimates: the middle of the ability
/// range, then keys for rating, potential and confidence.
fn report_keys(
    report: &crate::scouting_dossier::ScoutReport,
) -> (u32, &'static str, &'static str, &'static str) {
    let rating_base = u32::from(report.ability.mid());
    let rating_key = if rating_base >= 80 {
        "common.scoutRatings.excellent"
    } else if rating_base >= 70 {
        "common.scoutRatings.veryGood"
    } else if rating_base >= 60 {
        "common.scoutRatings.good"
    } else if rating_base >= 50 {
        "common.scoutRatings.average"
    } else {
        "common.scoutRatings.belowAverage"
    };
    let potential_key = match report.potential.map(|range| range.mid()) {
        Some(85..) => "common.scoutPotential.worldClass",
        Some(70..=84) => "common.scoutPotential.strong",
        Some(_) => "common.scoutPotential.moderate",
        None => "common.scoutPotential.unclear",
    };
    let confidence_key = match report.confidence() {
        crate::scouting_dossier::Confidence::High => "common.scoutConfidence.high",
        crate::scouting_dossier::Confidence::Moderate => "common.scoutConfidence.moderate",
        crate::scouting_dossier::Confidence::Low => "common.scoutConfidence.low",
    };
    (rating_base, rating_key, potential_key, confidence_key)
}

/// The scout's report as the inbox shows it, from the estimates already written to the dossier
/// (`read` is the attributes as he read them).
fn build_scout_report(
    assignment_id: &str,
    player: &Player,
    report: &crate::scouting_dossier::ScoutReport,
    read: &domain::player::PlayerAttributes,
    team_name: Option<&str>,
    date: &str,
) -> InboxMessage {
    // Which of the six headline attributes the scout got to see is part of the assignment, so two
    // replays of the day agree.
    let mut rng = crate::seed::rng_from_key(&format!("scout-report/{assignment_id}/{date}"));
    let judging_ability = report.judging_ability;
    let scout_name = report.scout_name.as_str();

    let headline: [(u8, &str); 6] = [
        (read.pace, "Pace"),
        (read.shooting, "Shooting"),
        (read.passing, "Passing"),
        (read.dribbling, "Dribbling"),
        (read.defending, "Defending"),
        (read.strength, "Physical"),
    ];
    // Discovery mechanic: scout ability determines how many attrs are revealed
    // 80+: all 6 attrs + condition + morale
    // 60-79: 5 attrs + condition
    // 40-59: 3 attrs
    // <40: 2 attrs
    let reveal_count: usize = match judging_ability {
        80.. => 6,
        60..=79 => 5,
        40..=59 => 3,
        _ => 2,
    };
    let mut indices: Vec<usize> = (0..6).collect();
    for i in (1..indices.len()).rev() {
        let j = rng.random_range(0..=i);
        indices.swap(i, j);
    }
    let revealed: std::collections::HashSet<usize> =
        indices[..reveal_count].iter().cloned().collect();
    let to_opt = |idx: usize| -> Option<u8> { revealed.contains(&idx).then_some(headline[idx].0) };

    let reported_condition = (judging_ability >= 60).then_some(player.condition);
    let reported_morale = (judging_ability >= 80).then_some(player.morale);

    let (rating_base, rating_key, potential_key, confidence_key) = report_keys(report);

    let report_data = ScoutReportData {
        player_id: player.id.clone(),
        player_name: player.match_name.clone(),
        position: format!("{:?}", player.natural_position),
        nationality: player.nationality.clone(),
        dob: player.date_of_birth.clone(),
        team_name: team_name.map(|s| s.to_string()),
        pace: to_opt(0),
        shooting: to_opt(1),
        passing: to_opt(2),
        dribbling: to_opt(3),
        defending: to_opt(4),
        physical: to_opt(5),
        condition: reported_condition,
        morale: reported_morale,
        avg_rating: Some(rating_base),
        rating_key: rating_key.to_string(),
        potential_key: potential_key.to_string(),
        confidence_key: confidence_key.to_string(),
    };

    let msg_id = format!("scout_report_{}", assignment_id);
    InboxMessage::new(
        msg_id,
        String::new(),
        String::new(),
        scout_name.to_string(),
        date.to_string(),
    )
    .with_category(MessageCategory::ScoutReport)
    .with_priority(MessagePriority::Normal)
    .with_sender_role("Scout")
    .with_action(MessageAction {
        id: "ack".to_string(),
        label: "Noted".to_string(),
        action_type: ActionType::Acknowledge,
        resolved: false,
        label_key: Some("be.msg.event.ack".to_string()),
    })
    .with_context(MessageContext {
        player_id: Some(player.id.clone()),
        scout_report: Some(report_data),
        ..Default::default()
    })
    .with_i18n("be.msg.scoutReport.subject", "be.msg.scoutReport.body", {
        let mut p = params(&[("player", &player.match_name), ("scout", scout_name)]);
        p.insert("ratingDesc".to_string(), rating_key.to_string());
        p.insert("potentialDesc".to_string(), potential_key.to_string());
        p.insert("confidence".to_string(), confidence_key.to_string());
        p
    })
    .with_sender_i18n("be.sender.scout", "be.role.scout")
}

#[cfg(test)]
mod tests {
    use super::build_scout_report;
    use crate::scouting_dossier::{Range, ScoutReport};
    use domain::message::{ActionType, MessageCategory, MessagePriority};
    use domain::player::{Player, PlayerAttributes, Position};

    fn sample_attrs() -> PlayerAttributes {
        PlayerAttributes {
            pace: 70,
            stamina: 68,
            strength: 66,
            agility: 69,
            passing: 72,
            shooting: 64,
            tackling: 58,
            dribbling: 71,
            defending: 57,
            positioning: 65,
            vision: 70,
            decisions: 67,
            composure: 68,
            aggression: 52,
            teamwork: 73,
            leadership: 48,
            handling: 18,
            reflexes: 20,
            aerial: 55,
        }
    }

    fn sample_player() -> Player {
        let mut player = Player::new(
            "player-1".to_string(),
            "Jamie Prospect".to_string(),
            "Jamie Prospect".to_string(),
            "2004-03-12".to_string(),
            "ENG".to_string(),
            Position::Midfielder,
            sample_attrs(),
        );
        player.condition = 89;
        player.morale = 74;
        player
    }

    fn sample_report(judging: u8, potential: Option<Range>) -> ScoutReport {
        ScoutReport {
            player_id: "player-1".into(),
            scout_id: "scout-1".into(),
            scout_name: "Alex Scout".into(),
            scouted_on: "2026-08-01".into(),
            judging_ability: judging,
            judging_potential: judging,
            reports: 1,
            ability: Range { low: 64, high: 70 },
            potential,
            age: 22,
            strengths: vec![],
            weaknesses: vec![],
        }
    }

    fn message_json(assignment: &str, judging: u8) -> String {
        let message = build_scout_report(
            assignment,
            &sample_player(),
            &sample_report(judging, Some(Range { low: 74, high: 84 })),
            &sample_attrs(),
            Some("London FC"),
            "2026-08-01",
        );
        serde_json::to_value(message).unwrap().to_string()
    }

    /// Given a scout assigned to a player,
    /// When the report is written twice, for each of thirty assignments,
    /// Then it reads the same both times — a replayed day must not see different numbers — and
    ///      which attributes the scout got to see is not the same for every assignment.
    #[test]
    fn a_scout_report_reads_the_same_each_time() {
        let ids: Vec<String> = (0..30).map(|n| format!("assignment-{n}")).collect();
        for id in &ids {
            assert_eq!(message_json(id, 50), message_json(id, 50), "{id}");
        }
        let distinct: std::collections::BTreeSet<String> = ids
            .iter()
            .map(|id| message_json(id, 50).replace(id.as_str(), ""))
            .collect();
        assert!(distinct.len() > 1, "the revealed attributes never varied");
    }

    /// Given reports from a weak scout and a strong one
    /// When the number of attributes each shows is counted
    /// Then the strong scout shows all six and the weak one only two.
    #[test]
    fn a_better_scout_shows_more_of_the_player() {
        let count = |judging: u8| {
            let message = build_scout_report(
                "a",
                &sample_player(),
                &sample_report(judging, None),
                &sample_attrs(),
                None,
                "2026-08-01",
            );
            let data = message.context.scout_report.unwrap();
            [
                data.pace,
                data.shooting,
                data.passing,
                data.dribbling,
                data.defending,
                data.physical,
            ]
            .iter()
            .flatten()
            .count()
        };
        assert_eq!(count(85), 6);
        assert_eq!(count(30), 2);
    }

    /// Given a report whose potential the scout could not judge
    /// When the message is built
    /// Then it says the potential is unclear rather than inventing a grade.
    #[test]
    fn a_scout_who_cannot_judge_potential_says_so() {
        let message = build_scout_report(
            "a",
            &sample_player(),
            &sample_report(50, None),
            &sample_attrs(),
            None,
            "2026-08-01",
        );
        assert_eq!(
            message.i18n_params.get("potentialDesc").map(String::as_str),
            Some("common.scoutPotential.unclear")
        );
    }

    #[test]
    fn build_scout_report_uses_i18n_keys_without_raw_fallbacks() {
        let message = build_scout_report(
            "assignment-1",
            &sample_player(),
            &sample_report(85, Some(Range { low: 74, high: 84 })),
            &sample_attrs(),
            Some("London FC"),
            "2026-08-01",
        );
        assert_eq!(message.subject, "");
        assert_eq!(message.body, "");
        assert_eq!(message.sender, "Alex Scout");
        assert_eq!(message.sender_role, "Scout");
        assert_eq!(message.category, MessageCategory::ScoutReport);
        assert_eq!(message.priority, MessagePriority::Normal);
        assert_eq!(
            message.subject_key.as_deref(),
            Some("be.msg.scoutReport.subject")
        );
        assert_eq!(message.body_key.as_deref(), Some("be.msg.scoutReport.body"));
        assert_eq!(message.sender_key.as_deref(), Some("be.sender.scout"));
        assert_eq!(message.sender_role_key.as_deref(), Some("be.role.scout"));
        assert_eq!(
            message.i18n_params.get("player"),
            Some(&"Jamie Prospect".to_string())
        );
        assert_eq!(
            message.i18n_params.get("scout"),
            Some(&"Alex Scout".to_string())
        );
        assert!(
            matches!(message.actions.as_slice(), [action] if matches!(action.action_type, ActionType::Acknowledge))
        );
        let report = message
            .context
            .scout_report
            .expect("scout report context should be attached");
        assert_eq!(report.player_id, "player-1");
        assert_eq!(report.player_name, "Jamie Prospect");
        assert_eq!(report.team_name.as_deref(), Some("London FC"));
    }
}
