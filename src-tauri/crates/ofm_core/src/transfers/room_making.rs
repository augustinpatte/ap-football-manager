//! A club with a full senior list that has found a signing it wants makes room before it signs.
//!
//! The registration limit is 30 seniors, and a place promised to a deal waiting to register or to a
//! senior out on loan is not free (`registration`). A club at the limit used to be left out of the
//! market altogether (`clubs_without_a_senior_place`), so a club that wanted a better full-back and
//! had a surplus fourth keeper could only wait for the keeper to be bought by a club that happened
//! to want him. Here, once the club has chosen a signing it is otherwise able to make (the whole
//! buyer's case, `BuyerTurn::would_sign`), it asks whether a legitimate departure can free the one
//! place that is missing, **and completes that departure first**. Only then does the sweep carry out the
//! signing, through the same authoritative transfer. The departures that count, in the order they
//! are tried:
//!
//! 1. **a permanent sale of a surplus player** to another club that would take him (the same buyer's
//!    case, the same price rule, the same executor as any transfer between two AI clubs);
//! 2. **a squad reassignment** the rules permit: a senior young enough for the academy
//!    ([`crate::roster::YOUTH_ACADEMY_MAX_AGE`], the limit the manager meets) the club would not miss.
//!
//! A departure that is already complete is not made twice: if the club's place was freed earlier in
//! the sweep (a sale it made to another buyer), it simply signs. What does **not** free a place, on
//! purpose: a loan out (the returning loanee holds his place for the whole loan), a transfer listing
//! (nobody has bought him), and a sale agreed but not yet registered (it can still be voided).
//! A contract that runs out before a signing's registration date cannot occur here: an AI club's
//! signing registers the day it is made.
//!
//! The player to sell is never a starter: he is one the club's own squad model calls surplus or
//! unwanted (`PlayerStatus`), outside the eleven it fields, and not one the manager has protected;
//! the club does not give up a first choice to buy a slightly better one. The signing itself has to
//! be worth its fee on the club's own scale (not the waiver a club short of bodies gets). The
//! manager's club never takes part: it is neither a seller nor a taker here, so nothing of the
//! manager's is sold, reassigned or bought without the manager.
//!
//! The counters and the list of swaps at the end are measurement only, like the funnel in
//! `interest`: nothing recorded feeds a decision.

use super::buyer_turn::{BuyerTurn, SweepFacts};
use super::buyer_view::BuyerView;
use super::*;

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// How many surplus players a club offers to clubs, at most, while it looks for the one whose sale
/// has a taker.
const MAX_DEPARTURES_TRIED: usize = 3;

/// How many clubs are weighed as the taker of one surplus player. Each is a full buyer's turn, so
/// this bounds the cost of an attempt.
const MAX_TAKERS_WEIGHED: usize = 6;

/// Whether the sweep makes room at all. On by default; the harness turns it off to measure the
/// market as it was before this module (the baseline of the ALPHA6.1 studies), in one binary.
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_room_making_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub(super) fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// What the sweep knows that room-making reads and never changes.
pub(super) struct RoomFacts<'a> {
    pub sweep: &'a SweepFacts<'a>,
    pub asking: &'a sale_view::AskingContext<'a>,
    pub statuses: &'a HashMap<String, PlayerStatus>,
    pub position_depths: &'a HashMap<String, [usize; 4]>,
    pub team_reputation: &'a HashMap<String, u32>,
    /// Each club's seniors' ratings per group when the sweep began, to choose whom to ask.
    pub squad_levels: &'a HashMap<String, [Vec<u8>; 4]>,
    pub user_team_id: Option<&'a str>,
    pub max_completed_ai_transfers: usize,
}

/// What a sweep changes as it goes, kept in one place so that every completed move updates all of
/// it the same way.
pub(super) struct SweepBooks<'a> {
    pub club_players: &'a mut HashMap<String, Vec<usize>>,
    pub wage_bills: &'a mut HashMap<String, i64>,
    pub moved_player_ids: &'a mut HashSet<String>,
    pub completed_ai_transfers: &'a mut usize,
    /// The clubs with no place for one more senior, kept current as deals complete.
    pub clubs_without_a_place: &'a mut HashSet<String>,
}

