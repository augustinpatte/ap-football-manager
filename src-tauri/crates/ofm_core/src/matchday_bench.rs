//! The official matchday bench, chosen from the players available for a fixture.
//!
//! A club's available players and the bench of a match are different things: a club may have 26
//! players fit to play, but a match lists eleven starters and a bounded number of substitutes. The
//! match builder used to name every available player not starting as a substitute, so a squad of
//! 26 sent fifteen to the M12 engine, which accepts at most twelve and rejected the whole match
//! ("bench has more than 12 players"). The permanent squad is never touched: this only decides
//! who is on the sheet today.

use crate::player_rating::natural_ovr;
use domain::player::{Player, Position};

/// Most substitutes the M12 match engine accepts for one team. No competition in the game's data
/// states a smaller bench, so this is the limit; a competition rule, when the data has one, can
/// only lower it.
pub const M12_MAX_BENCH: usize = 12;

/// Candidates best first: quality, then id, so two calls on the same squad name the same bench.
pub(crate) fn rank_for_bench(candidates: &mut [&Player]) {
    candidates.sort_by(|left, right| {
        natural_ovr(right)
            .total_cmp(&natural_ovr(left))
            .then_with(|| left.id.cmp(&right.id))
    });
}

/// At most `limit` substitutes from `ranked` (best first), kept in that order.
///
/// A bench that fits is returned untouched. A larger one keeps its best goalkeeper first (a bench
/// without one cannot replace an injured keeper), then the best defender, midfielder and forward
/// (so any starter can be covered by his own kind), then the best of the rest.
pub(crate) fn select_matchday_bench<'a>(ranked: &[&'a Player], limit: usize) -> Vec<&'a Player> {
    if ranked.len() <= limit {
        return ranked.to_vec();
    }
    let mut chosen = vec![false; ranked.len()];
    let mut taken = 0;
    let mut take = |index: usize, chosen: &mut Vec<bool>| {
        if taken < limit && !chosen[index] {
            chosen[index] = true;
            taken += 1;
        }
    };
    let first_of = |group: Position| {
        ranked
            .iter()
            .position(|player| player.natural_position.to_group_position() == group)
    };
    for group in [
        Position::Goalkeeper,
        Position::Defender,
        Position::Midfielder,
        Position::Forward,
    ] {
        if let Some(index) = first_of(group) {
            take(index, &mut chosen);
        }
    }
    for index in 0..ranked.len() {
        take(index, &mut chosen);
    }
    ranked
        .iter()
        .zip(chosen)
        .filter_map(|(player, kept)| kept.then_some(*player))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;

    fn player(id: &str, position: Position, rating: u8) -> Player {
        Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "1998-01-01".to_string(),
            "GB".to_string(),
            position,
            uniform_attributes(rating),
        )
    }

    /// Two keepers, then defenders, midfielders and forwards, the keepers the weakest.
    fn candidates(count: usize) -> Vec<Player> {
        let shape = [
            Position::Goalkeeper,
            Position::Defender,
            Position::Midfielder,
            Position::Forward,
            Position::Defender,
            Position::Midfielder,
            Position::Forward,
            Position::Goalkeeper,
        ];
        (0..count)
            .map(|index| {
                let position = shape[index % shape.len()].clone();
                let rating = match position {
                    Position::Goalkeeper => 50,
                    _ => 90 - index as u8,
                };
                player(&format!("p{index:02}"), position, rating)
            })
            .collect()
    }

    fn pick(players: &[Player], limit: usize) -> Vec<&Player> {
        let mut refs: Vec<&Player> = players.iter().collect();
        rank_for_bench(&mut refs);
        select_matchday_bench(&refs, limit)
    }

    /// Given 0, 7, 12, 13, 15 and 20 substitutes available
    /// When the official bench is chosen
    /// Then it holds all of them up to twelve and never more.
    #[test]
    fn the_bench_is_everyone_up_to_the_limit_and_never_more() {
        for (available, expected) in [(0, 0), (7, 7), (12, 12), (13, 12), (15, 12), (20, 12)] {
            let players = candidates(available);
            assert_eq!(
                pick(&players, M12_MAX_BENCH).len(),
                expected,
                "{available} available"
            );
        }
    }

    /// Given a squad bigger than the bench, whose keepers are its weakest players
    /// When the bench is chosen
    /// Then a goalkeeper and one player of each outfield group are on it before quality fills the
    /// rest, so a keeper or a line can be replaced.
    #[test]
    fn a_trimmed_bench_still_covers_every_line_and_the_goalkeeper() {
        let players = candidates(20);
        let bench = pick(&players, M12_MAX_BENCH);
        for group in [
            Position::Goalkeeper,
            Position::Defender,
            Position::Midfielder,
            Position::Forward,
        ] {
            assert!(
                bench
                    .iter()
                    .any(|p| p.natural_position.to_group_position() == group),
                "{group:?} missing from {:?}",
                bench.iter().map(|p| &p.id).collect::<Vec<_>>()
            );
        }
        let keepers = bench
            .iter()
            .filter(|p| p.natural_position == Position::Goalkeeper)
            .count();
        assert_eq!(keepers, 1, "one keeper, not both weakest players");
    }

    /// Given the same squad twice, in a different order
    /// When the bench is chosen
    /// Then it is the same bench in the same order: the choice does not depend on list order.
    #[test]
    fn the_bench_is_deterministic_whatever_the_order_of_the_squad() {
        let players = candidates(20);
        let mut reversed = players.clone();
        reversed.reverse();
        let ids = |bench: Vec<&Player>| bench.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids(pick(&players, M12_MAX_BENCH)),
            ids(pick(&reversed, M12_MAX_BENCH))
        );
    }

    /// Given a bench that fits
    /// When it is chosen
    /// Then it is returned as ranked, nobody dropped or reordered.
    #[test]
    fn a_bench_that_fits_is_left_alone() {
        let players = candidates(9);
        let mut ranked: Vec<&Player> = players.iter().collect();
        rank_for_bench(&mut ranked);
        let chosen = select_matchday_bench(&ranked, M12_MAX_BENCH);
        assert_eq!(
            chosen.iter().map(|p| &p.id).collect::<Vec<_>>(),
            ranked.iter().map(|p| &p.id).collect::<Vec<_>>()
        );
    }
}
