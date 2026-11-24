//! The league-phase draw: who plays whom, as a constraint search.
//!
//! The draw is built, not retried: every pairing block (a pair of pots) starts as an exact
//! structure, a cycle or a perfect matching, so each club has precisely the opponents per pot the
//! rules demand; a min-conflicts repair then swaps partners *within a block* (which cannot break
//! that count) until the association rules hold. It is bounded (steps per restart, seeded
//! restarts), checked by an independent [`verify`], and a field no draw can satisfy is reported by
//! a typed error; no constraint is relaxed to manufacture a result.
//!
//! The search decides only the undirected structure. Home and away follow from it: the
//! regulation's "one home, one away" per pot (or per pair of pots) means each club's two matches
//! in a group are an in-edge and an out-edge of a directed trail, so every connected trail can be
//! oriented either way and the balance is exact by construction. The trails are returned so the
//! calendar step can flip whole trails to tidy a club's home/away sequence.

use domain::league_phase::{HomeAwayRule, LeaguePhaseRules};
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};
use std::collections::BTreeMap;

/// Repair steps one restart may use before it gives up and starts again.
const STEPS_PER_RESTART: u64 = 20_000;
/// Seeded restarts before the draw is declared unsolvable.
const MAX_RESTARTS: u32 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Infeasibility {
    /// Fewer clubs than the draw needs are eligible in one pot.
    PotTooSmall { pot: usize, eligible: usize },
    /// The association rules leave fewer eligible opponents than matches to play.
    AssociationCap { eligible: usize, needed: usize },
    /// One association's clubs in `from_pot` need opponents from `to_pot`, which has room for
    /// fewer clubs of other associations than they need to meet: the pots, as ranked, cluster the
    /// association too tightly.
    PotBlock {
        from_pot: usize,
        to_pot: usize,
        members: usize,
        room: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawError {
    InvalidRules(&'static str),
    /// The pots are not the shape the rules ask for.
    FieldShape {
        expected: usize,
        found: usize,
    },
    DuplicateClub {
        club_id: String,
    },
    /// A club has no association on record: never guessed from its name.
    MissingAssociation {
        club_id: String,
    },
    Infeasible {
        club_id: String,
        reason: Infeasibility,
    },
    /// The search found no draw inside its budget. Not proof of impossibility, but no draw.
    SearchExhausted {
        restarts: u32,
        nodes: u64,
    },
}

impl DrawError {
    /// A stable translation key for the error, for the places a player or modder reads it.
    pub fn key(&self) -> &'static str {
        match self {
            Self::InvalidRules(key) => key,
            Self::FieldShape { .. } => "be.error.leaguePhase.draw.fieldShape",
            Self::DuplicateClub { .. } => "be.error.leaguePhase.draw.duplicateClub",
            Self::MissingAssociation { .. } => "be.error.leaguePhase.draw.missingAssociation",
            Self::Infeasible { .. } => "be.error.leaguePhase.draw.infeasible",
            Self::SearchExhausted { .. } => "be.error.leaguePhase.draw.searchExhausted",
        }
    }
}

/// One undirected pairing of the draw, by club index (position in the flattened pots).
pub type DrawnEdge = (usize, usize);

/// A connected run of edges that must be oriented together: each club on it is the home side of
/// exactly one of its two edges in the run. `edges` lists `(edge index, traversed forward)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trail {
    pub edges: Vec<(usize, bool)>,
}

#[derive(Debug, Clone)]
pub struct LeagueGraph {
    pub edges: Vec<DrawnEdge>,
    pub trails: Vec<Trail>,
    pub restarts: u32,
    pub nodes: u64,
}

impl LeagueGraph {
    /// `(home, away)` of edge `index` when every trail is oriented as listed, or reversed for the
    /// trails `flipped` names.
    pub fn orient(&self, flipped: &[bool]) -> Vec<(usize, usize)> {
        let mut oriented = vec![(0, 0); self.edges.len()];
        for (trail_index, trail) in self.trails.iter().enumerate() {
            let flip = flipped.get(trail_index).copied().unwrap_or(false);
            for &(edge, forward) in &trail.edges {
                let (a, b) = self.edges[edge];
                let (tail, head) = if forward { (a, b) } else { (b, a) };
                oriented[edge] = if flip { (head, tail) } else { (tail, head) };
            }
        }
        oriented
    }
}

pub struct DrawInput<'a> {
    pub pots: &'a [Vec<String>],
    pub associations: &'a BTreeMap<String, String>,
    pub rules: &'a LeaguePhaseRules,
    pub seed: u64,
    /// Names the stream, so two competitions drawn from one seed do not draw alike.
    pub tag: &'a str,
}

struct Field {
    pot_of: Vec<usize>,
    assoc_of: Vec<usize>,
    members: Vec<Vec<usize>>,
}

