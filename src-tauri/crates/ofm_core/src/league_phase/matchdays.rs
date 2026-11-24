//! Putting the draw on matchdays: every club plays exactly once per matchday.
//!
//! The pairings are an `M`-regular graph over the clubs, and a matchday is a perfect matching of
//! it, so this is a proper edge colouring with `M` colours (Kempe-chain repair, seeded restarts).
//! Which matchday is which, and which way each orientation trail runs, are then free choices, and
//! they are spent on the regulation's soft home/away pattern (17.02): no more than two home or
//! two away matches in a row, and one of each across the first two and the last two matchdays.
//! The pattern is *normally* applied (UEFA may derogate), so the search minimises violations and
//! reports what is left rather than failing.

use super::draw::LeagueGraph;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

const COLOURING_ATTEMPTS: u32 = 400;
const PATTERN_MOVES: u32 = 120_000;
/// Proper colourings tried (each annealed) before the best is kept.
const COLOURINGS_TRIED: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledMatch {
    /// 0-based matchday.
    pub matchday: usize,
    pub home: usize,
    pub away: usize,
}

#[derive(Debug, Clone)]
pub struct MatchdayPlan {
    pub matches: Vec<ScheduledMatch>,
    /// Home/away pattern violations left (see [`pattern_cost`]); 0 satisfies 17.02 everywhere.
    pub pattern_violations: usize,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    /// No proper colouring was found: the pairings cannot be put on `matchdays` matchdays.
    NoColouring { matchdays: usize, attempts: u32 },
}

/// Colours the edges with `matchdays` colours so that no club has two edges of one colour.
fn colour_edges<R: Rng>(
    club_count: usize,
    edges: &[(usize, usize)],
    matchdays: usize,
    rng: &mut R,
) -> Option<Vec<usize>> {
    // at[club][colour] = the edge of that colour at the club.
    let mut at: Vec<Vec<Option<usize>>> = vec![vec![None; matchdays]; club_count];
    let mut colour: Vec<Option<usize>> = vec![None; edges.len()];
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.shuffle(rng);
    for edge in order {
        let (u, v) = edges[edge];
        let mut common: Vec<usize> = (0..matchdays)
            .filter(|c| at[u][*c].is_none() && at[v][*c].is_none())
            .collect();
        common.shuffle(rng);
        if let Some(&c) = common.first() {
            set(&mut at, &mut colour, edges, edge, c);
            continue;
        }
        if !recolour_by_kempe_chain(&mut at, &mut colour, edges, edge, rng) {
            return None;
        }
    }
    colour.into_iter().collect()
}

fn set(
    at: &mut [Vec<Option<usize>>],
    colour: &mut [Option<usize>],
    edges: &[(usize, usize)],
    edge: usize,
    c: usize,
) {
    let (u, v) = edges[edge];
    at[u][c] = Some(edge);
    at[v][c] = Some(edge);
    colour[edge] = Some(c);
}

fn clear(
    at: &mut [Vec<Option<usize>>],
    colour: &mut [Option<usize>],
    edges: &[(usize, usize)],
    edge: usize,
) {
    if let Some(c) = colour[edge].take() {
        let (u, v) = edges[edge];
        at[u][c] = None;
        at[v][c] = None;
    }
}

