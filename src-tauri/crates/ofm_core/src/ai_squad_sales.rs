//! AI clubs decide which of their players they are willing to sell.
//!
//! The transfer market asks of every player whether he is for sale: the player's own club answers
//! by listing him. The player does it by hand; until this module no AI club ever did, so the
//! market had a buyer for every need and a seller for none — the players an AI club would have let
//! go were the ones that happened to be generated listed, and a world built from a save had none.
//!
//! On its weekly review day a club lists a player when it has a reason a club has:
//!
//! - **surplus**: he is deeper in the squad than the club needs in his position — behind the
//!   starters of his group and the cover [`DESIRED_DEPTH`] asks for;
//! - **leaving anyway**: his contract runs out within the year and the club has decided not to
//!   renew him (`ai_contracts::keeps`) — better a fee than nothing;
//! - **the books**: the club is over its wage budget or in debt, and he is among the best paid of
//!   those who are not first choice.
//!
//! and takes him off the list when none of those holds any more. A club keeps at most
//! [`MAX_LISTED`] players on the list. Nothing here sells anybody: the market's own sweep finds
//! the buyer (who must need him and be able to pay), and the squad floor decides whether the club
//! can let him go.

use crate::ai_contracts::{SquadFacts, keeps_given};
use crate::game::Game;
use crate::squad_floor::{STARTERS_PER_GROUP as STARTERS, group_index};
use domain::player::{Player, SquadRole};
use domain::team::Team;

/// Seniors a club wants in each group, `[GK, DEF, MID, FWD]`: the starters of a back four, a
/// midfield four and a front two, with cover — 22 in all.
const DESIRED_DEPTH: [usize; 4] = [3, 7, 7, 5];

/// Players a club has on the list at once.
const MAX_LISTED: usize = 3;

/// A contract with this many days or fewer left, that the club will not renew, is one it sells.
const LEAVING_HORIZON_DAYS: i64 = 365;

/// Run the listing decisions of every AI club whose review falls on this weekday.
pub(crate) fn apply_ai_listing_decisions(game: &mut Game, weekday_num: u32) {
    let current_date = game.clock.current_date.date_naive();
    for team_id in crate::ai_tactics::ai_clubs_reviewing_on(game, weekday_num) {
        let Some(team) = game.teams.iter().find(|team| team.id == team_id).cloned() else {
            continue;
        };
        let previously_listed: std::collections::HashSet<String> = game
            .players
            .iter()
            .filter(|player| {
                player.transfer_listed && player.team_id.as_deref() == Some(team_id.as_str())
            })
            .map(|player| player.id.clone())
            .collect();
        let decisions = list_decisions(game, &team, current_date);
        for (index, listed) in decisions {
            game.players[index].transfer_listed = listed;
        }
        release_unsold_surplus(game, &team, &previously_listed, current_date);
        // Young reserves for loan, as the opening market lists them: a loan returns its player and
        // the club lists the next.
        crate::transfers::top_up_loan_listings(game, &team_id);
        // And the loans a squad's players call for: prospects not playing, spares nobody bought.
        crate::transfers::list_loans_by_status(game, &team_id);
    }
}

/// A club this far over its wage budget (per cent) pays a player off rather than carry him.
const RELEASE_OVER_BUDGET_PCT: i64 = 115;

/// The most of its cash (per cent) a club will pay in severance to be rid of one player.
const RELEASE_MAX_CASH_SHARE_PCT: i64 = 2;