fn field_of(input: &DrawInput) -> Result<Field, DrawError> {
    input.rules.validate().map_err(DrawError::InvalidRules)?;
    let rules = input.rules;
    let pot_count = usize::from(rules.pot_count);
    if input.pots.len() != pot_count {
        return Err(DrawError::FieldShape {
            expected: pot_count,
            found: input.pots.len(),
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut association_ids: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut pot_of, mut assoc_of) = (Vec::new(), Vec::new());
    let mut members = vec![Vec::new(); pot_count];
    for (pot, clubs) in input.pots.iter().enumerate() {
        if clubs.len() != rules.pot_size() {
            return Err(DrawError::FieldShape {
                expected: rules.pot_size(),
                found: clubs.len(),
            });
        }
        for club in clubs {
            if !seen.insert(club.as_str()) {
                return Err(DrawError::DuplicateClub {
                    club_id: club.clone(),
                });
            }
            let association = input
                .associations
                .get(club)
                .filter(|association| !association.is_empty())
                .ok_or_else(|| DrawError::MissingAssociation {
                    club_id: club.clone(),
                })?;
            let next_id = association_ids.len();
            let id = *association_ids
                .entry(association.as_str())
                .or_insert(next_id);
            members[pot].push(pot_of.len());
            pot_of.push(pot);
            assoc_of.push(id);
        }
    }
    Ok(Field {
        pot_of,
        assoc_of,
        members,
    })
}

/// Fails fast on a field no draw can satisfy, naming the first club that cannot be given its
/// matches: pot by pot, then across associations.
fn check_feasible(field: &Field, input: &DrawInput) -> Result<(), DrawError> {
    let rules = input.rules;
    let per_pot = usize::from(rules.opponents_per_pot);
    let cap = usize::from(rules.max_opponents_per_association);
    let club_of = |index: usize| -> String {
        input
            .pots
            .iter()
            .flatten()
            .nth(index)
            .cloned()
            .unwrap_or_default()
    };
    for club in 0..field.pot_of.len() {
        let mut per_association: BTreeMap<usize, usize> = BTreeMap::new();
        for (pot, members) in field.members.iter().enumerate() {
            let eligible: Vec<usize> = members
                .iter()
                .copied()
                .filter(|other| {
                    *other != club
                        && (rules.allow_same_association
                            || field.assoc_of[*other] != field.assoc_of[club])
                })
                .collect();
            if eligible.len() < per_pot {
                return Err(DrawError::Infeasible {
                    club_id: club_of(club),
                    reason: Infeasibility::PotTooSmall {
                        pot,
                        eligible: eligible.len(),
                    },
                });
            }
            for other in eligible {
                *per_association.entry(field.assoc_of[other]).or_default() += 1;
            }
        }
        let reachable: usize = per_association
            .iter()
            .map(|(association, count)| {
                if *association == field.assoc_of[club] {
                    *count
                } else {
                    (*count).min(cap)
                }
            })
            .sum();
        if reachable < rules.matches_per_club() {
            return Err(DrawError::Infeasible {
                club_id: club_of(club),
                reason: Infeasibility::AssociationCap {
                    eligible: reachable,
                    needed: rules.matches_per_club(),
                },
            });
        }
    }
    check_pot_blocks(field, input, &club_of)?;
    check_association_capacity(field, input, club_of)
}

/// The pot-level condition the same-association rule imposes: the clubs of one association in pot
/// `P` meet `opponents_per_pot` clubs each from pot `Q`, all of other associations, and every
/// club of `Q` outside the association can meet only that many from `P`. So `P` may not hold more
/// of an association than `Q` holds clubs outside it (`P` and `Q` may be one pot).
fn check_pot_blocks(
    field: &Field,
    input: &DrawInput,
    club_of: &impl Fn(usize) -> String,
) -> Result<(), DrawError> {
    if input.rules.allow_same_association {
        return Ok(());
    }
    let pots = usize::from(input.rules.pot_count);
    let pot_size = input.rules.pot_size();
    let mut members: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for club in 0..field.pot_of.len() {
        *members
            .entry((field.assoc_of[club], field.pot_of[club]))
            .or_default() += 1;
    }
    for (&(association, from_pot), &count) in &members {
        for to_pot in 0..pots {
            let theirs = members.get(&(association, to_pot)).copied().unwrap_or(0);
            let room = pot_size - theirs;
            if count > room {
                let club = (0..field.pot_of.len())
                    .find(|club| {
                        field.assoc_of[*club] == association && field.pot_of[*club] == from_pot
                    })
                    .unwrap_or(0);
                return Err(DrawError::Infeasible {
                    club_id: club_of(club),
                    reason: Infeasibility::PotBlock {
                        from_pot,
                        to_pot,
                        members: count,
                        room,
                    },
                });
            }
        }
    }
    Ok(())
}

/// Hall-style condition on whole associations: the matches an association's clubs must play
/// against the others cannot exceed what the per-association cap lets the two sides share, since
/// at most `cap × min(size, other size)` matches join two associations. A field that breaks it is
/// impossible however the search is tried, so it is named rather than searched.
fn check_association_capacity(
    field: &Field,
    input: &DrawInput,
    club_of: impl Fn(usize) -> String,
) -> Result<(), DrawError> {
    let rules = input.rules;
    let cap = usize::from(rules.max_opponents_per_association);
    if rules.allow_same_association {
        return Ok(());
    }
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for association in &field.assoc_of {
        *sizes.entry(*association).or_default() += 1;
    }
    for (association, size) in &sizes {
        let needed = size * rules.matches_per_club();
        let capacity: usize = sizes
            .iter()
            .filter(|(other, _)| *other != association)
            .map(|(_, other_size)| cap * size.min(other_size))
            .sum();
        if capacity < needed {
            let club = field
                .assoc_of
                .iter()
                .position(|candidate| candidate == association)
                .unwrap_or(0);
            return Err(DrawError::Infeasible {
                club_id: club_of(club),
                reason: Infeasibility::AssociationCap {
                    eligible: capacity,
                    needed,
                },
            });
        }
    }
    Ok(())
}

/// Local-search state: the pairings so far, with the counts the association rules are read from.
///
/// Every move swaps the partners of two edges *of one block* (a pair of pots, or one pot with
/// itself), so each club keeps exactly its demanded number of opponents from every pot and the
/// graph stays simple. Only the association rules can be violated, and `conflicts` counts how
/// many are.
struct Repair<'a, R: Rng> {
    field: &'a Field,
    cap: u8,
    same_ok: bool,
    edges: Vec<DrawnEdge>,
    /// Edge indices of each block, so a swap partner is found without scanning the draw.
    blocks: Vec<Vec<usize>>,
    block_of_edge: Vec<usize>,
    adjacent: Vec<Vec<bool>>,
    association_count: Vec<Vec<u8>>,
    conflicts: usize,
    rng: &'a mut R,
}

impl<R: Rng> Repair<'_, R> {
    fn same_association(&self, a: usize, b: usize) -> bool {
        self.field.assoc_of[a] == self.field.assoc_of[b]
    }

    /// Conflicts an edge adds when it is the `count`th of its kind at a club: one per same
    /// association pair, one per opponent past the cap.
    fn add_edge(&mut self, a: usize, b: usize) {
        self.adjacent[a][b] = true;
        self.adjacent[b][a] = true;
        if self.same_association(a, b) {
            self.conflicts += usize::from(!self.same_ok);
            return;
        }
        for (club, other) in [(a, b), (b, a)] {
            let slot = &mut self.association_count[club][self.field.assoc_of[other]];
            *slot += 1;
            self.conflicts += usize::from(*slot > self.cap);
        }
    }

    fn remove_edge(&mut self, a: usize, b: usize) {
        self.adjacent[a][b] = false;
        self.adjacent[b][a] = false;
        if self.same_association(a, b) {
            self.conflicts -= usize::from(!self.same_ok);
            return;
        }
        for (club, other) in [(a, b), (b, a)] {
            let slot = &mut self.association_count[club][self.field.assoc_of[other]];
            self.conflicts -= usize::from(*slot > self.cap);
            *slot -= 1;
        }
    }

    fn edge_is_in_conflict(&self, (a, b): DrawnEdge) -> bool {
        if self.same_association(a, b) {
            return !self.same_ok;
        }
        self.association_count[a][self.field.assoc_of[b]] > self.cap
            || self.association_count[b][self.field.assoc_of[a]] > self.cap
    }

    /// The ways of swapping partners between edge `e` and the other edges of its block.
    fn swaps(&self, e: usize) -> Vec<(usize, DrawnEdge, DrawnEdge)> {
        let (a, b) = self.edges[e];
        let same_pot = self.field.pot_of[a] == self.field.pot_of[b];
        let mut moves = Vec::new();
        for &f in &self.blocks[self.block_of_edge[e]] {
            if f == e {
                continue;
            }
            let (c, d) = self.edges[f];
            if same_pot {
                if [a, b].contains(&c) || [a, b].contains(&d) {
                    continue;
                }
                for (x, y) in [((a, c), (b, d)), ((a, d), (b, c))] {
                    if !self.adjacent[x.0][x.1] && !self.adjacent[y.0][y.1] {
                        moves.push((f, x, y));
                    }
                }
            } else if c != a && d != b && !self.adjacent[a][d] && !self.adjacent[c][b] {
                moves.push((f, (a, d), (c, b)));
            }
        }
        moves
    }

    fn apply(&mut self, e: usize, f: usize, new_e: DrawnEdge, new_f: DrawnEdge) {
        let (old_e, old_f) = (self.edges[e], self.edges[f]);
        self.remove_edge(old_e.0, old_e.1);
        self.remove_edge(old_f.0, old_f.1);
        self.add_edge(new_e.0, new_e.1);
        self.add_edge(new_f.0, new_f.1);
        self.edges[e] = new_e;
        self.edges[f] = new_f;
    }

    /// One repair step. Returns whether the draw is conflict-free afterwards.
    fn step(&mut self) -> bool {
        let conflicted: Vec<usize> = (0..self.edges.len())
            .filter(|e| self.edge_is_in_conflict(self.edges[*e]))
            .collect();
        let Some(&e) = conflicted.get(self.rng.random_range(0..conflicted.len().max(1))) else {
            return true;
        };
        let moves = self.swaps(e);
        if moves.is_empty() {
            return false;
        }
        let before = self.conflicts;
        let mut best: Option<(usize, usize)> = None;
        for (index, &(f, new_e, new_f)) in moves.iter().enumerate() {
            let (old_e, old_f) = (self.edges[e], self.edges[f]);
            self.apply(e, f, new_e, new_f);
            let after = self.conflicts;
            self.apply(e, f, old_e, old_f);
            let better = best.is_none_or(|(_, held)| after < held)
                || (best.is_some_and(|(_, held)| after == held) && self.rng.random_bool(0.5));
            if better {
                best = Some((index, after));
            }
        }
        let Some((chosen, after)) = best else {
            return false;
        };
        // Sideways and, now and then, uphill moves keep the search from stalling on a plateau.
        let noise = self.rng.random_bool(0.04);
        if after <= before || noise {
            let index = if noise {
                self.rng.random_range(0..moves.len())
            } else {
                chosen
            };
            let (f, new_e, new_f) = moves[index];
            self.apply(e, f, new_e, new_f);
        }
        self.conflicts == 0
    }
}

