//! Leaf helpers shared across the contract clusters: owned-player lookups,
//! wage/tenure maths, and contract-date arithmetic. Byte-faithful extraction
//! from the original contracts.rs.

use super::*;

pub(crate) fn owned_player<'a>(game: &'a Game, player_id: &str) -> Result<&'a Player, String> {
    let manager_team_id = game
        .manager
        .team_id
        .as_deref()
        .ok_or(ERR_NO_TEAM_ASSIGNED.to_string())?;
    let player = game
        .players
        .iter()
        .find(|candidate| candidate.id == player_id)
        .ok_or(ERR_PLAYER_NOT_FOUND.to_string())?;

    if player.contract_club_id() != Some(manager_team_id) {
        return Err(ERR_PLAYER_NOT_OWNED_BY_CLUB.to_string());
    }

    Ok(player)
}

pub(crate) fn owned_player_index(game: &Game, player_id: &str) -> Result<usize, String> {
    let manager_team_id = game
        .manager
        .team_id
        .as_deref()
        .ok_or(ERR_NO_TEAM_ASSIGNED.to_string())?;
    let player_index = game
        .players
        .iter()
        .position(|candidate| candidate.id == player_id)
        .ok_or(ERR_PLAYER_NOT_FOUND.to_string())?;

    if game.players[player_index].contract_club_id() != Some(manager_team_id) {
        return Err(ERR_PLAYER_NOT_OWNED_BY_CLUB.to_string());
    }

    Ok(player_index)
}

pub(crate) fn backend_text_with_param(key: &str, param_name: &str, param_value: &str) -> String {
    let mut message = String::with_capacity(key.len() + param_name.len() + param_value.len() + 2);
    message.push_str(key);
    message.push('?');
    message.push_str(param_name);
    message.push('=');
    message.push_str(param_value);
    message
}

pub(crate) fn expected_wage(player: &Player, team: &Team, current_date: NaiveDate) -> u32 {
    let mut wage = reference_player_wage(player) as f32;
    let age = domain::age::age_on_or_unknown(current_date, &player.date_of_birth);
    let remaining_days = remaining_contract_days(player, current_date);

    if age <= 27 {
        wage *= 1.05;
    } else if age >= 32 {
        wage *= 0.95;
    }

    if player.morale <= 50 {
        wage *= 1.10;
    }

    wage *= importance_wage_multiplier(player);

    if team.reputation < 40 {
        wage *= 1.05;
    }

    if remaining_days <= 180 {
        wage *= 1.10;
    } else if remaining_days <= 365 {
        wage *= 1.05;
    }

    let rounded = round_up_wage(wage.ceil() as u32);
    rounded.max(reference_player_wage(player))
}

pub(crate) fn reference_player_wage(player: &Player) -> u32 {
    // What he is paid, or what he last was: a free agent's going rate is his own history, not a
    // figure derived from his market value, which is a different scale in a world whose pay was
    // not generated from values (an FM24 club pays a tenth of `value / 200`).
    let known = player.last_known_wage();
    if known > 0 {
        return known;
    }

    let derived_wage = (player.market_value / MARKET_VALUE_TO_WAGE_RATIO).max(MINIMUM_DEFAULT_WAGE);

    round_up_wage(derived_wage.min(u32::MAX as u64) as u32)
}

/// What a club that has room offers a player it wants to keep whose pay has fallen behind his value:
/// the wage his market value implies.
///
/// A wage that only ever follows the wage he was paid last never follows his rating: an academy
/// graduate rated 75 who started on the youth minimum asks for a tenth more at every renewal, and a
/// club with a hundred million in the bank has no way to pay him what he is worth. F5.3 measured what
/// that does once the wage ratchet is closed: the rich clubs used 60 to 70 % of their wage budgets,
/// the poor ones all of theirs, and the strongest quarter of the world's clubs stopped being
/// stronger than the weakest. Whether the club can afford the catch-up is not decided here
/// (`ai_contracts::affordable_renewal_wage` holds it to the room under the wage ceiling), so this
/// raises nothing at a club with none: a club spends what it has, no more.
pub(crate) fn quality_wage_catch_up(player: &Player) -> u32 {
    if player.last_known_wage() == 0 {
        return 0;
    }
    round_up_wage(
        (player.market_value / MARKET_VALUE_TO_WAGE_RATIO).min(u64::from(u32::MAX)) as u32,
    )
}

