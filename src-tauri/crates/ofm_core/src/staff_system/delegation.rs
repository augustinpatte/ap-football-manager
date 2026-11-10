//! Delegated duties: what the assistant does by itself once the manager has handed a
//! responsibility over, and how the manager answers the proposals he leaves for them.
//!
//! A delegated duty acts through the same functions as the manager (`market`, `assistant`,
//! `scouting`), so what the assistant does is exactly what the manager could have done. It
//! needs an assistant manager in post; without one nothing runs. The assistant's own quality
//! shows in how well he judges candidates. Hires, releases, renewals and promotions are the
//! major decisions: with `confirm_major` they are left as proposals instead of being made.

use chrono::NaiveDate;
use domain::delegation::{DecisionOutcome, DelegationLogEntry, Duty, ProposalAction};
use domain::staff::{Staff, StaffPost};
use domain::team::{TrainingFocus, TrainingIntensity};

use super::assistant::{self, RecAction, RecKind};
use super::delegation_control as control;
use super::delegation_listing as listing;
use super::delegation_offers as offers;
use super::delegation_policy::{Authorization, Books, Reason, Spend, authorize_in_books};
use super::delegation_recruitment::{self as recruiting, Eye};
use super::delegation_report as report;
use super::delegation_squad as squad;
use super::market::{self, OfferResponse};
use super::profile::{StaffProfile, team_staff};
use crate::game::Game;
use crate::stable_hash::stable_hash;

/// Days before the end of a contract that the assistant starts on its renewal.
pub const RENEWAL_LEAD_DAYS: i64 = 60;
/// Lowest overall the assistant renews on his own account.
pub const RENEWAL_FLOOR: u8 = 45;
/// A free agent must beat the worst holder of a seat by this much to be proposed as an upgrade.
pub const UPGRADE_MARGIN: u8 = 12;
/// Proposals older than this are dropped.
pub const PROPOSAL_LIFETIME_DAYS: i64 = 21;

pub(super) fn date(game: &Game) -> NaiveDate {
    game.clock.current_date.date_naive()
}

pub(super) fn stamp(game: &Game) -> String {
    date(game).format("%Y-%m-%d").to_string()
}

pub(super) fn user_team(game: &Game) -> Option<String> {
    game.manager.team_id.clone()
}

/// How the assistant sees a candidate: their real quality, blurred by how poor his own eye is
/// (up to ±10 points for an assistant of 0, none at 100). Fixed per pair, so it never flickers.
fn perceived(assistant_rating: f64, assistant_id: &str, candidate: &Staff) -> f64 {
    let blur = (100.0 - assistant_rating).clamp(0.0, 100.0) / 100.0 * 10.0;
    let h = stable_hash(format!("{assistant_id}/{}", candidate.id).as_bytes(), 0);
    let unit = (h % 2001) as f64 / 1000.0 - 1.0; // -1..=1
    f64::from(candidate.overall()) + unit * blur
}

pub(super) fn log(game: &mut Game, duty: Duty, title_key: &str, params: Vec<(String, String)>) {
    record(game, duty, title_key, params, DecisionOutcome::Executed);
}

pub(super) fn record(
    game: &mut Game,
    duty: Duty,
    title_key: &str,
    params: Vec<(String, String)>,
    outcome: DecisionOutcome,
) {
    let date = stamp(game);
    game.delegation.push_log(DelegationLogEntry {
        date,
        duty,
        title_key: title_key.to_string(),
        params,
        outcome,
    });
}