/// The starting pairings of one block, a structure with the demanded degrees and no repeats:
/// a cycle (two opponents per club) or a perfect matching (one), over a seeded order.
fn initial_block<R: Rng>(
    field: &Field,
    rules: &LeaguePhaseRules,
    (low, high): (usize, usize),
    rng: &mut R,
) -> Vec<DrawnEdge> {
    let mut first = field.members[low].clone();
    first.shuffle(rng);
    let two = rules.opponents_per_pot == 2;
    if low == high {
        let size = first.len();
        return if two {
            (0..size)
                .map(|i| (first[i], first[(i + 1) % size]))
                .collect()
        } else {
            first.chunks(2).map(|pair| (pair[0], pair[1])).collect()
        };
    }
    let mut second = field.members[high].clone();
    second.shuffle(rng);
    let size = first.len();
    let mut edges: Vec<DrawnEdge> = (0..size).map(|i| (first[i], second[i])).collect();
    if two {
        edges.extend((0..size).map(|i| (first[i], second[(i + 1) % size])));
    }
    edges
}

fn try_draw(field: &Field, input: &DrawInput, restart: u32) -> (Option<Vec<DrawnEdge>>, u64) {
    let rules = input.rules;
    let mut rng = crate::seed::rng_for_seed(input.seed, input.tag, &restart.to_string());
    let count = field.pot_of.len();
    let mut repair = Repair {
        field,
        cap: rules.max_opponents_per_association,
        same_ok: rules.allow_same_association,
        edges: Vec::new(),
        blocks: Vec::new(),
        block_of_edge: Vec::new(),
        adjacent: vec![vec![false; count]; count],
        association_count: vec![vec![0; count]; count],
        conflicts: 0,
        rng: &mut rng,
    };
    let pots = usize::from(rules.pot_count);
    for low in 0..pots {
        for high in low..pots {
            let block = initial_block(field, rules, (low, high), repair.rng);
            let block_index = repair.blocks.len();
            let mut indices = Vec::new();
            for (a, b) in block {
                indices.push(repair.edges.len());
                repair.block_of_edge.push(block_index);
                repair.edges.push((a, b));
                repair.add_edge(a, b);
            }
            repair.blocks.push(indices);
        }
    }
    let mut steps = 0;
    while repair.conflicts > 0 && steps < STEPS_PER_RESTART {
        steps += 1;
        repair.step();
    }
    let solved = repair.conflicts == 0;
    (solved.then_some(repair.edges), steps)
}

