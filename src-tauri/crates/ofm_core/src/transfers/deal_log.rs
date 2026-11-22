//! The harness's trace of completed AI transfers: what the market knew when it made each one.
//!
//! Measurement only. Nothing recorded here feeds a decision, and nothing is recorded unless the
//! harness switches the log on.

use crate::game::Game;
use domain::player::Player;
use domain::team::Team;

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

pub(super) static DEAL_LOG_ON: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
pub(super) static DEAL_LOG: std::sync::Mutex<Vec<DealRecord>> = std::sync::Mutex::new(Vec::new());

pub fn enable_deal_log() {
    DEAL_LOG_ON.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// The deals recorded since the last call.
pub fn take_deal_log() -> Vec<DealRecord> {
    std::mem::take(&mut *DEAL_LOG.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Whether the lineup picker would field `signing` today in `buyer`'s side, from the buyer's healthy
/// seniors and him. Measurement only: it asks the picker that picks the side, not the model.
pub(super) fn picker_would_start(game: &Game, buyer: &Team, signing: &Player) -> bool {
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
