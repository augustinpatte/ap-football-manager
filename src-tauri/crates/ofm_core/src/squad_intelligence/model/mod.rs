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
use crate::player_rating::{formation_slots, positional_fit_for_assignment};
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

/// One player as the model sees him.
#[derive(Debug, Clone)]
pub(crate) struct Member {
    pub id: String,
    /// 0 goalkeeper, 1 defender, 2 midfielder, 3 forward.
    pub group: usize,
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
}

impl Member {
    /// A player as the model sees him on `today`, for the slots of a formation. `None` for a
    /// player who is not part of a senior squad (academy, retired, on loan).
    pub(crate) fn of(player: &Player, slots: &[Position], today: NaiveDate) -> Option<Self> {
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
        Some(Self {
            id: player.id.clone(),
            group: planned.group,
            natural_keeper: player.natural_position == Position::Goalkeeper,
            age: planned.age,
            ovr: player.ovr,
            potential: player.potential,
            fit: fits(player, slots),
            absence: absence_of(player),
            shift,
        })
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
        let mut member = Self::of(&signed, slots, today)?;
        let mut planned = PlannedPlayer::of(&signed, today);
        planned.contract_days = Some(NEW_CONTRACT_DAYS);
        let out_now = member.shift[0].is_none();
        member.shift = projected_shifts(&planned);
        if out_now {
            member.shift[0] = None;
        }
        if perceived_error != 0.0 {
            for fit in &mut member.fit {
                *fit = (*fit + perceived_error).clamp(1.0, 99.0);
            }
        }
        Some(member)
    }

    /// His rating at `slot` in horizon year `year`, if he is there. A natural keeper is no better
    /// than an emergency body away from goal: the game's generator makes keepers fair outfield
    /// players on paper, but no club covers a winger with its second goalkeeper, and a squad must
    /// not look deep because it carries three of them.
    fn level(&self, slot: usize, year: usize) -> Option<f32> {
        let fit = if self.natural_keeper && slot != 0 {
            self.fit[slot].min(UNCOVERED_FIT)
        } else {
            self.fit[slot]
        };
        self.shift[year].map(|shift| (fit + shift).clamp(1.0, 99.0))
    }
}

/// The rating of a player in each slot, today.
pub(crate) fn fits(player: &Player, slots: &[Position]) -> [f32; SLOTS] {
    let mut fit = [UNCOVERED_FIT; SLOTS];
    for (cell, slot) in fit.iter_mut().zip(slots) {
        *cell = positional_fit_for_assignment(player, slot) as f32;
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
    chance.min(MAX_ABSENCE)
}

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
    /// Chance the starter of each slot is absent.
    pub absence: [f32; SLOTS],
}

/// What a squad is worth: a view of each horizon year and their weighted sum.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Evaluation {
    pub years: [YearView; HORIZON],
    pub value: f32,
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
}

impl SquadModel {
    /// The squad of `players` (the club's own, in any order) for `formation` on `today`.
    pub(crate) fn build<'a>(
        players: impl Iterator<Item = &'a Player>,
        formation: &str,
        today: NaiveDate,
        weights: [f32; HORIZON],
    ) -> Self {
        let slots = slots_of(formation);
        let mut members: Vec<Member> = players
            .filter_map(|player| Member::of(player, &slots, today))
            .collect();
        members.sort_by(|a, b| a.id.cmp(&b.id));
        Self {
            formation: formation.to_string(),
            slots,
            members,
            weights,
        }
    }

    /// The model after the squad takes on `member`.
    pub(crate) fn evaluate_with(&self, member: &Member) -> Evaluation {
        self.evaluate(Some(member), None)
    }

    /// The model after `id` leaves the squad.
    pub(crate) fn evaluate_without(&self, id: &str) -> Evaluation {
        self.evaluate(None, Some(id))
    }

    /// The squad as it stands.
    pub(crate) fn evaluate_now(&self) -> Evaluation {
        self.evaluate(None, None)
    }

    fn evaluate(&self, extra: Option<&Member>, without: Option<&str>) -> Evaluation {
        let squad: Vec<&Member> = self
            .members
            .iter()
            .filter(|member| Some(member.id.as_str()) != without)
            .chain(extra)
            .collect();
        let mut years = [EMPTY_YEAR; HORIZON];
        let mut value = 0.0;
        for (year, view) in years.iter_mut().enumerate() {
            *view = year_view(&squad, year);
            value += self.weights[year] * view.expected;
        }
        Evaluation { years, value }
    }
}

const EMPTY_YEAR: YearView = YearView {
    xi: 0.0,
    expected: 0.0,
    starter: [0.0; SLOTS],
    cover: [UNCOVERED_FIT; SLOTS],
    starter_index: [None; SLOTS],
    absence: [0.0; SLOTS],
};

