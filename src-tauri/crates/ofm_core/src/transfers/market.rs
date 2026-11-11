//! The daily sweep: who wants whom, and which of them gets to ask.
//!
//! This is the top of the call graph. It scores every player once, sorts the attractive ones,
//! then walks the buying clubs and lets each open at most one approach — subject to the
//! budget in `lifecycle`, which is what keeps a listed player from being buried in offers.

use super::buyer_view::{BuyerView, squad_indices_by_club};
use super::*;

use crate::squad_floor::group_index as position_group_index;
use chrono::Datelike;

/// How many buyer turns the sweep has taken in this process: a measure of the market's work, read
/// by the harness. It never feeds a decision.
static BUYER_EVALUATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn market_buyer_evaluations() -> u64 {
    BUYER_EVALUATIONS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Why the world's clubs entered the market, for the harness, one counter per question:
/// `[teams considered, positional need, Squad Intelligence need, loan need, shopped, shopped on
/// their review day, shopped for a need in the closing week, shopped because the whole market is
/// open]`. Measurement only, like `BUYER_EVALUATIONS`; it never feeds a decision.
pub const PARTICIPATION_COUNTERS: usize = 8;
static PARTICIPATION: [std::sync::atomic::AtomicU64; PARTICIPATION_COUNTERS] =
    [const { std::sync::atomic::AtomicU64::new(0) }; PARTICIPATION_COUNTERS];

pub fn market_participation() -> [u64; PARTICIPATION_COUNTERS] {
    std::array::from_fn(|index| PARTICIPATION[index].load(std::sync::atomic::Ordering::Relaxed))
}

/// What the market knew about one completed AI transfer when it was made: the harness's view of
/// *why* a price was what it was. It never feeds a decision, and nothing is recorded unless the
/// harness switches the log on.
#[derive(Debug, Clone)]
pub struct DealRecord {
    pub player_id: String,
    pub buyer_id: String,
    pub position_group: usize,
    pub ovr: u8,
    pub status: &'static str,
    pub phase: &'static str,
    pub listed: bool,
    pub fee: u64,
    pub asking: u64,
    pub value: u64,
    pub age: i32,
    pub contract_days: Option<i64>,
    pub date: String,
    pub buyer_reputation: u32,
    pub seller_reputation: u32,
    pub buyer_country: String,
    pub seller_country: String,
    pub improvement: i32,
    pub thin: bool,
    /// The buyer's formation at the time of the deal.
    pub buyer_formation: String,
    /// The buyer's registered seniors in the target's group, best first: what `target_fills_a_need`
    /// and `group_is_thin` read.
    pub buyer_levels: Vec<u8>,
    /// The target's natural position and any he is trained to cover.
    pub natural_position: String,
    pub alternate_positions: Vec<String>,
    pub potential: u8,
    /// The buyer's senior depth in the target's broad group when the deal was agreed: how many it
    /// already had available, and how many it planned to have a year on.
    pub buyer_group_depth: usize,
    pub buyer_planned_depth: usize,
    /// What the buyer's own squad model said he would add when the deal was agreed.
    pub role: &'static str,
    pub starts_at: Option<usize>,
    pub points: i32,
    pub gain: f32,
    pub weekly_wage: u32,
    /// Measurement only: where the club's model would place him over the horizon, whether the
    /// lineup picker itself would start him today, and whom he would displace. Never feeds a
    /// decision.
    pub placement: Option<DealPlacement>,
}

/// What the harness's market trace records about one signing's pathway (measurement only).
#[derive(Debug, Clone, Copy)]
pub struct DealPlacement {
    /// Gain in this season's expected strength alone.
    pub now_gain: f32,
    /// Slot the model starts him in, year 0/1/2.
    pub model_start: [Option<u8>; 3],
    /// Whether the lineup picker (`turn::squad::first_choice_eleven`) would field him today, from
    /// the buyer's healthy seniors and him: the model's prediction checked against the real thing.
    pub picker_starts_now: bool,
    /// The player he displaces in each year, if the slot was occupied.
    pub blocker_slot: [Option<u8>; 3],
    pub blocker_ovr: [Option<u8>; 3],
    pub blocker_age: [Option<i32>; 3],
    pub blocker_keeper: [bool; 3],
}

static DEAL_LOG_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static DEAL_LOG: std::sync::Mutex<Vec<DealRecord>> = std::sync::Mutex::new(Vec::new());

pub fn enable_deal_log() {
    DEAL_LOG_ON.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// The deals recorded since the last call.
pub fn take_deal_log() -> Vec<DealRecord> {
    std::mem::take(&mut *DEAL_LOG.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Whether the lineup picker would field `signing` today in `buyer`'s side, from the buyer's healthy
/// seniors and him. Measurement only: it asks the picker that picks the side, not the model.
fn picker_would_start(game: &Game, buyer: &Team, signing: &Player) -> bool {
    let pool: Vec<&Player> = game
        .players
        .iter()
        .filter(|player| {
            player.team_id.as_deref() == Some(buyer.id.as_str())
                && player.squad_role == domain::player::SquadRole::Senior
                && player.can_play()
        })
        .chain(Some(signing))
        .collect();
    crate::turn::squad::first_choice_eleven(&pool, &buyer.formation)
        .iter()
        .any(|player| player.id == signing.id)
}

/// How much further up the reputation ladder a club that has just come up looks for a signing:
/// its new division is above the reputation it earned in the old one.
const PROMOTED_REPUTATION_REACH: u32 = 50;

/// An imbalance large enough to warrant immediate market participation before a club's ordinary
/// review day. Smaller needs still inform target appraisal once the club is shopping.
const URGENT_SQUAD_NEED_SEVERITY: f32 = 7.0;

/// The age from which an AI club will not pay a transfer fee for a player: his career is nearly
/// over and the fee buys no resale. A goalkeeper's lasts longer. Free agents and loans are not
/// bound by it: they cost no fee.
pub(crate) fn retirement_horizon_age(position_group_index: usize) -> i32 {
    if position_group_index == 0 { 36 } else { 33 }
}

/// Whether a club has a realistic reason to pursue a target: it isn't far below
/// the player's current club in stature, and it isn't already overloaded in the
/// player's position group. A player who fills a weak starting slot is a reason
/// even when the broad group is deep: eight centre-backs do not cover a full-back
/// slot the side actually fields.
pub(crate) fn buyer_has_genuine_interest(
    buyer_reputation: u32,
    owner_reputation: u32,
    buyer_position_depth: usize,
    fills_a_starting_need: bool,
) -> bool {
    let reputation_deficit = owner_reputation as i32 - buyer_reputation as i32;
    reputation_deficit <= MAX_BUYER_REPUTATION_DEFICIT
        && (fills_a_starting_need || buyer_position_depth < POSITION_GROUP_SURPLUS_THRESHOLD)
}
/// Whether a target would make a buyer's squad better where it matters: the club is thin in his
/// group (under the floor plus the planning margin) or he is rated above the bar the club's plan
/// sets for that group. `levels` is the buyer's seniors in that group, best first.
///
/// Building up to [`DESIRED_DEPTH`] is a need; beyond it, only a first-team upgrade is. A club
/// sells down to the same plan (`ai_squad_sales`), so a fee for an eighth defender, a fourth
/// goalkeeper or another cover player is a fee for someone the club would itself sell: the
/// shipped world's AI clubs bought goalkeepers at several times the rate of any outfield group,
/// and four-fifths of those signings never played.
///
/// Buying whoever scores best on appeal, whatever the buyer already has, sent clubs after players
/// worse than the ones on their bench; a transfer should come from a need.
pub(crate) fn target_fills_a_need(levels: &[u8], group: usize, target_ovr: u8) -> bool {
    use crate::squad_floor::STARTERS_PER_GROUP;
    if group_is_thin(levels, group) {
        return true;
    }
    let bar = if levels.len() >= DESIRED_DEPTH[group] {
        STARTERS_PER_GROUP[group].saturating_sub(1)
    } else {
        STARTERS_PER_GROUP[group]
    };
    levels.get(bar).is_none_or(|cover| target_ovr > *cover)
}

/// The players who count toward a club's squad today: registered seniors, not on loan, not out
/// for the season. Both the squad as it is and the squad a year on are read from this one list.
fn registered_seniors(game: &Game) -> impl Iterator<Item = (&str, &Player)> {
    game.players.iter().filter_map(|player| {
        let team_id = player.team_id.as_deref()?;
        crate::squad_outlook::counts_toward_squad(player).then_some((team_id, player))
    })
}

/// Each club's registered seniors' ratings per position group, best first, for [`target_fills_a_need`].
fn squad_levels_by_club(game: &Game) -> std::collections::HashMap<String, [Vec<u8>; 4]> {
    let mut levels: std::collections::HashMap<String, [Vec<u8>; 4]> =
        std::collections::HashMap::new();
    for (team_id, player) in registered_seniors(game) {
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

/// What every senior is to his club, and which clubs are in trouble (over their wage budget or in
/// debt), for the sweep to read prices and willingness to sell from.
fn player_statuses(
    game: &Game,
    current_date: NaiveDate,
    wage_bills: &std::collections::HashMap<String, i64>,
    division_moves: &std::collections::HashMap<String, crate::division_move::DivisionMove>,
    strategies: &std::collections::HashMap<String, strategy::ClubProfile>,
) -> (
    std::collections::HashMap<String, PlayerStatus>,
    HashSet<String>,
    HashSet<String>,
) {
    let mut by_club: std::collections::HashMap<&str, Vec<&Player>> =
        std::collections::HashMap::new();
    for player in &game.players {
        if let Some(team_id) = player.team_id.as_deref()
            && player.active_loan.is_none()
        {
            by_club.entry(team_id).or_default().push(player);
        }
    }
    let age_of =
        |player: &Player| domain::age::age_on_or_unknown(current_date, &player.date_of_birth);
    let mut statuses = std::collections::HashMap::new();
    let mut troubled = HashSet::new();
    let mut clubs_with_squad_needs = HashSet::new();
    for team in &game.teams {
        let Some(players) = by_club.get(team.id.as_str()) else {
            continue;
        };
        let bill = wage_bills.get(&team.id).copied().unwrap_or(0);
        // A club that has just come down is held to a stricter line: it is "in trouble" from 90 %
        // of the budget its new division plans on, so its costly spares are the first to go.
        let relegated =
            division_moves.get(&team.id) == Some(&crate::division_move::DivisionMove::Relegated);
        let in_trouble = bill > team.wage_budget
            || team.finance < 0
            || (relegated && bill * 100 > team.wage_budget * 90);
        if in_trouble {
            troubled.insert(team.id.clone());
        }
        // What each player is to his club: from what his sale would cost the side it fields (Squad
        // Intelligence). The manager's own club keeps the rank-based reading: its players are not
        // for an AI club to price, they are offered at the usual incoming fee.
        if game.manager.team_id.as_deref() == Some(team.id.as_str()) {
            statuses.extend(club_statuses(
                players,
                &team.starting_xi_ids,
                team.wage_budget,
                in_trouble,
                &age_of,
            ));
        } else {
            let profile = strategies
                .get(&team.id)
                .copied()
                .unwrap_or(strategy::ClubProfile::NEUTRAL);
            let squad = crate::squad_intelligence::ClubSquad::for_planning(
                players.iter().copied(),
                team,
                current_date,
                crate::squad_intelligence::appetite::Appetite::of(&profile),
            );
            if squad
                .needs()
                .iter()
                .any(|need| need.severity >= URGENT_SQUAD_NEED_SEVERITY)
            {
                clubs_with_squad_needs.insert(team.id.clone());
            }
            statuses.extend(sale_view::club_statuses_by_impact(
                &squad,
                &team.id,
                players,
                team.wage_budget,
                in_trouble,
                &age_of,
            ));
        }
    }
    (statuses, troubled, clubs_with_squad_needs)
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
/// The least a club takes for a player, at a point in the window: the club that cannot replace a first-team
/// player asks more as the close nears, one with a fringe player asks less, and a contract that
/// ends soon is worth less to sell later. Each term is added to the multiplier once, then the
/// whole is clamped, so nothing compounds across the rounds of a negotiation.
pub(crate) fn minimum_acceptable_fee_at(
    current_date: NaiveDate,
    player: &domain::player::Player,
    owner_team: &domain::team::Team,
    buyer_team: &domain::team::Team,
    phase: WindowPhase,
) -> u64 {
    let pressure = phase.pressure();
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
        PlayerImportance::Key => multiplier += 0.2 + 0.15 * pressure,
        PlayerImportance::Regular => multiplier += 0.1,
        PlayerImportance::Fringe => multiplier -= 0.15 * pressure,
    }
    if contract_days_remaining(current_date, player.contract_end()).is_some_and(|days| days <= 60) {
        multiplier -= 0.10 * pressure;
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
    /// Where the player sits in `Game::players` when the targets are collected. The sweep appends
    /// players but never reorders them, so this still names him when a club comes to borrow: the
    /// search reads him in place instead of scanning the whole list, by id, once per target.
    player_index: usize,
    player_id: String,
    owner_team_id: String,
    owner_reputation: u32,
    position_group_index: usize,
    ovr: u8,
}

/// Immutable inputs to one club's search for a loan. Keeping them together makes it explicit
/// that a loan is judged from a single squad snapshot before the registration mutates the game.
struct LoanSearch<'a> {
    borrower: &'a Team,
    levels: &'a [Vec<u8>; 4],
    view: &'a BuyerView,
    books: &'a crate::ai_finance_guard::SigningBooks,
    targets: &'a [LoanTarget],
    moved_player_ids: &'a HashSet<String>,
    current_date: NaiveDate,
}

fn invalidate_loan_views(
    sale_view: &sale_view::SaleView,
    targets: &[LoanTarget],
    player_id: &str,
    borrower_id: &str,
) {
    if let Some(target) = targets.iter().find(|target| target.player_id == player_id) {
        sale_view.invalidate(&target.owner_team_id);
    }
    sale_view.invalidate(borrower_id);
}

/// The AI clubs' loan-listed players an AI borrower may ask for, best first. A club at the floor
/// in his group cannot lend him (`execute_loan` refuses it regardless).
fn collect_loan_targets(
    players: &[Player],
    user_team_id: Option<&str>,
    position_depths: &std::collections::HashMap<String, [usize; 4]>,
    team_reputation: &std::collections::HashMap<String, u32>,
) -> Vec<LoanTarget> {
    let mut targets: Vec<LoanTarget> = players
        .iter()
        .enumerate()
        .filter(|(_, player)| player.loan_listed && !player_has_pending_registration(player))
        .filter(|(_, player)| player.squad_role == domain::player::SquadRole::Senior)
        .filter_map(|(index, player)| {
            let owner = player.team_id.as_deref()?;
            if Some(owner) == user_team_id
                || position_depths.get(owner).is_some_and(|depths| {
                    !crate::squad_floor::can_spare_one(*depths, &player.position)
                })
            {
                return None;
            }
            Some(LoanTarget {
                player_index: index,
                player_id: player.id.clone(),
                owner_team_id: owner.to_string(),
                owner_reputation: team_reputation.get(owner).copied().unwrap_or(0),
                position_group_index: position_group_index(&player.position),
                ovr: player.ovr,
            })
        })
        .collect();
    // Highest appeal first; a stable sort preserves the original ordering among
    // equally appealing targets, so selection is unchanged.
    targets.sort_by(|a, b| {
        b.ovr
            .cmp(&a.ovr)
            .then_with(|| a.player_id.cmp(&b.player_id))
    });
    targets
}

/// An AI club borrows a loan-listed player when he would improve a genuine current need, on the
/// least share of his wage his club accepts, when it can carry that share. The same loan the
/// player's own club can ask for (`make_loan_offer`): the owner's terms, the squad floor, the
/// borrower's wage policy, all unchanged. Returns the player borrowed.
fn try_ai_loan(game: &mut Game, search: LoanSearch<'_>) -> Option<String> {
    let today = search.current_date.format("%Y-%m-%d").to_string();
    for target in search.targets {
        let Some(player) = game.players.get(target.player_index) else {
            continue;
        };
        // The index is only a shortcut: check the id so a list that ever stops standing still can
        // never land a loan on the wrong footballer.
        if player.id != target.player_id {
            continue;
        }
        if target.owner_team_id == search.borrower.id
            || search.moved_player_ids.contains(&target.player_id)
            || target.owner_reputation as i32 - search.borrower.reputation as i32
                > MAX_BUYER_REPUTATION_DEFICIT
            // A broad group can look covered while the side the club fields has a weak slot: the
            // same exception the purchase path makes for a specialist, judged on the squad model.
            || (!target_fills_a_need(
                &search.levels[target.position_group_index],
                target.position_group_index,
                target.ovr,
            ) && !search.view.has_starter_need_for(player))
        {
            continue;
        }
        // A loan is for this season: it has to make this season's side better.
        let age = domain::age::age_on_or_unknown(search.current_date, &player.date_of_birth);
        if !search.view.worth_borrowing(player, age) {
            continue;
        }
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
        let Some(end_date) = default_loan_end_date(search.current_date, player) else {
            continue;
        };
        let share = minimum_loan_wage_contribution_pct(player, owner);
        if !search.books.can_add_wage(loan_wage_share(player, share))
            || validate_loan_borrower_affordability(game, &search.borrower.id, player, share)
                .is_err()
        {
            continue;
        }
        let player_id = target.player_id.clone();
        if execute_loan(
            game,
            &player_id,
            &target.owner_team_id,
            &search.borrower.id,
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
    let best_listed_loan_by_group = game
        .players
        .iter()
        .filter_map(|player| {
            (player.loan_listed
                && player.team_id.is_some()
                && !player_has_pending_registration(player))
            .then_some((position_group_index(&player.position), player.ovr))
        })
        .fold([0_u8; 4], |mut best, (group, ovr)| {
            best[group] = best[group].max(ovr);
            best
        });
    let clubs_with_loan_needs: HashSet<&str> = squad_levels
        .iter()
        .filter_map(|(club, levels)| {
            (0..4)
                .any(|group| {
                    best_listed_loan_by_group[group] > 0
                        && target_fills_a_need(
                            &levels[group],
                            group,
                            best_listed_loan_by_group[group],
                        )
                })
                .then_some(club.as_str())
        })
        .collect();
    // Whether the player himself would go: he is a party to every move, the AI's included.
    let consent = SweepConsent::new(game, &squad_levels);
    let home_matches: std::collections::HashMap<String, i64> =
        crate::ai_finance_guard::home_league_matches_by_club(game)
            .into_iter()
            .map(|(club, matches)| (club.to_string(), matches))
            .collect();
    // Every club's wage bill, read once: a buyer's check needs its own, and finding one scans
    // every player. The sweep moves players, so the two clubs of a completed transfer are
    // recounted below.
    let mut wage_bills = crate::finances::wage_bills_by_club(game);
    // Each club's operating strategy, read once: what it prefers, not what it may do.
    let strategies = strategy::profiles(game, &wage_bills);
    // Where each target sits in `game.players`, so the buyer's wage check need not search.
    // The sweep moves players between clubs but never adds or removes one.
    let phase = window_phase(game.season_context.transfer_window.days_remaining);
    let division_moves = crate::division_move::recent_division_moves(game);
    let promoted_clubs: HashSet<&str> = division_moves
        .iter()
        .filter(|(_, change)| **change == crate::division_move::DivisionMove::Promoted)
        .map(|(club, _)| club.as_str())
        .collect();
    let relegated_clubs: HashSet<&str> = division_moves
        .iter()
        .filter(|(_, change)| **change == crate::division_move::DivisionMove::Relegated)
        .map(|(club, _)| club.as_str())
        .collect();
    let (statuses, troubled_clubs, clubs_with_squad_needs) = player_statuses(
        game,
        current_date,
        &wage_bills,
        &division_moves,
        &strategies,
    );
    // Relegated clubs that pay more than the budget of the division they now play in.
    let cutting_clubs: HashSet<String> = game
        .teams
        .iter()
        .filter(|team| relegated_clubs.contains(team.id.as_str()))
        .filter(|team| wage_bills.get(&team.id).copied().unwrap_or(0) > team.wage_budget)
        .map(|team| team.id.clone())
        .collect();
    let player_index: std::collections::HashMap<String, usize> = game
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.id.clone(), index))
        .collect();
    // Where each club's players stand, for the buyer's reading of its own squad. A completed deal
    // moves one index between two lists below.
    let mut club_players = squad_indices_by_club(game);
    let sale_view = sale_view::SaleView::new();

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
            // `position_depths` keeps the squad floor safe, but it cannot see a weak striker
            // behind two excellent goalkeepers. `player_statuses` has already built this club's
            // planning squad to price its sales, so retaining its shared need model activates a
            // real sporting need without another squad-model pass.
            let positional_need = position_depths.get(&team.id).is_some_and(has_unmet_need);
            let squad_need = clubs_with_squad_needs.contains(&team.id);
            let loan_need = clubs_with_loan_needs.contains(team.id.as_str());
            let unmet_need = positional_need || squad_need || loan_need;
            // A club with nothing pressing may wait: early in the window most do.
            let patient = !unmet_need
                && !promoted_clubs.contains(team.id.as_str())
                && !shops_without_pressing_need(
                    phase,
                    crate::stable_hash::stable_hash(team.id.as_bytes(), day_number),
                );
            let in_scope = active_team_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&team.id));
            let review_day = crate::ai_tactics::review_weekday(&team.id) == weekday;
            let whole_market = whole_market_is_open(phase);
            // A real hole is acted on at once once the window is closing, but before that it only
            // shapes what a dormant club buys on the day it shops. Letting a need bypass the
            // review day at every phase makes every out-of-scope club a daily shopper: it multiplied
            // the sweep's work and its completed deals about fivefold, and the extra deals were
            // cover that never played while the sides' elevens were unchanged.
            let closing_need = phase >= WindowPhase::LastWeek && unmet_need;
            let selected = !patient && (in_scope || review_day || closing_need || whole_market);
            let count = |index: usize| {
                PARTICIPATION[index].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            };
            count(0);
            if positional_need {
                count(1);
            }
            if squad_need {
                count(2);
            }
            if loan_need {
                count(3);
            }
            if selected {
                count(4);
                if review_day {
                    count(5);
                }
                if closing_need {
                    count(6);
                }
                if whole_market {
                    count(7);
                }
            }
            selected
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
    let max_completed_ai_transfers = ((MAX_COMPLETED_AI_TRANSFERS_PER_DAY
        .max(buyer_ids.len() / BUYERS_PER_COMPLETED_AI_TRANSFER_PER_DAY)
        as f64)
        * phase.activity_factor())
    .round() as usize;
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
                clubs_in_cooldown_of(
                    player,
                    current_date,
                    phase.rebid_cooldown_days(REBID_COOLDOWN_DAYS),
                ),
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
        let status = statuses
            .get(&player.id)
            .copied()
            .unwrap_or(PlayerStatus::Rotation);
        // A club with a spare it has not sold looks harder for a buyer as the window closes.
        let is_spare = !is_user_owned
            && matches!(status, PlayerStatus::Surplus | PlayerStatus::Unwanted)
            && phase >= WindowPhase::Mid;
        if is_spare {
            score += (30.0 * phase.pressure()) as i32;
        }
        // A player an AI club has put up for sale is a target whatever his score: the score
        // measures appeal to a club shopping for a bargain, and his price already is one. So is a
        // spare it would let go.
        if score < 35 && (is_user_owned || !(player.transfer_listed || is_spare)) {
            continue;
        }
        let age = domain::age::age_on_or_unknown(current_date, &player.date_of_birth);
        let group = position_group_index(&player.position);
        // Buyer-independent, so priced once. A user's player is offered for at the usual
        // incoming fee: his price is the user's to set.
        let fee = if is_user_owned {
            suggested_incoming_fee(current_date, player)
        } else {
            asking_fee(&AskingFacts {
                value: player.market_value,
                status,
                phase,
                contract_days_left: contract_days_remaining(current_date, player.contract_end()),
                listed: player.transfer_listed,
                age,
                potential_gap: i32::from(player.potential) - i32::from(player.ovr),
                morale: player.morale,
                seller_in_trouble: troubled_clubs.contains(owner_team_id),
                seller_cutting_costs: cutting_clubs.contains(owner_team_id),
                replacement_hard: position_depths.get(owner_team_id).is_some_and(|depths| {
                    depths[group]
                        <= crate::squad_floor::MIN_PLAYERS_PER_GROUP[group].1
                            + crate::squad_floor::PLANNING_MARGIN_PER_GROUP
                }),
            })
        };
        shortlist.push(MarketTarget {
            player_id: player.id.clone(),
            owner_team_id: owner_team_id.to_string(),
            is_user_owned,
            score,
            fee,
            position_group_index: group,
            owner_reputation: team_reputation.get(owner_team_id).copied().unwrap_or(0),
            ovr: player.ovr,
            is_listed: player.transfer_listed,
            age,
            potential_gap: i32::from(player.potential) - i32::from(player.ovr),
            value: player.market_value,
            status,
            contract_days: contract_days_remaining(current_date, player.contract_end()),
        });
    }
    // Highest appeal first; a stable sort preserves the original ordering among
    // equally appealing targets, so selection is unchanged.
    shortlist.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));

    let loan_targets = collect_loan_targets(
        &game.players,
        user_team_id.as_deref(),
        &position_depths,
        &team_reputation,
    );
    let max_ai_loans = ((((buyer_ids.len() / BUYERS_PER_COMPLETED_AI_LOAN_PER_DAY).max(1)) as f64)
        * phase.activity_factor())
    .round() as usize;
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
        let (loan_levels, buyer_levels) = crate::squad_outlook::club_outlook(
            club_players
                .get(&buyer_id)
                .into_iter()
                .flatten()
                .map(|&index| &game.players[index])
                .filter(|player| {
                    player.team_id.as_deref() == Some(buyer_id.as_str())
                        && crate::squad_outlook::counts_toward_squad(player)
                }),
            current_date,
        );
        let mut available_buyer_depths = [0_usize; 4];
        for &index in club_players.get(&buyer_id).map_or(&[][..], Vec::as_slice) {
            let player = &game.players[index];
            if player.team_id.as_deref() == Some(buyer_id.as_str())
                && player.squad_role == domain::player::SquadRole::Senior
                && player
                    .injury
                    .as_ref()
                    .is_none_or(|injury| injury.days_remaining < LONG_INJURY_DAYS)
            {
                available_buyer_depths[position_group_index(&player.position)] += 1;
            }
        }
        let buyer_books = crate::ai_finance_guard::SigningBooks::given(
            &buyer_team,
            wage_bills.get(&buyer_id).copied().unwrap_or(0),
            home_matches.get(&buyer_id).copied().unwrap_or(0),
        );
        let profile = strategies
            .get(&buyer_id)
            .copied()
            .unwrap_or(strategy::ClubProfile::NEUTRAL);
        // Worked out once for this buyer, and only if a target gets as far as needing it.
        let mut buyer_wage_facts: Option<BuyerWageFacts> = None;
        let buyer_promoted = promoted_clubs.contains(buyer_id.as_str());
        let buyer_overstretched = is_overstretched_at(
            wage_bills.get(&buyer_id).copied().unwrap_or(0),
            buyer_team.wage_budget,
            buyer_team.finance,
            // A club that has just come down does not spend as the division it left did.
            if relegated_clubs.contains(buyer_id.as_str()) {
                100
            } else {
                120
            },
        );
        let buyer_cash = buyer_team.transfer_budget.min(buyer_team.finance);
        // What he adds to the side this buyer fields (Squad Intelligence: the team that plays its
        // formation, not a broad group's first cover), and whether the buyer is short in his group.
        let view = BuyerView::new(
            game,
            club_players.get(&buyer_id).map_or(&[][..], Vec::as_slice),
            &buyer_team,
            profile,
            current_date,
        );
        // An exceptional offer for a player the club would not otherwise sell: whether the seller
        // is better off for it (it cannot lose a player nobody replaces, and it can mend more than
        // he is worth).
        let exceptional_sale_is_rational = |target: &MarketTarget, fee: u64| -> bool {
            let (Some(&index), Some(seller_team), Some(squad)) = (
                player_index.get(&target.player_id),
                game.teams
                    .iter()
                    .find(|team| team.id == target.owner_team_id),
                club_players.get(&target.owner_team_id),
            ) else {
                return false;
            };
            sale_view.rational_sale(
                game,
                &sale_view::Seller {
                    team: seller_team,
                    squad,
                    profile: strategies
                        .get(&target.owner_team_id)
                        .copied()
                        .unwrap_or(strategy::ClubProfile::NEUTRAL),
                    today: current_date,
                },
                &game.players[index],
                fee,
            )
        };
        let target_player = |target: &MarketTarget| {
            player_index
                .get(&target.player_id)
                .map(|&index| &game.players[index])
        };
        let fit = |target: &MarketTarget| -> (i32, bool) {
            let levels = &buyer_levels[target.position_group_index];
            (
                target_player(target).map_or(0, |player| view.improvement(player)),
                group_is_thin(levels, target.position_group_index),
            )
        };
        // The fee this buyer would pay: the seller's price when it is within his ceiling, a little
        // under it when the ceiling is close, none otherwise. A user's player has no asking price
        // to haggle over: the incoming offer is the buyer's own.
        let price = |target: &MarketTarget| -> Option<u64> {
            if target.is_user_owned {
                return Some(target.fee);
            }
            let (improvement, thin) = fit(target);
            agreed_fee(
                target.fee,
                buyer_ceiling(&CeilingFacts {
                    value: target.value,
                    improvement,
                    thin,
                    age: target.age,
                    phase,
                    alternatives: 1,
                    budget: buyer_cash,
                    // A club that has come up wants to be ready for its new division.
                    ambition: if buyer_promoted { 0.04 } else { 0.0 },
                }),
            )
        };
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
            // A club that would not sell him on the usual terms still hears an exceptional offer
            // (settled below, once the fee is known).
            let usual_terms = target.is_user_owned
                || willing_to_sell(
                    target.status,
                    target.is_listed,
                    target.score,
                    phase,
                    target.contract_days,
                );
            if !usual_terms && !sale_view::is_exceptional(target.fee, target.value) {
                return false;
            }
            // The cheap tests come first, in front of anything that reads the buyer's side: a sweep
            // puts thousands of targets in front of every buyer, and only the few that pass these
            // are worth the evaluation of what they would add.
            let thin = group_is_thin(
                &buyer_levels[target.position_group_index],
                target.position_group_index,
            );
            // Below the floor the club is not choosing a signing, it is fielding a side: any body
            // will do. At the floor it has the bodies it needs and is choosing again.
            let below_floor = buyer_levels[target.position_group_index].len()
                < crate::squad_floor::MIN_PLAYERS_PER_GROUP[target.position_group_index].1;
            // Clubs only chase players that fit their stature and a position they
            // actually need, so a single star doesn't draw the whole division. A club with a hole
            // to fill near the close will look a little higher.
            let reach = if thin {
                (60.0 * phase.pressure()) as u32
            } else {
                0
            } + if buyer_promoted {
                PROMOTED_REPUTATION_REACH
            } else {
                0
            };
            let fills_a_starting_need =
                target_player(target).is_some_and(|player| view.has_starter_need_for(player));
            if !buyer_has_genuine_interest(
                buyer_team.reputation,
                target.owner_reputation.saturating_sub(reach),
                available_buyer_depths[target.position_group_index],
                fills_a_starting_need,
            ) {
                return false;
            }
            // Nobody pays a fee for a player at the end of his career, a goalkeeper's coming later.
            if !target.is_user_owned
                && target.age >= retirement_horizon_age(target.position_group_index)
            {
                return false;
            }
            // A club that cannot carry what it already pays sells and lends before it buys.
            if !target.is_user_owned && buyer_overstretched && target.fee > 0 {
                return false;
            }
            // The first cover usually screens weak targets cheaply, but broad groups cannot see
            // a specialist for a weak starting slot. The side appraises that exception below.
            if !target.is_user_owned
                && !target_fills_a_need(
                    &buyer_levels[target.position_group_index],
                    target.position_group_index,
                    target.planned_ovr(),
                )
                && !target_player(target).is_some_and(|player| view.has_starter_need_for(player))
            {
                return false;
            }
            // A cheap foreign filler is not worth the trip.
            if !target.is_user_owned {
                let same_country = team_country
                    .get(&target.owner_team_id)
                    .is_some_and(|country| *country == buyer_team.country);
                let improvement =
                    if same_country || thin || target.value >= CROSS_BORDER_FILLER_VALUE {
                        0
                    } else {
                        fit(target).0
                    };
                if !worth_the_border(same_country, target.value, improvement, thin) {
                    return false;
                }
            }
            let Some(price) = price(target) else {
                return false;
            };
            // A club short of money spends little on one player unless it has a hole to fill.
            if !target.is_user_owned && !profile.allows_spending(price, buyer_cash, thin) {
                return false;
            }
            if !usual_terms && !exceptional_sale_is_rational(target, price) {
                return false;
            }
            // An AI club buys what improves the side by more than the money is worth to it. A club
            // short of bodies may waive the price test, but never the question of whether the
            // player adds anything at all: that is the third keeper a club with two already has,
            // and the fee for a spectator. The floor is the model's own (`MIN_GAIN`), the same one
            // the loan path asks of a borrower, so a fee is not a way round it. Only a club under
            // the legal squad floor, which must find a body to field a side, is excused entirely.
            //
            // A thin club's waiver is for a player who makes the side better *now*. A player whose
            // value is all in a later season is a succession signing, and a succession signing has
            // to be worth its fee on the club's own scale like any other: without that, a club at
            // the floor bought future bench cover with the price test waived, and the cover never
            // started even in its own model.
            if !target.is_user_owned {
                let valued = target_player(target)
                    .and_then(|player| view.appraise(player, price))
                    .is_some_and(|case| match (below_floor, thin) {
                        (true, _) => true,
                        (false, true) => case.is_worth_it() || case.helps_now(),
                        (false, false) => case.is_worth_it(),
                    });
                if !valued {
                    return false;
                }
            }
            buyer_team.transfer_budget >= price as i64
                && buyer_team.finance >= price as i64
                && player_index.get(&target.player_id).is_some_and(|&index| {
                    let player = &game.players[index];
                    let Some(deal_terms) = consent.terms_for(player, &buyer_team, current_date)
                    else {
                        return false;
                    };
                    let facts = buyer_wage_facts.get_or_insert_with(|| {
                        BuyerWageFacts::new(
                            wage_bills.get(&buyer_id).copied().unwrap_or(0),
                            buyer_depths,
                        )
                    });
                    buyer_can_pay_standard_wage(
                        player,
                        &buyer_team,
                        facts,
                        current_date,
                        &deal_terms,
                    ) && (target.is_user_owned
                        || buyer_books.can_carry(
                            i64::from(
                                standard_contract_terms(
                                    player,
                                    &buyer_team,
                                    current_date,
                                    deal_terms.player_weekly_wage,
                                )
                                .map_or(0, |(wage, _)| wage),
                            ),
                            price as i64,
                        ))
                })
        };
        // Every option the buyer could take is weighed by what it adds to the squad and by the
        // country it is in. A club's own market is no longer tried first: a big club hardly
        // minds the border, a small one pays for it, and a clear upgrade is worth the trip to
        // either. The first few options in appeal order are weighed, which bounds the work.
        let mut chosen: Option<(&MarketTarget, f64, u64)> = None;
        // The first few options of each kind, so that a long run of foreign stars at the top of
        // the list cannot hide a good option at home (nor the reverse).
        let mut weighed = [0_usize; 2];
        for target in &shortlist {
            let same_country = team_country
                .get(&target.owner_team_id)
                .is_some_and(|country| *country == buyer_team.country);
            let bucket = usize::from(same_country);
            if weighed[bucket] >= OPTIONS_WEIGHED_PER_BUYER {
                if weighed[1 - bucket] >= OPTIONS_WEIGHED_PER_BUYER {
                    break;
                }
                continue;
            }
            if !acceptable(&target) {
                continue;
            }
            let Some(fee) = price(target) else {
                continue;
            };
            let (improvement, _) = fit(target);
            let priced_points = target_player(target)
                .and_then(|player| view.appraise(player, fee))
                .map_or(0, |case| case.utility_points());
            let rank = target_rank(
                target.score,
                priced_points,
                geographic_affinity(&GeoFacts {
                    same_country,
                    buyer_reputation: buyer_team.reputation,
                    improvement,
                    fee,
                    buyer_budget: buyer_cash,
                    value: target.value,
                }),
            ) * profile.rank_weight(&strategy::TargetFacts {
                age: target.age,
                potential_gap: target.potential_gap,
                improvement,
                fee,
                value: target.value,
            });
            if chosen.is_none_or(|(_, best, _)| rank > best) {
                chosen = Some((target, rank, fee));
            }
            weighed[bucket] += 1;
        }

        let Some((target, _, agreed)) = chosen else {
            // Nothing to buy: perhaps something to borrow.
            if completed_ai_loans < max_ai_loans
                && let Some(player_id) = try_ai_loan(
                    game,
                    LoanSearch {
                        borrower: &buyer_team,
                        levels: &loan_levels,
                        view: &view,
                        books: &buyer_books,
                        targets: &loan_targets,
                        moved_player_ids: &moved_player_ids,
                        current_date,
                    },
                )
            {
                invalidate_loan_views(&sale_view, &loan_targets, &player_id, &buyer_id);
                moved_player_ids.insert(player_id);
                completed_ai_loans += 1;
            }
            continue;
        };
        let candidate = MarketCandidate {
            player_id: target.player_id.clone(),
            owner_team_id: target.owner_team_id.clone(),
            fee: agreed,
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

        // Whether the club would sell him was settled with the choice (`willing_to_sell`).
        if completed_ai_transfers >= max_completed_ai_transfers {
            continue;
        }

        let deal_record = DEAL_LOG_ON
            .load(std::sync::atomic::Ordering::Relaxed)
            .then(|| {
                let player = &game.players[*player_index.get(&target.player_id)?];
                let (improvement, thin) = fit(target);
                let marginal = view.marginal(player);
                let placement = view.placement(player);
                Some(DealRecord {
                    player_id: target.player_id.clone(),
                    buyer_id: buyer_id.clone(),
                    position_group: target.position_group_index,
                    ovr: player.ovr,
                    status: target.status.name(),
                    phase: phase.name(),
                    listed: target.is_listed,
                    fee: candidate.fee,
                    asking: target.fee,
                    value: target.value,
                    age: target.age,
                    contract_days: contract_days_remaining(current_date, player.contract_end()),
                    date: today.clone(),
                    buyer_reputation: buyer_team.reputation,
                    seller_reputation: target.owner_reputation,
                    buyer_country: buyer_team.country.clone(),
                    seller_country: team_country
                        .get(&target.owner_team_id)
                        .cloned()
                        .unwrap_or_default(),
                    improvement,
                    thin,
                    buyer_formation: buyer_team.formation.clone(),
                    buyer_levels: buyer_levels[target.position_group_index].clone(),
                    natural_position: format!("{:?}", player.natural_position),
                    alternate_positions: player
                        .alternate_positions
                        .iter()
                        .map(|position| format!("{position:?}"))
                        .collect(),
                    potential: player.potential,
                    buyer_group_depth: available_buyer_depths[target.position_group_index],
                    buyer_planned_depth: buyer_levels[target.position_group_index].len(),
                    role: marginal.map_or("None", |marginal| marginal.reason.name()),
                    starts_at: marginal.and_then(|marginal| marginal.starts_at),
                    points: marginal.map_or(0, |marginal| marginal.points),
                    gain: marginal.map_or(0.0, |marginal| marginal.gain),
                    weekly_wage: player.wage(),
                    placement: placement.map(|placement| DealPlacement {
                        now_gain: placement.now_gain,
                        model_start: placement.model_start,
                        picker_starts_now: picker_would_start(game, &buyer_team, player),
                        blocker_slot: std::array::from_fn(|year| {
                            placement.blocker[year].map(|blocker| blocker.slot)
                        }),
                        blocker_ovr: std::array::from_fn(|year| {
                            placement.blocker[year].map(|blocker| blocker.ovr)
                        }),
                        blocker_age: std::array::from_fn(|year| {
                            placement.blocker[year].map(|blocker| blocker.age)
                        }),
                        blocker_keeper: std::array::from_fn(|year| {
                            placement.blocker[year].is_some_and(|blocker| blocker.natural_keeper)
                        }),
                    }),
                })
            })
            .flatten();
        // The wage he was judged to come for is the wage he is signed at.
        let deal_terms = player_index
            .get(&candidate.player_id)
            .and_then(|&index| consent.terms_for(&game.players[index], &buyer_team, current_date))
            .unwrap_or_default();
        if completed_ai_loans < max_ai_loans
            && let Some(player_id) = try_ai_loan(
                game,
                LoanSearch {
                    borrower: &buyer_team,
                    levels: &loan_levels,
                    view: &view,
                    books: &buyer_books,
                    targets: &loan_targets,
                    moved_player_ids: &moved_player_ids,
                    current_date,
                },
            )
        {
            invalidate_loan_views(&sale_view, &loan_targets, &player_id, &buyer_id);
            moved_player_ids.insert(player_id);
            completed_ai_loans += 1;
            continue;
        }
        if execute_transfer_with_terms(
            game,
            &candidate.player_id,
            &buyer_id,
            &candidate.owner_team_id,
            candidate.fee,
            &deal_terms,
        )
        .is_ok()
        {
            sale_view.invalidate(&candidate.owner_team_id);
            sale_view.invalidate(&buyer_id);
            if let Some(record) = deal_record {
                DEAL_LOG
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(record);
            }
            if let Some(&index) = player_index.get(&candidate.player_id) {
                if let Some(seller) = club_players.get_mut(&candidate.owner_team_id) {
                    seller.retain(|&held| held != index);
                }
                club_players
                    .entry(buyer_id.clone())
                    .or_default()
                    .push(index);
            }
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

#[cfg(test)]
mod tests {
    use super::{LoanTarget, collect_loan_targets, target_fills_a_need};
    use crate::test_support::uniform_attributes;
    use domain::player::{Player, Position};
    use std::collections::HashMap;

    fn loan_listed(id: &str, owner: &str, position: Position, ovr_attributes: u8) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "1998-01-01".to_string(),
            "England".to_string(),
            position,
            uniform_attributes(ovr_attributes),
        );
        player.team_id = Some(owner.to_string());
        player.loan_listed = true;
        player
    }

    /// Given a squad list where two of three seniors are loan-listed, one of them worse than the
    /// other, when the market collects the targets a borrower may ask for
    /// Then each target names the place its player holds in that list as well as his id, so the
    /// borrower's search reads him straight out of the list instead of scanning every player for
    /// every target, and the better player is offered first.
    #[test]
    fn a_loan_target_names_the_place_and_the_id_of_its_player() {
        let players = vec![
            loan_listed("steady", "seller", Position::CentralMidfielder, 60),
            loan_listed("spare", "seller", Position::Striker, 75),
            Player::new(
                "not-listed".to_string(),
                "not-listed".to_string(),
                "not-listed".to_string(),
                "1998-01-01".to_string(),
                "England".to_string(),
                Position::Goalkeeper,
                uniform_attributes(80),
            ),
        ];
        let depths = HashMap::new();
        let reputation = HashMap::new();
        let targets = collect_loan_targets(&players, None, &depths, &reputation);
        let summary: Vec<(&str, usize, &str)> = targets
            .iter()
            .map(|target: &LoanTarget| {
                (
                    target.player_id.as_str(),
                    target.player_index,
                    players[target.player_index].id.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![("spare", 1, "spare"), ("steady", 0, "steady")]
        );
    }

    /// Given a club carrying the seven defenders it plans for, with four starters and three cover
    /// players, when a defender better than its first cover but worse than its last starter is
    /// offered, then the club passes: it would sell the eighth body it was buying.
    #[test]
    fn a_full_group_pays_only_for_a_first_team_upgrade() {
        let levels = [70, 70, 70, 70, 60, 60, 60];
        assert!(!target_fills_a_need(&levels, 1, 65));
        assert!(target_fills_a_need(&levels, 1, 72));
    }

    /// Given a club one defender short of the seven it plans for, when a defender better than its
    /// first cover is offered, then it fills the need: building toward the plan is allowed.
    #[test]
    fn a_group_below_its_plan_may_upgrade_its_cover() {
        let levels = [70, 70, 70, 70, 60, 60];
        assert!(target_fills_a_need(&levels, 1, 65));
    }

    /// Given a group under the floor plus planning margin, when any player for it is offered, then
    /// he fills the need: a club short of bodies takes one.
    #[test]
    fn a_thin_group_takes_any_body() {
        let levels = [60, 60, 60];
        assert!(target_fills_a_need(&levels, 1, 40));
    }

    /// Given the three keepers a club plans for, when a better backup is offered, then it is not a
    /// fee signing: only a better number one is.
    #[test]
    fn a_goalkeeper_group_at_its_plan_pays_only_for_a_better_number_one() {
        let levels = [75, 65, 60];
        assert!(!target_fills_a_need(&levels, 0, 70));
        assert!(target_fills_a_need(&levels, 0, 76));
    }
}
