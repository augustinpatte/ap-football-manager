//! The delegated duties that concern the players: naming the eleven the day before a match and
//! renewing the contracts that are running down.
//!
//! Both act through the rules the manager's own hand uses: the eleven is picked by the same
//! selector an AI club's manager uses (`turn::squad`), and a renewal goes through
//! `delegated_renewals` or `contracts::apply_agreed_renewal`, so the board's wage policy applies
//! to the assistant exactly as it does to the manager.

use chrono::NaiveDate;
use domain::delegation::{DecisionOutcome, Duty, ProposalAction};
use domain::player::Player;

use super::delegation::{books_of, date, log, matches_within, param, record, stamp};
use super::delegation_control as control;
use super::delegation_policy::{Authorization, Reason, Spend, authorize_in_books};
use crate::contract_wage_policy::wage_policy_verdict;
use crate::contracts::{
    ContractWarningStage, DelegatedRenewalOptions, DelegatedRenewalResultStatus, RenewalAgreedBy,
    apply_agreed_renewal, contract_warning_stage, expected_contract_years, expected_wage,
    has_active_manager_block, has_let_expire_intent,
};
use crate::game::Game;
use crate::turn::squad::delegated_eleven;

/// Days the assistant leaves a player alone after a renewal attempt that did not conclude, so a
/// stalled case is not reopened (and re-reported) every week.
const RETRY_AFTER_DAYS: i64 = 21;
/// Longest contract the assistant ever agrees on his own authority.
const MAX_AUTO_YEARS: u32 = 5;

/// Names the eleven when a match is a day away: the strongest fit side, with spent starters
/// rested, written to the club's saved line-up so the manager sees exactly what will play.
pub(super) fn lineup(game: &mut Game, team_id: &str, assistant_rating: f64) {
    if !matches_within(game, team_id, 1) {
        return;
    }
    let changed = control::eleven_was_changed(game, team_id, game.delegation.applied(Duty::Lineup));
    if control::manager_is_in_control(game, Duty::Lineup, changed) {
        return;
    }
    let eleven = delegated_eleven(game, team_id, assistant_rating);
    if eleven.is_empty() {
        return;
    }
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return;
    };
    if team.starting_xi_ids == eleven {
        let print = control::lineup_print(&eleven);
        game.delegation.record_applied(Duty::Lineup, &print);
        return;
    }
    let rested = team
        .starting_xi_ids
        .iter()
        .filter(|id| !eleven.contains(id))
        .count();
    let (captain, penalty, free_kick, corner) =
        crate::live_match_manager::auto_select_set_pieces(game, &eleven);
    if let Some(team) = game.teams.iter_mut().find(|t| t.id == team_id) {
        team.starting_xi_ids = eleven.clone();
        let roles = &mut team.match_roles;
        keep_or_replace(&mut roles.captain, captain, &eleven);
        keep_or_replace(&mut roles.penalty_taker, penalty, &eleven);
        keep_or_replace(&mut roles.free_kick_taker, free_kick, &eleven);
        keep_or_replace(&mut roles.corner_taker, corner, &eleven);
    }
    let print = control::lineup_print(&eleven);
    game.delegation.record_applied(Duty::Lineup, &print);
    log(
        game,
        Duty::Lineup,
        "delegation.log.lineupChosen",
        vec![param("changes", rested)],
    );
}

/// A set-piece role stays with its holder while he is in the eleven; otherwise it passes to the
/// pick the selector would make. A role nobody held is left empty: the assistant does not invent
/// appointments the manager never made.
fn keep_or_replace(slot: &mut Option<String>, pick: Option<String>, eleven: &[String]) {
    if slot.as_ref().is_some_and(|holder| !eleven.contains(holder)) {
        *slot = pick;
    }
}

fn contract_is_running_down(player: &Player, today: NaiveDate) -> bool {
    matches!(
        contract_warning_stage(player.contract_end(), today),
        Some(
            ContractWarningStage::SixMonths
                | ContractWarningStage::ThreeMonths
                | ContractWarningStage::FinalWeeks
        )
    )
}

fn attempted_recently(player: &Player, today: NaiveDate) -> bool {
    player
        .morale_core
        .renewal_state
        .as_ref()
        .and_then(|state| state.last_assistant_attempt_date.as_deref())
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .is_some_and(|last| (today - last).num_days() < RETRY_AFTER_DAYS)
}

