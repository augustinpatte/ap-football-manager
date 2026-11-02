//! Test support: lets a free-agent negotiation run its course the way the daily clock would.

use chrono::Duration;
use ofm_core::contracts::process_free_agent_talks;
use ofm_core::game::Game;

/// Most days a player takes to answer an offer (`MAX_RESPONSE_DAYS`), and one more for the day the
/// answer is acted on.
pub const DAYS_TO_AN_ANSWER: u32 = 3;

/// Plays `days` days of the free-agent market: the date moves on, then the day's talks are processed.
pub fn run_days(game: &mut Game, days: u32) {
    for _ in 0..days {
        game.clock.current_date += Duration::days(1);
        process_free_agent_talks(game);
    }
}