/// Frees a colour at both ends of `edge` by swapping the colours `a` and `b` along the
/// alternating path that leaves `v`, then colours the edge. `a` is free at `u`, `b` at `v`.
fn recolour_by_kempe_chain<R: Rng>(
    at: &mut [Vec<Option<usize>>],
    colour: &mut [Option<usize>],
    edges: &[(usize, usize)],
    edge: usize,
    rng: &mut R,
) -> bool {
    let (u, v) = edges[edge];
    let matchdays = at[u].len();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for a in (0..matchdays).filter(|c| at[u][*c].is_none()) {
        for b in (0..matchdays).filter(|c| at[v][*c].is_none()) {
            pairs.push((a, b));
        }
    }
    pairs.shuffle(rng);
    for (a, b) in pairs {
        // The (a, b)-alternating path from v starts with colour a (b is free at v).
        let mut path = Vec::new();
        let (mut at_club, mut wanted) = (v, a);
        while let Some(next) = at[at_club][wanted] {
            path.push(next);
            let (x, y) = edges[next];
            at_club = if x == at_club { y } else { x };
            wanted = if wanted == a { b } else { a };
        }
        // A path that ends at u would be closed into a cycle by the swap: try another pair.
        if at_club == u {
            continue;
        }
        let swapped: Vec<(usize, usize)> = path
            .iter()
            .map(|&e| (e, if colour[e] == Some(a) { b } else { a }))
            .collect();
        for &(e, _) in &swapped {
            clear(at, colour, edges, e);
        }
        for &(e, c) in &swapped {
            set(at, colour, edges, e, c);
        }
        set(at, colour, edges, edge, a);
        return true;
    }
    false
}

/// Home/away pattern violations of one club's sequence over the matchdays (`true` = home).
fn sequence_cost(sequence: &[bool]) -> usize {
    let triples = sequence
        .windows(3)
        .filter(|w| w[0] == w[1] && w[1] == w[2])
        .count();
    let count = sequence.len();
    let opening = usize::from(count >= 2 && sequence[0] == sequence[1]);
    let closing = usize::from(count >= 2 && sequence[count - 1] == sequence[count - 2]);
    triples + opening + closing
}

/// Total home/away pattern violations over all clubs: the cost the pattern search minimises.
pub fn pattern_cost(club_count: usize, matchdays: usize, matches: &[ScheduledMatch]) -> usize {
    let mut sequences = vec![vec![true; matchdays]; club_count];
    for m in matches {
        sequences[m.home][m.matchday] = true;
        sequences[m.away][m.matchday] = false;
    }
    sequences.iter().map(|s| sequence_cost(s)).sum()
}

fn place(
    graph: &LeagueGraph,
    colours: &[usize],
    labels: &[usize],
    flips: &[bool],
) -> Vec<ScheduledMatch> {
    graph
        .orient(flips)
        .into_iter()
        .enumerate()
        .map(|(edge, (home, away))| ScheduledMatch {
            matchday: labels[colours[edge]],
            home,
            away,
        })
        .collect()
}

/// Swaps the colours `a` and `b` along the chain of edges through `club` that alternate between
/// them. A proper colouring stays proper, and the matches of the clubs on the chain trade
/// matchdays: the move that lets the pattern search change *who plays when*, not only the order
/// of the matchdays.
fn kempe_swap(
    colours: &mut [usize],
    edges: &[(usize, usize)],
    club_count: usize,
    matchdays: usize,
    (club, a, b): (usize, usize, usize),
) {
    let mut at = vec![vec![usize::MAX; matchdays]; club_count];
    for (edge, &(x, y)) in edges.iter().enumerate() {
        at[x][colours[edge]] = edge;
        at[y][colours[edge]] = edge;
    }
    let Some(start) = [a, b]
        .into_iter()
        .map(|c| at[club][c])
        .find(|edge| *edge != usize::MAX)
    else {
        return;
    };
    let mut chain = vec![start];
    let mut seen = std::collections::BTreeSet::from([start]);
    let mut cursor = 0;
    while cursor < chain.len() {
        let edge = chain[cursor];
        cursor += 1;
        let other = if colours[edge] == a { b } else { a };
        for end in [edges[edge].0, edges[edge].1] {
            let next = at[end][other];
            if next != usize::MAX && seen.insert(next) {
                chain.push(next);
            }
        }
    }
    for edge in chain {
        colours[edge] = if colours[edge] == a { b } else { a };
    }
}