/// Whether the manager turned down a renewal proposal for this player lately. The assistant
/// takes the answer: he does not put the same case to him again the following week.
fn declined_recently(game: &Game, player_id: &str, today: NaiveDate) -> bool {
    game.delegation.log.iter().any(|entry| {
        entry.outcome == DecisionOutcome::Declined
            && entry.duty == Duty::PlayerRenewals
            && entry
                .params
                .iter()
                .any(|(key, value)| key == "player_id" && value == player_id)
            && NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d")
                .is_ok_and(|on| (today - on).num_days() < RETRY_AFTER_DAYS)
    })
}

/// A renewal worked out by the assistant: what the player would sign at.
struct Terms {
    player_id: String,
    name: String,
    weekly_wage: u32,
    years: u8,
    current_wage: u32,
}

/// Contracts that are within six months of ending, at the terms the player would ask.
fn renewal_candidates(game: &Game, team_id: &str) -> Vec<Terms> {
    let today = date(game);
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return Vec::new();
    };
    let mut terms: Vec<Terms> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref() == Some(team_id) && !p.retired)
        .filter(|p| p.wage() > 0 && contract_is_running_down(p, today))
        .filter(|p| !has_let_expire_intent(p) && !has_active_manager_block(p, today))
        .filter(|p| !attempted_recently(p, today) && !declined_recently(game, &p.id, today))
        .filter(|p| {
            !game.delegation.proposals.iter().any(|proposal| {
                matches!(&proposal.action,
                    ProposalAction::RenewPlayer { player_id, .. } if *player_id == p.id)
            })
        })
        .map(|p| Terms {
            player_id: p.id.clone(),
            name: p.match_name.clone(),
            weekly_wage: expected_wage(p, team, today),
            years: expected_contract_years(p, today).min(MAX_AUTO_YEARS) as u8,
            current_wage: p.wage(),
        })
        .collect();
    terms.sort_by(|a, b| a.player_id.cmp(&b.player_id));
    terms
}

/// Renews the contracts that are running down: within the manager's limits by himself, above
/// them (or when the manager asks to be consulted) as a proposal.
pub(super) fn player_renewals(game: &mut Game, team_id: &str) {
    let today = stamp(game);
    let mut automatic: Vec<Terms> = Vec::new();
    for terms in renewal_candidates(game, team_id) {
        let authorization = authorize_in_books(
            &game.delegation,
            Duty::PlayerRenewals,
            Spend::PlayerContract {
                weekly_wage: terms.weekly_wage,
                years: terms.years,
            },
            &books_of(game, team_id),
            // A renewal adds the raise to the weekly bill, not the whole wage.
            terms.weekly_wage.saturating_sub(terms.current_wage),
        );
        match authorization {
            Authorization::Auto => automatic.push(terms),
            Authorization::NeedsApproval(reason) => {
                propose_renewal(game, team_id, &today, terms, reason);
            }
            Authorization::NotDelegated => {}
        }
    }
    renew_within_limits(game, automatic);
}

fn propose_renewal(game: &mut Game, team_id: &str, today: &str, terms: Terms, reason: Reason) {
    let affordable = {
        let player = game.players.iter().find(|p| p.id == terms.player_id);
        let team = game.teams.iter().find(|t| t.id == team_id);
        match (player, team) {
            (Some(player), Some(team)) => {
                wage_policy_verdict(game, team, player, terms.weekly_wage).permits()
            }
            _ => false,
        }
    };
    if !affordable {
        return;
    }
    let reason_key = match reason {
        Reason::ApprovalRequired => "delegation.reason.contractEnding",
        _ => "delegation.reason.aboveLimit",
    };
    game.delegation.propose(
        Duty::PlayerRenewals,
        today,
        ProposalAction::RenewPlayer {
            player_id: terms.player_id.clone(),
            wage: terms.weekly_wage,
            years: terms.years,
        },
        "delegation.proposal.renewPlayer",
        vec![
            param("player", &terms.name),
            // Not shown: lets the history remember whom the manager said no about.
            param("player_id", &terms.player_id),
            param("wage", terms.weekly_wage),
            param("years", terms.years),
        ],
        reason_key,
    );
}

