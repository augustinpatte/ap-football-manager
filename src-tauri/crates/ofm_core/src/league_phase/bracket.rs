//! The knockout bracket: who meets whom, and which path a winner inherits.
//!
//! Positions are fixed by the league-phase ranking and never redrawn from scratch: a club that
//! wins a tie takes over the bracket path of the club it eliminated ("the team that eliminates
//! them takes over their seeding position", Article 19.04). Only the permitted draws are made:
//! within a seeded/unseeded pairing for the play-offs, and between the two clubs of a seeded pair
//! for the round of 16. Both prefer a pairing of different associations when the other choice
//! allows one. Article 19's text names no such restriction for the knockout draws, so it is a
//! preference, not a rule (recorded as an approximation in the audit).
//!
//! The quarter-final and semi-final layout is expressed as data below and was checked against the
//! Annex B diagram of the 2026/27 regulations (read from the image in INTEGRATION5): the round of
//! 16 pairs seeds 1/2 with the play-off winners of 15/16 v 17/18, 3/4 with 13/14 v 19/20, 5/6
//! with 11/12 v 21/22 and 7/8 with 9/10 v 23/24; a quarter-final joins the tie of seed pair 5/6
//! with that of 3/4, or 7/8 with 1/2; the two quarter-finals of a side meet in its semi-final; and
//! the silver side's finalist is the nominal home team. The diagram labels each side's slots
//! "X/Y" (silver) and "Y/X" (blue), so *which* seed of a pair sits on which side is the draw's;
//! this module fixes one arrangement (seeds 1, 3, 5 and 8 silver), the one UEFA's 2024/25
//! bracket followed, which is one valid reading of the labels, not the only one.

use domain::league_phase::{KnockoutStage, KnockoutTie, LeaguePhaseOutcome};
use rand::{Rng, RngExt};
use std::collections::BTreeMap;

/// The round-of-16 seeds (path numbers) whose winners meet in each quarter-final.
pub const QUARTER_FINAL_PATHS: [[u8; 2]; 4] = [[2, 7], [3, 5], [4, 6], [1, 8]];
/// The quarter-finals whose winners meet in each semi-final.
pub const SEMI_FINAL_QUARTERS: [[usize; 2]; 2] = [[0, 2], [1, 3]];

/// A tie to create: where it sits, who plays it and who hosts the return leg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TieSpec {
    pub stage: KnockoutStage,
    pub slot: u8,
    pub path: u8,
    pub seeded: String,
    pub unseeded: String,
}

fn clash(associations: &BTreeMap<String, String>, a: &str, b: &str) -> bool {
    matches!((associations.get(a), associations.get(b)), (Some(x), Some(y)) if x == y)
}

/// Chooses between pairing `(a0, b0), (a1, b1)` and `(a0, b1), (a1, b0)`: the one with fewer
/// same-association pairs, by the draw when they are equal. `true` means the second.
fn draw_option<R: Rng>(
    associations: &BTreeMap<String, String>,
    a: [&str; 2],
    b: [&str; 2],
    rng: &mut R,
) -> bool {
    let straight =
        usize::from(clash(associations, a[0], b[0])) + usize::from(clash(associations, a[1], b[1]));
    let crossed =
        usize::from(clash(associations, a[0], b[1])) + usize::from(clash(associations, a[1], b[0]));
    match straight.cmp(&crossed) {
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => rng.random_bool(0.5),
    }
}

