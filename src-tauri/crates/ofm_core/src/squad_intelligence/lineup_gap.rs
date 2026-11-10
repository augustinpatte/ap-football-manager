//! Measurement only: how far the squad model's best side is from the side the game's lineup picker
//! fields, club by club, from the same healthy senior pool.
//!
//! The model solves each year as an optimal assignment; `turn::squad::first_choice_eleven` walks the
//! formation's slots in order and takes the best remaining fit for each. They are different
//! algorithms (audit F1), and nothing in production compared them. This reads both from one pool so
//! the harness can say how often they disagree and by how much. Strength is scored in the game's
//! own raw slot rating (`positional_fit_for_assignment`), which neither side's tie-breaking or
//! rounding can bias, and the model's anchored rating is reported beside it.
//!
//! A player on loan *to* a club carries that club's `team_id`, so the picker fields him; before P4's
//! second slice `Member::of` did not see him, and the loaned-in count here showed the blind spot.
//! It decides nothing and changes nothing.

use chrono::NaiveDate;
use domain::player::{Player, SquadRole};

use super::model::{Availability, SLOTS, SquadModel};
use crate::game::Game;
use crate::player_rating::positional_fit_for_assignment;

/// One club's model side against the picker's, on the same healthy seniors today.
#[derive(Debug, Clone, PartialEq)]
pub struct LineupGap {
    pub team_id: String,
    /// Healthy seniors both sides were chosen from (players on loan to the club included).
    pub pool: usize,
    /// Healthy players on loan to the club in the pool.
    pub loaned_in: usize,
    /// Of the real picker's eleven, how many are on loan to the club.
    pub loaned_in_starters: u8,
    /// Mean raw slot rating of the model's optimal side.
    pub optimal_raw: f32,
    /// Mean raw slot rating of the side the real picker fields from the same pool.
    pub picker_raw: f32,
    /// Mean model-unit rating (anchored, rounded) of the optimal side and of the picker's side.
    pub optimal_modelled: f32,
    pub picker_modelled: f32,
    /// Starters in the optimal side who are not in the picker's side.
    pub changed_starters: u8,
    /// Players in both sides who are given a different slot.
    pub moved_starters: u8,
    /// Mean raw slot rating of the side the slot-order walk (the picker before P4) would field.
    pub legacy_picker_raw: f32,
}

/// The gap for every club in `game` that has a full side to pick.
pub fn lineup_gaps(game: &Game) -> Vec<LineupGap> {
    let today = game.clock.current_date.date_naive();
    let by_club = crate::match_day_eligibility::players_by_club(&game.players);
    game.teams
        .iter()
        .filter_map(|team| {
            let club = by_club.get(team.id.as_str())?;
            let healthy: Vec<&Player> = club
                .iter()
                .copied()
                .filter(|p| p.squad_role == SquadRole::Senior && p.can_play())
                .collect();
            gap_of(&team.id, &team.formation, &healthy, today)
        })
        .collect()
}

