//! The daily sweep: who wants whom, and which of them gets to ask.
//!
//! This is the top of the call graph. It scores every player once, sorts the attractive ones,
//! then walks the buying clubs and lets each open at most one approach — subject to the
//! budget in `lifecycle`, which is what keeps a listed player from being buried in offers.

use super::*;

use crate::squad_floor::group_index as position_group_index;
use chrono::Datelike;

/// How many buyer turns the sweep has taken in this process: a measure of the market's work, read
/// by the harness. It never feeds a decision.
static BUYER_EVALUATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn market_buyer_evaluations() -> u64 {
    BUYER_EVALUATIONS.load(std::sync::atomic::Ordering::Relaxed)
}

/// The age from which an AI club will not pay a transfer fee for a player: his career is nearly
/// over and the fee buys no resale. A goalkeeper's lasts longer. Free agents and loans are not
/// bound by it: they cost no fee.
pub(crate) fn retirement_horizon_age(position_group_index: usize) -> i32 {
    if position_group_index == 0 { 36 } else { 33 }
}

/// Whether a club has a realistic reason to pursue a target: it isn't far below
/// the player's current club in stature, and it isn't already overloaded in the
/// player's position group.
pub(crate) fn buyer_has_genuine_interest(
    buyer_reputation: u32,
    owner_reputation: u32,
    buyer_position_depth: usize,
) -> bool {
    let reputation_deficit = owner_reputation as i32 - buyer_reputation as i32;
    reputation_deficit <= MAX_BUYER_REPUTATION_DEFICIT
        && buyer_position_depth < POSITION_GROUP_SURPLUS_THRESHOLD
}
/// Whether a target would make a buyer's squad better where it matters: the club is thin in his
/// group (under the floor plus the planning margin) or he is rated above its first cover — the
/// player just behind the starters. `levels` is the buyer's seniors in that group, best first.
///
/// Buying whoever scores best on appeal, whatever the buyer already has, sent clubs after players
/// worse than the ones on their bench; a transfer should come from a need.
pub(crate) fn target_fills_a_need(levels: &[u8], group: usize, target_ovr: u8) -> bool {
    use crate::squad_floor::{
        MIN_PLAYERS_PER_GROUP, PLANNING_MARGIN_PER_GROUP, STARTERS_PER_GROUP,
    };
    let thin = levels.len() < MIN_PLAYERS_PER_GROUP[group].1 + PLANNING_MARGIN_PER_GROUP;
    thin || levels
        .get(STARTERS_PER_GROUP[group])
        .is_none_or(|first_cover| target_ovr > *first_cover)
}

/// Each club's registered seniors' ratings per position group, best first, for [`target_fills_a_need`].
fn squad_levels_by_club(game: &Game) -> std::collections::HashMap<String, [Vec<u8>; 4]> {
    let mut levels: std::collections::HashMap<String, [Vec<u8>; 4]> =
        std::collections::HashMap::new();
    for player in &game.players {
        let Some(team_id) = player.team_id.as_deref() else {
            continue;
        };
        if player.squad_role != domain::player::SquadRole::Senior || player.active_loan.is_some() {
            continue;
        }
        levels.entry(team_id.to_string()).or_default()[position_group_index(&player.position)]
            .push(player.ovr);
    }
    for groups in levels.values_mut() {
        for group in groups.iter_mut() {
            group.sort_unstable_by(|a, b| b.cmp(a));
        }
    }
    levels
}