/// The eight play-off ties from ranks 9–24: 9/10 against 23/24, 11/12 against 21/22, 13/14
/// against 19/20 and 15/16 against 17/18, the draw choosing which of each seeded pair meets which
/// of the unseeded pair. The seeded club hosts the return leg.
pub fn playoff_ties<R: Rng>(
    outcome: &LeaguePhaseOutcome,
    associations: &BTreeMap<String, String>,
    rng: &mut R,
) -> Vec<TieSpec> {
    let ranked = &outcome.playoff;
    let mut ties = Vec::new();
    for group in 0..4 {
        let seeded = [&ranked[2 * group], &ranked[2 * group + 1]];
        // Rank r sits at index r - 9: 23/24 are 14/15, 21/22 are 12/13, and so on down to 17/18.
        let unseeded = [&ranked[14 - 2 * group], &ranked[15 - 2 * group]];
        let crossed = draw_option(
            associations,
            [seeded[0], seeded[1]],
            [unseeded[0], unseeded[1]],
            rng,
        );
        for (index, seeded_club) in seeded.iter().enumerate() {
            let opponent = unseeded[if crossed { 1 - index } else { index }];
            ties.push(TieSpec {
                stage: KnockoutStage::Playoffs,
                slot: (2 * group + index) as u8,
                path: 0,
                seeded: (*seeded_club).clone(),
                unseeded: opponent.clone(),
            });
        }
    }
    ties
}

/// The round-of-16 ties: seed pair `p` (seeds `2p+1`, `2p+2`) is drawn against the winners of the
/// play-off group that fed it (15/16 v 17/18 for seeds 1/2, down to 9/10 v 23/24 for seeds 7/8).
/// A tie lies on the path numbered after its seed, and the seed hosts the return leg.
pub fn round_of_16_ties<R: Rng>(
    outcome: &LeaguePhaseOutcome,
    playoffs: &[KnockoutTie],
    associations: &BTreeMap<String, String>,
    rng: &mut R,
) -> Option<Vec<TieSpec>> {
    let winner = |slot: usize| {
        playoffs
            .iter()
            .find(|tie| usize::from(tie.slot) == slot)
            .and_then(KnockoutTie::winner_id)
    };
    let mut ties = Vec::new();
    for pair in 0..4 {
        let group = 3 - pair;
        let winners = [winner(2 * group)?, winner(2 * group + 1)?];
        let seeds = [&outcome.direct[2 * pair], &outcome.direct[2 * pair + 1]];
        let crossed = draw_option(associations, [seeds[0], seeds[1]], winners, rng);
        for (index, seed_club) in seeds.iter().enumerate() {
            let seed_number = 2 * pair + index + 1;
            let opponent = winners[if crossed { 1 - index } else { index }];
            ties.push(TieSpec {
                stage: KnockoutStage::RoundOf16,
                slot: (seed_number - 1) as u8,
                path: seed_number as u8,
                seeded: (*seed_club).clone(),
                unseeded: opponent.to_string(),
            });
        }
    }
    Some(ties)
}

/// A tie of a later round from the two ties that feed it: the winner of the lower path hosts the
/// return leg, and the tie takes that path.
fn feeder_tie(stage: KnockoutStage, slot: usize, feeders: [&KnockoutTie; 2]) -> Option<TieSpec> {
    let [first, second] = feeders;
    let (low, high) = if first.path <= second.path {
        (first, second)
    } else {
        (second, first)
    };
    Some(TieSpec {
        stage,
        slot: slot as u8,
        path: low.path,
        seeded: low.winner_id()?.to_string(),
        unseeded: high.winner_id()?.to_string(),
    })
}