fn gap_of(
    team_id: &str,
    formation: &str,
    healthy: &[&Player],
    today: NaiveDate,
) -> Option<LineupGap> {
    // The model reads players on loan to the club (a borrower's squad includes them while the loan
    // lasts), as the picker does, so both choose from everyone healthy.
    let pool: Vec<&Player> = healthy.to_vec();
    if pool.len() < SLOTS {
        return None;
    }
    let loaned_in = pool.iter().filter(|p| p.active_loan.is_some()).count();
    let loaned_in_starters = crate::turn::squad::first_choice_eleven(&pool, formation)
        .iter()
        .filter(|p| p.active_loan.is_some())
        .count() as u8;

    let model = SquadModel::build(
        pool.iter().copied(),
        formation,
        today,
        [1.0, 0.0, 0.0],
        Availability::Planning,
    );
    let optimal = model.evaluate_now().years[0].starter_index;
    let squad: Vec<&_> = model.members.iter().collect();
    let real_players = crate::turn::squad::first_choice_eleven(&pool, formation);
    // The real picker's side, as indices into the model's id-sorted members.
    let mut picker = [None; SLOTS];
    for (slot, player) in real_players.iter().take(SLOTS).enumerate() {
        picker[slot] = squad.iter().position(|member| member.id == player.id);
    }
    if optimal.iter().any(Option::is_none) || picker.iter().any(Option::is_none) {
        return None;
    }

    let slots = &model.slots;
    let legacy = crate::turn::squad::slot_walk_eleven(&pool, &model.slots);
    let mut legacy_picks = [None; SLOTS];
    for (slot, player) in legacy.iter().take(SLOTS).enumerate() {
        legacy_picks[slot] = squad.iter().position(|member| member.id == player.id);
    }
    let player_of = |index: usize| pool.iter().find(|p| p.id == squad[index].id).copied();
    let raw = |picks: &[Option<usize>; SLOTS]| -> f32 {
        picks
            .iter()
            .enumerate()
            .filter_map(|(slot, pick)| {
                Some(positional_fit_for_assignment(player_of((*pick)?)?, &slots[slot]) as f32)
            })
            .sum::<f32>()
            / SLOTS as f32
    };
    let modelled = |picks: &[Option<usize>; SLOTS]| -> f32 {
        picks
            .iter()
            .enumerate()
            .filter_map(|(slot, pick)| squad[(*pick)?].level(slot, 0))
            .sum::<f32>()
            / SLOTS as f32
    };
    let slot_of = |picks: &[Option<usize>; SLOTS], index: usize| {
        picks.iter().position(|pick| *pick == Some(index))
    };
    let mut changed_starters = 0u8;
    let mut moved_starters = 0u8;
    for index in optimal.iter().flatten() {
        match slot_of(&picker, *index) {
            None => changed_starters += 1,
            Some(picker_slot) if Some(picker_slot) != slot_of(&optimal, *index) => {
                moved_starters += 1
            }
            Some(_) => {}
        }
    }

    Some(LineupGap {
        team_id: team_id.to_string(),
        pool: pool.len(),
        loaned_in,
        loaned_in_starters,
        optimal_raw: raw(&optimal),
        picker_raw: raw(&picker),
        optimal_modelled: modelled(&optimal),
        picker_modelled: modelled(&picker),
        changed_starters,
        moved_starters,
        legacy_picker_raw: raw(&legacy_picks),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_rating::formation_slots;
    use crate::squad_intelligence::test_squads::{player as specialist, today};
    use domain::player::Position;

    const FORMATION: &str = "4-2-3-1";

    /// A 4-2-3-1 of specialists of 70, with a striker of 88 who also fits earlier slots, an
    /// attacking midfielder of 66 and a striker of 62 in place of the natural ones.
    fn squad_with_a_striker_the_slot_walk_misplaces() -> Vec<Player> {
        let mut squad: Vec<Player> = formation_slots(FORMATION)
            .into_iter()
            .enumerate()
            .filter(|(_, position)| {
                *position != Position::Striker && *position != Position::AttackingMidfielder
            })
            .map(|(slot, position)| specialist(&format!("starter{slot:02}"), position, 70, 27))
            .collect();
        squad.push(specialist("star", Position::Striker, 88, 27));
        squad.push(specialist(
            "playmaker",
            Position::AttackingMidfielder,
            66,
            27,
        ));
        squad.push(specialist("reserve", Position::Striker, 62, 27));
        squad
    }

    /// Given a squad the old slot walk misplaces
    /// When the gap between the model's side and the picker's is read
    /// Then the picker's side is the model's (nobody left out, no raw rating left behind) and is
    /// clearly better than the side the slot walk would have fielded.
    #[test]
    fn the_fielded_side_is_the_models_and_beats_the_old_slot_walk() {
        let squad = squad_with_a_striker_the_slot_walk_misplaces();
        let pool: Vec<&Player> = squad.iter().collect();
        let gap = gap_of("club", FORMATION, &pool, today()).expect("a full side");
        assert_eq!(gap.changed_starters, 0, "{gap:?}");
        assert!(gap.picker_raw >= gap.optimal_raw - 0.05, "{gap:?}");
        assert!(gap.picker_raw > gap.legacy_picker_raw + 1.0, "{gap:?}");
        assert_eq!(
            (gap.pool, gap.loaned_in, gap.loaned_in_starters),
            (12, 0, 0)
        );
    }

    /// Given the same squad with the 88-rated striker on loan to the club
    /// When the gap is read
    /// Then he is counted as loaned in and as a starter, and the model still agrees with the picker
    /// about the side (he is in the pool both read).
    #[test]
    fn a_player_on_loan_to_the_club_is_in_both_sides_and_counted() {
        let mut squad = squad_with_a_striker_the_slot_walk_misplaces();
        let star = squad.iter_mut().find(|p| p.id == "star").expect("the star");
        star.active_loan = Some(domain::player::ActiveLoan {
            parent_team_id: "parent".to_string(),
            loan_team_id: "club".to_string(),
            loan_team_name: None,
            start_date: "2026-07-01".to_string(),
            end_date: "2027-06-30".to_string(),
            wage_contribution_pct: 50,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
        });
        let pool: Vec<&Player> = squad.iter().collect();
        let gap = gap_of("club", FORMATION, &pool, today()).expect("a full side");
        assert_eq!((gap.loaned_in, gap.loaned_in_starters), (1, 1), "{gap:?}");
        assert_eq!(gap.changed_starters, 0, "{gap:?}");
    }

    /// Given fewer healthy seniors than the formation has slots
    /// When the gap is asked for
    /// Then there is none to report: there is no full side to compare.
    #[test]
    fn a_club_without_a_full_side_has_no_gap_to_read() {
        let squad: Vec<Player> = squad_with_a_striker_the_slot_walk_misplaces()
            .into_iter()
            .take(10)
            .collect();
        let pool: Vec<&Player> = squad.iter().collect();
        assert!(gap_of("club", FORMATION, &pool, today()).is_none());
    }
}
