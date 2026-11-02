//! Makes generated players distinguishable people.
//!
//! Names are drawn independently from finite pools and birth dates from a narrow window, so two
//! youngsters of one nationality occasionally came out with the same name *and* the same birth
//! date (about two per 12,000 players). A person is identified by those three facts in the save,
//! the news and the search box, so a clash makes two different players the same player on screen.
//!
//! The fix touches no random stream: the later of two clashing players is moved to the next free
//! birth date, a day at a time. Ids, names, ratings and every other draw are unchanged, and the
//! result is a pure function of the generated list, so a seed still yields one world.

use chrono::{Days, NaiveDate};
use domain::player::Player;
use std::collections::HashSet;

fn identity_key(player: &Player, date_of_birth: &str) -> (String, String, String) {
    (
        player.full_name.trim().to_lowercase(),
        date_of_birth.to_string(),
        player.nationality.clone(),
    )
}

/// Gives every player a (name, birth date, nationality) no earlier player has. Returns how many
/// birth dates moved. A player whose birth date does not parse is left alone: it cannot be moved
/// meaningfully, and the integrity audit reports it.
pub(super) fn ensure_distinct_identities(players: &mut [Player]) -> usize {
    let mut taken: HashSet<(String, String, String)> = HashSet::with_capacity(players.len());
    let mut moved = 0;
    for player in players.iter_mut() {
        let mut key = identity_key(player, &player.date_of_birth);
        if taken.contains(&key) {
            if let Ok(mut date) = NaiveDate::parse_from_str(&player.date_of_birth, "%Y-%m-%d") {
                while taken.contains(&key) {
                    let Some(next) = date.checked_add_days(Days::new(1)) else {
                        break;
                    };
                    date = next;
                    key = identity_key(player, &date.format("%Y-%m-%d").to_string());
                }
                player.date_of_birth = date.format("%Y-%m-%d").to_string();
                moved += 1;
            }
        }
        taken.insert(key);
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::ensure_distinct_identities;
    use domain::player::{Player, PlayerAttributes, Position};

    fn player(id: &str, name: &str, dob: &str, nationality: &str) -> Player {
        let attributes = PlayerAttributes {
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
            handling: 20,
            reflexes: 20,
            aerial: 50,
        };
        Player::new(
            id.into(),
            name.into(),
            name.into(),
            dob.into(),
            nationality.into(),
            Position::Midfielder,
            attributes,
        )
    }

    /// Given two players with one name, birth date and nationality, when identities are made
    /// distinct, then only the later one moves, by one day, and its id is untouched.
    #[test]
    fn the_later_of_two_clashing_players_takes_the_next_free_day() {
        let mut players = vec![
            player("a", "Bruno Oliveira", "2009-08-01", "BR"),
            player("b", "Bruno Oliveira", "2009-08-01", "BR"),
        ];
        assert_eq!(ensure_distinct_identities(&mut players), 1);
        assert_eq!(players[0].date_of_birth, "2009-08-01");
        assert_eq!(players[1].date_of_birth, "2009-08-02");
        assert_eq!(players[1].id, "b");
    }

    /// Given a clash whose next day is also taken, when made distinct, then the move skips past it.
    #[test]
    fn a_taken_next_day_is_skipped() {
        let mut players = vec![
            player("a", "Luke Green", "2009-05-04", "ENG"),
            player("b", "Luke Green", "2009-05-05", "ENG"),
            player("c", "Luke Green", "2009-05-04", "ENG"),
        ];
        ensure_distinct_identities(&mut players);
        assert_eq!(players[2].date_of_birth, "2009-05-06");
    }

    /// Given the same name and birth date but a different nationality or case, when made
    /// distinct, then nothing moves.
    #[test]
    fn different_nationalities_are_different_people() {
        let mut players = vec![
            player("a", "Luke Green", "2009-05-04", "ENG"),
            player("b", "Luke Green", "2009-05-04", "SCO"),
        ];
        assert_eq!(ensure_distinct_identities(&mut players), 0);
    }

    /// Given a clash at the end of a month, when made distinct, then the date rolls over correctly.
    #[test]
    fn the_move_rolls_over_month_and_leap_day() {
        let mut players = vec![
            player("a", "Ana Silva", "2008-02-28", "PT"),
            player("b", "Ana Silva", "2008-02-28", "PT"),
            player("c", "Ana Silva", "2008-02-28", "PT"),
        ];
        ensure_distinct_identities(&mut players);
        assert_eq!(players[1].date_of_birth, "2008-02-29");
        assert_eq!(players[2].date_of_birth, "2008-03-01");
    }

    /// Given an unparsable birth date, when made distinct, then the player is left as found.
    #[test]
    fn an_unparsable_birth_date_is_left_alone() {
        let mut players = vec![
            player("a", "X Y", "garbage", "BR"),
            player("b", "X Y", "garbage", "BR"),
        ];
        assert_eq!(ensure_distinct_identities(&mut players), 0);
        assert_eq!(players[1].date_of_birth, "garbage");
    }
}
