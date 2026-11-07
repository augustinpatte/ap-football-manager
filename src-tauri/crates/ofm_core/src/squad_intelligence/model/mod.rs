//! A club's squad as a team that plays a formation: who starts where, who covers for whom, and
//! how strong that is today and in the next two seasons.
//!
//! The model asks the same two questions the game itself asks when it picks a side: *which slot
//! does each player fit* (`player_rating::positional_fit_for_assignment`, the rating of a player
//! in a granular slot after the compatibility and footedness penalties) and *who is picked for
//! each slot* (the lineup picker's greedy walk over the formation's slots, a natural keeper first
//! in goal). Nothing here invents a rating bonus: a signing is worth what he changes in the side
//! that the real picker would field.
//!
//! **Expected strength.** A starter is not always there. Each player has a probability of being
//! absent (injury, suspension, tiredness); in that case the best bench player who is *not already
//! covering another slot* plays instead, and a slot that nobody is left to cover is played by an
//! emergency body ([`UNCOVERED_FIT`]). The expected strength of a slot is
//! `(1 - absence) * starter + absence * cover`, so depth is worth something exactly in proportion
//! to how likely it is to be needed and how much worse the alternative would be.
//!
//! **Horizon.** The same walk is repeated for the squad as planning expects it in one and in two
//! years ([`super::horizon`]): a prospect has grown, a veteran has declined or gone. The value of a
//! squad is the weighted sum of the three, the weights being the club's own appetite
//! ([`super::appetite`]).
//!
//! The model is pure and deterministic: no dice, no clock beyond the date it is built for, ties
//! broken by position in a list that is sorted by player id.

use chrono::NaiveDate;
use domain::player::{Player, Position, SquadRole};

use super::horizon::{HORIZON, projected_shifts};
use crate::player_rating::{formation_slots, natural_ovr, positional_fit_for_assignment};
use crate::squad_outlook::PlannedPlayer;

/// Players on the pitch.
pub(crate) const SLOTS: usize = 11;

/// What a slot is worth when nobody is left to cover it: an academy player or a body from another
/// line. A squad with a thin bench is judged by it, which is what makes depth count.
pub(crate) const UNCOVERED_FIT: f32 = 25.0;

/// Days left on the contract a signing is planned to sign (three years): he is not one the club
/// will have to renew inside the horizon.
const NEW_CONTRACT_DAYS: i64 = 1_095;

/// Chance that any given player cannot play a given match, before what is known about him.
const BASE_ABSENCE: f32 = 0.10;
/// Extra chance for a player who is injured now (he may still be on the mend when it matters).
const INJURED_ABSENCE: f32 = 0.25;
/// Extra chance for a player under suspension.
const SUSPENDED_ABSENCE: f32 = 0.20;
/// A player's long-term fitness below this raises his chance of missing games.
const FITNESS_COMFORT: f32 = 70.0;
/// Absence added per point of fitness under [`FITNESS_COMFORT`].
const ABSENCE_PER_FITNESS_POINT: f32 = 0.0025;
/// Nobody is more likely to be missing than this, or the model stops being about the player.
const MAX_ABSENCE: f32 = 0.60;

/// Days of injury from which a player is out of the club's plans for the present (the market's
/// own line, `transfers::LONG_INJURY_DAYS`); he is back in the plans for the years that follow.
const LONG_INJURY_DAYS: u32 = crate::transfers::LONG_INJURY_DAYS;

/// How a player's availability is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Availability {
    /// As he is today: a short injury or a suspension raises his chance of missing a match. What a
    /// signing and a need are judged by.
    Today,
    /// As planning sees him: the ordinary chance of missing a match for everyone, a long injury
    /// (which takes him out of the present's side altogether) apart. What he is worth to his club
    /// (his status, whether he is listed, kept or sold) is judged by, so that it does not change
    /// with every knock.
    Planning,
}