/// A club that has been over its budget and has had a player on the list since its last review,
/// unsold, pays him off: the one way out of a long contract nobody will buy. It is the last
/// resort the manager's own termination is, for the same price (the weeks left on the contract at
/// his wage), and it takes one player a review, the dearest of those it listed, who is kept by
/// no rule of the squad floor and whose contract has more than a year to run (a shorter one
/// expires on its own sooner than the saving pays back the severance). A club in debt cannot
/// afford it and does not.
fn release_unsold_surplus(
    game: &mut Game,
    team: &Team,
    previously_listed: &std::collections::HashSet<String>,
    current_date: chrono::NaiveDate,
) -> Option<String> {
    if previously_listed.is_empty() || team.finance <= 0 || team.wage_budget <= 0 {
        return None;
    }
    let bill = crate::finances::calc_wages(game, &team.id);
    if bill * 100 <= team.wage_budget * RELEASE_OVER_BUDGET_PCT {
        return None;
    }
    let mut candidates: Vec<usize> = game
        .players
        .iter()
        .enumerate()
        .filter(|(_, player)| {
            player.transfer_listed
                && previously_listed.contains(&player.id)
                && player.team_id.as_deref() == Some(team.id.as_str())
                && player.active_loan.is_none()
                && player.wage() > 0
                && crate::contracts::contract_days_remaining(player.contract_end(), current_date)
                    .is_some_and(|days| days > 365)
        })
        .map(|(index, _)| index)
        .collect();
    candidates.sort_by(|&a, &b| {
        let (a, b) = (&game.players[a], &game.players[b]);
        b.wage()
            .cmp(&a.wage())
            .then_with(|| a.ovr.cmp(&b.ovr))
            .then_with(|| a.id.cmp(&b.id))
    });
    for index in candidates {
        let player = &game.players[index];
        let severance = crate::contracts::termination_severance_cost(player, current_date);
        if severance * 100 > team.finance * RELEASE_MAX_CASH_SHARE_PCT {
            continue;
        }
        let player_id = player.id.clone();
        if crate::squad_floor::ensure_departure_keeps_floor(game, &player_id).is_err() {
            continue;
        }
        if crate::finances::post(
            game,
            &team.id,
            -severance,
            crate::finances::CashKind::ContractTermination,
            current_date,
        )
        .is_err()
        {
            continue;
        }
        game.players[index].transfer_listed = false;
        crate::contracts::release_player_contract(
            game,
            index,
            crate::contracts::ContractReleaseReason::ManagerTermination {
                severance_cost: severance,
            },
        );
        return Some(player_id);
    }
    None
}

/// Whom the club lists and whom it takes off the list, as (index in `game.players`, listed).
/// Only the changes.
fn list_decisions(game: &Game, team: &Team, current_date: chrono::NaiveDate) -> Vec<(usize, bool)> {
    let seniors: Vec<usize> = game
        .players
        .iter()
        .enumerate()
        .filter(|(_, player)| player.team_id.as_deref() == Some(team.id.as_str()))
        .filter(|(_, player)| player.squad_role == SquadRole::Senior)
        .filter(|(_, player)| player.active_loan.is_none())
        .map(|(index, _)| index)
        .collect();

    // Rank in his group by rating: 0 is the best. Ties: younger first, then id, so the answer
    // does not depend on the order players are stored in.
    let mut ranks: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for group in 0..4 {
        let mut members: Vec<usize> = seniors
            .iter()
            .copied()
            .filter(|&index| group_index(&game.players[index].position) == group)
            .collect();
        members.sort_by(|&a, &b| {
            let (a, b) = (&game.players[a], &game.players[b]);
            b.ovr
                .cmp(&a.ovr)
                .then_with(|| b.date_of_birth.cmp(&a.date_of_birth))
                .then_with(|| a.id.cmp(&b.id))
        });
        for (rank, index) in members.into_iter().enumerate() {
            ranks.insert(index, rank);
        }
    }

    let bill = crate::finances::calc_wages(game, &team.id);
    let in_trouble = bill > team.wage_budget || team.finance < 0;
    let facts = SquadFacts::of(game, &team.id);
    let mut sellable: Vec<(usize, Reason)> = seniors
        .iter()
        .copied()
        .filter_map(|index| {
            reason_to_sell(
                &facts,
                &game.players[index],
                ranks[&index],
                in_trouble,
                current_date,
            )
            .map(|reason| (index, reason))
        })
        .collect();
    // The ones he would most like to see go first: surplus beyond need, then the leaving, then the
    // dear; within each the lowest rated, then the dearest. Stable on id.
    sellable.sort_by(|&(a, ra), &(b, rb)| {
        let (a, b) = (&game.players[a], &game.players[b]);
        ra.cmp(&rb)
            .then_with(|| a.ovr.cmp(&b.ovr))
            .then_with(|| b.wage().cmp(&a.wage()))
            .then_with(|| a.id.cmp(&b.id))
    });
    let wanted: Vec<usize> = sellable
        .into_iter()
        .take(MAX_LISTED)
        .map(|(index, _)| index)
        .collect();

    let mut changes = Vec::new();
    for &index in &seniors {
        let should_be_listed = wanted.contains(&index);
        if game.players[index].transfer_listed != should_be_listed {
            changes.push((index, should_be_listed));
        }
    }
    changes
}