/// Searches colouring, matchday order and trail orientation for the fewest pattern violations,
/// by simulated annealing from a proper colouring. Returns the best `(colours, labels, flips,
/// cost)` seen.
fn anneal_pattern<R: Rng>(
    graph: &LeagueGraph,
    start: &[usize],
    club_count: usize,
    matchdays: usize,
    rng: &mut R,
) -> (Vec<usize>, Vec<usize>, Vec<bool>, usize) {
    let mut colours = start.to_vec();
    let mut labels: Vec<usize> = (0..matchdays).collect();
    let mut flips: Vec<bool> = (0..graph.trails.len())
        .map(|_| rng.random_bool(0.5))
        .collect();
    let cost_of = |colours: &[usize], labels: &[usize], flips: &[bool]| {
        pattern_cost(club_count, matchdays, &place(graph, colours, labels, flips))
    };
    let mut cost = cost_of(&colours, &labels, &flips);
    let mut best = (colours.clone(), labels.clone(), flips.clone(), cost);
    for step in 0..PATTERN_MOVES {
        if best.3 == 0 {
            break;
        }
        // Linear cooling from a temperature that accepts a few violations uphill to one that
        // does not.
        let temperature = 1.2 * (1.0 - f64::from(step) / f64::from(PATTERN_MOVES)) + 0.05;
        let saved = (colours.clone(), labels.clone(), flips.clone());
        match rng.random_range(0..10) {
            0..=2 if !flips.is_empty() => {
                let trail = rng.random_range(0..flips.len());
                flips[trail] = !flips[trail];
            }
            3 => {
                let (x, y) = (
                    rng.random_range(0..matchdays),
                    rng.random_range(0..matchdays),
                );
                labels.swap(x, y);
            }
            _ => {
                let a = rng.random_range(0..matchdays);
                let b = (a + 1 + rng.random_range(0..matchdays - 1)) % matchdays;
                let club = rng.random_range(0..club_count);
                kempe_swap(
                    &mut colours,
                    &graph.edges,
                    club_count,
                    matchdays,
                    (club, a, b),
                );
            }
        }
        let next = cost_of(&colours, &labels, &flips);
        let accept = next <= cost
            || rng.random_range(0.0..1.0) < (-((next - cost) as f64) / temperature).exp();
        if accept {
            cost = next;
            if cost < best.3 {
                best = (colours.clone(), labels.clone(), flips.clone(), cost);
            }
        } else {
            (colours, labels, flips) = saved;
        }
    }
    best
}

/// Puts the draw on `matchdays` matchdays. Deterministic in `seed` and `tag`.
pub fn schedule(
    graph: &LeagueGraph,
    club_count: usize,
    matchdays: usize,
    seed: u64,
    tag: &str,
) -> Result<MatchdayPlan, ScheduleError> {
    let mut best: Option<MatchdayPlan> = None;
    let mut colourings = 0;
    for attempt in 0..COLOURING_ATTEMPTS {
        let mut rng = crate::seed::rng_for_seed(seed, tag, &format!("colour/{attempt}"));
        let Some(colours) = colour_edges(club_count, &graph.edges, matchdays, &mut rng) else {
            continue;
        };
        colourings += 1;
        let mut rng = crate::seed::rng_for_seed(seed, tag, &format!("pattern/{attempt}"));
        let (colours, labels, flips, cost) =
            anneal_pattern(graph, &colours, club_count, matchdays, &mut rng);
        if best
            .as_ref()
            .is_none_or(|held| cost < held.pattern_violations)
        {
            best = Some(MatchdayPlan {
                matches: place(graph, &colours, &labels, &flips),
                pattern_violations: cost,
                attempts: attempt + 1,
            });
        }
        if cost == 0 || colourings >= COLOURINGS_TRIED {
            break;
        }
    }
    best.ok_or(ScheduleError::NoColouring {
        matchdays,
        attempts: COLOURING_ATTEMPTS,
    })
}