/// Why a step is a proposal: the manager's standing request, or a limit he set.
fn reason_key(authorization: Authorization, usual: &'static str) -> &'static str {
    match authorization {
        Authorization::NeedsApproval(Reason::ApprovalRequired) | Authorization::Auto => usual,
        Authorization::NeedsApproval(_) | Authorization::NotDelegated => {
            "delegation.reason.aboveLimit"
        }
    }
}

pub(super) fn param(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

fn due(game: &Game, duty: Duty) -> bool {
    if !game.delegation.is_delegated(duty) {
        return false;
    }
    match game
        .delegation
        .last_run_on(duty)
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    {
        Some(last) => (date(game) - last).num_days() >= duty.period_days(),
        None => true,
    }
}

/// What the club can carry today: the books Total authority is measured against.
pub(super) fn books_of(game: &Game, team_id: &str) -> Books {
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return Books {
            transfer_cash: 0,
            wage_room: 0,
            staff_room: 0,
        };
    };
    Books {
        transfer_cash: team.transfer_budget.min(team.finance).max(0),
        wage_room: (team.wage_budget - crate::finances::calc_wages(game, team_id)).max(0),
        staff_room: market::staff_headroom(game, team_id),
    }
}

pub(super) fn assistant_of(game: &Game, team_id: &str) -> Option<(String, f64)> {
    let staff = team_staff(game, team_id);
    let a = staff
        .iter()
        .find(|s| s.post() == StaffPost::AssistantManager)?;
    Some((a.id.clone(), StaffProfile::from_staff(&staff).assistant))
}

/// Before the day's training: a delegated training plan.
///
/// Written only if the manager has not set the training himself since (see
/// [`control::manager_is_in_control`]); the history records a change of intensity and a recovery
/// order, the decisions, not the daily rotation of the focus.
pub fn run_training(game: &mut Game, weekday_num: u32) {
    let Some(team_id) = user_team(game) else {
        return;
    };
    if !game.delegation.is_delegated(Duty::Training) || assistant_of(game, &team_id).is_none() {
        return;
    }
    let Some(current) = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map(control::training_print)
    else {
        return;
    };
    let changed = game
        .delegation
        .applied(Duty::Training)
        .is_some_and(|applied| applied != current);
    if control::manager_is_in_control(game, Duty::Training, changed) {
        return;
    }
    let by_club = crate::match_day_eligibility::players_by_club(&game.players);
    let club = by_club.get(team_id.as_str()).map_or(&[][..], Vec::as_slice);
    let Some((focus, intensity)) =
        crate::ai_training::plan_for_team(&game.teams, club, &team_id, weekday_num)
    else {
        return;
    };
    drop(by_club);
    let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) else {
        return;
    };
    let eased_to_recovery = focus == TrainingFocus::Recovery
        && intensity == TrainingIntensity::Low
        && team.training_focus != TrainingFocus::Recovery;
    let intensity_changed = team.training_intensity != intensity;
    team.training_focus = focus.clone();
    team.training_intensity = intensity.clone();
    let print = control::training_print(team);
    game.delegation.record_applied(Duty::Training, &print);
    if eased_to_recovery {
        log(
            game,
            Duty::Training,
            "delegation.log.recoveryOrdered",
            vec![],
        );
    } else if intensity_changed {
        log(
            game,
            Duty::Training,
            "delegation.log.trainingIntensity",
            vec![
                param("focus", format!("{focus:?}")),
                param("intensity", format!("{intensity:?}")),
            ],
        );
    }
}

/// After the day: everything else that is delegated and due.
pub fn run_after_day(game: &mut Game) {
    let Some(team_id) = user_team(game) else {
        return;
    };
    let Some((assistant_id, rating)) = assistant_of(game, &team_id) else {
        report::note_missing_assistant(game, &team_id);
        return;
    };
    let today = stamp(game);
    // The eleven is named before the preparation reads it.
    for duty in [
        Duty::Lineup,
        Duty::MatchPreparation,
        Duty::Scouting,
        Duty::Youth,
        Duty::StaffRecruitment,
        Duty::StaffRenewals,
        Duty::PlayerRenewals,
        Duty::PlayerRecruitment,
        Duty::Tactics,
        Duty::Offers,
    ] {
        if !due(game, duty) {
            continue;
        }
        game.delegation.mark_run(duty, &today);
        match duty {
            Duty::Lineup => squad::lineup(game, &team_id, rating),
            Duty::MatchPreparation => match_preparation(game, &team_id),
            Duty::Scouting => scouting(game, &team_id, rating),
            Duty::Youth => youth(game, &team_id),
            Duty::StaffRecruitment => recruitment(game, &team_id, &assistant_id, rating),
            Duty::StaffRenewals => renewals(game, &team_id, &assistant_id, rating),
            Duty::PlayerRenewals => squad::player_renewals(game, &team_id),
            Duty::PlayerRecruitment => recruiting::player_recruitment(
                game,
                &team_id,
                Eye {
                    id: &assistant_id,
                    rating,
                },
            ),
            Duty::Tactics => control::tactics(game, &team_id, rating),
            Duty::Offers => {
                let eye = Eye {
                    id: &assistant_id,
                    rating,
                };
                offers::answer_offers(game, &team_id, eye);
                listing::review_listings(game, &team_id, eye);
            }
            Duty::Training => {}
        }
    }
    expire_proposals(game);
    report::notify(game, &team_id);
}

/// Rests tired starters and covers injured ones when a match is a day away, and sets the
/// style the opponent calls for.
fn match_preparation(game: &mut Game, team_id: &str) {
    let recs = assistant::recommendations(game, team_id);
    // Only when the match is imminent: the recommendation set already looks three days ahead,
    // the preparation acts when it is tomorrow or today.
    let imminent = matches_within(game, team_id, 1);
    if !imminent {
        return;
    }
    for rec in recs {
        let acts = matches!(
            rec.kind,
            RecKind::ReplaceStarter | RecKind::RestPlayer | RecKind::TacticalAdjustment
        );
        if !acts || rec.severity < 2 || rec.action == RecAction::None {
            continue;
        }
        if assistant::apply_action(game, team_id, &rec.action).is_ok() {
            log(
                game,
                Duty::MatchPreparation,
                &rec.title_key,
                rec.params.clone(),
            );
        }
    }
}

