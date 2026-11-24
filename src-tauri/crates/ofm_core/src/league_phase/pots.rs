//! The one relaxation the draw allows itself: trading two clubs across a pot boundary.
//!
//! UEFA ranks the pots by club coefficient and then forbids two clubs of one association from
//! meeting. The two collide when the ranking puts too many clubs of one association into one pot:
//! with nine clubs a pot, five of one association need ten opponents from the four others, so no
//! draw exists (a pot-level Hall condition). Regulation 16.03 lets UEFA adapt the draw conditions
//! for such a case; this module is the game's smallest version of that.
//!
//! The pots are left as ranked whenever a draw exists. Only when one does not, the crowded
//! association's lowest-ranked club trades places with the nearest-ranked club of another
//! association in the neighbouring pot (or its highest-ranked club trades upwards), one trade at a
//! time, until the draw is possible. No pairing restriction is ever relaxed, and every trade is
//! returned so it can be recorded and shown. A field no sequence of trades can repair is left to
//! the draw to refuse.

use domain::league_phase::{LeaguePhaseRules, PotAdjustment};
use std::collections::BTreeMap;

/// Trades tried before the field is given back to the draw as it stands.
const MAX_TRADES: usize = 6;

/// Pot-block violations: for each association, each pair of pots `(from, to)`, how many clubs it
/// has in `from` beyond the room the other clubs of `to` leave it. The same condition
/// `draw::check_pot_blocks` states, counted instead of named so trades can be compared.
fn violations(
    seeding: &[String],
    pot_size: usize,
    associations: &BTreeMap<String, String>,
) -> usize {
    let pot_count = seeding.len() / pot_size;
    let mut members: BTreeMap<(&str, usize), usize> = BTreeMap::new();
    for (index, club) in seeding.iter().enumerate() {
        let association = associations.get(club).map_or("", String::as_str);
        *members.entry((association, index / pot_size)).or_default() += 1;
    }
    let mut total = 0;
    for (&(association, _), &count) in &members {
        for to in 0..pot_count {
            let theirs = members.get(&(association, to)).copied().unwrap_or(0);
            if count > pot_size - theirs {
                total += count - (pot_size - theirs);
            }
        }
    }
    total
}

/// The seeding after a single trade that moves the crowded association's club out of `pot` toward
/// `neighbour`, or `None` when there is nobody to trade with.
fn trade(
    seeding: &[String],
    pot_size: usize,
    associations: &BTreeMap<String, String>,
    association: &str,
    pot: usize,
    neighbour: usize,
) -> Option<(Vec<String>, PotAdjustment)> {
    let of = |club: &String| associations.get(club).map_or("", String::as_str);
    let range = |p: usize| p * pot_size..(p + 1) * pot_size;
    let downward = neighbour > pot;
    // Nearest to the boundary on both sides keeps the displacement in ranks smallest.
    let moved_index = if downward {
        range(pot).rev().find(|i| of(&seeding[*i]) == association)?
    } else {
        range(pot).find(|i| of(&seeding[*i]) == association)?
    };
    let exchanged_index = if downward {
        range(neighbour).find(|i| of(&seeding[*i]) != association)?
    } else {
        range(neighbour)
            .rev()
            .find(|i| of(&seeding[*i]) != association)?
    };
    let mut traded = seeding.to_vec();
    traded.swap(moved_index, exchanged_index);
    let adjustment = PotAdjustment {
        moved_club_id: seeding[moved_index].clone(),
        exchanged_club_id: seeding[exchanged_index].clone(),
        from_pot: pot as u8,
        to_pot: neighbour as u8,
    };
    Some((traded, adjustment))
}