/// The renewals the limits let the assistant make alone, through the existing delegated-renewal
/// service. Its own judgement still applies: a player the assistant cannot win over, or one too
/// important for him to settle, stays with the manager and is reported as such.
fn renew_within_limits(game: &mut Game, automatic: Vec<Terms>) {
    if automatic.is_empty() {
        return;
    }
    // Every case was already vetted against the limits at its own terms; the percentage only has
    // to be wide enough for the dearest of them not to read as "beyond the limits".
    let widest = automatic
        .iter()
        .map(|t| (u64::from(t.weekly_wage) * 100).div_ceil(u64::from(t.current_wage.max(1))))
        .max()
        .unwrap_or(100);
    let options = DelegatedRenewalOptions {
        player_ids: Some(automatic.iter().map(|t| t.player_id.clone()).collect()),
        max_wage_increase_pct: u32::try_from(widest.saturating_sub(100)).unwrap_or(u32::MAX),
        max_contract_years: MAX_AUTO_YEARS,
    };
    let Ok(outcome) = crate::delegated_renewals::delegate_renewals(game, options) else {
        return;
    };
    for case in outcome.cases {
        if case.status == DelegatedRenewalResultStatus::Successful {
            log(
                game,
                Duty::PlayerRenewals,
                "delegation.log.playerRenewed",
                vec![
                    param("player", &case.player_name),
                    param("wage", case.agreed_wage.unwrap_or_default()),
                    param("years", case.agreed_years.unwrap_or_default()),
                ],
            );
        } else {
            record(
                game,
                Duty::PlayerRenewals,
                "delegation.log.playerRenewalLeft",
                vec![param("player", &case.player_name)],
                DecisionOutcome::LeftToManager,
            );
        }
    }
}

/// The manager approved a proposed renewal: it is made at the terms he was shown, unless the
/// board's wage policy now forbids them.
pub(super) fn renew_player(
    game: &mut Game,
    team_id: &str,
    player_id: &str,
    wage: u32,
    years: u8,
) -> Result<(), String> {
    let index = game
        .players
        .iter()
        .position(|p| p.id == player_id && p.team_id.as_deref() == Some(team_id))
        .ok_or("be.error.playerNotInClub")?;
    let team = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .ok_or("be.error.teamNotFound")?
        .clone();
    if !wage_policy_verdict(game, &team, &game.players[index], wage).permits() {
        return Err("be.error.delegation.wagePolicy".to_string());
    }
    let today = date(game);
    apply_agreed_renewal(
        &mut game.players[index],
        &team,
        wage,
        u32::from(years),
        today,
        RenewalAgreedBy::Assistant,
    )
}

#[cfg(test)]
mod tests {
    use super::super::assistant::tests::game;
    use super::*;
    use chrono::Duration;
    use domain::delegation::DelegationMode;
    use domain::player::Injury;

    /// Every duty acting alone within the manager's ceilings: the tests here are about those
    /// ceilings, so the Total authority the Full preset now carries is taken back.
    fn full(g: &mut Game) {
        g.delegation.apply_mode(DelegationMode::Full);
        g.delegation.limits.finance_bound = false;
    }

    fn named_eleven(g: &Game) -> Vec<String> {
        g.teams[0].starting_xi_ids.clone()
    }

    /// Given a match tomorrow and three exhausted starters with fresh deputies, when the lineup
    /// is delegated, then the saved eleven is eleven distinct fit players without the spent ones.
    #[test]
    fn the_assistant_names_a_valid_eleven_and_rests_the_spent_starters() {
        let mut g = game(Some(80), 1);
        full(&mut g);
        for id in ["s0", "s5", "s9"] {
            g.players.iter_mut().find(|p| p.id == id).unwrap().condition = 20;
        }
        lineup(&mut g, "team1", 80.0);
        let eleven = named_eleven(&g);
        assert_eq!(eleven.len(), 11);
        let distinct: std::collections::HashSet<_> = eleven.iter().collect();
        assert_eq!(distinct.len(), 11);
        for spent in ["s0", "s5", "s9"] {
            assert!(!eleven.iter().any(|id| id == spent), "{spent} should rest");
        }
        assert!(eleven.iter().all(|id| {
            g.players
                .iter()
                .find(|p| &p.id == id)
                .is_some_and(|p| p.injury.is_none() && p.team_id.as_deref() == Some("team1"))
        }));
        assert_eq!(g.delegation.log.len(), 1);
        assert_eq!(g.delegation.log[0].duty, Duty::Lineup);
    }

    /// Given the same state twice, when the lineup is delegated, then the same eleven comes out.
    #[test]
    fn the_lineup_is_deterministic_for_the_same_state() {
        let build = || {
            let mut g = game(Some(55), 1);
            full(&mut g);
            for id in ["s1", "s2", "s7"] {
                g.players.iter_mut().find(|p| p.id == id).unwrap().condition = 30;
            }
            lineup(&mut g, "team1", 55.0);
            named_eleven(&g)
        };
        assert_eq!(build(), build());
    }