pub(super) fn matches_within(game: &Game, team_id: &str, days: i64) -> bool {
    let today = date(game);
    game.competitions_in_play()
        .iter()
        .filter(|c| game.competition_in_active_scope(c))
        .flat_map(|c| c.fixtures.iter())
        .filter(|f| f.status == domain::league::FixtureStatus::Scheduled)
        .filter(|f| f.home_team_id == team_id || f.away_team_id == team_id)
        .filter_map(|f| NaiveDate::parse_from_str(&f.date, "%Y-%m-%d").ok())
        .any(|d| (0..=days).contains(&(d - today).num_days()))
}

/// Sends idle scouts to look at players for the positions the squad is short in; with no gap,
/// at the best young talent the club could afford.
fn scouting(game: &mut Game, team_id: &str, _rating: f64) {
    let scouts: Vec<String> = team_staff(game, team_id)
        .iter()
        .filter(|s| s.role == domain::staff::StaffRole::Scout && s.post() != StaffPost::Analyst)
        .map(|s| s.id.clone())
        .collect();
    let busy: std::collections::HashSet<&str> = game
        .scouting_assignments
        .iter()
        .map(|a| a.scout_id.as_str())
        .chain(
            game.youth_scouting_assignments
                .iter()
                .map(|a| a.scout_id.as_str()),
        )
        .collect();
    let idle: Vec<String> = scouts
        .into_iter()
        .filter(|s| !busy.contains(s.as_str()))
        .collect();
    if idle.is_empty() {
        return;
    }
    let weak_groups: Vec<domain::player::Position> = assistant::recommendations(game, team_id)
        .iter()
        .filter(|r| r.kind == RecKind::WeakPosition)
        .filter_map(|r| r.params.iter().find(|(k, _)| k == "position"))
        .filter_map(|(_, v)| match v.as_str() {
            "Goalkeeper" => Some(domain::player::Position::Goalkeeper),
            "Defender" => Some(domain::player::Position::Defender),
            "Midfielder" => Some(domain::player::Position::Midfielder),
            "Forward" => Some(domain::player::Position::Forward),
            _ => None,
        })
        .collect();
    let budget = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map_or(0, |t| t.transfer_budget.max(0)) as u64;
    let today = date(game);
    let already: std::collections::HashSet<&str> = game
        .scouted_players
        .iter()
        .map(|s| s.player_id.as_str())
        .collect();
    let watched: std::collections::HashSet<&str> = game
        .scouting_assignments
        .iter()
        .map(|a| a.player_id.as_str())
        .collect();
    let mut targets: Vec<(&domain::player::Player, f64)> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref().is_some_and(|t| t != team_id) && !p.retired)
        .filter(|p| !already.contains(p.id.as_str()) && !watched.contains(p.id.as_str()))
        .filter(|p| p.market_value <= budget.max(1))
        .filter(|p| {
            weak_groups.is_empty() || weak_groups.contains(&p.natural_position.to_group_position())
        })
        .filter(|p| {
            domain::age::age_on(today, &p.date_of_birth).is_some_and(|a| {
                if weak_groups.is_empty() {
                    a <= 21
                } else {
                    a <= 30
                }
            })
        })
        .map(|p| (p, f64::from(p.ovr) * 0.6 + f64::from(p.potential) * 0.4))
        .collect();
    targets.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.id.cmp(&b.0.id)));
    let picks: Vec<(String, String, String)> = idle
        .iter()
        .zip(targets.iter())
        .map(|(scout, (p, _))| (scout.clone(), p.id.clone(), p.match_name.clone()))
        .collect();
    for (scout_id, player_id, name) in picks {
        if crate::scouting::send_scout(game, &scout_id, &player_id).is_ok() {
            log(
                game,
                Duty::Scouting,
                "delegation.log.scoutSent",
                vec![param("player", name)],
            );
        }
    }
}

/// Brings academy players who have reached the first team's level into it.
fn youth(game: &mut Game, team_id: &str) {
    let recs = assistant::recommendations(game, team_id);
    let confirm = game.delegation.needs_confirmation(Duty::Youth);
    let date = stamp(game);
    for rec in recs.into_iter().filter(|r| r.kind == RecKind::PromoteYouth) {
        if confirm {
            game.delegation.propose(
                Duty::Youth,
                &date,
                rec.action.clone().into_proposal(),
                &rec.title_key,
                rec.params.clone(),
                &rec.reason_key,
            );
        } else if assistant::apply_action(game, team_id, &rec.action).is_ok() {
            log(
                game,
                Duty::Youth,
                "delegation.log.promoted",
                rec.params.clone(),
            );
        }
    }
}