/// One player as the model sees him.
#[derive(Debug, Clone)]
pub(crate) struct Member {
    pub id: String,
    pub natural_keeper: bool,
    pub age: i32,
    pub ovr: u8,
    pub potential: u8,
    /// His rating in each slot of the formation today, before any horizon shift.
    pub fit: [f32; SLOTS],
    /// Chance he is not available for a given match.
    pub absence: f32,
    /// Change of rating by horizon year (`None`: he is not there in that year).
    pub shift: [Option<f32>; HORIZON],
    /// His rating in each slot in each horizon year, worked out from `fit` and `shift` (see
    /// [`Member::level`]); meaningless where `shift` is `None`.
    levels: [[f32; SLOTS]; HORIZON],
}

impl Member {
    /// A player as the model sees him on `today`, for the slots of a formation. `None` for a
    /// player who is not part of a senior squad (academy, retired, on loan).
    pub(crate) fn of(
        player: &Player,
        slots: &[Position],
        today: NaiveDate,
        availability: Availability,
    ) -> Option<Self> {
        if player.squad_role != SquadRole::Senior || player.active_loan.is_some() || player.retired
        {
            return None;
        }
        let planned = PlannedPlayer::of(player, today);
        let mut shift = projected_shifts(&planned);
        let out_now = player
            .injury
            .as_ref()
            .is_some_and(|injury| injury.days_remaining >= LONG_INJURY_DAYS);
        if out_now {
            shift[0] = None;
        }
        let mut member = Self {
            id: player.id.clone(),
            natural_keeper: player.natural_position == Position::Goalkeeper,
            age: planned.age,
            ovr: player.ovr,
            potential: player.potential,
            fit: fits(player, slots),
            absence: match availability {
                Availability::Today => absence_of(player),
                Availability::Planning => BASE_ABSENCE,
            },
            shift,
            levels: [[0.0; SLOTS]; HORIZON],
        };
        member.refresh_levels();
        Some(member)
    }

    /// A player the club could sign, on a fresh contract, as the club reads him: his rating
    /// moved by `perceived_error` points (an assistant's blur; zero for a club that reads the
    /// world as it is).
    pub(crate) fn candidate(
        player: &Player,
        slots: &[Position],
        today: NaiveDate,
        perceived_error: f32,
    ) -> Option<Self> {
        let mut signed = player.clone();
        signed.active_loan = None;
        let mut member = Self::of(&signed, slots, today, Availability::Today)?;
        let mut planned = PlannedPlayer::of(&signed, today);
        planned.contract_days = Some(NEW_CONTRACT_DAYS);
        let out_now = member.shift[0].is_none();
        member.shift = projected_shifts(&planned);
        if out_now {
            member.shift[0] = None;
        }
        if perceived_error != 0.0 {
            for fit in &mut member.fit {
                *fit = (*fit + perceived_error).clamp(1.0, 99.0).round();
            }
        }
        member.refresh_levels();
        Some(member)
    }

    /// His rating at `slot` in horizon year `year`, if he is there. A natural keeper is no better
    /// than an emergency body away from goal: the game's generator makes keepers fair outfield
    /// players on paper, but no club covers a winger with its second goalkeeper, and a squad must
    /// not look deep because it carries three of them.
    fn level(&self, slot: usize, year: usize) -> Option<f32> {
        self.shift[year].map(|_| self.levels[year][slot])
    }

    /// Works out [`Member::level`] for every slot and year; run whenever `fit` or `shift` changes.
    pub(super) fn refresh_levels(&mut self) {
        for year in 0..HORIZON {
            let Some(shift) = self.shift[year] else {
                continue;
            };
            for slot in 0..SLOTS {
                let fit = if self.natural_keeper && slot != 0 {
                    self.fit[slot].min(UNCOVERED_FIT)
                } else {
                    self.fit[slot]
                };
                self.levels[year][slot] = (fit + shift).clamp(1.0, 99.0);
            }
        }
    }
}