pub(crate) fn importance_wage_multiplier(player: &Player) -> f32 {
    if player.market_value >= 2_000_000 {
        return 1.18;
    }

    if player.market_value >= 750_000 {
        return 1.10;
    }

    if player.market_value <= 150_000 {
        return 0.95;
    }

    1.0
}

pub(crate) fn expected_contract_years(player: &Player, current_date: NaiveDate) -> u32 {
    let age = domain::age::age_on_or_unknown(current_date, &player.date_of_birth);

    if age <= 28 {
        return 3;
    }

    if age <= 32 {
        return 2;
    }

    1
}

pub(crate) fn minimum_acceptable_wage(current_wage: u32) -> u32 {
    ((current_wage as f32) * 0.85).floor() as u32
}

/// The lowest wage a player will not take as an insult, for the wage he anchors on.
pub(crate) fn insulting_floor(anchor_wage: u32) -> u32 {
    ((anchor_wage as f32) * 0.65).floor() as u32
}

pub(crate) fn is_insulting_wage_offer(
    reference_wage: u32,
    expected_wage: u32,
    offered_wage: u32,
) -> bool {
    offered_wage < insulting_floor(reference_wage.max(expected_wage))
}

/// What an AI club bids a free agent who `asks` a wage when it can carry `room` more a week: what
/// it can carry, if that is more than he would take as an insult and less than he asks, so a club
/// adapts its recruitment to its means instead of passing over everyone priced on a richer scale.
/// A bid never exceeds the ask and never falls below the insult line: the club that cannot reach
/// it does not make the bid worth making, and the caller decides whether it must sign anyway.
pub(crate) fn free_agent_bid(asks: u32, reference_wage: u32, room: i64) -> u32 {
    let room = u32::try_from(room.max(0)).unwrap_or(u32::MAX);
    asks.min(room)
        .max(insulting_floor(reference_wage.max(asks)))
}

pub(crate) fn remaining_contract_days(player: &Player, current_date: NaiveDate) -> i64 {
    contract_days_remaining(player.contract_end(), current_date)
        .unwrap_or(0)
        .max(0)
}

/// Round a weekly wage up to the grain a wage of that size is agreed in: whole thousands from
/// 10 000 (as it has always been), hundreds from 1 000, tens below.
///
/// A flat thousand turned a 700 wage into 1 000 (+43 %) at every renewal, so in a world whose pay
/// scale is a tenth of the generated one a club's wage bill climbed by rounding alone.
pub(crate) fn round_up_wage(value: u32) -> u32 {
    let grain = match value {
        0 => return 0,
        1..=999 => 10,
        1_000..=9_999 => 100,
        _ => 1_000,
    };
    value.div_ceil(grain) * grain
}

/// A contract date as stored: `"YYYY-MM-DD"`, or nothing if it is not one.
///
/// Dates are plain strings on the wire and in saves, so every reader parses them;
/// this is the one place that says how.
pub(crate) fn parse_contract_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

