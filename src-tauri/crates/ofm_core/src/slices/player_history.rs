use crate::game::Game;
use domain::player::{CareerEntry, PlayerMovementEntry};
use serde::Serialize;

/// What the game state sent to the screen leaves out of every player (`Game::ui_clone`):
/// the season-by-season career and the moves and contracts. One profile asks for them.
#[derive(Debug, Serialize, PartialEq)]
pub struct PlayerHistory {
    pub player_id: String,
    pub career: Vec<CareerEntry>,
    pub movement_history: Vec<PlayerMovementEntry>,
}

pub fn query_player_history(game: &Game, player_id: &str) -> Option<PlayerHistory> {
    game.players
        .iter()
        .find(|player| player.id == player_id)
        .map(|player| PlayerHistory {
            player_id: player.id.clone(),
            career: player.career.clone(),
            movement_history: player.movement_history.to_vec(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use chrono::Utc;
    use domain::manager::Manager;
    use domain::player::{Player, PlayerAttributes, PlayerMovementKind, Position};

    fn game_with_a_past() -> Game {
        let clock = GameClock {
            current_date: Utc::now(),
            start_date: Utc::now(),
        };
        let manager = Manager::new(
            "m1".to_string(),
            "A".to_string(),
            "B".to_string(),
            "1980-01-01".to_string(),
            "GB".to_string(),
        );
        let attributes: PlayerAttributes = serde_json::from_str(r#"{"pace":50,"stamina":50,"strength":50,"agility":50,"passing":50,"shooting":50,"tackling":50,"dribbling":50,"defending":50,"positioning":50,"vision":50,"decisions":50,"composure":50,"aggression":50,"teamwork":50,"leadership":50,"handling":50,"reflexes":50,"aerial":50}"#).unwrap();
        let mut players = Vec::new();
        for id in ["p1", "p2"] {
            let mut player = Player::new(
                id.to_string(),
                id.to_string(),
                id.to_string(),
                "1999-01-01".to_string(),
                "GB".to_string(),
                Position::Midfielder,
                attributes.clone(),
            );
            player.career.push(CareerEntry {
                season: 2025,
                team_id: format!("club-of-{id}"),
                team_name: id.to_string(),
                appearances: 10,
                goals: 1,
                assists: 2,
            });
            player
                .record_movement(PlayerMovementEntry::new(
                    "2026-07-01",
                    PlayerMovementKind::PermanentTransfer,
                ))
                .unwrap();
            players.push(player);
        }
        Game::new(clock, manager, vec![], players, vec![], vec![])
    }

    /// Given a game whose players have a career and moves
    /// When the history of one is queried
    /// Then it is that player's, and an unknown player has none
    #[test]
    fn a_players_history_is_his_own() {
        let game = game_with_a_past();

        let history = query_player_history(&game, "p2").unwrap();

        assert_eq!(history.player_id, "p2");
        assert_eq!(history.career[0].team_id, "club-of-p2");
        assert_eq!(history.movement_history.len(), 1);
        assert!(query_player_history(&game, "nobody").is_none());
    }

    /// Given the copy of the game that goes to the screen
    /// When it is serialized
    /// Then no player carries a career or moves, yet wage and contract dates stay
    /// and the history queried from the real game is still whole
    #[test]
    fn the_screen_copy_is_light_but_the_game_keeps_the_history() {
        let mut game = game_with_a_past();
        game.players[0].stage_wage(1_234);

        let wire = serde_json::to_value(game.ui_clone()).unwrap();
        let player = &wire["players"][0];

        assert_eq!(player["career"].as_array().unwrap().len(), 0);
        assert!(player.get("movement_history").is_none());
        assert_eq!(player["wage"], 1_234);
        assert_eq!(game.players[0].career.len(), 1);
        assert_eq!(query_player_history(&game, "p1").unwrap().career.len(), 1);
        let full = serde_json::to_value(&game).unwrap();
        assert_eq!(
            full["players"][0]["movement_history"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