/// The rating of a player in each slot, today: his rating less what the game takes off for playing
/// that slot instead of his natural one (`positional_fit_for_assignment` against `natural_ovr`,
/// the compatibility and footedness penalties and the difference in what each slot is rated on).
/// Anchored on the stored `ovr`, the rating every screen, price and wage reads, so the model can
/// never disagree with the game about how good a player is in his own position. A player not yet
/// rated (`ovr` 0: built by hand, before `refresh_player_derived` has run) is read from his
/// attributes alone.
pub(crate) fn fits(player: &Player, slots: &[Position]) -> [f32; SLOTS] {
    let anchor = if player.ovr == 0 {
        0.0
    } else {
        f64::from(player.ovr) - natural_ovr(player)
    };
    let mut fit = [UNCOVERED_FIT; SLOTS];
    for (cell, slot) in fit.iter_mut().zip(slots) {
        *cell = (positional_fit_for_assignment(player, slot) + anchor)
            .clamp(1.0, 99.0)
            .round() as f32;
    }
    fit
}

/// How likely a player is to miss a given match, from what the club knows about him today.
pub(crate) fn absence_of(player: &Player) -> f32 {
    let mut chance = BASE_ABSENCE;
    if player.injury.is_some() {
        chance += INJURED_ABSENCE;
    }
    if player.suspended_matches > 0 {
        chance += SUSPENDED_ABSENCE;
    }
    chance += (FITNESS_COMFORT - f32::from(player.fitness)).max(0.0) * ABSENCE_PER_FITNESS_POINT;
    // To the nearest twentieth: a player's chance of a miss is not known better than that, and an
    // unchanged squad then keeps its fingerprint (and its kept answers) from one day to the next.
    (chance.min(MAX_ABSENCE) * ABSENCE_STEPS).round() / ABSENCE_STEPS
}

/// Absence is kept in steps of one over this many.
const ABSENCE_STEPS: f32 = 20.0;

/// The formation's slots, always [`SLOTS`] of them (an unreadable formation is 4-4-2, as the
/// lineup picker reads it).
pub(crate) fn slots_of(formation: &str) -> Vec<Position> {
    let mut slots = formation_slots(formation);
    slots.truncate(SLOTS);
    while slots.len() < SLOTS {
        slots.push(Position::CentralMidfielder);
    }
    slots
}

/// One horizon year of a squad: who starts where, and what the team is worth.
#[derive(Debug, Clone, Copy)]
pub(crate) struct YearView {
    /// Mean rating of the eleven that start.
    pub xi: f32,
    /// Mean expected rating of the eleven slots once absences are allowed for.
    pub expected: f32,
    /// The starter's rating in each slot.
    pub starter: [f32; SLOTS],
    /// The rating of whoever covers each slot when its starter is absent.
    pub cover: [f32; SLOTS],
    /// Index into the squad of the starter of each slot (`None`: nobody is left).
    pub starter_index: [Option<usize>; SLOTS],
    /// Index into the squad of whoever covers each slot (`None`: an emergency body does).
    pub cover_index: [Option<usize>; SLOTS],
    /// Chance the starter of each slot is absent.
    pub absence: [f32; SLOTS],
}

/// What a squad is worth: a view of each horizon year and their weighted sum.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Evaluation {
    pub years: [YearView; HORIZON],
    pub value: f32,
}

impl Evaluation {
    /// Whether the squad member at `index` plays any part in the side, in any horizon year: he
    /// starts, or he is the one who covers a slot. A member who does neither costs the side exactly
    /// nothing to lose, which is what lets a whole squad's sale costs be read without evaluating
    /// the side without each of its twenty-six.
    pub(crate) fn involves(&self, index: usize) -> bool {
        self.years.iter().any(|year| {
            year.starter_index.contains(&Some(index)) || year.cover_index.contains(&Some(index))
        })
    }
}

/// A club's senior squad for a formation.
#[derive(Debug, Clone)]
pub(crate) struct SquadModel {
    pub formation: String,
    pub slots: Vec<Position>,
    /// Sorted by player id, so that ties in the walk never depend on storage order.
    pub members: Vec<Member>,
    /// How much each horizon year counts, summing to one.
    pub weights: [f32; HORIZON],
    /// The squad as it stands, evaluated once, and the best side of each horizon year as an
    /// assignment, from which a signing is added as a chain of moves instead of a fresh solve.
    base: Evaluation,
    solutions: [YearSolution; HORIZON],
}

