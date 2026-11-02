//! The club's senior squad list, counted in one place.
//!
//! The limit is a registration limit of the club, not a decision of the player: a free agent who
//! would gladly sign is still not signed into a full list. The refusal used to say only that the
//! squad was full, without how full or what counted, so a manager who had released three players
//! could not tell whether the release had registered. Every number the refusal and the contract
//! screen show comes from [`squad_capacity`], and the check that refuses is the same function.

use super::consts::{ERR_SQUAD_FULL, MAX_SENIOR_SQUAD};
use crate::game::Game;
use domain::player::{Player, SquadRole};
use serde::{Deserialize, Serialize};

/// Who is on a club's lists, as the registration limit sees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SquadCapacity {
    /// Senior-squad players at the club today, loanees from other clubs included, players it has
    /// lent out and retired players not. This is the number held against `senior_limit`.
    pub registered_seniors: usize,
    pub senior_limit: usize,
    /// Youth-squad players: they hold no place on the senior list.
    pub youth: usize,
    /// Of `registered_seniors`, those on loan from another club.
    pub loaned_in: usize,
    /// The club's own players on loan elsewhere: registered by the borrower, not here.
    pub loaned_out: usize,
    pub free_places: usize,
    /// Places to free before one more player can be signed; 0 when there is room.
    pub releases_needed: usize,
    pub blocks_signing: bool,
}

pub fn squad_capacity(game: &Game, club_id: &str) -> SquadCapacity {
    squad_capacity_of(&game.players, club_id)
}

pub(crate) fn squad_capacity_of(players: &[Player], club_id: &str) -> SquadCapacity {
    let mut registered_seniors = 0;
    let mut youth = 0;
    let mut loaned_in = 0;
    let mut loaned_out = 0;
    for player in players.iter().filter(|player| !player.retired) {
        if player.team_id.as_deref() == Some(club_id) {
            match player.squad_role {
                SquadRole::Senior => {
                    registered_seniors += 1;
                    if player.active_loan.is_some() {
                        loaned_in += 1;
                    }
                }
                SquadRole::Youth => youth += 1,
            }
        } else if player
            .active_loan
            .as_ref()
            .is_some_and(|loan| loan.parent_team_id == club_id)
        {
            loaned_out += 1;
        }
    }
    let senior_limit = MAX_SENIOR_SQUAD;
    let free_places = senior_limit.saturating_sub(registered_seniors);
    let blocks_signing = free_places == 0;
    SquadCapacity {
        registered_seniors,
        senior_limit,
        youth,
        loaned_in,
        loaned_out,
        free_places,
        releases_needed: if blocks_signing {
            registered_seniors + 1 - senior_limit
        } else {
            0
        },
        blocks_signing,
    }
}

impl SquadCapacity {
    /// The refusal as a translation key with the numbers the player needs to act on it.
    pub(crate) fn full_error(&self) -> String {
        format!(
            "{ERR_SQUAD_FULL}?count={}&max={}&release={}",
            self.registered_seniors, self.senior_limit, self.releases_needed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;
    use domain::player::{ActiveLoan, Position};

    fn player(id: usize, team: Option<&str>, role: SquadRole) -> Player {
        let mut player = Player::new(
            format!("p{id}"),
            format!("P{id}"),
            format!("P {id}"),
            "2000-01-01".to_string(),
            "England".to_string(),
            Position::Midfielder,
            uniform_attributes(60),
        );
        player.team_id = team.map(str::to_string);
        player.squad_role = role;
        player
    }

    fn squad(seniors: usize) -> Vec<Player> {
        (0..seniors)
            .map(|id| player(id, Some("club"), SquadRole::Senior))
            .collect()
    }

    fn loan(parent: &str, borrower: &str) -> ActiveLoan {
        ActiveLoan {
            parent_team_id: parent.to_string(),
            loan_team_id: borrower.to_string(),
            loan_team_name: None,
            start_date: "2026-08-01".to_string(),
            end_date: "2027-06-30".to_string(),
            wage_contribution_pct: 100,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
        }
    }

    /// Given a club that is exactly at the limit
    /// When capacity is read
    /// Then it is full and one place must be freed; one place short of the limit it has room.
    #[test]
    fn a_club_at_the_limit_must_free_one_place_and_one_below_it_has_room() {
        let full = squad_capacity_of(&squad(30), "club");
        assert!(full.blocks_signing);
        assert_eq!((full.free_places, full.releases_needed), (0, 1));

        let room = squad_capacity_of(&squad(29), "club");
        assert!(!room.blocks_signing);
        assert_eq!((room.free_places, room.releases_needed), (1, 0));
    }

    /// Given a squad of 33 seniors from which three players are released
    /// When capacity is read after each release
    /// Then it is 33, 32, 31, 30 registered and the places still to free fall 4, 3, 2, 1: the
    /// refusal after three releases is correct, and says so with these numbers.
    #[test]
    fn releasing_three_players_from_thirty_three_still_leaves_the_list_full() {
        let mut players = squad(33);
        let mut needed = Vec::new();
        for released in 0..4 {
            let capacity = squad_capacity_of(&players, "club");
            needed.push((capacity.registered_seniors, capacity.releases_needed));
            if released < 3 {
                players[released].team_id = None;
            }
        }
        assert_eq!(needed, [(33, 4), (32, 3), (31, 2), (30, 1)]);
        let error = squad_capacity_of(&players, "club").full_error();
        assert_eq!(
            error,
            "be.error.freeAgents.squadFull?count=30&max=30&release=1"
        );
    }

    /// Given youth players, a retired player, players lent out and players borrowed
    /// When capacity is read
    /// Then youth and retired hold no senior place, loaned-out players are counted as loaned out
    /// (the borrower registers them), and loanees from other clubs hold a place.
    #[test]
    fn youth_retired_and_lent_out_players_hold_no_place_but_borrowed_ones_do() {
        let mut players = squad(20);
        players.extend((100..105).map(|id| player(id, Some("club"), SquadRole::Youth)));
        let mut retired = player(200, Some("club"), SquadRole::Senior);
        retired.retired = true;
        players.push(retired);
        let mut lent = player(300, Some("other"), SquadRole::Senior);
        lent.active_loan = Some(loan("club", "other"));
        players.push(lent);
        let mut borrowed = player(400, Some("club"), SquadRole::Senior);
        borrowed.active_loan = Some(loan("other", "club"));
        players.push(borrowed);

        let capacity = squad_capacity_of(&players, "club");

        assert_eq!(
            capacity.registered_seniors, 21,
            "20 own seniors and one loanee"
        );
        assert_eq!(capacity.youth, 5);
        assert_eq!(capacity.loaned_in, 1);
        assert_eq!(capacity.loaned_out, 1);
        assert_eq!(capacity.free_places, 9);
    }
}