impl RecAction {
    fn into_proposal(self) -> ProposalAction {
        match self {
            RecAction::PromoteYouth { player_id } => ProposalAction::PromoteYouth { player_id },
            _ => unreachable!("only promotions are proposed"),
        }
    }
}

/// Fills vacant posts with the best affordable free agent, and proposes upgrades.
fn recruitment(game: &mut Game, team_id: &str, assistant_id: &str, rating: f64) {
    let reputation = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map_or(0, |t| t.reputation);
    let today = stamp(game);
    for status in market::structure(game, team_id) {
        if !status.is_vacant() {
            continue;
        }
        let mut candidates: Vec<&Staff> = game
            .staff
            .iter()
            .filter(|s| s.team_id.is_none() && s.post() == status.post)
            .filter(|s| {
                market::hire_blocker(game, team_id, &s.id, market::asking_wage(s, reputation))
                    .is_none()
            })
            .collect();
        candidates.sort_by(|a, b| {
            perceived(rating, assistant_id, b)
                .total_cmp(&perceived(rating, assistant_id, a))
                .then(a.id.cmp(&b.id))
        });
        let Some(choice) = candidates.first() else {
            continue;
        };
        let (id, name, wage) = (
            choice.id.clone(),
            choice.display_name.clone(),
            market::asking_wage(choice, reputation),
        );
        let params = vec![
            param("name", &name),
            param("post", format!("{:?}", status.post)),
            param("wage", wage),
        ];
        let authorization = authorize_in_books(
            &game.delegation,
            Duty::StaffRecruitment,
            Spend::StaffContract {
                weekly_wage: wage,
                years: 2,
            },
            &books_of(game, team_id),
            0,
        );
        if authorization != Authorization::Auto {
            game.delegation.propose(
                Duty::StaffRecruitment,
                &today,
                ProposalAction::HireStaff {
                    staff_id: id,
                    wage,
                    years: 2,
                },
                "delegation.proposal.hire",
                params,
                reason_key(authorization, "delegation.reason.vacancy"),
            );
        } else if market::hire(game, team_id, &id, wage, 2).is_ok() {
            log(game, Duty::StaffRecruitment, "delegation.log.hired", params);
        }
    }
    // Upgrades are always the manager's call: the assistant never dismisses anyone by himself.
    for status in market::structure(game, team_id) {
        let members: Vec<&Staff> = team_staff(game, team_id)
            .into_iter()
            .filter(|s| s.post() == status.post)
            .collect();
        let Some(worst) = members.iter().min_by_key(|s| (s.overall(), s.id.clone())) else {
            continue;
        };
        if status.has_room() {
            continue;
        }
        let best_free = game
            .staff
            .iter()
            .filter(|s| s.team_id.is_none() && s.post() == status.post)
            .filter(|s| s.overall() >= worst.overall().saturating_add(UPGRADE_MARGIN))
            .filter(|s| {
                i64::from(market::asking_wage(s, reputation))
                    <= market::staff_headroom(game, team_id) + i64::from(worst.wage)
            })
            .max_by(|a, b| a.overall().cmp(&b.overall()).then(b.id.cmp(&a.id)));
        if let Some(candidate) = best_free {
            let params = vec![
                param("name", &candidate.display_name),
                param("replaces", &worst.display_name),
                param("ovr", candidate.overall()),
                param("current", worst.overall()),
            ];
            let wage = market::asking_wage(candidate, reputation);
            let action = ProposalAction::ReleaseStaff {
                staff_id: worst.id.clone(),
            };
            let hire = ProposalAction::HireStaff {
                staff_id: candidate.id.clone(),
                wage,
                years: 2,
            };
            let (a, h) = (action, hire);
            game.delegation.propose(
                Duty::StaffRecruitment,
                &today,
                a,
                "delegation.proposal.upgradeRelease",
                params.clone(),
                "delegation.reason.upgrade",
            );
            game.delegation.propose(
                Duty::StaffRecruitment,
                &today,
                h,
                "delegation.proposal.upgradeHire",
                params,
                "delegation.reason.upgrade",
            );
        }
    }
}

/// Renews good members whose contracts are running out.
fn renewals(game: &mut Game, team_id: &str, assistant_id: &str, rating: f64) {
    let reputation = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map_or(0, |t| t.reputation);
    let today = stamp(game);
    let ending: Vec<(String, String, u32)> = team_staff(game, team_id)
        .into_iter()
        .filter(|s| market::days_left(game, s).is_some_and(|d| d <= RENEWAL_LEAD_DAYS))
        .filter(|s| perceived(rating, assistant_id, s) >= f64::from(RENEWAL_FLOOR))
        .map(|s| {
            (
                s.id.clone(),
                s.display_name.clone(),
                market::renewal_ask(s, reputation),
            )
        })
        .collect();
    for (id, name, ask) in ending {
        let years = 2;
        let params = vec![
            param("name", &name),
            param("wage", ask),
            param("years", years),
        ];
        let authorization = authorize_in_books(
            &game.delegation,
            Duty::StaffRenewals,
            Spend::StaffContract {
                weekly_wage: ask,
                years,
            },
            &books_of(game, team_id),
            0,
        );
        if authorization != Authorization::Auto {
            game.delegation.propose(
                Duty::StaffRenewals,
                &today,
                ProposalAction::RenewStaff {
                    staff_id: id,
                    wage: ask,
                    years,
                },
                "delegation.proposal.renew",
                params,
                reason_key(authorization, "delegation.reason.contractEnding"),
            );
        } else if market::negotiate_renewal(game, team_id, &id, ask, years)
            .is_ok_and(|r| r == OfferResponse::Accepted)
        {
            log(game, Duty::StaffRenewals, "delegation.log.renewed", params);
        }
    }
}

