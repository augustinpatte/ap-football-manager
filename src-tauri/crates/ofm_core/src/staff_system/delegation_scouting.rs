//! The scouting duty: the scouts the club employs are put to work, and what they find is put on
//! the shortlist, by whoever the manager put in charge (the assistant, or the head of scouting).
//!
//! The person in charge only decides *where to look*; the report itself is written by the scout
//! he sends, with that scout's judging, so a poor scout under an excellent head of scouting still
//! writes a poor report (his department lifts him a few points, `effective_judging`).
//!
//! The in-charge reads the world as the club does, not as it is: he ranks candidates by the
//! public rating and by what the squad model says they would add *as rated*, never by a hidden
//! potential. Order of work, one pass a run:
//!
//! 1. players the manager shortlisted that the club has no current report on (the manager asked);
//! 2. players for the positions the squad model says are weak, or, when it names none, the most
//!    precocious youngsters the club can afford.
//!
//! The reports that arrive are then reviewed once each: a player who would start or cover for the
//! side, whose fee the club can pay and whose wage its policy allows, goes onto the shortlist.
//! Shortlisting spends nothing, so it needs no approval; buying him is the recruitment duty's
//! business, under its own approvals.

use std::collections::HashSet;

use domain::delegation::Duty;
use domain::player::{Player, Position};
use domain::staff::{StaffPost, StaffRole};

use super::assistant::{self, RecKind};
use super::delegation::{date, log, param, stamp};
use super::profile::team_staff;
use crate::game::Game;
use crate::scouting_view::{ReportStatus, SquadImpact, targets_view};
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;

/// Candidates the squad model is asked about per run: the best by public rating among those that
/// pass the cheap tests. The model prices each one, so it is not asked about the whole world.
const CANDIDATES_ASKED: usize = 40;

/// Oldest a speculative target is when the squad names no weak position: the duty looks for
/// talent then, not for a veteran.
const SPECULATIVE_MAX_AGE: i32 = 21;

/// Oldest a target is for a position the squad is short in.
const NEED_MAX_AGE: i32 = 30;

/// Reports the in-charge adds to the shortlist in one run.
const SHORTLIST_PER_RUN: usize = 2;

pub(super) fn run(game: &mut Game, team_id: &str, rating: f64) {
    shortlist_fresh_fits(game);
    send_idle_scouts(game, team_id, rating);
}

fn idle_scouts(game: &Game, team_id: &str) -> Vec<String> {
    let busy: HashSet<&str> = game
        .scouting_assignments
        .iter()
        .map(|a| a.scout_id.as_str())
        .chain(
            game.youth_scouting_assignments
                .iter()
                .map(|a| a.scout_id.as_str()),
        )
        .collect();
    team_staff(game, team_id)
        .iter()
        .filter(|s| s.role == StaffRole::Scout && s.post() != StaffPost::Analyst)
        .filter(|s| !busy.contains(s.id.as_str()))
        .map(|s| s.id.clone())
        .collect()
}

fn weak_groups(game: &Game, team_id: &str, rating: f64) -> Vec<Position> {
    assistant::recommendations_read_by(game, team_id, Some(rating))
        .iter()
        .filter(|r| r.kind == RecKind::WeakPosition)
        .filter_map(|r| r.params.iter().find(|(k, _)| k == "position"))
        .filter_map(|(_, v)| match v.as_str() {
            "Goalkeeper" => Some(Position::Goalkeeper),
            "Defender" => Some(Position::Defender),
            "Midfielder" => Some(Position::Midfielder),
            "Forward" => Some(Position::Forward),
            _ => None,
        })
        .collect()
}

fn send_idle_scouts(game: &mut Game, team_id: &str, rating: f64) {
    let idle = idle_scouts(game, team_id);
    if idle.is_empty() {
        return;
    }
    let targets = pick_targets(game, team_id, rating, idle.len());
    for (scout_id, (player_id, name)) in idle.iter().zip(targets) {
        if crate::scouting::send_scout(game, scout_id, &player_id).is_ok() {
            log(
                game,
                Duty::Scouting,
                "delegation.log.scoutSent",
                vec![param("player", name)],
            );
        }
    }
}

/// Up to `wanted` players to send scouts to, best first: the manager's shortlist that lacks a
/// current report, then the squad's needs.
fn pick_targets(game: &Game, team_id: &str, rating: f64, wanted: usize) -> Vec<(String, String)> {
    let watched: HashSet<&str> = game
        .scouting_assignments
        .iter()
        .map(|a| a.player_id.as_str())
        .collect();
    let season_start = game.season_context.season_start.as_deref();
    let has_current_report = |id: &str| {
        game.scouting_dossier
            .current_report_on(id, season_start)
            .is_some()
    };
    let eligible = |p: &Player| {
        p.team_id.as_deref().is_some_and(|t| t != team_id)
            && !p.retired
            && !watched.contains(p.id.as_str())
            && !has_current_report(&p.id)
    };

    let mut picks: Vec<(String, String)> = game
        .scouting_dossier
        .shortlist
        .iter()
        .filter_map(|entry| game.players.iter().find(|p| p.id == entry.player_id))
        .filter(|p| eligible(p))
        .map(|p| (p.id.clone(), p.match_name.clone()))
        .collect();
    picks.truncate(wanted);
    if picks.len() >= wanted {
        return picks;
    }

    let chosen: HashSet<String> = picks.iter().map(|(id, _)| id.clone()).collect();
    for player in squad_candidates(game, team_id, rating, &eligible) {
        if picks.len() >= wanted {
            break;
        }
        if !chosen.contains(&player.id) {
            picks.push((player.id.clone(), player.match_name.clone()));
        }
    }
    picks
}