impl SquadModel {
    /// The squad of `players` (the club's own, in any order) for `formation` on `today`.
    pub(crate) fn build<'a>(
        players: impl Iterator<Item = &'a Player>,
        formation: &str,
        today: NaiveDate,
        weights: [f32; HORIZON],
        availability: Availability,
    ) -> Self {
        let slots = slots_of(formation);
        let mut members: Vec<Member> = players
            .filter_map(|player| Member::of(player, &slots, today, availability))
            .collect();
        members.sort_by(|a, b| a.id.cmp(&b.id));
        let mut model = Self {
            formation: formation.to_string(),
            slots,
            members,
            weights,
            base: EMPTY_EVALUATION,
            solutions: [EMPTY_SOLUTION; HORIZON],
        };
        model.solve_base();
        model
    }

    /// Solves the squad as it stands, year by year, and keeps the assignments.
    fn solve_base(&mut self) {
        let squad: Vec<&Member> = self.members.iter().collect();
        SCRATCH.with(|scratch| {
            let scratch = &mut *scratch.borrow_mut();
            let mut years = [EMPTY_YEAR; HORIZON];
            let mut value = 0.0;
            for (year, view) in years.iter_mut().enumerate() {
                scratch.used.clear();
                scratch.used.resize(squad.len(), false);
                let picks = best_assignment(&squad, year, scratch);
                self.solutions[year] = solution_of(&squad, year, &picks);
                *view = year_view_from(&squad, year, &picks, scratch);
                value += self.weights[year] * view.expected;
            }
            self.base = Evaluation { years, value };
        });
    }

    /// A number that changes when anything the model's answers depend on changes: who is in the
    /// squad, how good each is in each slot, how likely he is to be absent, where he is headed, the
    /// formation and the horizon weights. Quantised to what moves a decision, so an unchanged squad
    /// keeps its number from one day to the next and answers can be kept.
    pub(crate) fn fingerprint(&self) -> u64 {
        let mut bytes: Vec<u8> = Vec::with_capacity(self.members.len() * 48 + 32);
        bytes.extend_from_slice(self.formation.as_bytes());
        for weight in self.weights {
            bytes.extend_from_slice(&((weight * 1_000.0).round() as u16).to_le_bytes());
        }
        for member in &self.members {
            bytes.extend_from_slice(member.id.as_bytes());
            bytes.push(u8::from(member.natural_keeper));
            bytes.extend_from_slice(&member.age.to_le_bytes());
            bytes.push(member.ovr);
            bytes.push(member.potential);
            bytes.push((member.absence * ABSENCE_STEPS).round() as u8);
            for fit in member.fit {
                bytes.extend_from_slice(&(fit.round() as i16).to_le_bytes());
            }
            for shift in member.shift {
                bytes
                    .extend_from_slice(&shift.map_or(i16::MIN, |s| s.round() as i16).to_le_bytes());
            }
        }
        crate::stable_hash::stable_hash(&bytes, 0x7371_7561_6473)
    }

    /// A number that changes when anything an evaluation of `member` added to this squad depends on
    /// changes in him: how good he is in each slot, how likely he is to be absent, where he is
    /// headed. Quantised like [`SquadModel::fingerprint`].
    pub(crate) fn fingerprint_of(member: &Member) -> u64 {
        let mut bytes: Vec<u8> = Vec::with_capacity(48);
        bytes.push(u8::from(member.natural_keeper));
        bytes.push((member.absence * ABSENCE_STEPS).round() as u8);
        for fit in member.fit {
            bytes.extend_from_slice(&(fit.round() as i16).to_le_bytes());
        }
        for shift in member.shift {
            bytes.extend_from_slice(&shift.map_or(i16::MIN, |s| s.round() as i16).to_le_bytes());
        }
        crate::stable_hash::stable_hash(&bytes, 0x6d65_6d62_6572)
    }

    /// The model after the squad takes on `member`. Each horizon year's best side is the squad's
    /// own with at most one chain of moves added (see [`improving_chain`]); the cover is then read
    /// from that side.
    pub(crate) fn evaluate_with(&self, member: &Member) -> Evaluation {
        let squad: Vec<&Member> = self.members.iter().chain(Some(member)).collect();
        SCRATCH.with(|scratch| {
            let scratch = &mut *scratch.borrow_mut();
            let mut years = [EMPTY_YEAR; HORIZON];
            let mut value = 0.0;
            for (year, view) in years.iter_mut().enumerate() {
                let picks = match improving_chain(
                    &self.solutions[year],
                    member,
                    year,
                    self.members.len(),
                ) {
                    Chain::Unchanged => self.solutions[year].picks,
                    Chain::Moves(picks) => picks,
                    Chain::Unknown => {
                        scratch.used.clear();
                        scratch.used.resize(squad.len(), false);
                        best_assignment(&squad, year, scratch)
                    }
                };
                *view = year_view_from(&squad, year, &picks, scratch);
                value += self.weights[year] * view.expected;
            }
            Evaluation { years, value }
        })
    }

    /// [`SquadModel::evaluate_with`] by a fresh solve of every year, for the tests that check the
    /// chain against it.
    #[cfg(test)]
    pub(crate) fn evaluate_with_fresh(&self, member: &Member) -> Evaluation {
        self.evaluate(Some(member), None)
    }

    /// The model after `id` leaves the squad.
    pub(crate) fn evaluate_without(&self, id: &str) -> Evaluation {
        self.evaluate(None, Some(id))
    }

    /// The model after `id` leaves and `member` takes his place in the squad.
    pub(crate) fn evaluate_with_instead_of(&self, id: &str, member: &Member) -> Evaluation {
        self.evaluate(Some(member), Some(id))
    }

    /// The squad as it stands.
    pub(crate) fn evaluate_now(&self) -> Evaluation {
        self.base
    }

    fn evaluate(&self, extra: Option<&Member>, without: Option<&str>) -> Evaluation {
        let squad: Vec<&Member> = self
            .members
            .iter()
            .filter(|member| Some(member.id.as_str()) != without)
            .chain(extra)
            .collect();
        SCRATCH.with(|scratch| {
            let scratch = &mut *scratch.borrow_mut();
            let mut years = [EMPTY_YEAR; HORIZON];
            let mut value = 0.0;
            for (year, view) in years.iter_mut().enumerate() {
                *view = year_view(&squad, year, scratch);
                value += self.weights[year] * view.expected;
            }
            Evaluation { years, value }
        })
    }
}

