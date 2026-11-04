//! Suspensions (Run F4.2).
//!
//! A sending-off costs more than the match it happens in. A straight red card keeps a player out of
//! the next [`DIRECT_RED_BAN`] matches of his club, a second yellow of the next [`SECOND_YELLOW_BAN`];
//! the countdown is by the club's matches, so it runs through a quiet fortnight and into the next
//! season without a day being counted. The ban is set from the match report's own card events (the
//! referee's decisions in the engine that played the match) and nothing else: no event, no ban.
//! It applies to every match played through the career's finalisation, whichever engine played it.
//!
//! Yellow cards accumulate too: every [`YELLOW_CARDS_PER_BAN`]th yellow card of a season, in any
//! competition of the club, is a one-match ban (the common threshold of the leagues, without their
//! escalation at ten and fifteen). The count is the season's `stats.yellow_cards`, which resets with
//! the rest of the season's record at the rollover, so a ban is not carried into the next season.
//!
//! Who may play is [`domain::player::Player::can_play`], read by the squad selection of the user's
//! assistant and of the computer-managed clubs alike.

use crate::game::Game;

/// Matches missed after a straight red card (the usual minimum of the main leagues).
pub const DIRECT_RED_BAN: u8 = 3;
/// Matches missed after a second yellow card.
pub const SECOND_YELLOW_BAN: u8 = 1;

/// A season's yellow cards, counted every [`YELLOW_CARDS_PER_BAN`], cost the player one match.
pub const YELLOW_CARDS_PER_BAN: u32 = 5;
/// Matches missed for reaching the yellow-card threshold.
pub const YELLOW_ACCUMULATION_BAN: u8 = 1;

/// One match of the clubs at `positions` (indices into `game.players`) has been played: players who
/// were suspended for it have served one match, then the sendings-off of the report set new bans.
/// Order matters: a ban imposed by this match is first served in the next one. The player
/// statistics of the match have been applied already, so `stats.yellow_cards` is the season's
/// count including this match's.
pub(crate) fn settle(game: &mut Game, positions: &[usize], report: &engine::MatchReport) {
    for &position in positions {
        let player = &mut game.players[position];
        player.suspended_matches = player.suspended_matches.saturating_sub(1);
    }
    for event in &report.events {
        let ban = match event.event_type {
            engine::EventType::RedCard => DIRECT_RED_BAN,
            engine::EventType::SecondYellow => SECOND_YELLOW_BAN,
            _ => continue,
        };
        let Some(id) = event.player_id.as_deref() else {
            continue;
        };
        if let Some(&position) = positions.iter().find(|&&i| game.players[i].id == id) {
            let player = &mut game.players[position];
            player.suspended_matches = player.suspended_matches.max(ban);
        }
    }
    for &position in positions {
        let player = &mut game.players[position];
        let Some(booked) = report
            .player_stats
            .get(&player.id)
            .map(|s| u32::from(s.yellow_cards))
        else {
            continue;
        };
        let after = player.stats.yellow_cards;
        // A threshold crossed by one of this match's cards, not one already behind the player.
        if booked > 0
            && after / YELLOW_CARDS_PER_BAN > after.saturating_sub(booked) / YELLOW_CARDS_PER_BAN
        {
            player.suspended_matches = player.suspended_matches.max(YELLOW_ACCUMULATION_BAN);
        }
    }
}