#[cfg(test)]
mod tests {
    use super::super::draw::{DrawInput, draw};
    use super::*;
    use domain::league_phase::LeaguePhaseRules;
    use std::collections::{BTreeMap, BTreeSet};

    fn graph_for(rules: &LeaguePhaseRules, seed: u64) -> LeagueGraph {
        let clubs: Vec<String> = (0..36).map(|i| format!("c{i:02}")).collect();
        let map: BTreeMap<String, String> = clubs
            .iter()
            .enumerate()
            .map(|(i, c)| (c.clone(), format!("a{}", i % 18)))
            .collect();
        let pots: Vec<Vec<String>> = clubs
            .chunks(rules.pot_size())
            .map(<[String]>::to_vec)
            .collect();
        draw(&DrawInput {
            pots: &pots,
            associations: &map,
            rules,
            seed,
            tag: "md",
        })
        .unwrap()
    }

    /// Given a draw
    /// When it is put on matchdays
    /// Then every club plays exactly once on every matchday, each pairing exactly once.
    #[test]
    fn every_club_plays_once_per_matchday() {
        for rules in [
            LeaguePhaseRules::four_pots_of_nine(),
            LeaguePhaseRules::six_pots_of_six(),
        ] {
            let graph = graph_for(&rules, 9);
            let days = rules.matches_per_club();

            let plan = schedule(&graph, 36, days, 9, "md").unwrap();

            assert_eq!(plan.matches.len(), rules.league_fixture_count());
            for day in 0..days {
                let mut seen = BTreeSet::new();
                for m in plan.matches.iter().filter(|m| m.matchday == day) {
                    assert!(seen.insert(m.home) && seen.insert(m.away), "double booking");
                }
                assert_eq!(seen.len(), 36, "matchday {day} must hold all 36 clubs");
            }
        }
    }

    /// Given a schedule
    /// When each club's home and away matches are counted
    /// Then it is exact: four and four, or three and three.
    #[test]
    fn home_and_away_are_exactly_balanced() {
        for rules in [
            LeaguePhaseRules::four_pots_of_nine(),
            LeaguePhaseRules::six_pots_of_six(),
        ] {
            let plan =
                schedule(&graph_for(&rules, 4), 36, rules.matches_per_club(), 4, "md").unwrap();
            let mut home = vec![0; 36];
            for m in &plan.matches {
                home[m.home] += 1;
            }
            let each = rules.matches_per_club() / 2;
            assert!(home.iter().all(|n| *n == each), "{home:?}");
        }
    }

    /// Given several seeds of each format
    /// When each is scheduled
    /// Then it always colours, and the soft 17.02 pattern is nearly met: at most a handful of
    ///      club-level violations (the rule is "normally"; UEFA may derogate), never a wholesale
    ///      failure. The study example measures the full distribution.
    #[test]
    fn the_soft_pattern_is_nearly_met_for_every_seed() {
        for rules in [
            LeaguePhaseRules::four_pots_of_nine(),
            LeaguePhaseRules::six_pots_of_six(),
        ] {
            for seed in 0..8 {
                let plan = schedule(
                    &graph_for(&rules, seed),
                    36,
                    rules.matches_per_club(),
                    seed,
                    "md",
                )
                .unwrap();
                assert!(
                    plan.pattern_violations <= 8,
                    "seed {seed}: {} violations",
                    plan.pattern_violations
                );
            }
        }
    }

    /// Given a sequence of venues
    /// When its cost is read
    /// Then three in a row, and a repeated venue in the first or last two, each cost one.
    #[test]
    fn the_pattern_cost_counts_what_the_regulation_forbids() {
        assert_eq!(
            sequence_cost(&[true, false, true, false, true, false, true, false]),
            0
        );
        assert_eq!(
            sequence_cost(&[true, true, true, false, true, false, true, false]),
            2
        );
        assert_eq!(
            sequence_cost(&[true, false, true, false, true, false, false, false]),
            2
        );
    }
}
