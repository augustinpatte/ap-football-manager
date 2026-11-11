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
//!   starters of his group and the cover [`DESIRED_DEPTH`] asks for (and the side the club fields
//!   does not miss him);
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
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::impact::{REINVESTMENT_RATIO, STARTER_LOSS, SaleCase};
use crate::squad_intelligence::needs::NeedKind;
use crate::transfers::reinvestment_gain;
use domain::player::{Player, SquadRole};
use domain::team::Team;

use crate::transfers::DESIRED_DEPTH;

/// Players a club has on the list at once.
const MAX_LISTED: usize = 3;

/// A contract with this many days or fewer left, that the club will not renew, is one it sells.
const LEAVING_HORIZON_DAYS: i64 = 365;

/// Run the listing decisions of every AI club whose review falls on this weekday, sharing a day's
/// reading of every club's strategy with the other reviews that day (`transfers::profile_in`).
pub(crate) fn apply_ai_listing_decisions_with(
    game: &mut Game,
    weekday_num: u32,
    strategies: &mut Option<std::collections::HashMap<String, crate::transfers::ClubProfile>>,
) {
    let current_date = game.clock.current_date.date_naive();
    let reviewing = crate::ai_tactics::ai_clubs_reviewing_on(game, weekday_num);
    // Every wage bill, in one pass over the players. Asked per club it scanned all of them for
    // each one (`calc_wages`), which was the largest cost of this review in a full world. A club's
    // bill is read before the review touches its squad, and the only contract this pass ends is
    // one of the reviewing club's own, so no other club's bill moves under it.
    let bills = crate::finances::wage_bills_by_club(game);
    for team_id in reviewing.iter().cloned() {
        let Some(team) = game.teams.iter().find(|team| team.id == team_id).cloned() else {
            continue;
        };
        let bill = bills.get(&team_id).copied().unwrap_or(0);
        let previously_listed: std::collections::HashSet<String> = game
            .players
            .iter()
            .filter(|player| {
                player.transfer_listed && player.team_id.as_deref() == Some(team_id.as_str())
            })
            .map(|player| player.id.clone())
            .collect();
        let profile = crate::transfers::profile_in(game, strategies, &reviewing, &team_id);
        let decisions = list_decisions(game, &team, bill, current_date, profile);
        for (index, listed) in decisions {
            game.players[index].transfer_listed = listed;
        }
        release_unsold_surplus(game, &team, bill, &previously_listed, current_date);
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
pub(crate) fn release_unsold_surplus(
    game: &mut Game,
    team: &Team,
    bill: i64,
    previously_listed: &std::collections::HashSet<String>,
    current_date: chrono::NaiveDate,
) -> Option<String> {
    if previously_listed.is_empty() || team.finance <= 0 || team.wage_budget <= 0 {
        return None;
    }
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
fn list_decisions(
    game: &Game,
    team: &Team,
    bill: i64,
    current_date: chrono::NaiveDate,
    profile: crate::transfers::ClubProfile,
) -> Vec<(usize, bool)> {
    let wanted: Vec<usize> = ranked_sellable_for(game, team, bill, current_date, profile)
        .into_iter()
        .take(MAX_LISTED)
        .map(|candidate| candidate.index)
        .collect();
    let mut changes = Vec::new();
    for (index, player) in game.players.iter().enumerate() {
        let is_senior = player.team_id.as_deref() == Some(team.id.as_str())
            && player.squad_role == SquadRole::Senior
            && player.active_loan.is_none();
        if !is_senior {
            continue;
        }
        let should_be_listed = wanted.contains(&index);
        if player.transfer_listed != should_be_listed {
            changes.push((index, should_be_listed));
        }
    }
    changes
}

/// A player a club could put on the list, and why.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ListingCandidate {
    /// Where he stands in `game.players`.
    pub index: usize,
    pub reason: Reason,
    /// What the side the club fields would lose, net of a cheap replacement, in strength points.
    pub net_loss: f32,
}

/// The players a club would sell, the ones it would most like to see go first. The one definition
/// of "sellable": the AI clubs list from it, and so does the manager's assistant — each applies
/// its own policy to the answer. A player the side fielded cannot do without is never in it.
pub(crate) fn ranked_sellable(
    game: &Game,
    team: &Team,
    bill: i64,
    current_date: chrono::NaiveDate,
) -> Vec<ListingCandidate> {
    ranked_sellable_for(
        game,
        team,
        bill,
        current_date,
        crate::transfers::ClubProfile::NEUTRAL,
    )
}

/// [`ranked_sellable`] for a club with this strategy: a club contending holds on to the player a
/// sale would fund a replacement for (it asks more of the reinvestment), one that is small and
/// developing sells him on readily.
pub(crate) fn ranked_sellable_for(
    game: &Game,
    team: &Team,
    bill: i64,
    current_date: chrono::NaiveDate,
    profile: crate::transfers::ClubProfile,
) -> Vec<ListingCandidate> {
    let facts = SquadFacts::of_club(game, &team.id, profile);
    let seniors: Vec<usize> = facts
        .club()
        .iter()
        .copied()
        .filter(|&index| game.players[index].squad_role == SquadRole::Senior)
        .filter(|&index| game.players[index].active_loan.is_none())
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

    let in_trouble = bill > team.wage_budget || team.finance < 0;
    // What losing each of them would cost the side the club fields (Squad Intelligence): who is
    // never sold, and who is sold to fund a stronger side. The cost is the dear part of the
    // answer, so it is asked only where it can change it: of a player who is not the first choice
    // of his group (a club does not list those), and who could be listed by some rule at all: he
    // is beyond the depth the club plans for, or his contract is running out, or the club is in
    // trouble with its books, or it has a weakness pressing enough for a sale to mend.
    let pressing_need = facts.squad().is_some_and(|squad| {
        squad
            .needs()
            .iter()
            .any(|need| need.kind == NeedKind::Starter && need.severity >= PRESSING_NEED_SEVERITY)
    });
    let mut sellable: Vec<ListingCandidate> = seniors
        .iter()
        .copied()
        .filter_map(|index| {
            let player = &game.players[index];
            let group = group_index(&player.position);
            let first_choice = ranks[&index] < STARTERS[group];
            let listable_by_some_rule = ranks[&index] >= DESIRED_DEPTH[group]
                || in_trouble
                || pressing_need
                || crate::contracts::contract_days_remaining(player.contract_end(), current_date)
                    .is_some_and(|days| days <= LEAVING_HORIZON_DAYS);
            let case = if first_choice || !listable_by_some_rule {
                None
            } else {
                facts.sale_case(&player.id)
            };
            let funds = facts
                .squad()
                .zip(case.as_ref())
                .is_some_and(|(squad, case)| {
                    funds_a_bigger_need(squad, team, player, case, profile.ambition)
                });
            reason_to_sell(
                &facts,
                player,
                ranks[&index],
                case.as_ref(),
                funds,
                in_trouble,
                current_date,
            )
            .map(|reason| ListingCandidate {
                index,
                reason,
                net_loss: case.map_or(0.0, |case| case.net_loss),
            })
        })
        .collect();
    // The ones he would most like to see go first: surplus beyond need, then the leaving, then the
    // ones whose sale funds a stronger side, then the dear; within each the one the side misses
    // least, then the lowest rated, then the dearest. Stable on id.
    sellable.sort_by(|a, b| {
        let (pa, pb) = (&game.players[a.index], &game.players[b.index]);
        a.reason
            .cmp(&b.reason)
            .then_with(|| a.net_loss.total_cmp(&b.net_loss))
            .then_with(|| pa.ovr.cmp(&pb.ovr))
            .then_with(|| pb.wage().cmp(&pa.wage()))
            .then_with(|| pa.id.cmp(&pb.id))
    });
    sellable
}

/// A starter slot this many expected rating points below the club's own standard is a weakness a
/// sale could mend; under it, selling a player to fund an upgrade is not worth considering.
const PRESSING_NEED_SEVERITY: f32 = 4.0;

/// Why a club would sell this player, most compelling first (the order is the sort order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Reason {
    Surplus,
    LeavingAnyway,
    FundsAStrongerSide,
    TheBooks,
}

impl Reason {
    /// The reason as the history names it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Surplus => "surplus",
            Self::LeavingAnyway => "leaving",
            Self::FundsAStrongerSide => "funds",
            Self::TheBooks => "books",
        }
    }
}