/// Current squad depth per club and broad position group, computed once so the
/// market sweep doesn't re-scan every roster. The same count the squad floor
/// keeps, owned so the sweep can go on to move players.
pub(crate) fn squad_position_depths(game: &Game) -> std::collections::HashMap<String, [usize; 4]> {
    crate::squad_floor::registered_by_club(game)
        .into_iter()
        .map(|(team_id, depths)| (team_id.to_string(), depths))
        .collect()
}
pub(crate) fn contract_days_remaining(
    current_date: NaiveDate,
    contract_end: Option<&str>,
) -> Option<i64> {
    let contract_end = contract_end?;
    let contract_end_date = NaiveDate::parse_from_str(contract_end, "%Y-%m-%d").ok()?;
    Some((contract_end_date - current_date).num_days())
}
pub(crate) fn infer_player_importance(
    player: &domain::player::Player,
    owner_team: &domain::team::Team,
) -> PlayerImportance {
    if owner_team.starting_xi_ids.iter().any(|id| id == &player.id) {
        return PlayerImportance::Key;
    }

    if player.market_value >= 1_500_000 {
        return PlayerImportance::Regular;
    }

    PlayerImportance::Fringe
}
pub(crate) fn minimum_acceptable_fee(
    current_date: NaiveDate,
    player: &domain::player::Player,
    owner_team: &domain::team::Team,
    buyer_team: &domain::team::Team,
) -> u64 {
    let mut multiplier: f64 = if player.transfer_listed { 0.8 } else { 1.2 };

    if let Some(days_remaining) = contract_days_remaining(current_date, player.contract_end()) {
        if days_remaining <= 60 {
            multiplier -= 0.25;
        } else if days_remaining <= 180 {
            multiplier -= 0.15;
        } else if days_remaining <= 365 {
            multiplier -= 0.05;
        }
    }

    match infer_player_importance(player, owner_team) {
        PlayerImportance::Key => multiplier += 0.2,
        PlayerImportance::Regular => multiplier += 0.1,
        PlayerImportance::Fringe => {}
    }

    if player.morale <= 40 {
        multiplier -= 0.05;
    }

    let openness_score = player_move_openness_score(current_date, player, owner_team, buyer_team);
    if openness_score >= 60 {
        multiplier -= 0.20;
    } else if openness_score >= 40 {
        multiplier -= 0.10;
    }

    let multiplier = multiplier.clamp(0.55, 1.6);
    ((player.market_value as f64) * multiplier).round() as u64
}
pub(crate) fn player_move_openness_score(
    current_date: NaiveDate,
    player: &domain::player::Player,
    owner_team: &domain::team::Team,
    buyer_team: &domain::team::Team,
) -> i32 {
    let mut score = 0;

    if player.morale <= 45 {
        score += 20;
    } else if player.morale <= 60 {
        score += 10;
    }

    if player.stats.appearances <= 2 {
        score += 15;
    } else if player.stats.appearances <= 5 {
        score += 8;
    }

    if let Some(days_remaining) = contract_days_remaining(current_date, player.contract_end()) {
        if days_remaining <= 180 {
            score += 20;
        } else if days_remaining <= 365 {
            score += 10;
        }
    }

    let reputation_gap = buyer_team.reputation as i32 - owner_team.reputation as i32;
    if reputation_gap >= 200 {
        score += 25;
    } else if reputation_gap >= 75 {
        score += 15;
    }

    if player.transfer_listed {
        score += 10;
    }

    score
}
pub(crate) fn apply_blocked_move_consequences(
    player: &mut domain::player::Player,
    openness_score: i32,
) {
    if openness_score < 40 {
        return;
    }

    let morale_drop = if openness_score >= 60 { 10 } else { 6 };
    player.morale = (i16::from(player.morale) - morale_drop).clamp(0, 100) as u8;
    player.morale_core.manager_trust =
        (i16::from(player.morale_core.manager_trust) - 5).clamp(0, 100) as u8;
    player.morale_core.unresolved_issue = Some(domain::player::PlayerIssue {
        category: domain::player::PlayerIssueCategory::Contract,
        severity: if openness_score >= 60 { 75 } else { 60 },
    });
}
pub(crate) fn award_leaderboard_player_ids(game: &Game) -> HashSet<String> {
    let awards = crate::season_awards::compute_season_awards(game);

    awards
        .golden_boot
        .iter()
        .chain(awards.assist_king.iter())
        .chain(awards.player_of_year.iter())
        .chain(awards.clean_sheet_king.iter())
        .chain(awards.most_appearances.iter())
        .chain(awards.young_player.iter())
        .map(|entry| entry.player_id.clone())
        .collect()
}
/// A loan-listed player an AI club would lend out, with what a borrower needs to judge him.
pub(crate) struct LoanTarget {
    player_id: String,
    owner_team_id: String,
    owner_reputation: u32,
    position_group_index: usize,
    ovr: u8,
}