    /// Given an injured starter, when the lineup is delegated, then he is replaced.
    #[test]
    fn an_injured_starter_is_replaced() {
        let mut g = game(Some(80), 1);
        full(&mut g);
        g.players.iter_mut().find(|p| p.id == "s3").unwrap().injury = Some(Injury {
            name: "x".into(),
            days_remaining: 14,
        });
        lineup(&mut g, "team1", 80.0);
        assert!(!named_eleven(&g).contains(&"s3".to_string()));
        assert_eq!(named_eleven(&g).len(), 11);
    }

    /// Given a match five days away, when the lineup is delegated, then nothing changes yet.
    #[test]
    fn the_lineup_waits_until_the_match_is_a_day_away() {
        let mut g = game(Some(80), 5);
        full(&mut g);
        g.players
            .iter_mut()
            .find(|p| p.id == "s0")
            .unwrap()
            .condition = 10;
        let before = named_eleven(&g);
        lineup(&mut g, "team1", 80.0);
        assert_eq!(named_eleven(&g), before);
        assert!(g.delegation.log.is_empty());
    }

    fn expiring_player(g: &mut Game, id: &str) {
        let end = (g.clock.current_date + Duration::days(60))
            .format("%Y-%m-%d")
            .to_string();
        let p = g.players.iter_mut().find(|p| p.id == id).unwrap();
        p.stage_contract(Some("2024-08-01".into()), Some(end), 2_000);
        p.market_value = 100_000;
        p.morale = 90;
        p.morale_core.manager_trust = 90;
    }

    /// Given a cheap player at the end of his contract and full delegation, when the renewals
    /// duty runs, then the assistant renews him within the limits and the history says so.
    #[test]
    fn a_renewal_within_the_limits_is_made_without_asking() {
        let mut g = game(Some(90), 6);
        full(&mut g);
        expiring_player(&mut g, "s14");
        let before = g
            .players
            .iter()
            .find(|p| p.id == "s14")
            .unwrap()
            .contract_end()
            .map(str::to_owned);
        player_renewals(&mut g, "team1");
        let after = g
            .players
            .iter()
            .find(|p| p.id == "s14")
            .unwrap()
            .contract_end()
            .map(str::to_owned);
        assert_ne!(before, after, "contract should have been extended");
        assert!(g.delegation.proposals.is_empty());
        assert!(
            g.delegation
                .log
                .iter()
                .any(|e| e.title_key == "delegation.log.playerRenewed")
        );
    }

    /// Given a proposed renewal the manager declines, when the duty runs again the next week,
    /// then the same case is not put to him again.
    #[test]
    fn a_declined_renewal_is_not_proposed_again_the_next_week() {
        let mut g = game(Some(90), 6);
        g.delegation.apply_mode(DelegationMode::Approval);
        expiring_player(&mut g, "s14");
        player_renewals(&mut g, "team1");
        let id = g.delegation.proposals[0].id.clone();
        super::super::delegation::resolve_proposal(&mut g, &id, false).unwrap();

        g.clock.current_date += Duration::days(7);
        player_renewals(&mut g, "team1");
        assert!(g.delegation.proposals.is_empty(), "the answer was no");

        g.clock.current_date += Duration::days(21);
        player_renewals(&mut g, "team1");
        assert_eq!(
            g.delegation.proposals.len(),
            1,
            "a month on, it is a new case"
        );
    }

    /// Given the same renewal but a wage ceiling below the ask, when the duty runs, then the
    /// contract is left alone and a proposal waits for the manager; approving it renews him.
    #[test]
    fn a_renewal_above_the_wage_limit_is_proposed_not_made() {
        let mut g = game(Some(90), 6);
        full(&mut g);
        g.delegation.limits.max_player_wage = 1;
        expiring_player(&mut g, "s14");
        let before = g
            .players
            .iter()
            .find(|p| p.id == "s14")
            .unwrap()
            .contract_end()
            .map(str::to_owned);
        player_renewals(&mut g, "team1");
        let unchanged = g
            .players
            .iter()
            .find(|p| p.id == "s14")
            .unwrap()
            .contract_end()
            .map(str::to_owned);
        assert_eq!(before, unchanged, "nothing may be signed above the limit");
        assert_eq!(g.delegation.proposals.len(), 1);
        let id = g.delegation.proposals[0].id.clone();
        super::super::delegation::resolve_proposal(&mut g, &id, true).unwrap();
        let after = g
            .players
            .iter()
            .find(|p| p.id == "s14")
            .unwrap()
            .contract_end()
            .map(str::to_owned);
        assert_ne!(before, after);
        assert_eq!(
            g.delegation.log.last().unwrap().outcome,
            DecisionOutcome::Approved
        );
    }
}
