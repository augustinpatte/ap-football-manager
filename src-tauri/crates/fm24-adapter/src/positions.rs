//! FM positions -> OFM positions, secondary positions, preferred foot.
//!
//! Always a specific OFM position (never `Defender`/`Midfielder`/`Forward`): the engine only keeps
//! weak foot and alternate positions for a specific one.

use crate::config::PositionsCfg;
use domain::player::{Footedness, Position};
use std::collections::BTreeMap;

/// FM codes in the fixed order used to break ties (determinism). `SW` is deliberately absent:
/// its note is always 1 in the save (the role no longer exists in FM24).
const ORDER: [(&str, Position); 14] = [
    ("GK", Position::Goalkeeper),
    ("DC", Position::CenterBack),
    ("DL", Position::LeftBack),
    ("DR", Position::RightBack),
    ("WBL", Position::LeftWingBack),
    ("WBR", Position::RightWingBack),
    ("DM", Position::DefensiveMidfielder),
    ("MC", Position::CentralMidfielder),
    ("ML", Position::LeftMidfielder),
    ("MR", Position::RightMidfielder),
    ("AMC", Position::AttackingMidfielder),
    ("AML", Position::LeftWinger),
    ("AMR", Position::RightWinger),
    ("STC", Position::Striker),
];

pub fn from_fm_code(code: &str) -> Option<Position> {
    ORDER
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, p)| p.clone())
        .or_else(|| (code == "SW").then_some(Position::CenterBack))
}

/// Best-rated position; ties go to the earliest in [`ORDER`]. `None` if no known code is present.
pub fn natural(notes: &BTreeMap<String, i64>) -> Option<(&'static str, Position)> {
    let mut best: Option<(&'static str, Position, i64)> = None;
    for (code, position) in ORDER.iter() {
        if let Some(&rating) = notes.get(*code)
            && best.as_ref().is_none_or(|(_, _, b)| rating > *b)
        {
            best = Some((code, position.clone(), rating));
        }
    }
    best.map(|(code, position, _)| (code, position))
}

/// Every position within `within` points of the best FM note, in the fixed order. FM rates a
/// position 20 ("natural") or 19 ("accomplished") on a judgement call, so a one-point gap is noise:
/// the caller picks among them by OFM's own rating of the converted attributes (a forward rated 20
/// as a striker and 19 on the wing is a winger if that is where OFM rates him highest).
pub fn natural_candidates(notes: &BTreeMap<String, i64>, within: i64) -> Vec<(&'static str, Position)> {
    let best = ORDER
        .iter()
        .filter_map(|(code, _)| notes.get(*code).copied())
        .max();
    let Some(best) = best else {
        return Vec::new();
    };
    ORDER
        .iter()
        .filter(|(code, _)| notes.get(*code).is_some_and(|rating| *rating >= best - within))
        .map(|(code, position)| (*code, position.clone()))
        .collect()
}

/// Positions rated at least `alternate_min_rating`, other than the natural one, best first.
pub fn alternates(
    notes: &BTreeMap<String, i64>,
    natural_code: &str,
    cfg: &PositionsCfg,
) -> Vec<Position> {
    let natural_position = from_fm_code(natural_code);
    let mut found: Vec<(i64, usize, Position)> = Vec::new();
    for (index, (code, position)) in ORDER.iter().enumerate() {
        let rating = notes.get(*code).copied().unwrap_or(0);
        if rating >= cfg.alternate_min_rating
            && *code != natural_code
            && Some(position) != natural_position.as_ref()
            && !found.iter().any(|(_, _, p)| p == position)
        {
            found.push((rating, index, position.clone()));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, p)| p).collect()
}

/// Preferred foot and weak-foot skill (1-5) from the two foot notes (5-100).
pub fn foot(left: i64, right: i64, cfg: &PositionsCfg) -> (Footedness, u8) {
    let weaker = left.min(right);
    let weak_foot = ((weaker as f64 / 20.0).ceil() as i64).clamp(1, 5) as u8;
    if left >= cfg.both_feet_min && right >= cfg.both_feet_min {
        (Footedness::Both, weak_foot)
    } else if left > right {
        (Footedness::Left, weak_foot)
    } else {
        (Footedness::Right, weak_foot)
    }
}

pub fn footedness_name(foot: &Footedness) -> &'static str {
    match foot {
        Footedness::Left => "Left",
        Footedness::Right => "Right",
        Footedness::Both => "Both",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PositionsCfg {
        PositionsCfg {
            alternate_min_rating: 15,
            both_feet_min: 75,
            natural_within: 1,
        }
    }

    fn notes(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
        let mut map: BTreeMap<String, i64> = ORDER.iter().map(|(c, _)| (c.to_string(), 1)).collect();
        map.insert("SW".to_string(), 1);
        for (code, rating) in pairs {
            map.insert(code.to_string(), *rating);
        }
        map
    }

    #[test]
    fn the_best_rated_position_is_the_natural_one() {
        let n = notes(&[("STC", 20), ("AMC", 15), ("AMR", 15)]);
        assert_eq!(natural(&n), Some(("STC", Position::Striker)));
    }

    #[test]
    fn a_tie_goes_to_the_earliest_position_in_the_fixed_order() {
        let n = notes(&[("MC", 18), ("DM", 18)]);
        assert_eq!(natural(&n), Some(("DM", Position::DefensiveMidfielder)));
    }

    #[test]
    fn positions_within_a_point_of_the_best_note_are_candidates() {
        let n = notes(&[("ML", 20), ("AML", 20), ("AMR", 19), ("STC", 18)]);
        let near: Vec<_> = natural_candidates(&n, 1).into_iter().map(|(c, _)| c).collect();
        assert_eq!(near, vec!["ML", "AML", "AMR"]);
        let tied: Vec<_> = natural_candidates(&n, 0).into_iter().map(|(c, _)| c).collect();
        assert_eq!(tied, vec!["ML", "AML"]);
        assert!(natural_candidates(&BTreeMap::new(), 1).is_empty());
    }

    #[test]
    fn a_goalkeeper_is_a_goalkeeper() {
        let n = notes(&[("GK", 20)]);
        assert_eq!(natural(&n).unwrap().1, Position::Goalkeeper);
        assert!(alternates(&n, "GK", &cfg()).is_empty());
    }

    #[test]
    fn alternates_exclude_the_natural_position_and_low_notes() {
        let n = notes(&[("STC", 20), ("AMC", 15), ("AMR", 16), ("MC", 14)]);
        assert_eq!(
            alternates(&n, "STC", &cfg()),
            vec![Position::RightWinger, Position::AttackingMidfielder]
        );
    }

    #[test]
    fn sweeper_notes_are_ignored_for_the_natural_position() {
        let n = notes(&[("SW", 20), ("DC", 12)]);
        assert_eq!(natural(&n), Some(("DC", Position::CenterBack)));
    }

    #[test]
    fn feet() {
        assert_eq!(foot(100, 40, &cfg()), (Footedness::Left, 2));
        assert_eq!(foot(30, 100, &cfg()), (Footedness::Right, 2));
        assert_eq!(foot(80, 90, &cfg()), (Footedness::Both, 4));
        assert_eq!(foot(5, 100, &cfg()), (Footedness::Right, 1));
        assert_eq!(foot(100, 100, &cfg()), (Footedness::Both, 5));
    }
}
