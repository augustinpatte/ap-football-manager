//! What the scouting screens read: a player as the manager's club knows him.
//!
//! One projection for the dossier, the shortlist and the comparison, built from the club's reports
//! and from facts that are public in the game (age, position, rating, value, contract). It never
//! reads a player's hidden potential, and for a player nobody has reported on it says so: the
//! estimate fields are `None`, not a guess.
//!
//! The squad impact is the I4A squad model's own answer ([`ClubSquad::signing`]) asked about the
//! player *as the club believes him to be* (the middle of the scout's range, or the public rating
//! when nobody has looked), so a report moves the verdict the same way it would move the
//! assistant's.

use crate::game::Game;
use crate::scouting_dossier::{Confidence, Range, ScoutReport};
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::transfers::can_afford_fee;
use domain::player::{Player, Position};
use domain::team::Team;
use serde::Serialize;

/// Reports shown on the dossier screen, newest first. The dossier keeps more.
const REPORTS_SHOWN: usize = 60;

/// How far ahead a contract's end counts as expiring, in days.
const EXPIRING_WITHIN_DAYS: i64 = 365;

/// Age from which a player's decline is a risk the report names.
const DECLINE_RISK_AGE: i32 = 31;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    /// Nobody from the club has looked at him.
    Unscouted,
    /// A report from the current season.
    Current,
    /// A report from before this season started.
    Stale,
}