pub(crate) fn contract_days_remaining(
    contract_end: Option<&str>,
    current_date: NaiveDate,
) -> Option<i64> {
    let contract_end_date = parse_contract_date(contract_end?)?;
    Some((contract_end_date - current_date).num_days())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::{PlayerAttributes, Position};
    use domain::team::Team;

    /// Given weekly wages across the scales a world can have
    /// When they are rounded up for an agreement
    /// Then the grain follows the size: thousands from 10 000, hundreds from 1 000, tens below.
    #[test]
    fn a_wage_is_rounded_up_to_the_grain_of_its_size() {
        assert_eq!(round_up_wage(0), 0);
        assert_eq!(round_up_wage(731), 740);
        assert_eq!(round_up_wage(1_000), 1_000);
        assert_eq!(round_up_wage(1_201), 1_300);
        assert_eq!(round_up_wage(9_999), 10_000);
        assert_eq!(round_up_wage(12_001), 13_000);
        assert_eq!(round_up_wage(250_001), 251_000);
    }

    fn attributes() -> PlayerAttributes {
        PlayerAttributes {
            pace: 50,
            stamina: 50,
            strength: 50,
            agility: 50,
            passing: 50,
            shooting: 50,
            tackling: 50,
            dribbling: 50,
            defending: 50,
            positioning: 50,
            vision: 50,
            decisions: 50,
            composure: 50,
            aggression: 50,
            teamwork: 50,
            leadership: 50,
            handling: 50,
            reflexes: 50,
            aerial: 50,
        }
    }

    /// Given a fringe player paid 700 a week in a world whose wages are small
    /// When his club works out what he expects for a renewal
    /// Then the ask is his pay plus the usual premiums, not a jump to the next thousand.
    #[test]
    fn a_renewal_ask_does_not_jump_to_the_next_thousand() {
        let today = NaiveDate::from_ymd_opt(2027, 1, 10).unwrap();
        let mut player = Player::new(
            "p".to_string(),
            "P".to_string(),
            "P".to_string(),
            "1998-03-01".to_string(),
            "France".to_string(),
            Position::Midfielder,
            attributes(),
        );
        player.market_value = 100_000;
        player.stage_wage(700);
        player.stage_contract_end(Some("2027-06-30".to_string()));
        let team = Team::new(
            "t".to_string(),
            "T".to_string(),
            "TTT".to_string(),
            "France".to_string(),
            "Lyon".to_string(),
            "Ground".to_string(),
            10_000,
        );

        let ask = expected_wage(&player, &team, today);

        // 700 x 0.95 (fringe) x 1.10 (under six months left) = 732, asked as 740 (it was 1 000).
        assert_eq!(ask, 740);
    }

    /// Given a free agent who asks 10 000
    /// When a club bids with room for 20 000, 8 000, 3 000 and none
    /// Then it bids his ask when it can, what it can when that is above the insult line (6 500),
    ///      and the insult line when it cannot: never less than that, never more than he asks.
    #[test]
    fn an_ai_bid_is_what_the_club_can_carry_within_what_he_would_not_take_as_an_insult() {
        assert_eq!(free_agent_bid(10_000, 9_000, 20_000), 10_000);
        assert_eq!(free_agent_bid(10_000, 9_000, 8_000), 8_000);
        assert_eq!(free_agent_bid(10_000, 9_000, 3_000), 6_500);
        assert_eq!(free_agent_bid(10_000, 9_000, 0), 6_500);
        assert_eq!(free_agent_bid(10_000, 9_000, -5_000), 6_500);
    }

    /// The insult line is the one a renewal is judged by, so a bid made at it is not refused as one.
    #[test]
    fn the_lowest_bid_is_not_an_insulting_offer() {
        let ask = 12_345;
        let lowest = free_agent_bid(ask, 10_000, 0);
        assert!(!is_insulting_wage_offer(10_000, ask, lowest));
        assert!(is_insulting_wage_offer(10_000, ask, lowest - 1));
    }

    /// Given a player paid 1 000 whose market value implies 4 000 a week, one whose value implies
    /// 1 200, and one with no wage on record
    /// When the catch-up is worked out
    /// Then the first and the second are offered what their value implies and the third nothing:
    ///      the catch-up is a wage for a player the club keeps, not a wage for a stranger.
    #[test]
    fn a_players_pay_catches_up_with_his_value() {
        let player = |wage: u32, value: u64| {
            let mut p = Player::new(
                "p".to_string(),
                "P".to_string(),
                "P".to_string(),
                "1998-03-01".to_string(),
                "France".to_string(),
                Position::Midfielder,
                attributes(),
            );
            p.market_value = value;
            p.stage_wage(wage);
            p
        };
        assert_eq!(quality_wage_catch_up(&player(1_000, 4_000 * 200)), 4_000);
        assert_eq!(quality_wage_catch_up(&player(1_000, 1_200 * 200)), 1_200);
        assert_eq!(quality_wage_catch_up(&player(0, 4_000 * 200)), 0);
    }
}