fn expire_proposals(game: &mut Game) {
    let today = date(game);
    game.delegation.proposals.retain(|p| {
        NaiveDate::parse_from_str(&p.created, "%Y-%m-%d")
            .is_ok_and(|c| (today - c).num_days() <= PROPOSAL_LIFETIME_DAYS)
    });
}

/// The manager answers a proposal. Accepting carries it out through the ordinary rules, so it
/// can still fail (the budget changed, the member signed elsewhere); a failed proposal is
/// dropped and the error returned.
pub fn resolve_proposal(game: &mut Game, proposal_id: &str, accept: bool) -> Result<(), String> {
    let team_id = user_team(game).ok_or("be.error.noTeamAssigned")?;
    let index = game
        .delegation
        .proposals
        .iter()
        .position(|p| p.id == proposal_id)
        .ok_or("be.error.delegation.proposalNotFound")?;
    let proposal = game.delegation.proposals.remove(index);
    if !accept {
        match &proposal.action {
            ProposalAction::SignPlayer { player_id, .. } => {
                recruiting::on_declined(game, player_id);
            }
            ProposalAction::AcceptOffer { player_id, .. }
            | ProposalAction::CounterOffer { player_id, .. }
            | ProposalAction::ListForSale { player_id } => {
                offers::on_declined(game, player_id);
            }
            _ => {}
        }
        record(
            game,
            proposal.duty,
            &proposal.title_key,
            proposal.params,
            DecisionOutcome::Declined,
        );
        return Ok(());
    }
    let result = match &proposal.action {
        ProposalAction::HireStaff {
            staff_id,
            wage,
            years,
        } => market::hire(game, &team_id, staff_id, *wage, *years),
        ProposalAction::RenewStaff {
            staff_id,
            wage,
            years,
        } => market::negotiate_renewal(game, &team_id, staff_id, *wage, *years).and_then(|r| {
            if r == OfferResponse::Accepted {
                Ok(())
            } else {
                Err("be.error.staff.offerRefused".to_string())
            }
        }),
        ProposalAction::ReleaseStaff { staff_id } => {
            market::release(game, &team_id, staff_id).map(|_| ())
        }
        ProposalAction::SetTraining { focus, intensity } => assistant::apply_action(
            game,
            &team_id,
            &RecAction::SetTraining {
                focus: focus.clone(),
                intensity: intensity.clone(),
            },
        ),
        ProposalAction::PromoteYouth { player_id } => assistant::apply_action(
            game,
            &team_id,
            &RecAction::PromoteYouth {
                player_id: player_id.clone(),
            },
        ),
        ProposalAction::SendScout {
            scout_id,
            player_id,
        } => crate::scouting::send_scout(game, scout_id, player_id),
        ProposalAction::RenewPlayer {
            player_id,
            wage,
            years,
        } => squad::renew_player(game, &team_id, player_id, *wage, *years),
        ProposalAction::RestPlayers { .. } => Ok(()),
        ProposalAction::SignPlayer {
            player_id,
            fee,
            wage,
            years,
        } => recruiting::sign_approved(game, player_id, *fee, *wage, *years),
        ProposalAction::AcceptOffer {
            player_id,
            offer_id,
            fee,
        } => offers::accept_approved(game, player_id, offer_id, *fee),
        ProposalAction::CounterOffer {
            player_id,
            offer_id,
            fee,
        } => offers::counter_approved(game, player_id, offer_id, *fee),
        ProposalAction::ListForSale { player_id } => listing::list_approved(game, player_id),
    };
    if result.is_ok() {
        record(
            game,
            proposal.duty,
            &format!("{}.accepted", proposal.title_key),
            proposal.params,
            DecisionOutcome::Approved,
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::assistant::tests::{game, staff_member};
    use super::*;
    use domain::delegation::DelegationMode;
    use domain::player::SquadRole;

    fn delegate(g: &mut Game, duty: Duty, confirm: bool) {
        g.delegation.set(duty, true, confirm);
    }

    #[test]
    fn nothing_happens_when_nothing_is_delegated() {
        let mut g = game(Some(80), 1);
        g.staff.retain(|s| s.post() != StaffPost::Physio);
        run_after_day(&mut g);
        assert!(g.delegation.log.is_empty());
        assert!(g.delegation.proposals.is_empty());
        assert!(!g.staff.iter().any(|s| s.post() == StaffPost::Physio));
    }

    #[test]
    fn nothing_runs_without_an_assistant() {
        let mut g = game(None, 1);
        delegate(&mut g, Duty::StaffRecruitment, false);
        let mut free = staff_member("free", StaffPost::AssistantManager, 70, "x");
        free.team_id = None;
        g.staff.push(free);
        run_after_day(&mut g);
        assert!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .is_none()
        );
    }

    #[test]
    fn delegated_recruitment_fills_a_vacancy_without_asking() {
        let mut g = game(Some(80), 6);
        g.staff.retain(|s| s.post() != StaffPost::Physio);
        let mut free = staff_member("free", StaffPost::Physio, 70, "x");
        free.team_id = None;
        g.staff.push(free);
        delegate(&mut g, Duty::StaffRecruitment, false);
        run_after_day(&mut g);
        assert_eq!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .as_deref(),
            Some("team1")
        );
        assert_eq!(g.delegation.log.len(), 1);
        assert_eq!(g.delegation.log[0].duty, Duty::StaffRecruitment);
    }

    #[test]
    fn with_confirmation_the_vacancy_becomes_a_proposal_the_manager_decides() {
        let mut g = game(Some(80), 6);
        g.staff.retain(|s| s.post() != StaffPost::Physio);
        let mut free = staff_member("free", StaffPost::Physio, 70, "x");
        free.team_id = None;
        g.staff.push(free);
        delegate(&mut g, Duty::StaffRecruitment, true);
        run_after_day(&mut g);
        assert!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .is_none()
        );
        assert_eq!(g.delegation.proposals.len(), 1);
        let id = g.delegation.proposals[0].id.clone();

        // A refusal leaves everything as it was.
        let mut refused = g.clone();
        resolve_proposal(&mut refused, &id, false).unwrap();
        assert!(refused.delegation.proposals.is_empty());
        assert!(
            refused
                .staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .is_none()
        );

        // An acceptance hires through the ordinary rules.
        resolve_proposal(&mut g, &id, true).unwrap();
        assert_eq!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .as_deref(),
            Some("team1")
        );
        assert!(g.delegation.proposals.is_empty());
    }

    #[test]
    fn a_duty_runs_once_per_period() {
        let mut g = game(Some(80), 6);
        delegate(&mut g, Duty::Youth, false);
        let mut kid = domain::player::Player {
            ..g.players[0].clone()
        };
        kid.id = "kid".into();
        kid.squad_role = SquadRole::Youth;
        kid.date_of_birth = "2007-01-01".into();
        kid.ovr = 70;
        g.players.push(kid);
        run_after_day(&mut g);
        assert_eq!(
            g.players.iter().find(|p| p.id == "kid").unwrap().squad_role,
            SquadRole::Senior
        );
        let runs = g.delegation.last_run.len();
        // The next day it is not due again.
        g.clock.current_date += chrono::Duration::days(1);
        g.delegation.log.clear();
        run_after_day(&mut g);
        assert_eq!(g.delegation.last_run.len(), runs);
        assert!(g.delegation.log.is_empty());
    }

    #[test]
    fn delegated_renewals_renew_a_good_member_and_leave_a_poor_one() {
        let mut g = game(Some(80), 6);
        let mut good = staff_member("good", StaffPost::Scout, 70, "team1");
        good.contract_end = Some("2026-09-01".into());
        let mut poor = staff_member("poor", StaffPost::Scout, 10, "team1");
        poor.contract_end = Some("2026-09-01".into());
        g.staff.push(good);
        g.staff.push(poor);
        delegate(&mut g, Duty::StaffRenewals, false);
        run_after_day(&mut g);
        let good = g.staff.iter().find(|s| s.id == "good").unwrap();
        assert!(good.contract_end.as_deref().unwrap() > "2027-01-01");
        let poor = g.staff.iter().find(|s| s.id == "poor").unwrap();
        assert_eq!(poor.contract_end.as_deref(), Some("2026-09-01"));
    }

    #[test]
    fn delegated_match_preparation_rests_the_injured_starter_the_day_before() {
        let mut g = game(Some(80), 1);
        g.players.iter_mut().find(|p| p.id == "s11").unwrap().ovr = 68;
        g.players.iter_mut().find(|p| p.id == "s0").unwrap().injury =
            Some(domain::player::Injury {
                name: "x".into(),
                days_remaining: 10,
            });
        delegate(&mut g, Duty::MatchPreparation, false);
        run_after_day(&mut g);
        assert!(!g.teams[0].starting_xi_ids.contains(&"s0".to_string()));
        assert!(g.teams[0].starting_xi_ids.contains(&"s11".to_string()));
    }

    #[test]
    fn delegated_training_follows_the_assistants_plan_and_orders_recovery_when_the_eleven_is_spent()
    {
        let mut g = game(Some(80), 6);
        delegate(&mut g, Duty::Training, false);
        for p in g
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team1"))
        {
            p.condition = 5;
        }
        g.teams[0].training_focus = TrainingFocus::Attacking;
        g.teams[0].training_intensity = TrainingIntensity::High;
        run_training(&mut g, 1);
        assert_eq!(g.teams[0].training_focus, TrainingFocus::Recovery);
        assert_eq!(g.teams[0].training_intensity, TrainingIntensity::Low);
    }

    #[test]
    fn delegated_scouting_sends_an_idle_scout_after_a_player() {
        let mut g = game(Some(80), 6);
        let mut scout = staff_member("scout1", StaffPost::Scout, 70, "team1");
        scout.wage = 500;
        g.staff.retain(|s| s.post() != StaffPost::Scout);
        g.staff.push(scout);
        g.teams[0].transfer_budget = 10_000_000;
        for p in g
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team2"))
        {
            p.date_of_birth = "2005-01-01".into();
            p.market_value = 1_000_000;
            // The test squad has no forwards: that is the gap the scout is sent to fill.
            p.natural_position = domain::player::Position::Striker;
        }
        delegate(&mut g, Duty::Scouting, false);
        run_after_day(&mut g);
        assert_eq!(g.scouting_assignments.len(), 1);
        assert_eq!(g.scouting_assignments[0].scout_id, "scout1");
    }

    /// Given a duty that acts alone within the manager's ceilings (not Total authority) and a limit
    /// below the asking wage, when the recruitment duty runs, then the vacancy is proposed, not
    /// filled: the assistant signs nothing above a limit.
    #[test]
    fn a_hire_above_the_wage_limit_is_proposed_when_the_duty_acts_within_ceilings() {
        let mut g = game(Some(80), 6);
        g.delegation.apply_mode(DelegationMode::Full);
        g.delegation.limits.finance_bound = false;
        g.delegation.limits.max_staff_wage = 100;
        g.staff.retain(|s| s.post() != StaffPost::Physio);
        let mut free = staff_member("free", StaffPost::Physio, 70, "x");
        free.team_id = None;
        g.staff.push(free);
        run_after_day(&mut g);
        assert!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .is_none()
        );
        // The hire is the one proposal of the staff duty; other duties may have their own.
        let hires: Vec<_> = g
            .delegation
            .proposals
            .iter()
            .filter(|p| p.duty == Duty::StaffRecruitment)
            .collect();
        assert_eq!(hires.len(), 1);
        assert_eq!(hires[0].reason_key, "delegation.reason.aboveLimit");
    }

    /// Given the same vacancy under Total authority and a wage the old ceiling forbids but the staff
    /// budget can carry, when the recruitment duty runs, then the assistant hires him himself.
    #[test]
    fn under_total_authority_a_hire_within_the_staff_budget_is_made_without_asking() {
        let mut g = game(Some(80), 6);
        g.delegation.apply_mode(DelegationMode::Full);
        g.delegation.limits.max_staff_wage = 100;
        g.staff.retain(|s| s.post() != StaffPost::Physio);
        let mut free = staff_member("free", StaffPost::Physio, 70, "x");
        free.team_id = None;
        g.staff.push(free);
        g.teams[0].wage_budget = 50_000_000;
        run_after_day(&mut g);
        assert_eq!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .as_deref(),
            Some("team1")
        );
        assert!(
            g.delegation
                .proposals
                .iter()
                .all(|p| p.duty != Duty::StaffRecruitment)
        );
    }

    /// Given the manager answers proposals, when he accepts one and declines another, then the
    /// history records each answer with its outcome.
    #[test]
    fn the_history_records_what_the_manager_answered() {
        let mut g = game(Some(80), 6);
        g.delegation.apply_mode(DelegationMode::Approval);
        let required: Vec<StaffPost> = StaffPost::ALL
            .iter()
            .copied()
            .filter(|p| p.required() > 0 && *p != StaffPost::AssistantManager)
            .take(2)
            .collect();
        g.staff.retain(|s| !required.contains(&s.post()));
        for (id, post) in ["p", "a"].into_iter().zip(required) {
            let mut free = staff_member(id, post, 70, "x");
            free.team_id = None;
            g.staff.push(free);
        }
        run_after_day(&mut g);
        let ids: Vec<String> = g
            .delegation
            .proposals
            .iter()
            .map(|p| p.id.clone())
            .collect();
        assert!(ids.len() >= 2, "{ids:?}");
        resolve_proposal(&mut g, &ids[0], true).unwrap();
        resolve_proposal(&mut g, &ids[1], false).unwrap();
        let outcomes: Vec<DecisionOutcome> = g.delegation.log.iter().map(|e| e.outcome).collect();
        assert_eq!(
            outcomes,
            vec![DecisionOutcome::Approved, DecisionOutcome::Declined]
        );
    }

    /// Given full delegation switched back to manual and on again, when days pass, then the
    /// assistant acts only while the manager has handed duties over.
    #[test]
    fn switching_between_manual_and_full_delegation_takes_effect_at_once() {
        let mut g = game(Some(80), 6);
        g.staff.retain(|s| s.post() != StaffPost::Physio);
        let mut free = staff_member("free", StaffPost::Physio, 70, "x");
        free.team_id = None;
        g.staff.push(free);
        g.delegation.apply_mode(DelegationMode::Full);
        g.delegation.apply_mode(DelegationMode::Manual);
        run_after_day(&mut g);
        assert!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .is_none()
        );
        assert!(g.delegation.log.is_empty());
        g.delegation.apply_mode(DelegationMode::Full);
        run_after_day(&mut g);
        assert_eq!(
            g.staff
                .iter()
                .find(|s| s.id == "free")
                .unwrap()
                .team_id
                .as_deref(),
            Some("team1")
        );
    }

    /// Given a fortnight of delegated decisions, when it is over, then the manager gets one
    /// digest, and not a second one the next day.
    #[test]
    fn a_fortnight_of_decisions_makes_one_digest() {
        let mut g = game(Some(80), 6);
        g.delegation.apply_mode(DelegationMode::Full);
        log(
            &mut g,
            Duty::Training,
            "delegation.log.recoveryOrdered",
            vec![],
        );
        let digests = |g: &Game| {
            g.messages
                .iter()
                .filter(|m| m.id.starts_with("assistant_digest_"))
                .count()
        };
        run_after_day(&mut g);
        assert_eq!(digests(&g), 1);
        g.clock.current_date += chrono::Duration::days(1);
        log(
            &mut g,
            Duty::Training,
            "delegation.log.recoveryOrdered",
            vec![],
        );
        run_after_day(&mut g);
        assert_eq!(digests(&g), 1, "no second digest inside the fortnight");
        g.clock.current_date += chrono::Duration::days(14);
        run_after_day(&mut g);
        assert_eq!(digests(&g), 2);
    }

    /// Given proposals that stay unanswered for days, when each day ends, then the manager is
    /// reminded at most every three days.
    #[test]
    fn waiting_proposals_do_not_nag_every_day() {
        let mut g = game(Some(80), 6);
        g.delegation.apply_mode(DelegationMode::Approval);
        g.staff.retain(|s| s.post() != StaffPost::Physio);
        let mut free = staff_member("free", StaffPost::Physio, 70, "x");
        free.team_id = None;
        g.staff.push(free);
        for _ in 0..6 {
            run_after_day(&mut g);
            g.clock.current_date += chrono::Duration::days(1);
        }
        let reminders = g
            .messages
            .iter()
            .filter(|m| m.id.starts_with("assistant_proposals_"))
            .count();
        assert_eq!(reminders, 2);
    }

    /// Given duties handed over and an assistant who has left, when a day ends, then the
    /// manager is told once, and nothing is done in the meantime.
    #[test]
    fn an_assistant_who_leaves_is_missed_once() {
        let mut g = game(None, 6);
        g.delegation.apply_mode(DelegationMode::Full);
        run_after_day(&mut g);
        run_after_day(&mut g);
        let notices = g
            .messages
            .iter()
            .filter(|m| m.id.starts_with("assistant_missing_"))
            .count();
        assert_eq!(notices, 1);
        assert!(g.delegation.log.is_empty());
    }

    /// Given full delegation, when a match season rolls into the next one, then the settings,
    /// limits and history are untouched by the rollover.
    #[test]
    fn delegation_settings_survive_a_json_round_trip_with_limits_and_history() {
        let mut g = game(Some(80), 6);
        g.delegation.apply_mode(DelegationMode::Full);
        g.delegation.limits.max_staff_wage = 1_234;
        log(&mut g, Duty::Lineup, "delegation.log.lineupChosen", vec![]);
        let json = serde_json::to_string(&g).unwrap();
        let back: Game = serde_json::from_str(&json).unwrap();
        assert_eq!(back.delegation, g.delegation);
        assert_eq!(back.delegation.mode(), DelegationMode::Full);
    }

    #[test]
    fn the_delegation_state_survives_a_json_round_trip() {
        let mut g = game(Some(80), 6);
        delegate(&mut g, Duty::Scouting, false);
        g.delegation.mark_run(Duty::Scouting, "2026-08-01");
        let json = serde_json::to_string(&g.delegation).unwrap();
        let back: domain::delegation::DelegationState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, g.delegation);
    }
}