/// An AI club with nothing to buy borrows a loan-listed player who would improve its squad, on the
/// least share of his wage his club accepts, when it can carry that share. The same loan the
/// player's own club can ask for (`make_loan_offer`): the owner's terms, the squad floor, the
/// borrower's wage policy, all unchanged. Returns the player borrowed.
fn try_ai_loan(
    game: &mut Game,
    borrower: &Team,
    levels: &[Vec<u8>; 4],
    books: &crate::ai_finance_guard::SigningBooks,
    targets: &[LoanTarget],
    moved_player_ids: &HashSet<String>,
    current_date: NaiveDate,
) -> Option<String> {
    let today = current_date.format("%Y-%m-%d").to_string();
    for target in targets {
        if target.owner_team_id == borrower.id
            || moved_player_ids.contains(&target.player_id)
            || target.owner_reputation as i32 - borrower.reputation as i32
                > MAX_BUYER_REPUTATION_DEFICIT
            || !target_fills_a_need(
                &levels[target.position_group_index],
                target.position_group_index,
                target.ovr,
            )
        {
            continue;
        }
        let Some(player) = game.players.iter().find(|p| p.id == target.player_id) else {
            continue;
        };
        let Some(owner) = game.teams.iter().find(|t| t.id == target.owner_team_id) else {
            continue;
        };
        if !player.loan_listed
            || player.team_id.as_deref() != Some(target.owner_team_id.as_str())
            || player_has_active_or_pending_loan(player)
            || has_pending_transfer_registration(player)
        {
            continue;
        }
        let Some(end_date) = default_loan_end_date(current_date, player) else {
            continue;
        };
        let share = minimum_loan_wage_contribution_pct(player, owner);
        if !books.can_add_wage(loan_wage_share(player, share))
            || validate_loan_borrower_affordability(game, &borrower.id, player, share).is_err()
        {
            continue;
        }
        let player_id = target.player_id.clone();
        if execute_loan(
            game,
            &player_id,
            &target.owner_team_id,
            &borrower.id,
            &today,
            &end_date,
            share,
            None,
        )
        .is_ok()
        {
            return Some(player_id);
        }
    }
    None
}