/// The seeding and the trades that make its pots drawable, or the seeding untouched with no
/// trades when it already is or when no trade helps.
pub fn relax(
    seeding: &[String],
    associations: &BTreeMap<String, String>,
    rules: &LeaguePhaseRules,
) -> (Vec<String>, Vec<PotAdjustment>) {
    let pot_size = rules.pot_size();
    if rules.allow_same_association || seeding.len() != rules.field_size() {
        return (seeding.to_vec(), Vec::new());
    }
    let pot_count = usize::from(rules.pot_count);
    let mut current = seeding.to_vec();
    let mut trades = Vec::new();
    while trades.len() < MAX_TRADES {
        let before = violations(&current, pot_size, associations);
        if before == 0 {
            return (current, trades);
        }
        // Every pot holding an association's clubs, each way it could shed one.
        let mut best: Option<(usize, usize, Vec<String>, PotAdjustment)> = None;
        let mut associations_here: Vec<&str> = current
            .iter()
            .filter_map(|club| associations.get(club).map(String::as_str))
            .collect();
        associations_here.sort_unstable();
        associations_here.dedup();
        for association in associations_here {
            for pot in 0..pot_count {
                for neighbour in [pot.wrapping_sub(1), pot + 1] {
                    if neighbour >= pot_count {
                        continue;
                    }
                    let Some((traded, adjustment)) = trade(
                        &current,
                        pot_size,
                        associations,
                        association,
                        pot,
                        neighbour,
                    ) else {
                        continue;
                    };
                    let after = violations(&traded, pot_size, associations);
                    if after >= before {
                        continue;
                    }
                    let displacement = current.iter().zip(&traded).filter(|(a, b)| a != b).count();
                    let key = (after, displacement);
                    if best
                        .as_ref()
                        .is_none_or(|(held, moved, ..)| key < (*held, *moved))
                    {
                        best = Some((after, displacement, traded, adjustment));
                    }
                }
            }
        }
        let Some((_, _, traded, adjustment)) = best else {
            return (seeding.to_vec(), Vec::new());
        };
        current = traded;
        trades.push(adjustment);
    }
    if violations(&current, pot_size, associations) == 0 {
        (current, trades)
    } else {
        (seeding.to_vec(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(layout: &[&str]) -> (Vec<String>, BTreeMap<String, String>) {
        let clubs: Vec<String> = (0..layout.len()).map(|i| format!("c{i:02}")).collect();
        let associations = clubs
            .iter()
            .zip(layout)
            .map(|(club, association)| (club.clone(), (*association).to_string()))
            .collect();
        (clubs, associations)
    }

    fn four_pots() -> LeaguePhaseRules {
        LeaguePhaseRules::four_pots_of_nine()
    }

    /// 36 slots: `crowded` clubs of association "E" in the first pot, the rest all different.
    fn crowded_first_pot(crowded: usize) -> Vec<String> {
        let mut layout: Vec<String> = (0..36).map(|i| format!("u{i}")).collect();
        for slot in layout.iter_mut().take(crowded) {
            *slot = "E".to_string();
        }
        layout
    }

    /// Given pots in which five clubs of one association share the top pot
    /// When the pots are relaxed
    /// Then exactly one club moves to the next pot, in exchange for the nearest-ranked other
    ///      association's club, and no pot holds more than four of them.
    #[test]
    fn five_of_one_association_in_a_pot_costs_one_trade() {
        let layout = crowded_first_pot(5);
        let refs: Vec<&str> = layout.iter().map(String::as_str).collect();
        let (seeding, associations) = field(&refs);

        let (relaxed, trades) = relax(&seeding, &associations, &four_pots());

        assert_eq!(trades.len(), 1, "{trades:?}");
        // The lowest-ranked English club (index 4) trades with the highest-ranked of pot two.
        assert_eq!(trades[0].moved_club_id, "c04");
        assert_eq!(trades[0].exchanged_club_id, "c09");
        assert_eq!((trades[0].from_pot, trades[0].to_pot), (0, 1));
        assert_eq!(relaxed[4], "c09");
        assert_eq!(relaxed[9], "c04");
        for pot in relaxed.chunks(9) {
            let held = pot.iter().filter(|c| associations[*c] == "E").count();
            assert!(held <= 4);
        }
    }

    /// Given pots that already admit a draw
    /// When they are relaxed
    /// Then nothing moves and nothing is recorded.
    #[test]
    fn pots_that_admit_a_draw_are_left_exactly_as_ranked() {
        let layout = crowded_first_pot(4);
        let refs: Vec<&str> = layout.iter().map(String::as_str).collect();
        let (seeding, associations) = field(&refs);

        let (relaxed, trades) = relax(&seeding, &associations, &four_pots());

        assert!(trades.is_empty());
        assert_eq!(relaxed, seeding);
    }

    /// Given an association that no trade can repair (more clubs than the others can absorb)
    /// When the pots are relaxed
    /// Then the seeding is returned untouched, for the draw to refuse.
    #[test]
    fn an_unrepairable_field_is_returned_untouched() {
        let layout = crowded_first_pot(20);
        let refs: Vec<&str> = layout.iter().map(String::as_str).collect();
        let (seeding, associations) = field(&refs);

        let (relaxed, trades) = relax(&seeding, &associations, &four_pots());

        assert!(trades.is_empty());
        assert_eq!(relaxed, seeding);
    }

    /// Given the explicit same-association relaxation is on
    /// When the pots are relaxed
    /// Then they are left alone: one relaxation at a time.
    #[test]
    fn the_authored_same_association_relaxation_makes_trading_unnecessary() {
        let layout = crowded_first_pot(6);
        let refs: Vec<&str> = layout.iter().map(String::as_str).collect();
        let (seeding, associations) = field(&refs);
        let mut rules = four_pots();
        rules.allow_same_association = true;

        let (relaxed, trades) = relax(&seeding, &associations, &rules);

        assert!(trades.is_empty());
        assert_eq!(relaxed, seeding);
    }
}