impl SweepBooks<'_> {
    /// Records a completed permanent transfer: where each club's players stand, the day's count,
    /// both clubs' wage bills (either may shop or take a player later today) and who has a place.
    pub(super) fn note_completed_transfer(
        &mut self,
        game: &Game,
        facts: &SweepFacts<'_>,
        player_id: &str,
        seller_id: &str,
        buyer_id: &str,
    ) {
        facts.sale_view.invalidate(seller_id);
        facts.sale_view.invalidate(buyer_id);
        if let Some(&index) = facts.player_index.get(player_id) {
            if let Some(seller) = self.club_players.get_mut(seller_id) {
                seller.retain(|&held| held != index);
            }
            self.club_players
                .entry(buyer_id.to_string())
                .or_default()
                .push(index);
        }
        self.moved_player_ids.insert(player_id.to_string());
        *self.completed_ai_transfers += 1;
        for club in [seller_id, buyer_id] {
            self.wage_bills
                .insert(club.to_string(), calc_wages(game, club));
            self.refresh_place(game, club);
        }
    }

    /// Whether `club_id` has a place for one more senior, read afresh (a loan or a reassignment
    /// moves it as a sale does).
    pub(super) fn refresh_place(&mut self, game: &Game, club_id: &str) {
        refresh_place(game, self.clubs_without_a_place, club_id);
    }
}

/// Whether `club_id` has a place for one more senior, read afresh into `clubs_without_a_place`.
pub(super) fn refresh_place(
    game: &Game,
    clubs_without_a_place: &mut HashSet<String>,
    club_id: &str,
) {
    if signing_place_refusal(game, club_id, "").is_some() {
        clubs_without_a_place.insert(club_id.to_string());
    } else {
        clubs_without_a_place.remove(club_id);
    }
}

/// The signing a club wants to make: the player, what it will pay and the terms he agreed.
pub(super) struct Signing<'a> {
    pub target: &'a MarketTarget,
    pub fee: u64,
    pub terms: &'a TransferTerms,
}

/// What a club's attempt to make room came to.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum RoomOutcome {
    /// The club had a place already: a departure completed earlier in the sweep.
    AlreadyFree,
    /// A departure completed: the signing may go ahead.
    Made(Departure),
    /// No legitimate departure was found, and why.
    Refused(Refusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    /// More than one place is missing (places promised to loans and agreed deals): one departure
    /// cannot make room, and the club does not sell several players for one.
    TooManyPlacesShort,
    /// The signing would fail for another reason once the place was there.
    SigningWouldFail,
    /// The signing is not worth a departure on the club's own scale.
    NotWorthADeparture,
    /// Nobody in the squad is a player the club would let go.
    NoOneToLetGo,
    /// A player the club would let go, and nobody to take him.
    NoTaker,
}

/// A departure that freed a place.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Departure {
    kind: DepartureKind,
    player: RoomPlayer,
    /// The club that took him, for a sale.
    taker: Option<String>,
    fee: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DepartureKind {
    PermanentSale,
    AcademyReassignment,
}

impl DepartureKind {
    fn name(self) -> &'static str {
        match self {
            Self::PermanentSale => "permanent_sale",
            Self::AcademyReassignment => "academy_reassignment",
        }
    }
}