pub fn evaluate_transfer_market(game: &mut Game) {
    expire_stale_transfer_offers(game);
    expire_stale_loan_offers(game);
    prune_closed_offers(game);

    if !transfer_window_is_open(game) {
        return;
    }

    let user_team_id = game.manager.team_id.clone();

    let current_date = game.clock.current_date.date_naive();
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let award_leaderboards = award_leaderboard_player_ids(game);
    let team_reputation: std::collections::HashMap<String, u32> = game
        .teams
        .iter()
        .map(|team| (team.id.clone(), team.reputation))
        .collect();
    let team_country: std::collections::HashMap<String, String> = game
        .teams
        .iter()
        .map(|team| (team.id.clone(), team.country.clone()))
        .collect();
    let position_depths = squad_position_depths(game);
    let squad_levels = squad_levels_by_club(game);
    let home_matches: std::collections::HashMap<String, i64> =
        crate::ai_finance_guard::home_league_matches_by_club(game)
            .into_iter()
            .map(|(club, matches)| (club.to_string(), matches))
            .collect();
    // Every club's wage bill, read once: a buyer's check needs its own, and finding one scans
    // every player. The sweep moves players, so the two clubs of a completed transfer are
    // recounted below.
    let mut wage_bills = crate::finances::wage_bills_by_club(game);
    // Where each target sits in `game.players`, so the buyer's wage check need not search.
    // The sweep moves players between clubs but never adds or removes one.
    let player_index: std::collections::HashMap<String, usize> = game
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.id.clone(), index))
        .collect();

    // In a multi-competition world the clubs of the player's active scope shop every day, as in
    // OFM. The rest of the world shops too (a club abroad buys, sells and borrows by the same
    // rules), but on its own review weekday only: one day in seven, not every day, which keeps the
    // sweep's cost proportional to the clubs that are deciding. `None` means no scope is
    // configured, so every club is a potential buyer every day.
    let active_team_ids = game.active_team_ids();
    let weekday = game.clock.current_date.weekday().num_days_from_monday();
    let day_number = u64::try_from(current_date.num_days_from_ce()).unwrap_or(0);
    let mut buyer_ids: Vec<String> = game
        .teams
        .iter()
        .filter(|team| Some(team.id.as_str()) != user_team_id.as_deref())
        .filter(|team| {
            active_team_ids.as_ref().is_none_or(|ids| {
                ids.contains(&team.id) || crate::ai_tactics::review_weekday(&team.id) == weekday
            })
        })
        .map(|team| team.id.clone())
        .collect();
    if active_team_ids.is_some() {
        // The world's clubs are stored league by league and the sweep stops buying at the day's
        // cap: in stored order the first league would always shop first. A hash of the day and
        // the club puts them in a different, reproducible order every day.
        buyer_ids.sort_by_key(|id| crate::stable_hash::stable_hash(id.as_bytes(), day_number));
    }
    // One completed deal per this many shopping clubs a day, and never fewer than the base: with
    // the whole world shopping, a flat two a day capped the market below what its needs produce.
    let max_completed_ai_transfers = MAX_COMPLETED_AI_TRANSFERS_PER_DAY
        .max(buyer_ids.len() / BUYERS_PER_COMPLETED_AI_TRANSFER_PER_DAY);
    let mut completed_ai_transfers = 0_usize;
    let mut moved_player_ids: HashSet<String> = HashSet::new();
    // New incoming offers opened to user players today, tracked to throttle the
    // inbox: at most one new club per player and a hard squad-wide ceiling.
    let mut new_offers_per_player: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut new_loan_offers_per_player: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut new_user_offers_today = 0_usize;
    let mut new_user_loan_offers_today = 0_usize;
    // Clubs already in talks for each user player, kept live rather than snapshotted so offers
    // opened earlier in today's sweep count against the ceiling too. One club is one approach,
    // so this both caps the queue and stops a club holding two approaches on the same player.
    let mut approach_clubs: std::collections::HashMap<String, HashSet<String>> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == user_team_id.as_deref())
        .map(|player| (player.id.clone(), pending_approach_clubs(player)))
        .collect();
    // Clubs turned away recently. Unlike the queue above this is fixed for the day: a rejection
    // made today already carries its closure date, so it is in here from the next sweep onwards.
    let cooled_clubs: std::collections::HashMap<String, HashSet<String>> = game
        .players
        .iter()
        .filter(|player| player.team_id.as_deref() == user_team_id.as_deref())
        .map(|player| {
            (
                player.id.clone(),
                clubs_in_rebid_cooldown(player, current_date),
            )
        })
        .collect();

    // A player's transfer appeal and asking fee don't depend on who's buying, so
    // score every player once and keep only the genuinely attractive targets.
    // Each club then scans this short, score-sorted list instead of the whole
    // world, turning an O(clubs × players) sweep into O(players + clubs × shortlist).
    let mut shortlist: Vec<MarketTarget> = Vec::new();
    for player in &game.players {
        let Some(owner_team_id) = player.team_id.as_deref() else {
            continue;
        };
        if player_has_pending_registration(player) {
            continue;
        }
        // An AI club at the floor in his group will not sell him, so no buyer
        // should spend its approach on him. `execute_transfer` refuses the sale
        // regardless; this keeps the refusal from costing a buyer its day. The
        // player's own club is left to decide for itself.
        let is_user_owned = Some(owner_team_id) == user_team_id.as_deref();
        if !is_user_owned
            && player.squad_role == domain::player::SquadRole::Senior
            && position_depths
                .get(owner_team_id)
                .is_some_and(|depths| !crate::squad_floor::can_spare_one(*depths, &player.position))
        {
            continue;
        }
        let mut score = incoming_interest_score(current_date, player);
        if award_leaderboards.contains(&player.id) {
            score += AWARD_LEADERBOARD_INTEREST_BONUS;
        }
        // A player an AI club has put up for sale is a target whatever his score: the score
        // measures appeal to a club shopping for a bargain, and his price already is one.
        if score < 35 && (is_user_owned || !player.transfer_listed) {
            continue;
        }
        shortlist.push(MarketTarget {
            player_id: player.id.clone(),
            owner_team_id: owner_team_id.to_string(),
            is_user_owned,
            score,
            fee: suggested_incoming_fee(current_date, player),
            position_group_index: position_group_index(&player.position),
            owner_reputation: team_reputation.get(owner_team_id).copied().unwrap_or(0),
            ovr: player.ovr,
            is_listed: player.transfer_listed,
            age: domain::age::age_on_or_unknown(current_date, &player.date_of_birth),
        });
    }
    // Highest appeal first; a stable sort preserves the original ordering among
    // equally appealing targets, so selection is unchanged.
    shortlist.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));

    // The AI clubs' loan-listed players an AI borrower may ask for, best first. A club at the floor
    // in his group cannot lend him (`execute_loan` refuses it regardless).
    let mut loan_targets: Vec<LoanTarget> = game
        .players
        .iter()
        .filter(|player| player.loan_listed && !player_has_pending_registration(player))
        .filter(|player| player.squad_role == domain::player::SquadRole::Senior)
        .filter_map(|player| {
            let owner = player.team_id.as_deref()?;
            if Some(owner) == user_team_id.as_deref()
                || position_depths.get(owner).is_some_and(|depths| {
                    !crate::squad_floor::can_spare_one(*depths, &player.position)
                })
            {
                return None;
            }
            Some(LoanTarget {
                player_id: player.id.clone(),
                owner_team_id: owner.to_string(),
                owner_reputation: team_reputation.get(owner).copied().unwrap_or(0),
                position_group_index: position_group_index(&player.position),
                ovr: player.ovr,
            })
        })
        .collect();
    loan_targets.sort_by(|a, b| {
        b.ovr
            .cmp(&a.ovr)
            .then_with(|| a.player_id.cmp(&b.player_id))
    });
    let max_ai_loans = (buyer_ids.len() / BUYERS_PER_COMPLETED_AI_LOAN_PER_DAY).max(1);
    let mut completed_ai_loans = 0_usize;

    BUYER_EVALUATIONS.fetch_add(buyer_ids.len() as u64, std::sync::atomic::Ordering::Relaxed);
    for buyer_id in buyer_ids {
        let Some(buyer_team) = game.teams.iter().find(|team| team.id == buyer_id).cloned() else {
            continue;
        };
        let buyer_depths = position_depths.get(&buyer_id).copied().unwrap_or([0; 4]);

        let loan_offer_player_id = if let Some(user_team_id) = user_team_id.as_deref() {
            if new_user_loan_offers_today < MAX_NEW_INCOMING_USER_OFFERS_PER_DAY {
                create_incoming_user_loan_offer_if_any(
                    game,
                    user_team_id,
                    &buyer_id,
                    &buyer_team.name,
                    &today,
                    current_date,
                    IncomingOfferBudget {
                        new_today: &new_loan_offers_per_player,
                        approach_clubs: &approach_clubs,
                        cooled_clubs: &cooled_clubs,
                    },
                )
            } else {
                None
            }
        } else {
            None
        };
        if let Some(player_id) = loan_offer_player_id.as_ref() {
            *new_loan_offers_per_player
                .entry(player_id.clone())
                .or_insert(0) += 1;
            approach_clubs
                .entry(player_id.clone())
                .or_default()
                .insert(buyer_id.clone());
            new_user_loan_offers_today += 1;
        }

        // The list is score-sorted, so the first target clearing this club's
        // filters is its highest-appeal eligible signing.
        let buyer_levels = squad_levels.get(&buyer_id).cloned().unwrap_or_default();
        let buyer_books = crate::ai_finance_guard::SigningBooks::given(
            &buyer_team,
            wage_bills.get(&buyer_id).copied().unwrap_or(0),
            home_matches.get(&buyer_id).copied().unwrap_or(0),
        );
        // Worked out once for this buyer, and only if a target gets as far as needing it.
        let mut buyer_wage_facts: Option<BuyerWageFacts> = None;
        let mut acceptable = |target: &&MarketTarget| -> bool {
            if target.owner_team_id == buyer_id || moved_player_ids.contains(&target.player_id) {
                return false;
            }
            if loan_offer_player_id.as_deref() == Some(target.player_id.as_str()) {
                return false;
            }
            if target.is_user_owned {
                let budget = IncomingOfferBudget {
                    new_today: &new_offers_per_player,
                    approach_clubs: &approach_clubs,
                    cooled_clubs: &cooled_clubs,
                };
                if !budget.accepts(
                    &target.player_id,
                    &buyer_id,
                    MAX_NEW_INCOMING_OFFERS_PER_USER_PLAYER_PER_DAY,
                ) || new_user_offers_today >= MAX_NEW_INCOMING_USER_OFFERS_PER_DAY
                {
                    return false;
                }
            } else if completed_ai_transfers >= max_completed_ai_transfers {
                return false;
            }
            // Clubs only chase players that fit their stature and a position they
            // actually need, so a single star doesn't draw the whole division.
            if !buyer_has_genuine_interest(
                buyer_team.reputation,
                target.owner_reputation,
                buyer_depths[target.position_group_index],
            ) {
                return false;
            }
            // Nobody pays a fee for a player at the end of his career, a goalkeeper's coming later.
            if !target.is_user_owned
                && target.age >= retirement_horizon_age(target.position_group_index)
            {
                return false;
            }
            // An AI club buys to fill a need it has, not for appeal alone.
            if !target.is_user_owned
                && !target_fills_a_need(
                    &buyer_levels[target.position_group_index],
                    target.position_group_index,
                    target.ovr,
                )
            {
                return false;
            }
            buyer_team.transfer_budget >= target.fee as i64
                && buyer_team.finance >= target.fee as i64
                && player_index.get(&target.player_id).is_some_and(|&index| {
                    let facts = buyer_wage_facts.get_or_insert_with(|| {
                        BuyerWageFacts::new(
                            wage_bills.get(&buyer_id).copied().unwrap_or(0),
                            buyer_depths,
                        )
                    });
                    buyer_can_pay_standard_wage(
                        &game.players[index],
                        &buyer_team,
                        facts,
                        current_date,
                    ) && (target.is_user_owned
                        || buyer_books.can_carry(
                            i64::from(
                                standard_contract_terms(
                                    &game.players[index],
                                    &buyer_team,
                                    current_date,
                                    0,
                                )
                                .map_or(0, |(wage, _)| wage),
                            ),
                            target.fee as i64,
                        ))
                })
        };
        // A club looks at its own country's market first, and abroad only when nothing there
        // fits: moves between countries are the exception, not a draw from the whole world.
        let chosen = shortlist
            .iter()
            .filter(|target| {
                team_country
                    .get(&target.owner_team_id)
                    .is_some_and(|country| *country == buyer_team.country)
            })
            .find(&mut acceptable)
            .or_else(|| shortlist.iter().find(&mut acceptable));

        let Some(target) = chosen else {
            // Nothing to buy: perhaps something to borrow.
            if completed_ai_loans < max_ai_loans
                && let Some(player_id) = try_ai_loan(
                    game,
                    &buyer_team,
                    &buyer_levels,
                    &buyer_books,
                    &loan_targets,
                    &moved_player_ids,
                    current_date,
                )
            {
                moved_player_ids.insert(player_id);
                completed_ai_loans += 1;
            }
            continue;
        };
        let candidate = MarketCandidate {
            player_id: target.player_id.clone(),
            owner_team_id: target.owner_team_id.clone(),
            score: target.score,
            fee: target.fee,
            is_listed: target.is_listed,
        };

        if Some(candidate.owner_team_id.as_str()) == user_team_id.as_deref() {
            create_incoming_user_offer(game, &candidate, &buyer_id, &buyer_team.name, &today);
            *new_offers_per_player
                .entry(candidate.player_id.clone())
                .or_insert(0) += 1;
            approach_clubs
                .entry(candidate.player_id.clone())
                .or_default()
                .insert(buyer_id.clone());
            new_user_offers_today += 1;
            continue;
        }

        // A player his club has listed is for sale to any buyer that needs him; one it has not is
        // sold only when he is a bargain (a contract running out, an unhappy player).
        if !(candidate.is_listed || candidate.score > 60)
            || completed_ai_transfers >= max_completed_ai_transfers
        {
            continue;
        }

        if execute_transfer(
            game,
            &candidate.player_id,
            &buyer_id,
            &candidate.owner_team_id,
            candidate.fee,
        )
        .is_ok()
        {
            moved_player_ids.insert(candidate.player_id);
            completed_ai_transfers += 1;
            // Only the seller is recounted: a club that has just bought has had its turn (each
            // club is a buyer once a sweep), so its new bill is never read again today, while
            // the seller may still buy and is held to the bill it has after the sale.
            wage_bills.insert(
                candidate.owner_team_id.clone(),
                calc_wages(game, &candidate.owner_team_id),
            );
        }
    }
}
pub fn generate_incoming_transfer_offers(game: &mut Game) {
    evaluate_transfer_market(game);
}
