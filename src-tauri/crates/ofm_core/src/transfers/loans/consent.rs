//! The player's say in a loan.
//!
//! A loan changes nobody's contract, so wage and length play no part here; what a player weighs is
//! the football: is this a spell he needs, or a demotion. It is read from the part he plays at the
//! club that owns him (the same `infer_player_importance` the loan price uses) and from the step in
//! stature to the borrower, with the circumstances that make anyone want out. There is no random
//! element, so a saved game answers the same way after a reload.
//!
//! | He is at the owner | Goes to a club that is |
//! |---|---|
//! | on the fringe | any: a loan is minutes |
//! | a regular | not far below (`REGULAR_ACCEPTS_STEP_DOWN` reputation points at most) |
//! | in the starting eleven | at least his own standing (`KEY_PLAYER_ACCEPTS_STEP_DOWN`) |
//!
//! Whatever the role, a player who is unhappy or whom the club has put up for sale goes.

use super::*;

/// Reputation points a regular accepts to drop for a loan.
const REGULAR_ACCEPTS_STEP_DOWN: u32 = 150;
/// Reputation points a first-eleven player accepts to drop for a loan.
const KEY_PLAYER_ACCEPTS_STEP_DOWN: u32 = 50;
/// Morale at or under which a player is unhappy enough to leave on loan whatever the club.
const UNHAPPY_MORALE: u8 = 45;

/// Whether `player` would go on loan to `borrower_team_id`.
pub(crate) fn player_agrees_to_loan(game: &Game, player: &Player, borrower_team_id: &str) -> bool {
    let Some(owner) = player
        .team_id
        .as_deref()
        .and_then(|id| game.teams.iter().find(|team| team.id == id))
    else {
        return true;
    };
    let Some(borrower) = game.teams.iter().find(|team| team.id == borrower_team_id) else {
        return true;
    };
    if player.morale <= UNHAPPY_MORALE || player.transfer_listed {
        return true;
    }
    let step_down = owner.reputation.saturating_sub(borrower.reputation);
    match infer_player_importance(player, owner) {
        PlayerImportance::Fringe => true,
        PlayerImportance::Regular => step_down <= REGULAR_ACCEPTS_STEP_DOWN,
        PlayerImportance::Key => step_down <= KEY_PLAYER_ACCEPTS_STEP_DOWN,
    }
}

/// [`player_agrees_to_loan`] as the refusal the manager is shown.
pub(crate) fn ensure_player_agrees_to_loan(
    game: &Game,
    player_id: &str,
    borrower_team_id: &str,
) -> Result<(), String> {
    let player = game
        .players
        .iter()
        .find(|player| player.id == player_id)
        .ok_or("be.error.playerNotFound")?;
    if player_agrees_to_loan(game, player, borrower_team_id) {
        Ok(())
    } else {
        Err(ERR_PLAYER_REFUSES_LOAN.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_with_player(owner_rep: u32, borrower_rep: u32, in_xi: bool, value: u64) -> Game {
        let mut game = crate::transfers::tests::make_game();
        game.teams[0].reputation = owner_rep;
        game.teams[1].reputation = borrower_rep;
        game.players[0].team_id = Some(game.teams[0].id.clone());
        game.players[0].market_value = value;
        game.players[0].morale = 70;
        game.players[0].transfer_listed = false;
        game.teams[0].starting_xi_ids = if in_xi {
            vec![game.players[0].id.clone()]
        } else {
            Vec::new()
        };
        game
    }

    fn agrees(game: &Game) -> bool {
        player_agrees_to_loan(game, &game.players[0], &game.teams[1].id)
    }

    /// Given a fringe player
    /// When a much smaller club asks for him on loan
    /// Then he goes: a loan is minutes.
    #[test]
    fn a_fringe_player_welcomes_a_loan_to_any_club() {
        assert!(agrees(&game_with_player(800, 50, false, 100_000)));
    }

    /// Given a regular of a big club
    /// When a club a little below and one far below ask
    /// Then he accepts the first and refuses the second.
    #[test]
    fn a_regular_refuses_only_a_long_step_down() {
        assert!(agrees(&game_with_player(500, 400, false, 2_000_000)));
        assert!(!agrees(&game_with_player(800, 400, false, 2_000_000)));
    }

    /// Given a player of the starting eleven
    /// When a smaller club asks
    /// Then he refuses; a club of the same standing is welcome.
    #[test]
    fn a_first_eleven_player_will_not_drop_down_for_a_loan() {
        assert!(!agrees(&game_with_player(500, 300, true, 2_000_000)));
        assert!(agrees(&game_with_player(500, 480, true, 2_000_000)));
    }

    /// Given a first-eleven player who is unhappy, and another the club has listed for sale
    /// When a much smaller club asks
    /// Then each goes.
    #[test]
    fn an_unhappy_or_listed_player_goes_wherever_he_is_asked() {
        let mut unhappy = game_with_player(800, 100, true, 2_000_000);
        unhappy.players[0].morale = 40;
        assert!(agrees(&unhappy));
        let mut listed = game_with_player(800, 100, true, 2_000_000);
        listed.players[0].transfer_listed = true;
        assert!(agrees(&listed));
    }

    /// Given the same facts twice
    /// When asked
    /// Then the same answer: nothing is random.
    #[test]
    fn the_answer_is_repeatable() {
        let game = game_with_player(800, 400, false, 2_000_000);
        assert_eq!(agrees(&game), agrees(&game));
    }
}