const EMPTY_YEAR: YearView = YearView {
    xi: 0.0,
    expected: 0.0,
    starter: [0.0; SLOTS],
    cover: [UNCOVERED_FIT; SLOTS],
    starter_index: [None; SLOTS],
    cover_index: [None; SLOTS],
    absence: [0.0; SLOTS],
};

/// Buffers an evaluation works in, kept per thread so that the many evaluations of a market sweep
/// allocate nothing.
#[derive(Default)]
struct Scratch {
    used: Vec<bool>,
    alive: Vec<usize>,
    slot_cost: Vec<i64>,
    bench: Vec<usize>,
    taken: Vec<u8>,
    pairs: Vec<(f32, u8, u16)>,
    solver: Solver,
}

thread_local! {
    static SCRATCH: std::cell::RefCell<Scratch> = std::cell::RefCell::new(Scratch::default());
}

/// The side a club would field from `squad` in horizon year `year`, and who covers each slot.
fn year_view(squad: &[&Member], year: usize, scratch: &mut Scratch) -> YearView {
    scratch.used.clear();
    scratch.used.resize(squad.len(), false);
    let picks = best_assignment(squad, year, scratch);
    year_view_from(squad, year, &picks, scratch)
}

/// The view of horizon year `year` for the side `picks` (who starts in each slot): the starters,
/// who covers each slot, and what that is worth.
fn year_view_from(
    squad: &[&Member],
    year: usize,
    picks: &[Option<usize>; SLOTS],
    scratch: &mut Scratch,
) -> YearView {
    let mut view = EMPTY_YEAR;
    scratch.used.clear();
    scratch.used.resize(squad.len(), false);
    for (slot, index) in picks.iter().copied().enumerate() {
        if let Some(index) = index {
            scratch.used[index] = true;
            view.starter_index[slot] = Some(index);
            view.starter[slot] = squad[index].level(slot, year).unwrap_or(0.0);
            view.absence[slot] = squad[index].absence;
        }
    }
    assign_cover(squad, year, &mut view, scratch);
    let mut xi = 0.0;
    let mut expected = 0.0;
    for slot in 0..SLOTS {
        let absence = view.absence[slot];
        xi += view.starter[slot];
        expected += (1.0 - absence) * view.starter[slot] + absence * view.cover[slot];
    }
    view.xi = xi / SLOTS as f32;
    view.expected = expected / SLOTS as f32;
    view
}

