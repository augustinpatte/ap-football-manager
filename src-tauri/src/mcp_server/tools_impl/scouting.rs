//! MCP tool implementations: scouting

use mcp_results::scouting::{
    AcademyReviewResult, AcademyRowResult, DossierRow, ScoutDispatched, ScoutDossier, ScoutReport,
    ScoutReports, ScoutingAssignment, ScoutingCancelled, ShortlistChanged, YouthScoutingCancelled,
    YouthScoutingReassigned, YouthScoutingStarted,
};
use ofm_core::rating_view::RatingView;
use ofm_core::scouting_view::ScoutingTargetView;

use crate::mcp_server::context::McpContext;
use crate::mcp_server::tools_impl::helpers::require_game;
use std::sync::Arc;

// ─── scout_send ─────────────────────────────────────────────────────────────

pub fn scout_send(
    ctx: Arc<McpContext>,
    scout_id: String,
    player_id: String,
) -> Result<ScoutDispatched, String> {
    // `send_scout` validates fully before its single push, so an error path
    // leaves the game untouched even though `update_game` cannot roll back.
    ctx.state_manager
        .update_game(|game| ofm_core::scouting::send_scout(game, &scout_id, &player_id))
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())??;

    let scout_name = ctx
        .state_manager
        .get_game(|g| {
            g.staff
                .iter()
                .find(|s| s.id == scout_id)
                .map(|s| format!("{} {}", s.first_name, s.last_name))
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let player_name = ctx
        .state_manager
        .get_game(|g| {
            g.players
                .iter()
                .find(|p| p.id == player_id)
                .map(|p| p.match_name.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    {
        use tauri::Emitter;
        let _ = ctx.app_handle.emit("game-state-changed", ());
    }

    Ok(ScoutDispatched {
        scout_id,
        scout_name,
        player_id,
        player_name,
    })
}

// ─── scout_get_reports ─────────────────────────────────────────────────────

// ─── scout_get_reports ─────────────────────────────────────────────────────

pub fn scout_get_reports(ctx: Arc<McpContext>) -> Result<ScoutReports, String> {
    let game = require_game(&ctx.state_manager)?;

    let reports = game
        .messages
        .iter()
        .filter(|m| matches!(m.category, domain::message::MessageCategory::ScoutReport))
        .filter_map(|m| {
            m.context.scout_report.as_ref().map(|r| ScoutReport {
                message_id: m.id.clone(),
                player_name: r.player_name.clone(),
                position: r.position.clone(),
                avg_rating: r.avg_rating,
                team: r.team_name.clone(),
                read: m.read,
            })
        })
        .collect();

    let active_assignments = game
        .scouting_assignments
        .iter()
        .map(|a| ScoutingAssignment {
            id: a.id.clone(),
            scout_name: game
                .staff
                .iter()
                .find(|s| s.id == a.scout_id)
                .map(|s| format!("{} {}", s.first_name, s.last_name))
                .unwrap_or_default(),
            player_name: game
                .players
                .iter()
                .find(|p| p.id == a.player_id)
                .map(|p| p.match_name.clone())
                .unwrap_or_default(),
            days_remaining: a.days_remaining,
        })
        .collect();

    Ok(ScoutReports {
        reports,
        active_assignments,
    })
}

// ─── scout_youth_start ──────────────────────────────────────────────────────

// ─── scout_youth_start ──────────────────────────────────────────────────────

pub fn scout_youth_start(
    ctx: Arc<McpContext>,
    scout_id: String,
    region: Option<String>,
    objective: Option<String>,
    target_position: Option<String>,
) -> Result<YouthScoutingStarted, String> {
    let region = parse_youth_region(region.as_deref())?;
    let objective = parse_youth_objective(objective.as_deref())?;
    let target_position = parse_youth_target_position(target_position.as_deref())?;

    // Validates fully before its single push, so the error path leaves the game
    // untouched even though `update_game` cannot roll back.
    ctx.state_manager
        .update_game(|game| {
            ofm_core::scouting::start_youth_scouting(
                game,
                &scout_id,
                region,
                objective,
                target_position,
            )
        })
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())??;

    {
        use tauri::Emitter;
        let _ = ctx.app_handle.emit("game-state-changed", ());
    }

    Ok(YouthScoutingStarted { scout_id })
}

fn parse_youth_region(region: Option<&str>) -> Result<ofm_core::game::YouthScoutingRegion, String> {
    match region.unwrap_or("Domestic") {
        "Domestic" => Ok(ofm_core::game::YouthScoutingRegion::Domestic),
        "International" => Ok(ofm_core::game::YouthScoutingRegion::International),
        other => Err(format!("Unknown youth scouting region: {}", other)),
    }
}

fn parse_youth_objective(
    objective: Option<&str>,
) -> Result<ofm_core::game::YouthScoutingObjective, String> {
    match objective.unwrap_or("Balanced") {
        "Balanced" => Ok(ofm_core::game::YouthScoutingObjective::Balanced),
        "HighPotential" | "Potential" => Ok(ofm_core::game::YouthScoutingObjective::HighPotential),
        "ReadySoon" | "Immediate" => Ok(ofm_core::game::YouthScoutingObjective::ReadySoon),
        other => Err(format!("Unknown youth scouting objective: {}", other)),
    }
}

fn parse_youth_target_position(
    pos: Option<&str>,
) -> Result<Option<domain::player::Position>, String> {
    let Some(pos_str) = pos else { return Ok(None) };
    match pos_str.to_uppercase().as_str() {
        // Goalkeeper
        "GK" | "GOALKEEPER" => Ok(Some(domain::player::Position::Goalkeeper)),
        // Defender — broad + specific codes
        "DF" | "DEFENDER" | "CB" | "LCB" | "RCB" | "LB" | "RB" | "LWB" | "RWB" => {
            Ok(Some(domain::player::Position::Defender))
        }
        // Midfielder — broad + specific codes
        "MF" | "MIDFIELDER" | "DM" | "CDM" | "CM" | "LCM" | "RCM" | "AM" | "CAM" | "LM" | "RM" => {
            Ok(Some(domain::player::Position::Midfielder))
        }
        // Forward — broad + specific codes
        "FW" | "FORWARD" | "ST" | "CF" | "LS" | "RS" | "LW" | "RW" | "LF" | "RF" => {
            Ok(Some(domain::player::Position::Forward))
        }
        _ => Err(format!("Unknown position: {}", pos_str)),
    }
}

// ─── scout_youth_cancel ─────────────────────────────────────────────────────

// ─── scout_youth_cancel ─────────────────────────────────────────────────────

pub fn scout_youth_cancel(
    ctx: Arc<McpContext>,
    assignment_id: String,
) -> Result<YouthScoutingCancelled, String> {
    // The odd one out: `cancel_youth_scouting` retains first and reports the
    // failure afterwards. That is still safe to run in place — when it errors,
    // the retain matched nothing and removed nothing.
    ctx.state_manager
        .update_game(|game| ofm_core::scouting::cancel_youth_scouting(game, &assignment_id))
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())??;

    {
        use tauri::Emitter;
        let _ = ctx.app_handle.emit("game-state-changed", ());
    }

    Ok(YouthScoutingCancelled { assignment_id })
}

// ─── scout_youth_reassign ───────────────────────────────────────────────────

// ─── scout_youth_reassign ───────────────────────────────────────────────────

pub fn scout_youth_reassign(
    ctx: Arc<McpContext>,
    assignment_id: String,
    scout_id: String,
) -> Result<YouthScoutingReassigned, String> {
    // Validates fully before the one field it assigns.
    ctx.state_manager
        .update_game(|game| {
            ofm_core::scouting::reassign_youth_scouting(game, &assignment_id, &scout_id)
        })
        .ok_or_else(|| "be.error.noActiveGameSession".to_string())??;

    {
        use tauri::Emitter;
        let _ = ctx.app_handle.emit("game-state-changed", ());
    }

    Ok(YouthScoutingReassigned {
        assignment_id,
        scout_id,
    })
}

// ─── scout_get_dossier ──────────────────────────────────────────────────────

/// The rating as the agent may read it: what the manager's own screen shows, in words.
fn rating_text(rating: &RatingView) -> String {
    match rating {
        RatingView::Exact { ovr } => ovr.to_string(),
        // A band one point wide is one figure, as the screen's `formatRange` shows it.
        RatingView::Estimate { low, high, .. } if low == high => low.to_string(),
        RatingView::Estimate { low, high, .. } => format!("{low}-{high}"),
        RatingView::Standing { standing } => format!("{standing:?}").to_lowercase(),
    }
}

/// How far a reading can be trusted, in the words the screen uses (`low`, `moderate`, `high`).
fn confidence_text(confidence: ofm_core::scouting_dossier::Confidence) -> String {
    format!("{confidence:?}").to_lowercase()
}

fn dossier_row(view: ScoutingTargetView) -> DossierRow {
    let range = |r: ofm_core::scouting_dossier::Range| {
        if r.low == r.high {
            r.low.to_string()
        } else {
            format!("{}-{}", r.low, r.high)
        }
    };
    let report = view.report;
    DossierRow {
        player_id: view.player_id,
        player_name: view.full_name,
        position: format!("{:?}", view.natural_position),
        age: view.age,
        team: view.team_name,
        rating: rating_text(&view.rating),
        ability_range: report.as_ref().map(|r| range(r.ability)),
        potential_range: report.as_ref().and_then(|r| r.potential).map(range),
        confidence: report.as_ref().map(|r| confidence_text(r.confidence)),
        outlook: format!("{:?}", view.outlook),
        squad_impact: view.squad_impact.map(|i| format!("{i:?}")),
        fee_affordable: view.fee_affordable,
        wage_permitted: view.wage_permitted,
        risks: view.risks,
    }
}

pub fn scout_get_dossier(ctx: Arc<McpContext>) -> Result<ScoutDossier, String> {
    let view = crate::commands::scouting::get_scouting_dossier_internal(&ctx.state_manager)?;
    Ok(ScoutDossier {
        shortlist: view.shortlist.into_iter().map(dossier_row).collect(),
        reports: view.reports.into_iter().map(dossier_row).collect(),
    })
}

// ─── scout_shortlist_add / scout_shortlist_remove ───────────────────────────

fn shortlist_change(
    ctx: &Arc<McpContext>,
    player_id: &str,
    add: bool,
) -> Result<ShortlistChanged, String> {
    if add {
        crate::commands::scouting::shortlist_player_internal(&ctx.state_manager, player_id)?;
    } else {
        crate::commands::scouting::unshortlist_player_internal(&ctx.state_manager, player_id)?;
    }
    {
        use tauri::Emitter;
        let _ = ctx.app_handle.emit("game-state-changed", ());
    }
    let player_name = ctx
        .state_manager
        .get_game(|g| {
            g.players
                .iter()
                .find(|p| p.id == player_id)
                .map(|p| p.full_name.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    Ok(ShortlistChanged {
        player_name,
        shortlisted: add,
    })
}

pub fn scout_shortlist_add(
    ctx: Arc<McpContext>,
    player_id: String,
) -> Result<ShortlistChanged, String> {
    shortlist_change(&ctx, &player_id, true)
}

pub fn scout_shortlist_remove(
    ctx: Arc<McpContext>,
    player_id: String,
) -> Result<ShortlistChanged, String> {
    shortlist_change(&ctx, &player_id, false)
}

// ─── scout_cancel ───────────────────────────────────────────────────────────

pub fn scout_cancel(
    ctx: Arc<McpContext>,
    assignment_id: String,
) -> Result<ScoutingCancelled, String> {
    crate::commands::scouting::cancel_scouting_internal(&ctx.state_manager, &assignment_id)?;
    {
        use tauri::Emitter;
        let _ = ctx.app_handle.emit("game-state-changed", ());
    }
    Ok(ScoutingCancelled { assignment_id })
}

// ─── academy_get_review ─────────────────────────────────────────────────────

pub fn academy_get_review(ctx: Arc<McpContext>) -> Result<AcademyReviewResult, String> {
    let review = crate::commands::scouting::get_academy_review_internal(&ctx.state_manager)?
        .ok_or_else(|| "be.error.noTeamAssigned".to_string())?;
    Ok(academy_review_result(review))
}

/// The agent's view of the academy review: the screen's own fields, with the ceiling as the staff's
/// assessed range (never the exact potential) and its confidence.
fn academy_review_result(review: ofm_core::academy_review::AcademyReview) -> AcademyReviewResult {
    AcademyReviewResult {
        seniors: review.seniors,
        senior_limit: review.senior_limit,
        next_intake: review.next_intake,
        rows: review
            .rows
            .into_iter()
            .map(|row| AcademyRowResult {
                player_id: row.player_id,
                player_name: row.full_name,
                age: row.age,
                position: format!("{:?}", row.natural_position),
                rating: row.ovr,
                ceiling: rating_text(&row.ceiling),
                ceiling_confidence: match &row.ceiling {
                    RatingView::Estimate { confidence, .. } => confidence_text(*confidence),
                    RatingView::Exact { .. } | RatingView::Standing { .. } => String::new(),
                },
                verdict: format!("{:?}", row.verdict),
                reasons: row.reasons,
                promotion_blocked: row.promotion_blocked.is_some(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ofm_core::academy_advice::{ActionAdvice, AdviceState, RowAdvice};
    use ofm_core::academy_review::{AcademyReview, AcademyRow, Verdict};
    use ofm_core::scouting_dossier::Confidence;

    fn advice() -> ActionAdvice {
        ActionAdvice {
            state: AdviceState::Available,
            reasons: vec![],
        }
    }

    fn row(id: &str, ceiling: RatingView) -> AcademyRow {
        AcademyRow {
            player_id: id.to_string(),
            full_name: id.to_string(),
            age: 18,
            natural_position: domain::player::Position::Midfielder,
            ovr: 58,
            ceiling,
            verdict: Verdict::KeepDeveloping,
            reasons: vec!["stillImproving".to_string()],
            squad_cover: 60,
            seasons_left_in_academy: 4,
            injured: false,
            on_loan: false,
            loan_listed: false,
            contract_end: None,
            weekly_wage: 0,
            training_focus: None,
            promotion_blocked: None,
            advice: RowAdvice {
                promote: advice(),
                loan: advice(),
                keep: advice(),
                renew: None,
            },
        }
    }

    /// Given an academy review with a band, a one-point band and a (never expected) exact ceiling
    /// When the agent's result is built
    /// Then the band reads as a range with its confidence, a one-point band as one figure, and no
    ///      exact potential field exists on the wire.
    #[test]
    fn the_agents_academy_review_carries_a_band_and_never_a_potential() {
        let estimate = |low, high, confidence| RatingView::Estimate {
            low,
            high,
            confidence,
            current: true,
            scouted_on: "2026-09-15".to_string(),
        };
        let review = AcademyReview {
            rows: vec![
                row("wide", estimate(73, 84, Confidence::Moderate)),
                row("point", estimate(80, 80, Confidence::High)),
            ],
            summary: None,
            seniors: 24,
            senior_limit: 30,
            by_group: [0; 4],
            target_by_group: [1, 2, 2, 1],
            next_intake: 2,
        };

        let result = academy_review_result(review);

        assert_eq!(
            (
                result.rows[0].ceiling.as_str(),
                result.rows[0].ceiling_confidence.as_str()
            ),
            ("73-84", "moderate")
        );
        assert_eq!(
            (
                result.rows[1].ceiling.as_str(),
                result.rows[1].ceiling_confidence.as_str()
            ),
            ("80", "high")
        );
        let wire = serde_json::to_string(&result).expect("serializes");
        assert!(
            !wire.contains("potential") && !wire.contains("next_year"),
            "{wire}"
        );
    }
}