/// The side a club would field from `squad` in horizon year `year`, and who covers each slot.
fn year_view(squad: &[&Member], year: usize) -> YearView {
    let mut view = EMPTY_YEAR;
    let mut used = vec![false; squad.len()];
    for (slot, index) in best_assignment(squad, year).into_iter().enumerate() {
        if let Some(index) = index {
            used[index] = true;
            view.starter_index[slot] = Some(index);
            view.starter[slot] = squad[index].level(slot, year).unwrap_or(0.0);
            view.absence[slot] = squad[index].absence;
        }
    }
    assign_cover(squad, &used, year, &mut view);
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
fn best_assignment(squad: &[&Member], year: usize) -> [Option<usize>; SLOTS] {
    let alive: Vec<usize> = (0..squad.len())
        .filter(|&index| squad[index].shift[year].is_some())
        .collect();
    let columns = alive.len().max(SLOTS);
    // Missing players are dummy columns that cost nothing, so a real player is always preferred.
    let cost = |slot: usize, column: usize| -> i64 {
        alive
            .get(column)
            .map_or(0, |&index| cost_at(squad[index], slot, year))
    };
    let mut slot_cost = vec![0_i64; SLOTS * columns];
    for slot in 0..SLOTS {
        for column in 0..columns {
            slot_cost[slot * columns + column] = cost(slot, column);
        }
    }
    let columns_of_slots = hungarian(&slot_cost, SLOTS, columns);
    let mut picks = [None; SLOTS];
    for (slot, column) in columns_of_slots.into_iter().enumerate() {
        picks[slot] = alive
            .get(column)
            .copied()
            .filter(|&index| cost_at(squad[index], slot, year) < NOT_THERE);
    }
    picks
}

/// The column for each of `rows` rows (`rows <= columns`) that minimises the total of
/// `cost[row * columns + column]`, each column used once. The classic potentials-and-augmenting-path
/// form: `O(rows^2 * columns)`.
fn hungarian(cost: &[i64], rows: usize, columns: usize) -> Vec<usize> {
    const INF: i64 = i64::MAX / 4;
    let mut row_potential = vec![0_i64; rows + 1];
    let mut column_potential = vec![0_i64; columns + 1];
    let mut matched_row = vec![0_usize; columns + 1];
    let mut way = vec![0_usize; columns + 1];
    for row in 1..=rows {
        matched_row[0] = row;
        let mut column = 0;
        let mut minimum = vec![INF; columns + 1];
        let mut seen = vec![false; columns + 1];
        loop {
            seen[column] = true;
            let current = matched_row[column];
            let mut delta = INF;
            let mut next = 0;
            for candidate in 1..=columns {
                if seen[candidate] {
                    continue;
                }
                let reduced = cost[(current - 1) * columns + candidate - 1]
                    - row_potential[current]
                    - column_potential[candidate];
                if reduced < minimum[candidate] {
                    minimum[candidate] = reduced;
                    way[candidate] = column;
                }
                if minimum[candidate] < delta {
                    delta = minimum[candidate];
                    next = candidate;
                }
            }
            for candidate in 0..=columns {
                if seen[candidate] {
                    row_potential[matched_row[candidate]] += delta;
                    column_potential[candidate] -= delta;
                } else {
                    minimum[candidate] -= delta;
                }
            }
            column = next;
            if matched_row[column] == 0 {
                break;
            }
        }
        loop {
            let previous = way[column];
            matched_row[column] = matched_row[previous];
            column = previous;
            if column == 0 {
                break;
            }
        }
    }
    let mut result = vec![0; rows];
    for column in 1..=columns {
        if matched_row[column] != 0 {
            result[matched_row[column] - 1] = column - 1;
        }
    }
    result
}

/// Slots one bench player may cover. Two starters are rarely out together (at a one-in-ten
/// absence it is a one-in-a-hundred match), so a player covering two slots costs almost nothing,
/// while a squad whose bench is a handful of players covering everything is judged for it.
const SLOTS_PER_COVER: u8 = 2;

/// Gives each slot the bench player who would replace its starter, best pairing first, each bench
/// player covering at most [`SLOTS_PER_COVER`] slots. A slot left without a cover keeps
/// [`UNCOVERED_FIT`].
fn assign_cover(squad: &[&Member], used: &[bool], year: usize, view: &mut YearView) {
    let bench: Vec<usize> = (0..squad.len())
        .filter(|&index| !used[index] && squad[index].shift[year].is_some())
        .collect();
    let mut taken = vec![0_u8; bench.len()];
    let mut covered = [false; SLOTS];
    for _ in 0..SLOTS {
        let mut best: Option<(usize, usize, f32)> = None;
        for slot in (0..SLOTS).filter(|&slot| !covered[slot]) {
            for (position, &index) in bench.iter().enumerate() {
                if taken[position] >= SLOTS_PER_COVER {
                    continue;
                }
                let Some(level) = squad[index].level(slot, year) else {
                    continue;
                };
                if best.is_none_or(|(_, _, best_level)| level > best_level) {
                    best = Some((slot, position, level));
                }
            }
        }
        let Some((slot, position, level)) = best else {
            break;
        };
        covered[slot] = true;
        taken[position] += 1;
        view.cover[slot] = level;
    }
}

#[cfg(test)]
mod tests;