/// The ties of the round after `finished`, from that round's resolved ties. `None` while any
/// feeder is unresolved, and after the final.
pub fn next_round_ties(finished: KnockoutStage, ties: &[KnockoutTie]) -> Option<Vec<TieSpec>> {
    let of_stage = |stage: KnockoutStage| -> Vec<&KnockoutTie> {
        let mut selected: Vec<&KnockoutTie> =
            ties.iter().filter(|tie| tie.stage == stage).collect();
        selected.sort_by_key(|tie| tie.slot);
        selected
    };
    match finished {
        KnockoutStage::Playoffs | KnockoutStage::Final => None,
        KnockoutStage::RoundOf16 => {
            let round = of_stage(KnockoutStage::RoundOf16);
            let by_path = |path: u8| round.iter().find(|tie| tie.path == path).copied();
            QUARTER_FINAL_PATHS
                .iter()
                .enumerate()
                .map(|(slot, paths)| {
                    feeder_tie(
                        KnockoutStage::QuarterFinals,
                        slot,
                        [by_path(paths[0])?, by_path(paths[1])?],
                    )
                })
                .collect()
        }
        KnockoutStage::QuarterFinals => {
            let round = of_stage(KnockoutStage::QuarterFinals);
            SEMI_FINAL_QUARTERS
                .iter()
                .enumerate()
                .map(|(slot, quarters)| {
                    feeder_tie(
                        KnockoutStage::SemiFinals,
                        slot,
                        [*round.get(quarters[0])?, *round.get(quarters[1])?],
                    )
                })
                .collect()
        }
        KnockoutStage::SemiFinals => {
            let round = of_stage(KnockoutStage::SemiFinals);
            Some(vec![feeder_tie(
                KnockoutStage::Final,
                0,
                [*round.first()?, *round.get(1)?],
            )?])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::league_phase::{TieDecision, TieResolution};
    use rand::SeedableRng;

    fn outcome() -> LeaguePhaseOutcome {
        let ranking: Vec<String> = (1..=36).map(|rank| format!("r{rank:02}")).collect();
        LeaguePhaseOutcome {
            direct: ranking[..8].to_vec(),
            playoff: ranking[8..24].to_vec(),
            eliminated: ranking[24..].to_vec(),
            ranking,
            coefficient_fallback_used: false,
        }
    }

    fn rng(seed: u64) -> rand::rngs::StdRng {
        rand::rngs::StdRng::seed_from_u64(seed)
    }

    fn rank_of(club: &str) -> usize {
        club[1..].parse().unwrap()
    }

    fn resolved(spec: &TieSpec, seeded_wins: bool) -> KnockoutTie {
        KnockoutTie {
            id: format!("{:?}-{}", spec.stage, spec.slot),
            stage: spec.stage,
            slot: spec.slot,
            path: spec.path,
            seeded_team_id: spec.seeded.clone(),
            unseeded_team_id: spec.unseeded.clone(),
            fixture_ids: Vec::new(),
            resolution: Some(TieResolution {
                winner_id: if seeded_wins {
                    spec.seeded.clone()
                } else {
                    spec.unseeded.clone()
                },
                seeded_goals: 1,
                unseeded_goals: 0,
                penalties: None,
                decision: TieDecision::Aggregate,
            }),
        }
    }

    /// Given the Annex B diagram of the 2026/27 regulations, read from the image: every quarter-final
    ///       joins a tie of seed pair 5/6 with one of 3/4, or one of 7/8 with one of 1/2, each side
    ///       holds one quarter-final of each kind, and the two seeds of a pair sit on opposite sides
    /// When the layout constants are checked against that structure (stated here independently of
    ///      the constants)
    /// Then they agree, so a mis-keyed path fails this test rather than passing silently.
    #[test]
    fn the_layout_matches_the_annex_b_diagram() {
        let pair_of = |seed: u8| (seed - 1) / 2; // 0: 1/2, 1: 3/4, 2: 5/6, 3: 7/8
        // Which sides of the diagram a quarter-final may combine: (1/2 + 7/8) or (3/4 + 5/6).
        let kind_of = |quarter: &[u8; 2]| {
            let mut pairs = [pair_of(quarter[0]), pair_of(quarter[1])];
            pairs.sort_unstable();
            pairs
        };
        let upper = [0, 3];
        let lower = [1, 2];
        let kinds: Vec<[u8; 2]> = QUARTER_FINAL_PATHS.iter().map(kind_of).collect();
        assert_eq!(kinds.iter().filter(|k| **k == upper).count(), 2);
        assert_eq!(kinds.iter().filter(|k| **k == lower).count(), 2);
        // Every seed appears exactly once among the eight round-of-16 winners' places.
        let mut seeds: Vec<u8> = QUARTER_FINAL_PATHS.iter().flatten().copied().collect();
        seeds.sort_unstable();
        assert_eq!(seeds, (1..=8).collect::<Vec<u8>>());
        // A semi-final joins one quarter-final of each kind.
        for semi in SEMI_FINAL_QUARTERS {
            let (a, b) = (kinds[semi[0]], kinds[semi[1]]);
            assert!(a != b, "a side holds one quarter-final of each kind");
        }
        // The two seeds of a pair are on opposite sides, so seeds 1 and 2 cannot meet before the
        // final.
        let side_of = |seed: u8| {
            SEMI_FINAL_QUARTERS
                .iter()
                .position(|semi| {
                    semi.iter()
                        .any(|quarter| QUARTER_FINAL_PATHS[*quarter].contains(&seed))
                })
                .unwrap()
        };
        for pair in 0..4 {
            assert_ne!(side_of(2 * pair + 1), side_of(2 * pair + 2), "pair {pair}");
        }
    }

    /// Given a final ranking
    /// When the play-offs are drawn, for many seeds
    /// Then ranks 9/10 always meet 23/24, 11/12 meet 21/22, 13/14 meet 19/20, 15/16 meet 17/18,
    ///      each unseeded club exactly once, and the seeded club hosts the return leg.
    #[test]
    fn the_playoff_pairings_respect_the_seeded_groups_under_every_draw() {
        let groups = [
            (9, 10, 23, 24),
            (11, 12, 21, 22),
            (13, 14, 19, 20),
            (15, 16, 17, 18),
        ];
        for seed in 0..40 {
            let ties = playoff_ties(&outcome(), &BTreeMap::new(), &mut rng(seed));

            assert_eq!(ties.len(), 8);
            for (a, b, c, d) in groups {
                let group: Vec<&TieSpec> = ties
                    .iter()
                    .filter(|tie| [a, b].contains(&rank_of(&tie.seeded)))
                    .collect();
                assert_eq!(group.len(), 2, "seed {seed}");
                let mut opponents: Vec<usize> =
                    group.iter().map(|tie| rank_of(&tie.unseeded)).collect();
                opponents.sort_unstable();
                assert_eq!(opponents, vec![c, d], "seed {seed}");
            }
            let mut all: Vec<&String> =
                ties.iter().flat_map(|t| [&t.seeded, &t.unseeded]).collect();
            all.sort();
            all.dedup();
            assert_eq!(all.len(), 16);
        }
    }

    /// Given two draw options of which only one avoids a same-association pair
    /// When the play-offs are drawn
    /// Then the clean option is always taken.
    #[test]
    fn the_draw_avoids_a_same_association_pairing_when_it_can() {
        let associations: BTreeMap<String, String> = [("r09", "x"), ("r23", "x")]
            .into_iter()
            .map(|(club, association)| (club.to_string(), association.to_string()))
            .collect();
        for seed in 0..40 {
            let ties = playoff_ties(&outcome(), &associations, &mut rng(seed));
            let nine = ties.iter().find(|tie| tie.seeded == "r09").unwrap();
            assert_eq!(nine.unseeded, "r24", "seed {seed}");
        }
    }

    /// Given resolved play-offs
    /// When the round of 16 is drawn
    /// Then seed pair p meets the winners of its own play-off group, and a tie lies on the path of its seed.
    #[test]
    fn each_seed_pair_meets_the_winners_of_its_play_off_group() {
        let playoffs: Vec<KnockoutTie> = playoff_ties(&outcome(), &BTreeMap::new(), &mut rng(1))
            .iter()
            .map(|spec| resolved(spec, true))
            .collect();
        let feeding_group = |seed: usize| match seed {
            1 | 2 => [15, 16],
            3 | 4 => [13, 14],
            5 | 6 => [11, 12],
            _ => [9, 10],
        };

        for seed in 0..30 {
            let ties =
                round_of_16_ties(&outcome(), &playoffs, &BTreeMap::new(), &mut rng(seed)).unwrap();

            assert_eq!(ties.len(), 8);
            for tie in &ties {
                let seed_number = rank_of(&tie.seeded);
                assert_eq!(usize::from(tie.path), seed_number);
                assert!(
                    feeding_group(seed_number).contains(&rank_of(&tie.unseeded)),
                    "seed {seed}"
                );
            }
        }
    }

    /// Given a round of 16 whose seeded sides all win
    /// When the quarter-finals and semi-finals are built
    /// Then seeds 1–4 sit in different quarter-finals, seeds 1 and 2 in different semi-finals,
    ///      and the final pairs the winners of the two semi-finals.
    #[test]
    fn the_bracket_layout_satisfies_article_19_04() {
        let playoffs: Vec<KnockoutTie> = playoff_ties(&outcome(), &BTreeMap::new(), &mut rng(2))
            .iter()
            .map(|spec| resolved(spec, true))
            .collect();
        let r16: Vec<KnockoutTie> =
            round_of_16_ties(&outcome(), &playoffs, &BTreeMap::new(), &mut rng(2))
                .unwrap()
                .iter()
                .map(|spec| resolved(spec, true))
                .collect();
        let quarters: Vec<KnockoutTie> = next_round_ties(KnockoutStage::RoundOf16, &r16)
            .unwrap()
            .iter()
            .map(|spec| resolved(spec, true))
            .collect();
        let semis: Vec<KnockoutTie> = next_round_ties(KnockoutStage::QuarterFinals, &quarters)
            .unwrap()
            .iter()
            .map(|spec| resolved(spec, true))
            .collect();
        let finals = next_round_ties(KnockoutStage::SemiFinals, &semis).unwrap();

        let quarter_seeds: Vec<usize> = quarters
            .iter()
            .map(|t| rank_of(&t.seeded_team_id))
            .collect();
        let mut hosts = quarter_seeds.clone();
        hosts.sort_unstable();
        assert_eq!(
            hosts,
            vec![1, 2, 3, 4],
            "seeds 1-4 host the quarter-final return legs"
        );
        let semi_hosts: Vec<usize> = semis.iter().map(|t| rank_of(&t.seeded_team_id)).collect();
        assert!(semi_hosts.contains(&1) && semi_hosts.contains(&2));
        assert_eq!(finals.len(), 1);
        assert_eq!(finals[0].path, 1);
    }

    /// Given an upset: an unseeded club beats the seed it was drawn against
    /// When the next round is built
    /// Then it takes over the seed's bracket path, so it meets the same opponents the seed would have.
    #[test]
    fn an_upset_inherits_the_seeds_bracket_path() {
        let playoffs: Vec<KnockoutTie> = playoff_ties(&outcome(), &BTreeMap::new(), &mut rng(3))
            .iter()
            .map(|spec| resolved(spec, true))
            .collect();
        let specs = round_of_16_ties(&outcome(), &playoffs, &BTreeMap::new(), &mut rng(3)).unwrap();
        let upset_seed = 3;
        let r16: Vec<KnockoutTie> = specs
            .iter()
            .map(|spec| resolved(spec, usize::from(spec.path) != upset_seed))
            .collect();
        let underdog = r16
            .iter()
            .find(|t| usize::from(t.path) == upset_seed)
            .unwrap()
            .unseeded_team_id
            .clone();

        let quarters = next_round_ties(KnockoutStage::RoundOf16, &r16).unwrap();

        let quarter = quarters
            .iter()
            .find(|q| q.seeded == underdog || q.unseeded == underdog)
            .unwrap();
        assert_eq!(quarter.slot, 1, "path 3 feeds quarter-final B");
        assert_eq!(
            quarter.path, 3,
            "the quarter-final stays on the path the seed held"
        );
        assert_eq!(
            quarter.seeded, underdog,
            "and its host is the club holding path 3"
        );
    }

    /// Given an unresolved feeder
    /// When the next round is requested
    /// Then it is not built.
    #[test]
    fn an_unresolved_feeder_builds_nothing() {
        let playoffs: Vec<KnockoutTie> = playoff_ties(&outcome(), &BTreeMap::new(), &mut rng(4))
            .iter()
            .map(|spec| resolved(spec, true))
            .collect();
        let mut r16: Vec<KnockoutTie> =
            round_of_16_ties(&outcome(), &playoffs, &BTreeMap::new(), &mut rng(4))
                .unwrap()
                .iter()
                .map(|spec| resolved(spec, true))
                .collect();
        r16[5].resolution = None;

        assert!(next_round_ties(KnockoutStage::RoundOf16, &r16).is_none());
    }
}