/// Whether the money for this player would buy the club more than he is worth to its side, with
/// the sale worth a tenth of the transfer budget at least: a spare asset, not loose change.
fn funds_a_bigger_need(
    squad: &ClubSquad,
    team: &Team,
    player: &Player,
    case: &SaleCase,
    ambition: crate::transfers::Ambition,
) -> bool {
    !case.critical
        && player.market_value.saturating_mul(10)
            >= u64::try_from(team.transfer_budget).unwrap_or(0)
        && reinvestment_gain(squad, &player.id)
            >= REINVESTMENT_RATIO * reinvestment_demand(ambition) * case.net_loss
}

/// How much more or less of the money's worth a club asks before it sells a player to fund
/// another: a contender, playing for this season, wants a clear gain to part with a good one; a
/// small club that develops its own and lives on the fees is the readiest seller.
fn reinvestment_demand(ambition: crate::transfers::Ambition) -> f32 {
    match ambition {
        crate::transfers::Ambition::Contender => 1.5,
        crate::transfers::Ambition::Established => 1.0,
        crate::transfers::Ambition::Fighter => 0.75,
    }
}

fn reason_to_sell(
    facts: &SquadFacts,
    player: &Player,
    rank_in_group: usize,
    case: Option<&SaleCase>,
    funds_a_need: bool,
    in_trouble: bool,
    current_date: chrono::NaiveDate,
) -> Option<Reason> {
    let group = group_index(&player.position);
    // The side the club fields misses him badly and nobody replaces him, even on the cheap market:
    // he is not for sale for any reason of the books or of planning.
    if case.is_some_and(|case| case.critical) {
        return None;
    }
    if rank_in_group >= DESIRED_DEPTH[group] {
        return Some(Reason::Surplus);
    }
    // A first choice is not for sale for being unwanted: a club sells one only to fund a stronger
    // side. A player the model cannot place is judged by his rank, as before.
    let is_starter = case.map_or(rank_in_group < STARTERS[group], |case| {
        case.net_loss >= STARTER_LOSS
    });
    if is_starter {
        return funds_a_need.then_some(Reason::FundsAStrongerSide);
    }
    let days_left = crate::contracts::contract_days_remaining(player.contract_end(), current_date);
    if days_left.is_some_and(|days| days <= LEAVING_HORIZON_DAYS)
        && !keeps_given(facts, player, current_date)
    {
        return Some(Reason::LeavingAnyway);
    }
    if funds_a_need {
        return Some(Reason::FundsAStrongerSide);
    }
    if in_trouble && player.wage() > 0 {
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
        for (index, flag) in list_decisions(
            game,
            &team,
            crate::finances::calc_wages(game, &team.id),
            today(),
            crate::transfers::ClubProfile::NEUTRAL,
        ) {
            game.players[index].transfer_listed = flag;
        }
    }

    /// Given a club with a good midfielder a tenth of its transfer budget is worth, whom selling
    ///   would cost the side a little (not so much that he is critical) and would pay to mend its
    ///   weakest places three or four times over
    /// When the rule that decides whether the sale funds a replacement is asked for each of the
    ///   three ambitions, on the squad model's own numbers
    /// Then the small developing club and the established club take the sale and the contender,
    ///   whose bar is half as high again, does not.
    #[test]
    fn the_reinvestment_rule_asks_a_contender_for_more_on_the_squad_models_numbers() {
        use crate::transfers::Ambition::{Contender, Established, Fighter};
        let mut game = world([3, 7, 7, 5]);
        for player in &mut game.players {
            if player.id == "Forward0" || player.id == "Forward1" {
                player.ovr = 30;
                player.potential = 30;
                player.attributes = attrs(30);
            }
            if player.id == "Midfielder0" {
                player.ovr = 64;
                player.potential = 64;
                player.attributes = attrs(64);
                player.market_value = 6_000_000;
            }
        }
        game.teams[0].transfer_budget = 20_000_000;
        game.teams[0].finance = 100_000_000;
        let team = game.teams[0].clone();
        let facts = SquadFacts::of_club(&game, &team.id, crate::transfers::ClubProfile::NEUTRAL);
        let squad = facts.squad().expect("the squad model reads the club");
        let case = facts
            .sale_case("Midfielder0")
            .expect("the sale can be costed");
        let player = game
            .players
            .iter()
            .find(|player| player.id == "Midfielder0")
            .expect("he is in the squad");
        let ratio = reinvestment_gain(squad, "Midfielder0") / case.net_loss;
        assert!(
            !case.critical,
            "the fixture's sale is not one the side cannot do without"
        );
        assert!(
            ratio > REINVESTMENT_RATIO * reinvestment_demand(Fighter)
                && ratio < REINVESTMENT_RATIO * reinvestment_demand(Contender),
            "the fixture has to sit between the two bars: ratio {ratio}"
        );

        let takes = |ambition| funds_a_bigger_need(squad, &team, player, &case, ambition);

        assert!(takes(Fighter), "a small developing club takes it");
        assert!(takes(Established), "an established club takes it");
        assert!(!takes(Contender), "a contender asks for more");
    }

    /// Given the three ambitions
    /// When each asks what the money has to be worth before a sale funds another player
    /// Then a contender asks more than an established club and a fighter least, and the neutral
    /// club asks exactly what it always did.
    #[test]
    fn a_contender_asks_more_of_a_sale_than_a_club_that_lives_on_its_fees() {
        use crate::transfers::Ambition::{Contender, Established, Fighter};
        assert!(reinvestment_demand(Contender) > reinvestment_demand(Established));
        assert!(reinvestment_demand(Established) > reinvestment_demand(Fighter));
        assert!((reinvestment_demand(Established) - 1.0).abs() < f32::EPSILON);
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

        let bill = crate::finances::calc_wages(&game, &team.id);

        let released = release_unsold_surplus(&mut game, &team, bill, &listed, today());

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
            let bill = crate::finances::calc_wages(&game, &team.id);
            release_unsold_surplus(&mut game, &team, bill, listed, today())
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