/// Makes one place free on `club`'s senior list for `signing`, by a departure that completes now.
pub(super) fn make_room(
    game: &mut Game,
    facts: &RoomFacts<'_>,
    books: &mut SweepBooks<'_>,
    club: &BuyerTurn,
    signing: &Signing<'_>,
) -> RoomOutcome {
    let club_id = club.team.id.as_str();
    let signed_id = signing.target.player_id.as_str();
    note(Stat::FoundTargetLackedSpace, club_id);
    let short = places_to_free_for_a_signing(game, club_id, signed_id);
    if short == 0 {
        books.refresh_place(game, club_id);
        note(Stat::AlreadyFree, club_id);
        return RoomOutcome::AlreadyFree;
    }
    note(Stat::Attempted, club_id);
    let refused = |refusal: Refusal, stat: Stat| {
        note(stat, club_id);
        RoomOutcome::Refused(refusal)
    };
    if short > 1 {
        return refused(Refusal::TooManyPlacesShort, Stat::TooManyPlacesShort);
    }
    if purchase_refusal_but_for_a_place(
        game,
        signed_id,
        club_id,
        &signing.target.owner_team_id,
        signing.fee,
        signing.terms,
    )
    .is_err()
    {
        return refused(Refusal::SigningWouldFail, Stat::SigningWouldFail);
    }
    let worth_a_departure = facts
        .sweep
        .player_index
        .get(signed_id)
        .map(|&index| &game.players[index])
        .and_then(|player| club.view.appraise(player, signing.fee))
        .is_some_and(|case| case.is_worth_it());
    if !worth_a_departure {
        return refused(Refusal::NotWorthADeparture, Stat::NotWorthADeparture);
    }
    let candidates = departure_candidates(game, facts, books, club);
    if candidates.is_empty() {
        return refused(Refusal::NoOneToLetGo, Stat::NoOneToLetGo);
    }
    // Two completed deals are spent on a sale and the signing it makes room for: the day's cap on
    // the market's volume is not a way round, so a swap has to fit under it whole.
    if *books.completed_ai_transfers + 2 <= facts.max_completed_ai_transfers {
        let mut offered = 0;
        for &index in &candidates {
            match sell_surplus_player(game, facts, books, club, index) {
                Sale::Completed(departure) => {
                    note(Stat::FreedBySale, club_id);
                    return RoomOutcome::Made(departure);
                }
                // Only a player some club was weighed for counts against the bound: a club has
                // many players nobody wants, and they must not crowd out the one somebody does.
                Sale::NoTaker { weighed } if weighed > 0 => offered += 1,
                Sale::NoTaker { .. } => {}
                Sale::Refused => offered += 1,
            }
            if offered >= MAX_DEPARTURES_TRIED {
                break;
            }
        }
    } else {
        note(Stat::CapReached, club_id);
    }
    for &index in &candidates {
        if let Some(departure) = reassign_to_academy(game, facts, books, club_id, index) {
            note(Stat::FreedByReassignment, club_id);
            return RoomOutcome::Made(departure);
        }
    }
    refused(Refusal::NoTaker, Stat::NoTaker)
}

/// The club's players it would let go to make room, the ones it would most like to see go first:
/// where it holds the squad's surplus (`PlayerStatus`), outside the eleven it fields, not on loan
/// or in a deal, not protected, and not one whose departure would leave a group below the floor.
fn departure_candidates(
    game: &Game,
    facts: &RoomFacts<'_>,
    books: &SweepBooks<'_>,
    club: &BuyerTurn,
) -> Vec<usize> {
    let club_id = club.team.id.as_str();
    let mut candidates: Vec<(usize, bool)> = books
        .club_players
        .get(club_id)
        .into_iter()
        .flatten()
        .copied()
        .filter_map(|index| {
            let player = &game.players[index];
            let status = facts.statuses.get(&player.id).copied()?;
            let unwanted = match status {
                PlayerStatus::Unwanted => true,
                PlayerStatus::Surplus => false,
                _ => return None,
            };
            (player.team_id.as_deref() == Some(club_id)
                && player.squad_role == domain::player::SquadRole::Senior
                && !player.retired
                && player.active_loan.is_none()
                && !player_has_pending_registration(player)
                && !books.moved_player_ids.contains(&player.id)
                && !club.team.starting_xi_ids.contains(&player.id)
                && !game.delegation.is_protected(&player.id))
            .then_some((index, unwanted))
        })
        .collect();
    // Unwanted before surplus; the dearer wage first (the saving is the larger), then the more
    // valuable (the fee is the larger), then the id, so the answer does not depend on storage order.
    candidates.sort_by(|&(a, a_unwanted), &(b, b_unwanted)| {
        let (a, b) = (&game.players[a], &game.players[b]);
        b_unwanted
            .cmp(&a_unwanted)
            .then_with(|| b.wage().cmp(&a.wage()))
            .then_with(|| b.market_value.cmp(&a.market_value))
            .then_with(|| a.id.cmp(&b.id))
    });
    candidates.into_iter().map(|(index, _)| index).collect()
}

/// What came of offering a surplus player to the market.
enum Sale {
    Completed(Departure),
    /// Nobody took him; `weighed` clubs were asked.
    NoTaker {
        weighed: usize,
    },
    /// The transfer was refused (and neither club has changed).
    Refused,
}