/// Draws the league phase: the set of pairings every club plays, as an undirected graph with the
/// orientation trails that give each club its home/away balance. The result is verified against
/// every constraint before it is returned.
pub fn draw(input: &DrawInput) -> Result<LeagueGraph, DrawError> {
    let field = field_of(input)?;
    check_feasible(&field, input)?;
    let (mut restarts, mut nodes) = (0, 0);
    while restarts < MAX_RESTARTS {
        let (solved, steps) = try_draw(&field, input, restarts);
        nodes += steps;
        if let Some(edges) = solved {
            let trails = trails_of(&edges, &field, input.rules.home_away);
            let graph = LeagueGraph {
                edges,
                trails,
                restarts,
                nodes,
            };
            if let Err(reason) = verify(&graph, input) {
                debug_assert!(false, "the draw broke its own rules: {reason}");
                return Err(DrawError::SearchExhausted { restarts, nodes });
            }
            return Ok(graph);
        }
        restarts += 1;
    }
    Err(DrawError::SearchExhausted { restarts, nodes })
}

/// Checks a drawn graph against every constraint of the rules, independently of the search that
/// made it. `Err` names the first one broken.
pub fn verify(graph: &LeagueGraph, input: &DrawInput) -> Result<(), String> {
    let field = field_of(input).map_err(|error| format!("{error:?}"))?;
    let rules = input.rules;
    let count = field.pot_of.len();
    let mut pairs = std::collections::BTreeSet::new();
    let mut per_pot = vec![vec![0u8; usize::from(rules.pot_count)]; count];
    let mut per_association = vec![BTreeMap::<usize, u8>::new(); count];
    for &(a, b) in &graph.edges {
        if a == b || !pairs.insert((a.min(b), a.max(b))) {
            return Err(format!("self or repeated pairing {a}-{b}"));
        }
        let same = field.assoc_of[a] == field.assoc_of[b];
        if same && !rules.allow_same_association {
            return Err(format!("same-association pairing {a}-{b}"));
        }
        for (club, other) in [(a, b), (b, a)] {
            per_pot[club][field.pot_of[other]] += 1;
            if !same {
                *per_association[club]
                    .entry(field.assoc_of[other])
                    .or_default() += 1;
            }
        }
    }
    for club in 0..count {
        if per_pot[club].iter().any(|n| *n != rules.opponents_per_pot) {
            return Err(format!("club {club} has the wrong opponents per pot"));
        }
        if per_association[club]
            .values()
            .any(|n| *n > rules.max_opponents_per_association)
        {
            return Err(format!("club {club} meets one association too often"));
        }
    }
    Ok(())
}