/// The best side of one horizon year as an assignment, with what each starter would cost to play in
/// every other slot: all that is needed to add a player to it.
#[derive(Debug, Clone, Copy)]
struct YearSolution {
    picks: [Option<usize>; SLOTS],
    /// The cost of the player starting in slot `r` playing slot `r2`.
    elsewhere: [[i64; SLOTS]; SLOTS],
    /// The cost of each starter in his own slot.
    own: [i64; SLOTS],
    /// Every slot has a starter (otherwise the chain does not apply and the year is solved afresh).
    complete: bool,
}

const EMPTY_SOLUTION: YearSolution = YearSolution {
    picks: [None; SLOTS],
    elsewhere: [[0; SLOTS]; SLOTS],
    own: [0; SLOTS],
    complete: false,
};

const EMPTY_EVALUATION: Evaluation = Evaluation {
    years: [EMPTY_YEAR; HORIZON],
    value: 0.0,
};

fn solution_of(squad: &[&Member], year: usize, picks: &[Option<usize>; SLOTS]) -> YearSolution {
    let mut solution = EMPTY_SOLUTION;
    solution.picks = *picks;
    solution.complete = picks.iter().all(Option::is_some);
    for (slot, pick) in picks.iter().enumerate() {
        let Some(index) = *pick else {
            continue;
        };
        solution.own[slot] = cost_at(squad[index], slot, year);
        for other in 0..SLOTS {
            solution.elsewhere[slot][other] = cost_at(squad[index], other, year);
        }
    }
    solution
}

/// What adding a player does to a year's best side.
enum Chain {
    /// The side stays as it is: no chain of moves makes it better.
    Unchanged,
    /// The new side.
    Moves([Option<usize>; SLOTS]),
    /// The year has to be solved afresh (a slot has nobody, so the answer is not a chain).
    Unknown,
}

/// The cost no chain can reach when the new player is not there that year.
const NO_CHAIN: i64 = i64::MAX / 4;