/// Sells `game.players[index]` to the club that would pay the most for him among those that want
/// him, through the same transfer every purchase between two AI clubs is.
fn sell_surplus_player(
    game: &mut Game,
    facts: &RoomFacts<'_>,
    books: &mut SweepBooks<'_>,
    club: &BuyerTurn,
    index: usize,
) -> Sale {
    let club_id = club.team.id.as_str();
    let player = &game.players[index];
    // His club would let him go: it asks the price it asks for a player it has put up for sale.
    let status = facts
        .statuses
        .get(&player.id)
        .copied()
        .unwrap_or(PlayerStatus::Surplus);
    let age = domain::age::age_on_or_unknown(facts.sweep.current_date, &player.date_of_birth);
    let offered = MarketTarget {
        player_id: player.id.clone(),
        owner_team_id: club_id.to_string(),
        is_user_owned: false,
        score: incoming_interest_score(facts.sweep.current_date, player),
        fee: facts.asking.fee_for(player, club_id, status, true),
        position_group_index: crate::squad_floor::group_index(&player.position),
        owner_reputation: facts.team_reputation.get(club_id).copied().unwrap_or(0),
        ovr: player.ovr,
        is_listed: true,
        age,
        potential_gap: i32::from(player.potential) - i32::from(player.ovr),
        value: player.market_value,
        status,
        contract_days: contract_days_remaining(facts.sweep.current_date, player.contract_end()),
        sporting_only: false,
    };
    if crate::squad_floor::ensure_departure_keeps_floor(game, &offered.player_id).is_err() {
        return Sale::NoTaker { weighed: 0 };
    }
    let record = RoomPlayer::of(player, age, status);
    let search = best_taker(game, facts, books, &offered);
    let Some(taker) = search.best else {
        return Sale::NoTaker {
            weighed: search.weighed,
        };
    };
    let executed = execute_transfer_with_terms(
        game,
        &offered.player_id,
        &taker.club_id,
        club_id,
        taker.fee,
        &taker.terms,
    );
    if executed.is_err() {
        note(Stat::SaleRefused, club_id);
        return Sale::Refused;
    }
    books.note_completed_transfer(
        game,
        facts.sweep,
        &offered.player_id,
        club_id,
        &taker.club_id,
    );
    Sale::Completed(Departure {
        kind: DepartureKind::PermanentSale,
        player: record,
        taker: Some(taker.club_id),
        fee: taker.fee,
    })
}

/// The clubs weighed as takers for a surplus player, and the one that would pay most.
struct TakerSearch {
    weighed: usize,
    best: Option<Taker>,
}

/// A club that would take a surplus player, and on what terms.
struct Taker {
    club_id: String,
    fee: u64,
    terms: TransferTerms,
}

fn best_taker(
    game: &Game,
    facts: &RoomFacts<'_>,
    books: &SweepBooks<'_>,
    offered: &MarketTarget,
) -> TakerSearch {
    let sweep = facts.sweep;
    let group = offered.position_group_index;
    let mut clubs: Vec<(&Team, bool, u32)> =
        game.teams
            .iter()
            .filter(|team| {
                team.id != offered.owner_team_id
                    && Some(team.id.as_str()) != facts.user_team_id
                    && !books.clubs_without_a_place.contains(&team.id)
                    && facts.squad_levels.get(&team.id).is_some_and(|levels| {
                        target_fills_a_need(&levels[group], group, offered.ovr)
                    })
                    && offered.owner_reputation.saturating_sub(team.reputation)
                        <= MAX_BUYER_REPUTATION_DEFICIT as u32
            })
            .map(|team| {
                let same_country = sweep.team_country.get(&team.id)
                    == sweep.team_country.get(&offered.owner_team_id);
                (
                    team,
                    same_country,
                    team.reputation.abs_diff(offered.owner_reputation),
                )
            })
            .collect();
    // Nearest first (same country, closest in stature), so a bounded search looks where the
    // player would realistically go; the id breaks ties.
    clubs.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.id.cmp(&b.0.id))
    });
    let Some(&index) = sweep.player_index.get(&offered.player_id) else {
        return TakerSearch {
            weighed: 0,
            best: None,
        };
    };
    let player = &game.players[index];
    let mut best: Option<Taker> = None;
    let mut weighed = 0;
    for (team, _, _) in clubs.into_iter().take(MAX_TAKERS_WEIGHED) {
        weighed += 1;
        let mut turn = BuyerTurn::new(
            game,
            sweep,
            books.wage_bills,
            books.club_players,
            facts.position_depths,
            team.clone(),
        );
        turn.counted = false;
        if !turn.would_sign(game, sweep, books.club_players, offered) {
            continue;
        }
        let (Some(fee), Some(terms)) = (
            turn.price(game, sweep, offered),
            sweep
                .consent
                .terms_for(player, &turn.team, sweep.current_date),
        ) else {
            continue;
        };
        if best.as_ref().is_none_or(|held| fee > held.fee) {
            best = Some(Taker {
                club_id: turn.team.id.clone(),
                fee,
                terms,
            });
        }
    }
    TakerSearch { weighed, best }
}

