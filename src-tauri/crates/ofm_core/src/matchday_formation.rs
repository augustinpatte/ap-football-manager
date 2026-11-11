//! The shape an AI club takes the field in on a day when players are missing.
//!
//! The club's formation is its habit (`Team::formation`, reviewed by `ai_formation` against the
//! whole squad). A manager whose two best centre backs are injured does not play a back four with
//! a midfielder in it because that is what the club plays: he picks the shape the players he has
//! fit. That is a decision about one match, and it is not written back. The club's formation, its
//! reviews and the saved eleven are untouched, so the next match, with everybody fit, is played in
//! the club's own shape again.
//!
//! Nothing is invented here. The score of a shape is the squad's positional fit for it
//! (`player_rating::positional_fit_for_assignment`), the rating the line-up picker already seats
//! players by, over the players who can play today; a different shape is taken only when it is
//! clearly better than the club's own, and only on a day somebody is missing.

use domain::player::Player;

use crate::player_rating::{FORMATIONS, formation_slots, positional_fit_for_assignment};

/// Rating points, on the mean of the eleven, the best shape has to beat the club's own by before a
/// manager changes it for the day. The shapes of one healthy squad differ by a few hundredths of a
/// point (`ai_formation`), and one player seated badly is worth a point or so on the mean, so only
/// an absence that really hurts the club's own shape moves him.
pub(crate) const ABSENCE_MARGIN: f64 = 0.8;

/// Mean positional fit of the eleven `available` would field in `formation`; `None` when they
/// cannot fill it.
fn fit_of(formation: &str, available: &[&Player]) -> Option<f64> {
    let slots = formation_slots(formation);
    let eleven = crate::turn::squad::auto_select_starting_xi(available, formation);
    (eleven.len() == slots.len().min(11) && !eleven.is_empty()).then(|| {
        eleven
            .iter()
            .zip(&slots)
            .map(|(player, slot)| positional_fit_for_assignment(player, slot))
            .sum::<f64>()
            / eleven.len() as f64
    })
}

/// The formation a club plays today: its own, unless a senior of `club` cannot play and `available`
/// fit another shape clearly better.
pub(crate) fn formation_for_the_day(
    formation: &str,
    club: &[&Player],
    available: &[&Player],
) -> String {
    use domain::player::SquadRole;
    let somebody_is_missing = club
        .iter()
        .any(|player| player.squad_role == SquadRole::Senior && !player.can_play());
    if !somebody_is_missing {
        return formation.to_string();
    }
    let Some(own) = fit_of(formation, available) else {
        return formation.to_string();
    };
    FORMATIONS
        .iter()
        .filter(|candidate| **candidate != formation)
        .filter_map(|candidate| fit_of(candidate, available).map(|fit| (*candidate, fit)))
        .filter(|(_, fit)| fit - own >= ABSENCE_MARGIN)
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map_or_else(|| formation.to_string(), |(shape, _)| shape.to_string())
}

/// [`formation_for_the_day`] for a club of `game`, as the kick-off would read it today: the shape
/// the club takes the field in. For studies that measure what the adjustment is worth.
pub fn shape_for_the_day(game: &crate::game::Game, team_id: &str) -> String {
    let Some(team) = game.teams.iter().find(|team| team.id == team_id) else {
        return String::new();
    };
    let club = crate::match_day_eligibility::club_players(&game.players, team_id);
    let available: Vec<&Player> = crate::match_day_eligibility::eligible_among(&club)
        .into_iter()
        .filter(|player| player.can_play())
        .collect();
    formation_for_the_day(&team.formation, &club, &available)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::player::{Injury, PlayerAttributes, Position};

    fn attributes(value: u8) -> PlayerAttributes {
        PlayerAttributes {
            pace: value,
            stamina: value,
            strength: value,
            agility: value,
            passing: value,
            shooting: value,
            tackling: value,
            dribbling: value,
            defending: value,
            positioning: value,
            vision: value,
            decisions: value,
            composure: value,
            aggression: value,
            teamwork: value,
            leadership: value,
            handling: value,
            reflexes: value,
            aerial: value,
        }
    }

    fn player(id: &str, position: Position) -> Player {
        Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "1998-01-01".to_string(),
            "GB".to_string(),
            position,
            attributes(70),
        )
    }

    /// A squad of two keepers, `centre_backs` centre backs, midfielders and strikers.
    fn squad(centre_backs: usize, midfielders: usize, strikers: usize) -> Vec<Player> {
        let mut players = vec![
            player("gk-0", Position::Goalkeeper),
            player("gk-1", Position::Goalkeeper),
        ];
        for i in 0..centre_backs {
            players.push(player(&format!("cb-{i}"), Position::CenterBack));
        }
        for i in 0..midfielders {
            players.push(player(&format!("cm-{i}"), Position::CentralMidfielder));
        }
        for i in 0..strikers {
            players.push(player(&format!("st-{i}"), Position::Striker));
        }
        players
    }

    fn injure(players: &mut [Player], ids: &[&str]) {
        for player in players.iter_mut().filter(|p| ids.contains(&p.id.as_str())) {
            player.injury = Some(Injury {
                name: "Knock".to_string(),
                days_remaining: 20,
            });
        }
    }

    fn for_the_day(formation: &str, players: &[Player]) -> String {
        let club: Vec<&Player> = players.iter().collect();
        let available: Vec<&Player> = players.iter().filter(|p| p.can_play()).collect();
        formation_for_the_day(formation, &club, &available)
    }

    /// Given a club with nobody missing, in a shape another formation would suit better
    /// When the shape for the day is asked
    /// Then it is the club's own: the club's habit is not reviewed match by match.
    #[test]
    fn a_club_with_everybody_fit_plays_its_own_shape() {
        // Five centre backs and three centre midfielders: a back three and a five-man midfield is
        // not what this squad is built for, whichever shape it is in.
        let players = squad(5, 3, 3);
        assert_eq!(for_the_day("4-4-2", &players), "4-4-2");
    }

    /// Given a club whose back four lost two centre backs, leaving three, and which has midfielders
    ///   and strikers to spare
    /// When the shape for the day is asked
    /// Then it plays a back three, not a back four with a midfielder in it.
    #[test]
    fn a_club_short_of_defenders_plays_a_back_three() {
        let mut players = squad(5, 8, 4);
        injure(&mut players, &["cb-0", "cb-1"]);

        let shape = for_the_day("4-4-2", &players);

        assert!(
            shape.starts_with("3-"),
            "{shape}: three fit centre backs cannot make a back four"
        );
    }

    /// Given a club that lost one player whose shape does not need him replaced by anybody worse
    /// When the shape for the day is asked
    /// Then it stays in its own: an absence the shape can cover is not a reason to change it.
    #[test]
    fn an_absence_the_shape_can_cover_does_not_change_it() {
        let mut players = squad(6, 8, 4);
        injure(&mut players, &["st-0"]);

        assert_eq!(for_the_day("4-4-2", &players), "4-4-2");
    }

    /// Given a squad that cannot fill the club's own shape at all
    /// When the shape for the day is asked
    /// Then the club's own is returned: the kick-off's top-up deals with a short side.
    #[test]
    fn a_squad_that_cannot_fill_the_shape_keeps_it() {
        let mut players = squad(1, 1, 1);
        injure(&mut players, &["st-0"]);

        assert_eq!(for_the_day("4-4-2", &players), "4-4-2");
    }
}