/// Adds `member` to a year's best side. The old side is optimal, so the new best side differs from
/// it by at most one chain: `member` takes slot `a`, the player he displaces takes slot `b`
/// (displacing the player there), and so on, until the last one displaced goes to the bench. That
/// is a shortest path through the eleven slots (no cycle is shorter than nothing, because the old
/// side was best), found by Bellman-Ford in about a hundred steps.
fn improving_chain(
    solution: &YearSolution,
    member: &Member,
    year: usize,
    newcomer: usize,
) -> Chain {
    if !solution.complete {
        return Chain::Unknown;
    }
    if member.shift[year].is_none() {
        return Chain::Unchanged;
    }
    let mut reach = [NO_CHAIN; SLOTS];
    let mut came_from = [usize::MAX; SLOTS];
    for (slot, cost) in reach.iter_mut().enumerate() {
        *cost = cost_at(member, slot, year) - solution.own[slot];
    }
    for _ in 1..SLOTS {
        let mut moved = false;
        for from in 0..SLOTS {
            for to in 0..SLOTS {
                if from == to {
                    continue;
                }
                let through = reach[from] + solution.elsewhere[from][to] - solution.own[to];
                if through < reach[to] {
                    reach[to] = through;
                    came_from[to] = from;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    let (end, best) = reach
        .iter()
        .copied()
        .enumerate()
        .min_by_key(|&(slot, cost)| (cost, slot))
        .unwrap_or((0, 0));
    if best >= 0 {
        return Chain::Unchanged;
    }
    let mut picks = solution.picks;
    let mut slot = end;
    let mut steps = 0;
    loop {
        let previous = came_from[slot];
        if previous == usize::MAX {
            picks[slot] = Some(newcomer);
            break;
        }
        picks[slot] = solution.picks[previous];
        slot = previous;
        steps += 1;
        if steps > SLOTS {
            // A cycle cannot happen on an optimal side; if it ever did, solve afresh.
            return Chain::Unknown;
        }
    }
    Chain::Moves(picks)
}

/// Preference for a natural keeper in goal over any rating difference: the lineup picker's own
/// rule.
const KEEPER_IN_GOAL: f32 = 1_000.0;

/// A cost no real pairing can reach: a player who is not there that year.
const NOT_THERE: i64 = 1_000_000_000;

/// Ratings are compared to a thousandth of a point, as whole numbers, so the assignment never
/// depends on rounding noise.
const MILLI: f32 = 1_000.0;

/// What the side loses by playing `member` in `slot` (the negative of what he is worth there).
fn cost_at(member: &Member, slot: usize, year: usize) -> i64 {
    let Some(level) = member.level(slot, year) else {
        return NOT_THERE;
    };
    let level = if slot == 0 && member.natural_keeper {
        level + KEEPER_IN_GOAL
    } else {
        level
    };
    -((level * MILLI).round() as i64)
}

/// The player for each slot that makes the side strongest: the lineup picker fills the slots in
/// order, so a striker can be spent on attacking midfield before the forward line is reached and a
/// poorer one left at striker; a manager does not play it so. The best total is found with the
/// Hungarian algorithm (eleven slots, a few dozen players). A slot nobody can fill stays `None`.
fn best_assignment(
    squad: &[&Member],
    year: usize,
    scratch: &mut Scratch,
) -> [Option<usize>; SLOTS] {
    scratch.alive.clear();
    scratch
        .alive
        .extend((0..squad.len()).filter(|&index| squad[index].shift[year].is_some()));
    let columns = scratch.alive.len().max(SLOTS);
    scratch.slot_cost.clear();
    scratch.slot_cost.resize(SLOTS * columns, 0);
    // Missing players are dummy columns that cost nothing, so a real player is always preferred.
    for slot in 0..SLOTS {
        for (column, &index) in scratch.alive.iter().enumerate() {
            scratch.slot_cost[slot * columns + column] = cost_at(squad[index], slot, year);
        }
    }
    let Scratch {
        solver,
        slot_cost,
        alive,
        ..
    } = scratch;
    solver.solve(slot_cost, SLOTS, columns);
    let mut picks = [None; SLOTS];
    for (slot, pick) in picks.iter_mut().enumerate() {
        let column = solver.result[slot];
        *pick = alive
            .get(column)
            .copied()
            .filter(|&index| cost_at(squad[index], slot, year) < NOT_THERE);
    }
    picks
}

/// The assignment algorithm and the buffers it works in.
#[derive(Default)]
struct Solver {
    row_potential: Vec<i64>,
    column_potential: Vec<i64>,
    matched_row: Vec<usize>,
    way: Vec<usize>,
    minimum: Vec<i64>,
    seen: Vec<bool>,
    /// The column of each row after [`Solver::solve`].
    result: Vec<usize>,
}

impl Solver {
    /// Finds, for each of `rows` rows (`rows <= columns`), the column that minimises the total of
    /// `cost[row * columns + column]`, each column used once, and leaves it in `self.result`. The
    /// classic potentials-and-augmenting-path form: `O(rows^2 * columns)`.
    fn solve(&mut self, cost: &[i64], rows: usize, columns: usize) {
        const INF: i64 = i64::MAX / 4;
        self.row_potential.clear();
        self.row_potential.resize(rows + 1, 0);
        self.column_potential.clear();
        self.column_potential.resize(columns + 1, 0);
        self.matched_row.clear();
        self.matched_row.resize(columns + 1, 0);
        self.way.clear();
        self.way.resize(columns + 1, 0);
        for row in 1..=rows {
            self.matched_row[0] = row;
            let mut column = 0;
            self.minimum.clear();
            self.minimum.resize(columns + 1, INF);
            self.seen.clear();
            self.seen.resize(columns + 1, false);
            loop {
                self.seen[column] = true;
                let current = self.matched_row[column];
                let mut delta = INF;
                let mut next = 0;
                for candidate in 1..=columns {
                    if self.seen[candidate] {
                        continue;
                    }
                    let reduced = cost[(current - 1) * columns + candidate - 1]
                        - self.row_potential[current]
                        - self.column_potential[candidate];
                    if reduced < self.minimum[candidate] {
                        self.minimum[candidate] = reduced;
                        self.way[candidate] = column;
                    }
                    if self.minimum[candidate] < delta {
                        delta = self.minimum[candidate];
                        next = candidate;
                    }
                }
                for candidate in 0..=columns {
                    if self.seen[candidate] {
                        self.row_potential[self.matched_row[candidate]] += delta;
                        self.column_potential[candidate] -= delta;
                    } else {
                        self.minimum[candidate] -= delta;
                    }
                }
                column = next;
                if self.matched_row[column] == 0 {
                    break;
                }
            }
            loop {
                let previous = self.way[column];
                self.matched_row[column] = self.matched_row[previous];
                column = previous;
                if column == 0 {
                    break;
                }
            }
        }
        self.result.clear();
        self.result.resize(rows, 0);
        for column in 1..=columns {
            if self.matched_row[column] != 0 {
                self.result[self.matched_row[column] - 1] = column - 1;
            }
        }
    }
}

/// [`Solver::solve`] with fresh buffers, for the tests.
#[cfg(test)]
fn hungarian(cost: &[i64], rows: usize, columns: usize) -> Vec<usize> {
    let mut solver = Solver::default();
    solver.solve(cost, rows, columns);
    solver.result
}

/// Slots one bench player may cover. Two starters are rarely out together (at a one-in-ten
/// absence it is a one-in-a-hundred match), so a player covering two slots costs almost nothing,
/// while a squad whose bench is a handful of players covering everything is judged for it.
const SLOTS_PER_COVER: u8 = 2;

/// Gives each slot the bench player who would replace its starter, best pairing first, each bench
/// player covering at most [`SLOTS_PER_COVER`] slots. A slot left without a cover keeps
/// [`UNCOVERED_FIT`]. The pairings are taken in order of rating (then slot, then bench order), which
/// is the order of picking the best remaining pairing again and again.
fn assign_cover(squad: &[&Member], year: usize, view: &mut YearView, scratch: &mut Scratch) {
    let Scratch {
        used,
        bench,
        taken,
        pairs,
        ..
    } = scratch;
    bench.clear();
    bench.extend(
        (0..squad.len()).filter(|&index| !used[index] && squad[index].shift[year].is_some()),
    );
    pairs.clear();
    for (position, &index) in bench.iter().enumerate() {
        for slot in 0..SLOTS {
            pairs.push((squad[index].levels[year][slot], slot as u8, position as u16));
        }
    }
    pairs.sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    taken.clear();
    taken.resize(bench.len(), 0);
    let mut covered = [false; SLOTS];
    let mut left = SLOTS;
    for &(level, slot, position) in pairs.iter() {
        let (slot, position) = (usize::from(slot), usize::from(position));
        if covered[slot] || taken[position] >= SLOTS_PER_COVER {
            continue;
        }
        covered[slot] = true;
        taken[position] += 1;
        view.cover[slot] = level;
        view.cover_index[slot] = Some(bench[position]);
        left -= 1;
        if left == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