/// Players worth a look for the squad's needs, best first, judged on what the club can see.
fn squad_candidates<'g>(
    game: &'g Game,
    team_id: &str,
    rating: f64,
    eligible: &dyn Fn(&Player) -> bool,
) -> Vec<&'g Player> {
    let weak = weak_groups(game, team_id, rating);
    let budget = game
        .teams
        .iter()
        .find(|t| t.id == team_id)
        .map_or(0, |t| t.transfer_budget.max(0)) as u64;
    let today = date(game);
    let max_age = if weak.is_empty() {
        SPECULATIVE_MAX_AGE
    } else {
        NEED_MAX_AGE
    };
    let mut pool: Vec<(&Player, i32)> = game
        .players
        .iter()
        .filter(|p| eligible(p))
        .filter(|p| p.market_value <= budget.max(1))
        .filter(|p| weak.is_empty() || weak.contains(&p.natural_position.to_group_position()))
        .filter_map(|p| {
            let age = domain::age::age_on(today, &p.date_of_birth)?;
            (age <= max_age).then_some((p, age))
        })
        .collect();
    // With a gap to fill the public rating is what the club goes on; without one, a young player
    // who is already rated well for his age is the one worth a trip.
    let public_score = |p: &Player, age: i32| {
        let precocity = if weak.is_empty() {
            (SPECULATIVE_MAX_AGE - age).max(0) * 3
        } else {
            0
        };
        i32::from(p.ovr) + precocity
    };
    pool.sort_by(|(a, aa), (b, ba)| {
        public_score(b, *ba)
            .cmp(&public_score(a, *aa))
            .then(a.id.cmp(&b.id))
    });
    pool.truncate(CANDIDATES_ASKED);

    let Some(squad) = ClubSquad::of(game, team_id, Appetite::NEUTRAL) else {
        return pool.into_iter().map(|(p, _)| p).collect();
    };
    // What each would add if he is as good as he is rated: his potential is not known to the club.
    let mut priced: Vec<(&Player, f32)> = pool
        .into_iter()
        .map(|(p, _)| {
            let mut believed = crate::youth_pathway::as_senior(p);
            believed.potential = believed.ovr;
            let gain = squad.signing(&believed, 0.0).map_or(0.0, |m| m.gain);
            (p, gain)
        })
        .collect();
    priced.sort_by(|(a, ga), (b, gb)| {
        gb.total_cmp(ga)
            .then(b.ovr.cmp(&a.ovr))
            .then(a.id.cmp(&b.id))
    });
    priced.into_iter().map(|(p, _)| p).collect()
}

/// Looks at the reports delivered since the last review, once each, and shortlists the players
/// who would help the side at a price the club can pay.
fn shortlist_fresh_fits(game: &mut Game) {
    let reviewed_through = game.scouting_dossier.delegate_reviewed_through.clone();
    // The first review of a career, or of a save from before this duty kept a date, starts today:
    // reports the manager has already read are not new to him.
    if reviewed_through.is_empty() {
        game.scouting_dossier.delegate_reviewed_through = stamp(game);
        return;
    }
    let fresh: Vec<String> = game
        .scouting_dossier
        .reports
        .iter()
        .filter(|r| r.scouted_on.as_str() > reviewed_through.as_str())
        .map(|r| r.player_id.clone())
        .collect();
    game.scouting_dossier.delegate_reviewed_through = stamp(game);
    if fresh.is_empty() {
        return;
    }
    let fits: Vec<(String, String)> = targets_view(game, &fresh)
        .into_iter()
        .filter(|v| !v.shortlisted && v.status == ReportStatus::Current)
        .filter(|v| {
            matches!(
                v.squad_impact,
                Some(SquadImpact::Starter | SquadImpact::Cover)
            )
        })
        .filter(|v| v.fee_affordable != Some(false) && v.wage_permitted)
        .map(|v| (v.player_id, v.full_name))
        .take(SHORTLIST_PER_RUN)
        .collect();
    for (player_id, name) in fits {
        if crate::scouting_dossier::shortlist_add(game, &player_id).is_ok() {
            log(
                game,
                Duty::Scouting,
                "delegation.log.scoutShortlisted",
                vec![param("player", name)],
            );
        }
    }
}