/// What the player would do for the manager's side, by the squad model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SquadImpact {
    Starter,
    Cover,
    NoGain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outlook {
    /// Young with a clear gap between today and the ceiling.
    HighGrowth,
    Growing,
    NearPeak,
    Declining,
    /// The scout could not judge potential, or nobody has looked.
    Unclear,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportView {
    pub scouted_on: String,
    pub scout_name: String,
    /// Reports narrowed together: a higher number is a firmer estimate.
    pub reports: u32,
    pub ability: Range,
    pub potential: Option<Range>,
    pub confidence: Confidence,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoutingTargetView {
    pub player_id: String,
    pub full_name: String,
    pub age: i32,
    pub natural_position: Position,
    pub alternate_positions: Vec<Position>,
    pub nationality: String,
    pub team_id: Option<String>,
    pub team_name: Option<String>,
    /// The rating everyone can see.
    pub ovr: u8,
    pub market_value: u64,
    pub weekly_wage: u32,
    pub contract_end: Option<String>,
    pub is_free_agent: bool,
    pub transfer_listed: bool,
    pub loan_listed: bool,
    pub injured: bool,
    pub status: ReportStatus,
    pub shortlisted: bool,
    pub being_scouted: bool,
    pub report: Option<ReportView>,
    pub outlook: Outlook,
    pub squad_impact: Option<SquadImpact>,
    /// `None` for a free agent: there is no fee.
    pub fee_affordable: Option<bool>,
    pub wage_permitted: bool,
    /// Translation-key suffixes under `scouting.risk.`.
    pub risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DossierView {
    pub reports: Vec<ScoutingTargetView>,
    pub shortlist: Vec<ScoutingTargetView>,
}

/// What a view needs besides the player, found once per request.
struct Lens<'a> {
    game: &'a Game,
    today: chrono::NaiveDate,
    team: Option<&'a Team>,
    squad: Option<ClubSquad>,
}

impl<'a> Lens<'a> {
    fn of(game: &'a Game) -> Self {
        let team = game
            .manager
            .team_id
            .as_deref()
            .and_then(|id| game.teams.iter().find(|t| t.id == id));
        Self {
            game,
            today: game.clock.current_date.date_naive(),
            squad: team.and_then(|t| ClubSquad::of(game, &t.id, Appetite::NEUTRAL)),
            team,
        }
    }
}

/// The dossier screen: the latest reports and the shortlist.
pub fn dossier_view(game: &Game) -> DossierView {
    let lens = Lens::of(game);
    let mut reports: Vec<&ScoutReport> = game.scouting_dossier.reports.iter().collect();
    reports.sort_by(|a, b| {
        b.scouted_on
            .cmp(&a.scouted_on)
            .then(a.player_id.cmp(&b.player_id))
    });
    let reports = reports
        .into_iter()
        .take(REPORTS_SHOWN)
        .filter_map(|report| view_of_id(&lens, &report.player_id))
        .collect();
    let shortlist = game
        .scouting_dossier
        .shortlist
        .iter()
        .filter_map(|entry| view_of_id(&lens, &entry.player_id))
        .collect();
    DossierView { reports, shortlist }
}

/// The views of the given players, in the order asked; an id no player has is left out.
pub fn targets_view(game: &Game, player_ids: &[String]) -> Vec<ScoutingTargetView> {
    let lens = Lens::of(game);
    player_ids
        .iter()
        .filter_map(|id| view_of_id(&lens, id))
        .collect()
}

fn view_of_id(lens: &Lens, player_id: &str) -> Option<ScoutingTargetView> {
    let player = lens.game.players.iter().find(|p| p.id == player_id)?;
    Some(view_of(lens, player))
}

fn view_of(lens: &Lens, player: &Player) -> ScoutingTargetView {
    let game = lens.game;
    let report = game.scouting_dossier.report_on(&player.id);
    let status = match report {
        None => ReportStatus::Unscouted,
        Some(report)
            if game
                .season_context
                .season_start
                .as_deref()
                .is_some_and(|start| report.scouted_on.as_str() < start) =>
        {
            ReportStatus::Stale
        }
        Some(_) => ReportStatus::Current,
    };
    let age = domain::age::age_on_or_unknown(lens.today, &player.date_of_birth);
    let is_free_agent = player.team_id.is_none();
    let wage = crate::contracts::reference_player_wage(player);
    let fee_affordable = match (lens.team, is_free_agent) {
        (Some(team), false) => Some(can_afford_fee(team, player.market_value)),
        _ => None,
    };
    let wage_permitted = lens.team.is_none_or(|team| {
        crate::contract_wage_policy::purchase_wage_policy_verdict(game, team, player, wage)
            .permits()
    });

    let mut risks = Vec::new();
    if crate::contracts::contract_days_remaining(player.contract_end(), lens.today)
        .is_some_and(|days| days <= EXPIRING_WITHIN_DAYS)
    {
        risks.push("contractExpiring");
    }
    if player.injury.is_some() {
        risks.push("injured");
    }
    if age >= DECLINE_RISK_AGE {
        risks.push("ageing");
    }
    match (report, status) {
        (None, _) => risks.push("unscouted"),
        (Some(_), ReportStatus::Stale) => risks.push("staleReport"),
        (Some(r), ReportStatus::Current) if r.confidence() == Confidence::Low => {
            risks.push("uncertainEstimate");
        }
        _ => {}
    }
    if fee_affordable == Some(false) {
        risks.push("feeOverBudget");
    }
    if !wage_permitted {
        risks.push("wageOverPolicy");
    }

    ScoutingTargetView {
        player_id: player.id.clone(),
        full_name: player.full_name.clone(),
        age,
        natural_position: player.natural_position.clone(),
        alternate_positions: player.alternate_positions.clone(),
        nationality: player.nationality.clone(),
        team_id: player.team_id.clone(),
        team_name: player.team_id.as_deref().map(|id| game.team_name_or_id(id)),
        ovr: player.ovr,
        market_value: player.market_value,
        weekly_wage: wage,
        contract_end: player.contract_end().map(str::to_string),
        is_free_agent,
        transfer_listed: player.transfer_listed,
        loan_listed: player.loan_listed,
        injured: player.injury.is_some(),
        status,
        shortlisted: game.scouting_dossier.is_shortlisted(&player.id),
        being_scouted: game
            .scouting_assignments
            .iter()
            .any(|a| a.player_id == player.id),
        outlook: outlook_of(report, age),
        squad_impact: squad_impact(lens, player, report),
        report: report.map(|r| ReportView {
            scouted_on: r.scouted_on.clone(),
            scout_name: r.scout_name.clone(),
            reports: r.reports,
            ability: r.ability,
            potential: r.potential,
            confidence: r.confidence(),
            strengths: r.strengths.clone(),
            weaknesses: r.weaknesses.clone(),
        }),
        fee_affordable,
        wage_permitted,
        risks: risks.into_iter().map(str::to_string).collect(),
    }
}

/// The development outlook the report supports: the gap between the middle of the potential and
/// of the ability range, read against his age. A player nobody has judged is `Unclear`, however
/// young.
fn outlook_of(report: Option<&ScoutReport>, age: i32) -> Outlook {
    let Some(potential) = report.and_then(|r| r.potential) else {
        return if age >= DECLINE_RISK_AGE + 1 {
            Outlook::Declining
        } else {
            Outlook::Unclear
        };
    };
    let gap = i32::from(potential.mid()) - i32::from(report.map_or(0, |r| r.ability.mid()));
    match age {
        a if a >= DECLINE_RISK_AGE + 1 => Outlook::Declining,
        a if a <= 21 && gap >= 10 => Outlook::HighGrowth,
        a if a <= 25 && gap >= 5 => Outlook::Growing,
        _ => Outlook::NearPeak,
    }
}

/// The squad model's verdict on the player as the club believes him to be.
fn squad_impact(lens: &Lens, player: &Player, report: Option<&ScoutReport>) -> Option<SquadImpact> {
    let squad = lens.squad.as_ref()?;
    let mut believed = player.clone();
    believed.squad_role = domain::player::SquadRole::Senior;
    if let Some(report) = report {
        believed.ovr = report.ability.mid();
        believed.potential = report
            .potential
            .map_or(believed.ovr, |p| p.mid().max(believed.ovr));
    } else {
        // Nobody has judged his potential: the club counts what it can see.
        believed.potential = believed.ovr;
    }
    let marginal = squad.signing(&believed, 0.0)?;
    Some(if marginal.starts_at.is_some() && marginal.now_gain > 0.0 {
        SquadImpact::Starter
    } else if marginal.gain >= crate::squad_intelligence::impact::MIN_GAIN {
        SquadImpact::Cover
    } else {
        SquadImpact::NoGain
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(ability: Range, potential: Option<Range>) -> ScoutReport {
        ScoutReport {
            player_id: "p".into(),
            scout_id: "s".into(),
            scout_name: "S".into(),
            scouted_on: "2026-08-01".into(),
            judging_ability: 70,
            judging_potential: 70,
            reports: 1,
            ability,
            potential,
            age: 19,
            strengths: vec![],
            weaknesses: vec![],
        }
    }

    /// Given reports on a teenager with a high ceiling, a mid-twenties player near it, a veteran,
    /// and a player whose potential the scout could not judge
    /// When the outlooks are read
    /// Then they are high growth, near peak, declining and unclear, and never claim growth the
    ///      report does not support.
    #[test]
    fn the_outlook_follows_the_report_and_the_age() {
        let high = report(
            Range { low: 54, high: 60 },
            Some(Range { low: 72, high: 82 }),
        );
        assert_eq!(outlook_of(Some(&high), 18), Outlook::HighGrowth);
        assert_eq!(outlook_of(Some(&high), 24), Outlook::Growing);
        let capped = report(
            Range { low: 68, high: 74 },
            Some(Range { low: 70, high: 76 }),
        );
        assert_eq!(outlook_of(Some(&capped), 26), Outlook::NearPeak);
        assert_eq!(outlook_of(Some(&capped), 33), Outlook::Declining);
        let blind = report(Range { low: 54, high: 60 }, None);
        assert_eq!(outlook_of(Some(&blind), 18), Outlook::Unclear);
        assert_eq!(outlook_of(None, 18), Outlook::Unclear);
    }
}