/// Why a club would sell this player, most compelling first (the order is the sort order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reason {
    Surplus,
    LeavingAnyway,
    TheBooks,
}

fn reason_to_sell(
    facts: &SquadFacts,
    player: &Player,
    rank_in_group: usize,
    in_trouble: bool,
    current_date: chrono::NaiveDate,
) -> Option<Reason> {
    let group = group_index(&player.position);
    if rank_in_group >= DESIRED_DEPTH[group] {
        return Some(Reason::Surplus);
    }
    // Starters are not for sale for being unwanted: a club sells a starter only for the books.
    if rank_in_group < STARTERS[group] {
        return None;
    }
    let days_left = crate::contracts::contract_days_remaining(player.contract_end(), current_date);
    if days_left.is_some_and(|days| days <= LEAVING_HORIZON_DAYS)
        && !keeps_given(facts, player, current_date)
    {
        return Some(Reason::LeavingAnyway);
    }
    if in_trouble && rank_in_group > STARTERS[group] && player.wage() > 0 {
        return Some(Reason::TheBooks);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use chrono::{Duration, TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::{PlayerAttributes, Position};

    const TODAY: (i32, u32, u32) = (2026, 8, 3);

    fn today() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(TODAY.0, TODAY.1, TODAY.2).unwrap()
    }

    fn attrs(level: u8) -> PlayerAttributes {
        PlayerAttributes {
            pace: level,
            stamina: level,
            strength: level,
            agility: level,
            passing: level,
            shooting: level,
            tackling: level,
            dribbling: level,
            defending: level,
            positioning: level,
            vision: level,
            decisions: level,
            composure: level,
            aggression: level,
            teamwork: level,
            leadership: level,
            handling: level,
            reflexes: level,
            aerial: level,
        }
    }

    fn player(id: &str, position: Position, ovr: u8, age: i32, days_left: i64) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            format!("{}-01-01", TODAY.0 - age),
            "England".to_string(),
            position,
            attrs(ovr),
        );
        player.team_id = Some("ai".to_string());
        player.ovr = ovr;
        player.potential = ovr;
        player.stage_wage(1_000);
        player.stage_contract_end(Some(
            (today() + Duration::days(days_left))
                .format("%Y-%m-%d")
                .to_string(),
        ));
        player
    }

    /// A club with `per_group` seniors, rated 60, aged 26, contracts running long.
    fn world(per_group: [usize; 4]) -> Game {
        let clock = GameClock::new(
            Utc.with_ymd_and_hms(TODAY.0, TODAY.1, TODAY.2, 12, 0, 0)
                .unwrap(),
        );
        let mut manager = Manager::new(
            "mgr".to_string(),
            "T".to_string(),
            "M".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("user".to_string());
        let mut team = Team::new(
            "ai".to_string(),
            "AI".to_string(),
            "AI".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Ground".to_string(),
            20_000,
        );
        team.wage_budget = 10_000_000;
        team.finance = 100_000_000;
        let mut players = Vec::new();
        for (group, count) in [
            Position::Goalkeeper,
            Position::Defender,
            Position::Midfielder,
            Position::Forward,
        ]
        .into_iter()
        .zip(per_group)
        {
            for i in 0..count {
                players.push(player(&format!("{group:?}{i}"), group.clone(), 60, 26, 900));
            }
        }
        Game::new(clock, manager, vec![team], players, vec![], vec![])
    }

    fn listed(game: &Game) -> Vec<String> {
        let mut ids: Vec<String> = game
            .players
            .iter()
            .filter(|p| p.transfer_listed)
            .map(|p| p.id.clone())
            .collect();
        ids.sort();
        ids
    }

    fn decide(game: &mut Game) {
        let team = game.teams[0].clone();
        for (index, flag) in list_decisions(game, &team, today()) {
            game.players[index].transfer_listed = flag;
        }
    }

    /// Given a club with exactly the depth it wants in every group
    /// When it reviews its squad
    /// Then it lists nobody.
    #[test]
    fn a_club_with_the_depth_it_wants_lists_nobody() {
        let mut game = world(DESIRED_DEPTH);
        decide(&mut game);
        assert!(listed(&game).is_empty());
    }

    /// Given a club two midfielders deeper than it wants, the weakest of them rated 50
    /// When it reviews its squad
    /// Then the two it ranks last are listed, the weakest included, and nobody else.
    #[test]
    fn a_club_lists_what_it_has_beyond_the_depth_it_wants() {
        let mut game = world([3, 7, 9, 5]);
        game.players
            .iter_mut()
            .find(|p| p.id == "Midfielder8")
            .unwrap()
            .ovr = 50;
        decide(&mut game);
        let listed = listed(&game);
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&"Midfielder8".to_string()));
    }

    /// Given a club with a surplus in every group
    /// When it reviews its squad
    /// Then it lists no more than [`MAX_LISTED`].
    #[test]
    fn a_club_never_lists_more_than_the_limit() {
        let mut game = world([6, 12, 12, 9]);
        decide(&mut game);
        assert_eq!(listed(&game).len(), MAX_LISTED);
    }

    /// Given a listed player who is now needed (the squad lost depth)
    /// When the club reviews its squad
    /// Then he is taken off the list.
    #[test]
    fn a_player_the_club_needs_again_comes_off_the_list() {
        let mut game = world(DESIRED_DEPTH);
        game.players
            .iter_mut()
            .find(|p| p.id == "Defender6")
            .unwrap()
            .transfer_listed = true;
        decide(&mut game);
        assert!(listed(&game).is_empty());
    }

    /// Given a first-choice player on a contract that runs out within the year
    /// When the club reviews its squad
    /// Then he is not listed: a starter is sold only for the books.
    #[test]
    fn a_starter_is_not_listed_for_leaving_anyway() {
        let mut game = world(DESIRED_DEPTH);
        let star = game
            .players
            .iter_mut()
            .find(|p| p.id == "Forward0")
            .unwrap();
        star.ovr = 80;
        star.stage_contract_end(Some(
            (today() + Duration::days(100))
                .format("%Y-%m-%d")
                .to_string(),
        ));
        game.players
            .iter_mut()
            .find(|p| p.id == "Forward1")
            .unwrap()
            .ovr = 70;
        decide(&mut game);
        assert!(listed(&game).is_empty());
    }

    /// Given a club over its wage budget
    /// When it reviews its squad
    /// Then it lists its dearest players who are not first choice, up to the limit.
    #[test]
    fn a_club_over_its_wage_budget_lists_its_dearest_reserves() {
        let mut game = world(DESIRED_DEPTH);
        game.teams[0].wage_budget = 1_000;
        for id in ["Midfielder5", "Midfielder6", "Defender5"] {
            game.players
                .iter_mut()
                .find(|p| p.id == id)
                .unwrap()
                .stage_wage(5_000);
        }
        // Make the three dearest also the lowest ranked in their groups.
        for id in ["Midfielder5", "Midfielder6", "Defender5"] {
            game.players.iter_mut().find(|p| p.id == id).unwrap().ovr = 55;
        }
        decide(&mut game);
        let listed = listed(&game);
        assert_eq!(listed.len(), MAX_LISTED);
        for id in ["Midfielder5", "Midfielder6", "Defender5"] {
            assert!(
                listed.contains(&id.to_string()),
                "{id} not listed: {listed:?}"
            );
        }
    }

    /// Given a club far over its wage budget with a listed, unsold player on a long contract
    /// When it reviews its squad
    /// Then it pays him off: he leaves, and the severance (weeks left x wage) comes out of its cash.
    #[test]
    fn a_club_far_over_budget_pays_off_a_player_it_could_not_sell() {
        let mut game = world(DESIRED_DEPTH);
        game.teams[0].wage_budget = 1_000;
        let dear = game
            .players
            .iter_mut()
            .find(|p| p.id == "Midfielder5")
            .unwrap();
        dear.stage_wage(2_000);
        dear.transfer_listed = true;
        let severance = crate::contracts::termination_severance_cost(dear, today());
        let cash = game.teams[0].finance;
        let team = game.teams[0].clone();
        let listed: std::collections::HashSet<String> = ["Midfielder5".to_string()].into();

        let released = release_unsold_surplus(&mut game, &team, &listed, today());

        assert_eq!(released.as_deref(), Some("Midfielder5"));
        let gone = game.players.iter().find(|p| p.id == "Midfielder5").unwrap();
        assert!(gone.team_id.is_none() && !gone.transfer_listed);
        assert_eq!(game.teams[0].finance, cash - severance);
    }

    /// Given a club within its budget, or a player listed only today, or a contract with a year
    /// or less to run, or a club in debt
    /// When it reviews its squad
    /// Then nobody is paid off.
    #[test]
    fn nobody_is_paid_off_without_the_reasons() {
        let listed: std::collections::HashSet<String> = ["Midfielder5".to_string()].into();
        let fresh = |budget: i64, days_left: i64, cash: i64| {
            let mut game = world(DESIRED_DEPTH);
            game.teams[0].wage_budget = budget;
            game.teams[0].finance = cash;
            let dear = game
                .players
                .iter_mut()
                .find(|p| p.id == "Midfielder5")
                .unwrap();
            dear.stage_wage(2_000);
            dear.transfer_listed = true;
            dear.stage_contract_end(Some(
                (today() + Duration::days(days_left))
                    .format("%Y-%m-%d")
                    .to_string(),
            ));
            game
        };
        let attempt = |mut game: Game, listed: &std::collections::HashSet<String>| {
            let team = game.teams[0].clone();
            release_unsold_surplus(&mut game, &team, listed, today())
        };
        assert_eq!(attempt(fresh(10_000_000, 900, 100_000_000), &listed), None);
        assert_eq!(
            attempt(fresh(1_000, 900, 100_000_000), &Default::default()),
            None
        );
        assert_eq!(attempt(fresh(1_000, 300, 100_000_000), &listed), None);
        assert_eq!(attempt(fresh(1_000, 900, -1), &listed), None);
        assert_eq!(
            attempt(fresh(1_000, 900, 10_000), &listed),
            None,
            "severance is over the share of cash"
        );
    }

    /// Given a prospect on the bench of a club that has more than it needs in his group
    /// When the club reviews its loans
    /// Then he is put on the loan list, and nobody of the first eleven is.
    #[test]
    fn a_prospect_who_is_not_playing_is_put_on_the_loan_list() {
        let mut game = world([3, 7, 7, 7]);
        game.season_context.transfer_window.status = domain::season::TransferWindowStatus::Open;
        // The youngest forward, with room to grow and the lowest rating.
        let prospect = game
            .players
            .iter_mut()
            .find(|p| p.id == "Forward6")
            .unwrap();
        prospect.ovr = 50;
        prospect.potential = 70;
        prospect.date_of_birth = format!("{}-01-01", TODAY.0 - 19);
        let listed = crate::transfers::list_loans_by_status(&mut game, "ai");
        assert!(listed >= 1);
        let on_list: Vec<&str> = game
            .players
            .iter()
            .filter(|p| p.loan_listed)
            .map(|p| p.id.as_str())
            .collect();
        assert!(on_list.contains(&"Forward6"), "{on_list:?}");
        assert!(on_list.len() <= 4);
        // With the window closed nothing is listed: no loan could start.
        for player in &mut game.players {
            player.loan_listed = false;
        }
        game.season_context.transfer_window.status = domain::season::TransferWindowStatus::Closed;
        assert_eq!(crate::transfers::list_loans_by_status(&mut game, "ai"), 0);
        game.season_context.transfer_window.status = domain::season::TransferWindowStatus::Open;
        crate::transfers::list_loans_by_status(&mut game, "ai");
        // Calling again adds nobody beyond the limit.
        crate::transfers::list_loans_by_status(&mut game, "ai");
        assert!(game.players.iter().filter(|p| p.loan_listed).count() <= 4);
    }

    /// Given a spare nobody has bought, late in the window
    /// When the club reviews its loans
    /// Then it offers him on loan too: a loan saves part of his wage when a sale cannot.
    #[test]
    fn a_spare_nobody_bought_is_offered_on_loan_as_the_window_closes() {
        let build = |days: i64| {
            let mut game = world([3, 7, 7, 8]);
            game.season_context.transfer_window.status = domain::season::TransferWindowStatus::Open;
            game.season_context.transfer_window.days_remaining = Some(days);
            let spare = game
                .players
                .iter_mut()
                .find(|p| p.id == "Forward7")
                .unwrap();
            spare.ovr = 50;
            spare.potential = 50;
            spare.date_of_birth = format!("{}-01-01", TODAY.0 - 30);
            spare.transfer_listed = true;
            crate::transfers::list_loans_by_status(&mut game, "ai");
            game.players
                .iter()
                .find(|p| p.id == "Forward7")
                .unwrap()
                .loan_listed
        };
        assert!(!build(30), "early on, the sale has its chance");
        assert!(build(2), "late, a loan is the way out");
    }
}