/// Registers one of the club's seniors with the academy, as the rules let the manager do with a
/// player no older than the academy's limit, when the club would not miss him. A reassignment is
/// not a sale: no fee, and the player stays at the club.
fn reassign_to_academy(
    game: &mut Game,
    facts: &RoomFacts<'_>,
    books: &mut SweepBooks<'_>,
    club_id: &str,
    index: usize,
) -> Option<Departure> {
    let player = &game.players[index];
    let age = domain::age::age_on_or_unknown(facts.sweep.current_date, &player.date_of_birth);
    if age > crate::roster::YOUTH_ACADEMY_MAX_AGE {
        return None;
    }
    let player_id = player.id.clone();
    crate::squad_floor::ensure_departure_keeps_floor(game, &player_id).ok()?;
    let status = facts
        .statuses
        .get(&player_id)
        .copied()
        .unwrap_or(PlayerStatus::Surplus);
    let record = RoomPlayer::of(player, age, status);
    game.players[index].squad_role = domain::player::SquadRole::Youth;
    if let Some(team) = game.teams.iter_mut().find(|team| team.id == club_id) {
        team.remove_player_references(&player_id);
    }
    // The club's senior group changed: a sale judged from the old squad would be stale.
    facts.sweep.sale_view.invalidate(club_id);
    books.refresh_place(game, club_id);
    Some(Departure {
        kind: DepartureKind::AcademyReassignment,
        player: record,
        taker: None,
        fee: 0,
    })
}

// --- Measurement -------------------------------------------------------------------------------

/// One question a club's attempt to make room answers, in the order the report prints them.
#[derive(Debug, Clone, Copy)]
enum Stat {
    /// A club with a full list chose a signing it could not register.
    FoundTargetLackedSpace,
    /// It looked for a departure.
    Attempted,
    /// Its place had been freed earlier in the sweep.
    AlreadyFree,
    FreedBySale,
    FreedByReassignment,
    TooManyPlacesShort,
    SigningWouldFail,
    NotWorthADeparture,
    NoOneToLetGo,
    NoTaker,
    CapReached,
    /// A sale it had arranged was refused by the transfer.
    SaleRefused,
    /// The signing completed after the room was made.
    SigningCompleted,
    /// The signing was refused after the room was made.
    SigningFailedAfterRoom,
}

pub const ROOM_STAT_NAMES: [&str; 14] = [
    "found_target_lacked_space",
    "attempted_to_free_space",
    "already_free_by_earlier_departure",
    "freed_by_permanent_sale",
    "freed_by_academy_reassignment",
    "refused_too_many_places_short",
    "refused_signing_would_fail",
    "refused_not_worth_a_departure",
    "refused_no_one_to_let_go",
    "refused_no_taker",
    "sale_blocked_by_daily_cap",
    "sale_refused_by_transfer",
    "signing_completed_after_room",
    "signing_failed_after_room",
];

/// A player as a swap names him.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomPlayer {
    pub player_id: String,
    pub name: String,
    pub ovr: u8,
    pub potential: u8,
    pub age: i32,
    pub position_group: usize,
    pub weekly_wage: u32,
    pub value: u64,
    pub status: &'static str,
    /// For a signing: the rating points he adds to the side the club fields, in its own model, and
    /// whether he would start in it. `None` for the player who left.
    pub added_points: Option<i32>,
    pub would_start: Option<bool>,
}

impl RoomPlayer {
    fn of(player: &Player, age: i32, status: PlayerStatus) -> Self {
        Self {
            player_id: player.id.clone(),
            name: player.full_name.clone(),
            ovr: player.ovr,
            potential: player.potential,
            age,
            position_group: crate::squad_floor::group_index(&player.position),
            weekly_wage: player.wage(),
            value: player.market_value,
            status: status.name(),
            added_points: None,
            would_start: None,
        }
    }
}