/// The group a club's match against `other` belongs to for the home/away rule: the opponent's
/// pot, or its pair of pots.
fn group_of(rule: HomeAwayRule, other_pot: usize) -> usize {
    match rule {
        HomeAwayRule::PerPot => other_pot,
        HomeAwayRule::PairedPots => other_pot / 2,
    }
}

/// Splits the edges into trails. At each club the two edges of a group are linked; following the
/// links from an edge walks a closed trail, and orienting the trail head-to-tail gives each club
/// on it one home and one away match in that group.
fn trails_of(edges: &[DrawnEdge], field: &Field, rule: HomeAwayRule) -> Vec<Trail> {
    // (club, group) → the edges of that group at the club.
    let mut linked: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, &(a, b)) in edges.iter().enumerate() {
        linked
            .entry((a, group_of(rule, field.pot_of[b])))
            .or_default()
            .push(index);
        linked
            .entry((b, group_of(rule, field.pot_of[a])))
            .or_default()
            .push(index);
    }
    let partner = |club: usize, edge: usize, other_pot: usize| -> Option<usize> {
        linked
            .get(&(club, group_of(rule, other_pot)))
            .and_then(|pair| pair.iter().copied().find(|candidate| *candidate != edge))
    };
    let mut visited = vec![false; edges.len()];
    let mut trails = Vec::new();
    for start in 0..edges.len() {
        if visited[start] {
            continue;
        }
        let mut trail = Vec::new();
        let (mut edge, mut forward) = (start, true);
        loop {
            visited[edge] = true;
            trail.push((edge, forward));
            let (a, b) = edges[edge];
            let (tail, head) = if forward { (a, b) } else { (b, a) };
            // Leave `head` along the other edge of the group this edge belongs to there.
            let Some(next) = partner(head, edge, field.pot_of[tail]) else {
                break;
            };
            if next == start {
                break;
            }
            if visited[next] {
                break;
            }
            let (next_a, _) = edges[next];
            forward = next_a == head;
            edge = next;
        }
        trails.push(Trail { edges: trail });
    }
    trails
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// 36 clubs, strongest first, dealt to associations of the given sizes round-robin (so each
    /// association's clubs are spread across the pots, the realistic and harder case), in pots.
    fn field_of_sizes(
        rules: &LeaguePhaseRules,
        sizes: &[usize],
    ) -> (Vec<Vec<String>>, BTreeMap<String, String>) {
        assert_eq!(sizes.iter().sum::<usize>(), 36);
        let mut left = sizes.to_vec();
        let mut next = 0;
        let mut map = BTreeMap::new();
        let clubs: Vec<String> = (0..36).map(|i| format!("club-{i:02}")).collect();
        for club in &clubs {
            while left[next % sizes.len()] == 0 {
                next += 1;
            }
            let association = next % sizes.len();
            left[association] -= 1;
            map.insert(club.clone(), format!("assoc-{association}"));
            next += 1;
        }
        let pots = clubs
            .chunks(rules.pot_size())
            .map(<[String]>::to_vec)
            .collect();
        (pots, map)
    }

    /// A field with a realistic skew: a few big associations, many small.
    const REALISTIC: [usize; 15] = [4, 4, 4, 4, 3, 3, 3, 2, 2, 2, 1, 1, 1, 1, 1];

    fn field(
        rules: &LeaguePhaseRules,
        associations: usize,
    ) -> (Vec<Vec<String>>, BTreeMap<String, String>) {
        let base = 36 / associations;
        let mut sizes = vec![base; associations];
        for size in sizes.iter_mut().take(36 - base * associations) {
            *size += 1;
        }
        field_of_sizes(rules, &sizes)
    }

    fn drawn(
        rules: &LeaguePhaseRules,
        associations: usize,
        seed: u64,
    ) -> Result<LeagueGraph, DrawError> {
        let (pots, map) = field(rules, associations);
        draw_field(rules, &pots, &map, seed)
    }

    fn draw_field(
        rules: &LeaguePhaseRules,
        pots: &[Vec<String>],
        map: &BTreeMap<String, String>,
        seed: u64,
    ) -> Result<LeagueGraph, DrawError> {
        draw(&DrawInput {
            pots,
            associations: map,
            rules,
            seed,
            tag: "test",
        })
    }

    /// Given a field with plenty of associations
    /// When the Champions League draw is made
    /// Then 144 distinct pairs, 8 per club, two from each pot, no same-association pair and at
    ///      most two opponents from any association.
    #[test]
    fn a_four_pot_draw_satisfies_every_regulation_constraint() {
        let rules = LeaguePhaseRules::four_pots_of_nine();
        let (pots, map) = field(&rules, 14);
        let graph = drawn(&rules, 14, 7).unwrap();

        assert_eq!(graph.edges.len(), 144);
        let flat: Vec<&String> = pots.iter().flatten().collect();
        let mut pairs = BTreeSet::new();
        let mut per_pot = vec![vec![0usize; 4]; 36];
        let mut per_association: Vec<BTreeMap<&String, usize>> = vec![BTreeMap::new(); 36];
        let mut degree = [0; 36];
        for &(a, b) in &graph.edges {
            assert_ne!(a, b);
            assert!(pairs.insert((a.min(b), a.max(b))), "repeated pair");
            assert_ne!(map[flat[a]], map[flat[b]], "same association");
            for (x, y) in [(a, b), (b, a)] {
                per_pot[x][y / 9] += 1;
                *per_association[x].entry(&map[flat[y]]).or_default() += 1;
                degree[x] += 1;
            }
        }
        assert!(degree.iter().all(|d| *d == 8));
        assert!(per_pot.iter().all(|row| row.iter().all(|n| *n == 2)));
        assert!(
            per_association
                .iter()
                .all(|row| row.values().all(|n| *n <= 2))
        );
    }

    /// Given the same seed and field
    /// When the draw is made twice, and once with another seed
    /// Then the first two are identical and the third differs.
    #[test]
    fn the_draw_is_reproducible_from_its_seed() {
        let rules = LeaguePhaseRules::four_pots_of_nine();
        let first = drawn(&rules, 14, 11).unwrap();
        let again = drawn(&rules, 14, 11).unwrap();
        let other = drawn(&rules, 14, 12).unwrap();

        assert_eq!(first.edges, again.edges);
        assert_eq!(first.trails, again.trails);
        assert_ne!(first.edges, other.edges);
    }

    /// Given the Conference League shape
    /// When it is drawn
    /// Then 108 pairs, six per club, exactly one opponent from every pot (its own included).
    #[test]
    fn a_six_pot_draw_gives_one_opponent_from_every_pot() {
        let rules = LeaguePhaseRules::six_pots_of_six();
        let graph = drawn(&rules, 14, 5).unwrap();

        assert_eq!(graph.edges.len(), 108);
        let mut per_pot = vec![vec![0usize; 6]; 36];
        for &(a, b) in &graph.edges {
            per_pot[a][b / 6] += 1;
            per_pot[b][a / 6] += 1;
        }
        assert!(per_pot.iter().all(|row| row.iter().all(|n| *n == 1)));
    }

    /// Given any drawn graph
    /// When its trails are oriented as listed or with some trails flipped
    /// Then each club is home exactly once and away exactly once per group, in both rules.
    #[test]
    fn orientation_balances_every_group_however_the_trails_are_flipped() {
        for rules in [
            LeaguePhaseRules::four_pots_of_nine(),
            LeaguePhaseRules::six_pots_of_six(),
        ] {
            let graph = drawn(&rules, 14, 3).unwrap();
            let flips: Vec<bool> = (0..graph.trails.len()).map(|i| i % 3 == 0).collect();
            for oriented in [graph.orient(&[]), graph.orient(&flips)] {
                let group = |other: usize| match rules.home_away {
                    HomeAwayRule::PerPot => other / rules.pot_size(),
                    HomeAwayRule::PairedPots => other / rules.pot_size() / 2,
                };
                let mut home = BTreeMap::<(usize, usize), usize>::new();
                let mut away = BTreeMap::<(usize, usize), usize>::new();
                for &(h, a) in &oriented {
                    *home.entry((h, group(a))).or_default() += 1;
                    *away.entry((a, group(h))).or_default() += 1;
                }
                assert!(home.values().all(|n| *n == 1), "{rules:?}");
                assert!(away.values().all(|n| *n == 1), "{rules:?}");
                let total_home: usize = home.values().sum();
                assert_eq!(total_home, graph.edges.len());
            }
        }
    }

    /// Given a field whose strongest pot is mostly one association
    /// When it is drawn
    /// Then the pot-level impossibility is named up front (not discovered by a search that gives up).
    #[test]
    fn an_association_clustered_in_one_pot_is_named_as_a_pot_block() {
        let rules = LeaguePhaseRules::four_pots_of_nine();
        let (pots, mut map) = field(&rules, 18);
        for club in pots[0].iter().take(5) {
            map.insert(club.clone(), "cluster".into());
        }

        let error = draw_field(&rules, &pots, &map, 1).unwrap_err();

        assert!(
            matches!(
                error,
                DrawError::Infeasible {
                    reason: Infeasibility::PotBlock {
                        from_pot: 0,
                        members: 5,
                        room: 4,
                        ..
                    },
                    ..
                }
            ),
            "{error:?}"
        );
    }

    /// Given a field where one association holds half the clubs
    /// When it is drawn
    /// Then a typed infeasibility names a club, and no constraint is relaxed.
    #[test]
    fn an_impossible_field_is_a_typed_error_not_a_relaxed_draw() {
        let rules = LeaguePhaseRules::four_pots_of_nine();

        let error = drawn(&rules, 3, 1).unwrap_err();

        assert!(
            matches!(
                error,
                DrawError::Infeasible {
                    reason: Infeasibility::AssociationCap { .. },
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(error.key(), "be.error.leaguePhase.draw.infeasible");
    }

    /// Given malformed input (a club without an association, a repeated club, wrong pot size)
    /// When it is drawn
    /// Then each is refused with its own error.
    #[test]
    fn malformed_fields_are_refused() {
        let rules = LeaguePhaseRules::four_pots_of_nine();
        let (pots, mut map) = field(&rules, 14);
        let input = |pots: &Vec<Vec<String>>, map: &BTreeMap<String, String>| {
            draw(&DrawInput {
                pots,
                associations: map,
                rules: &rules,
                seed: 1,
                tag: "t",
            })
        };

        map.remove("club-04");
        assert!(matches!(
            input(&pots, &map),
            Err(DrawError::MissingAssociation { .. })
        ));

        let (mut pots, map) = field(&rules, 14);
        pots[1][0] = pots[0][0].clone();
        assert!(matches!(
            input(&pots, &map),
            Err(DrawError::DuplicateClub { .. })
        ));

        let (mut pots, map) = field(&rules, 14);
        pots[3].pop();
        assert!(matches!(
            input(&pots, &map),
            Err(DrawError::FieldShape { .. })
        ));
    }

    /// Given the explicit relaxation of the same-association rule
    /// When a field that is otherwise impossible is drawn
    /// Then the draw is allowed, and only because the rules said so.
    #[test]
    fn same_association_can_only_be_relaxed_by_the_rules_themselves() {
        let mut rules = LeaguePhaseRules::four_pots_of_nine();
        assert!(drawn(&rules, 4, 1).is_err());

        rules.allow_same_association = true;
        let relaxed = drawn(&rules, 4, 1);

        assert!(relaxed.is_ok(), "{relaxed:?}");
    }

    /// Given many seeds on a realistic skewed field (associations of four down to one club)
    /// When each draw is made
    /// Then every one succeeds within the search budget, for both formats.
    #[test]
    fn every_seed_of_a_realistic_field_draws() {
        for rules in [
            LeaguePhaseRules::four_pots_of_nine(),
            LeaguePhaseRules::six_pots_of_six(),
        ] {
            let (pots, map) = field_of_sizes(&rules, &REALISTIC);
            for seed in 0..60 {
                let graph = draw_field(&rules, &pots, &map, seed)
                    .unwrap_or_else(|error| panic!("seed {seed}: {error:?}"));
                assert_eq!(graph.edges.len(), rules.league_fixture_count());
            }
        }
    }

    /// Given an association of eight clubs, more than the caps let the others absorb
    /// When it is drawn
    /// Then it is named infeasible up front instead of searched for.
    #[test]
    fn an_association_too_big_for_the_caps_is_named_before_any_search() {
        let rules = LeaguePhaseRules::four_pots_of_nine();
        let mut sizes = vec![8];
        sizes.extend([7, 7, 7, 7]);

        let (pots, map) = field_of_sizes(&rules, &sizes);
        let error = draw_field(&rules, &pots, &map, 1).unwrap_err();

        assert!(matches!(error, DrawError::Infeasible { .. }), "{error:?}");
    }

    /// Club counts per association of the three 2026/27 fields as UEFA published them
    /// (uefa.com pages of 4 and 23 November 2026): CL has five English and five Spanish clubs,
    /// four German, four Italian and three French; the Europa and Conference fields are flatter.
    const CL_2026_27: [usize; 16] = [5, 5, 4, 4, 3, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1];
    const EL_2026_27: [usize; 22] = [
        3, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ];
    const ECL_2026_27: [usize; 30] = [
        4, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ];

    /// Pots of a field whose clubs are ranked in a random order (the ranking is independent of the
    /// association, which is what a coefficient order is to the real field).
    fn shuffled_field(
        sizes: &[usize],
        pot_size: usize,
        shuffle_seed: u64,
    ) -> (Vec<String>, BTreeMap<String, String>) {
        use rand::SeedableRng;
        use rand::seq::SliceRandom;
        let mut order: Vec<usize> = Vec::new();
        for (association, size) in sizes.iter().enumerate() {
            order.extend(std::iter::repeat_n(association, *size));
        }
        assert_eq!(order.len(), 36);
        assert_eq!(36 % pot_size, 0);
        order.shuffle(&mut rand::rngs::StdRng::seed_from_u64(shuffle_seed));
        let clubs: Vec<String> = (0..36).map(|i| format!("club-{i:02}")).collect();
        let map = clubs
            .iter()
            .zip(&order)
            .map(|(club, association)| (club.clone(), format!("assoc-{association}")))
            .collect();
        (clubs, map)
    }

    /// Given the complete 2026/27 fields of all three competitions, with the invented
    ///       whole-edition association cap removed
    /// When each is drawn under 300 different rankings
    /// Then no legitimately qualified club is dropped: every draw succeeds and passes the
    ///      independent verification, with no same-association pair and at most two opponents
    ///      from one other association, relaxing the pots (by recorded trades) only where the
    ///      ranking leaves no other draw.
    #[test]
    fn the_real_2026_27_fields_draw_under_every_ranking_without_a_cap() {
        use crate::league_phase::pots;
        let cases: [(LeaguePhaseRules, &[usize]); 3] = [
            (LeaguePhaseRules::four_pots_of_nine(), &CL_2026_27),
            (LeaguePhaseRules::four_pots_of_nine(), &EL_2026_27),
            (LeaguePhaseRules::six_pots_of_six(), &ECL_2026_27),
        ];
        let mut traded_fields = 0;
        for (mut rules, sizes) in cases {
            rules.max_clubs_per_association = 0;
            assert_eq!(rules.validate(), Ok(()));
            for shuffle_seed in 0..300u64 {
                let (clubs, map) = shuffled_field(sizes, rules.pot_size(), shuffle_seed);
                let (seeding, trades) = pots::relax(&clubs, &map, &rules);
                traded_fields += usize::from(!trades.is_empty());
                let pots: Vec<Vec<String>> = seeding
                    .chunks(rules.pot_size())
                    .map(<[String]>::to_vec)
                    .collect();
                let graph = draw_field(&rules, &pots, &map, shuffle_seed)
                    .unwrap_or_else(|e| panic!("seed {shuffle_seed}, {sizes:?}: {e:?}"));
                let input = DrawInput {
                    pots: &pots,
                    associations: &map,
                    rules: &rules,
                    seed: shuffle_seed,
                    tag: "test",
                };
                assert_eq!(verify(&graph, &input), Ok(()));
                assert!(trades.len() <= 2, "{trades:?}");
            }
        }
        // The relaxation is the exception, not the rule: a few fields out of 900 need a trade.
        assert!(traded_fields < 30, "{traded_fields} of 900 needed a trade");
    }

    /// Given the 2026/27 Champions League field of five English and five Spanish clubs
    /// When the whole-edition limit is read from the rules
    /// Then the shipped profile admits all ten (no cap), where the old stand-in capped it at four.
    #[test]
    fn the_shipped_champions_league_profile_has_no_whole_edition_cap() {
        let rules = crate::league_phase::profiles::champions_league_2026();
        assert_eq!(rules.max_clubs_per_association, 0);
        let mut capped = LeaguePhaseRules::four_pots_of_nine();
        capped.max_clubs_per_association = 5;
        assert!(
            capped.validate().is_err(),
            "a cap above half a pot is still refused"
        );
    }
}