/// One departure made to make room for a signing, and what became of the signing.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomSwap {
    pub date: String,
    pub club_id: String,
    pub departure: &'static str,
    pub departed: RoomPlayer,
    pub taker: Option<String>,
    /// What the club was paid for him (0 for a reassignment).
    pub fee_received: u64,
    pub signed: RoomPlayer,
    pub fee_paid: u64,
    pub signing_completed: bool,
}

#[derive(Default)]
struct RoomLog {
    counters: [u64; ROOM_STAT_NAMES.len()],
    found_target_lacked_space: BTreeSet<String>,
    attempted: BTreeSet<String>,
    freed: BTreeSet<String>,
    failed: BTreeSet<String>,
    swaps: Vec<RoomSwap>,
}

static LOG: Mutex<RoomLog> = Mutex::new(RoomLog {
    counters: [0; ROOM_STAT_NAMES.len()],
    found_target_lacked_space: BTreeSet::new(),
    attempted: BTreeSet::new(),
    freed: BTreeSet::new(),
    failed: BTreeSet::new(),
    swaps: Vec::new(),
});

fn log() -> std::sync::MutexGuard<'static, RoomLog> {
    LOG.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The swaps kept for the report. A long career in a long-lived process would otherwise grow the
/// list for as long as it runs; the counters above it are not bounded by this.
const MAX_SWAPS_KEPT: usize = 5_000;

fn note(stat: Stat, club_id: &str) {
    let mut log = log();
    log.counters[stat as usize] += 1;
    let clubs = match stat {
        Stat::FoundTargetLackedSpace => &mut log.found_target_lacked_space,
        Stat::Attempted => &mut log.attempted,
        Stat::FreedBySale | Stat::FreedByReassignment | Stat::AlreadyFree => &mut log.freed,
        Stat::TooManyPlacesShort
        | Stat::SigningWouldFail
        | Stat::NotWorthADeparture
        | Stat::NoOneToLetGo
        | Stat::NoTaker => &mut log.failed,
        Stat::CapReached
        | Stat::SaleRefused
        | Stat::SigningCompleted
        | Stat::SigningFailedAfterRoom => return,
    };
    clubs.insert(club_id.to_string());
}

/// The signing a club made room for has been tried: records the swap and how it ended.
pub(super) fn record_swap(
    club_id: &str,
    date: &str,
    departure: Departure,
    signing: &Signing<'_>,
    signed: RoomPlayer,
    completed: bool,
) {
    note(
        if completed {
            Stat::SigningCompleted
        } else {
            Stat::SigningFailedAfterRoom
        },
        club_id,
    );
    let mut log = log();
    if log.swaps.len() >= MAX_SWAPS_KEPT {
        return;
    }
    log.swaps.push(RoomSwap {
        date: date.to_string(),
        club_id: club_id.to_string(),
        departure: departure.kind.name(),
        departed: departure.player,
        taker: departure.taker,
        fee_received: departure.fee,
        signed,
        fee_paid: signing.fee,
        signing_completed: completed,
    });
}

impl RoomPlayer {
    /// The player a club is signing, for the swap's record, as the club's own model reads him.
    pub(super) fn signing(player: &Player, target: &MarketTarget, view: &BuyerView) -> Self {
        Self {
            added_points: Some(view.improvement(player)),
            would_start: Some(view.marginal(player).is_some_and(|m| m.starts_at.is_some())),
            ..Self::of(player, target.age, target.status)
        }
    }
}

/// What the sweeps made of room-making since the last reset, for the harness.
#[derive(Debug, Clone, Default)]
pub struct RoomMakingReport {
    pub counters: Vec<(&'static str, u64)>,
    pub clubs_found_target_lacked_space: usize,
    pub clubs_attempted: usize,
    pub clubs_freed_a_place: usize,
    pub clubs_refused: usize,
    pub swaps: Vec<RoomSwap>,
}

pub fn room_making_report() -> RoomMakingReport {
    let log = log();
    RoomMakingReport {
        counters: ROOM_STAT_NAMES.iter().copied().zip(log.counters).collect(),
        clubs_found_target_lacked_space: log.found_target_lacked_space.len(),
        clubs_attempted: log.attempted.len(),
        clubs_freed_a_place: log.freed.len(),
        clubs_refused: log.failed.len(),
        swaps: log.swaps.clone(),
    }
}

pub fn reset_room_making_report() {
    *log() = RoomLog::default();
}
