//! AI clubs competing for the free agents worth having.
//!
//! A club shows interest by making an offer through the same talk a manager's offer goes through,
//! so the player weighs it against the manager's by the same score. A club only bids when the
//! player would improve its side, would plausibly come to it (the offer must be one he would
//! accept), and it could pay him. How many bids the market makes a day, and how many clubs are
//! looked at for each player, are bounded; nothing is scanned that does not have to be.

use super::appeal::{
    ClubSquad, OfferTerms, PlaceInSide, Verdict, ambition_pct, appeal_of, player_standing,
};
use super::consts::*;
use super::workflow::{facts_for, new_talk};
use super::*;
use crate::stable_hash::stable_hash;
use std::collections::HashMap;

/// Whether the club's books allow another wage: not in debt, within the board's soft cap.
pub(super) fn club_can_carry_wage(game: &Game, team: &Team, wage: u32) -> bool {
    let bill = crate::finances::calc_wages(game, &team.id);
    let home_matches = crate::ai_finance_guard::home_league_matches_by_club(game)
        .get(team.id.as_str())
        .copied()
        .unwrap_or(0);
    crate::ai_finance_guard::SigningBooks::given(team, bill, home_matches)
        .can_add_wage(i64::from(wage))
}

/// Whether a player would not consider a club on its standing alone — he is far above it and
/// nothing a club on that scale could add would make up for the drop. For the AI clubs that sign
/// a free agent on the spot to fill their squad: they are held to the same reluctance a manager's
/// offer meets.
pub(crate) fn declines_on_standing(player: &Player, team: &Team, today: NaiveDate) -> bool {
    let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
    let gap = player_standing(player.ovr, player.potential, age)
        - i32::try_from(team.reputation).unwrap_or(1000);
    if gap <= STEP_DOWN_FREE_GAP {
        return false;
    }
    let ambition = ambition_pct(age, player.ovr, player.potential);
    let penalty = (gap - STEP_DOWN_FREE_GAP) * ambition / 100 / STEP_DOWN_POINTS_PER_STANDING;
    // Starting a match for them is the most the club has to offer.
    penalty > ROLE_KEY
}

/// A club's cheap, first-look reasons to want the player: he would play, or its position group is
/// short.
fn improves_the_side(squad: &ClubSquad, player: &Player) -> bool {
    matches!(
        squad.place_for(player),
        PlaceInSide::Key | PlaceInSide::Starter
    )
}

/// Rival clubs make their offers for the day.
pub(super) fn generate_rival_interest(game: &mut Game, today: NaiveDate) {
    let mut free_agents: Vec<usize> = game
        .players
        .iter()
        .enumerate()
        .filter(|(_, player)| {
            player.team_id.is_none()
                && !player.retired
                && player.ovr >= ATTRACTIVE_FREE_AGENT_OVR
                && player.injury.is_none()
        })
        .map(|(index, _)| index)
        .collect();
    if free_agents.is_empty() {
        return;
    }
    free_agents.sort_by(|&left, &right| {
        let (left, right) = (&game.players[left], &game.players[right]);
        right
            .ovr
            .cmp(&left.ovr)
            .then_with(|| left.id.cmp(&right.id))
    });
    free_agents.truncate(ATTRACTIVE_FREE_AGENTS_PER_DAY);

    let mut clubs: Vec<&Team> = game
        .teams
        .iter()
        .filter(|team| game.manager.team_id.as_deref() != Some(team.id.as_str()))
        .collect();
    clubs.sort_by(|left, right| left.id.cmp(&right.id));
    if clubs.is_empty() {
        return;
    }
    let mut squads: HashMap<&str, ClubSquad> = HashMap::new();
    for player in &game.players {
        if let Some(team_id) = player.team_id.as_deref() {
            squads.entry(team_id).or_default().add(player, team_id);
        }
    }
    squads.values_mut().for_each(ClubSquad::finish);

    let home_matches = crate::ai_finance_guard::home_league_matches_by_club(game);
    let mut bills: HashMap<&str, i64> = HashMap::new();
    let mut new_talks: Vec<(usize, domain::free_agent_talk::FreeAgentTalk)> = Vec::new();

    for &index in &free_agents {
        if new_talks.len() >= MAX_NEW_RIVAL_TALKS_PER_DAY {
            break;
        }
        let player = &game.players[index];
        let open_rival_talks = player
            .morale_core
            .free_agent_talks
            .iter()
            .filter(|talk| talk.is_open() && !talk.made_by_manager)
            .count();
        if open_rival_talks >= MAX_OPEN_RIVAL_TALKS_PER_PLAYER {
            continue;
        }
        let rotation = stable_hash(format!("{}|{today}", player.id).as_bytes(), 0x7269_7661)
            as usize
            % clubs.len();
        for step in 0..CLUBS_SAMPLED_PER_FREE_AGENT.min(clubs.len()) {
            let team = clubs[(rotation + step) % clubs.len()];
            if player
                .morale_core
                .free_agent_talks
                .iter()
                .any(|talk| talk.club_id == team.id && talk.is_open())
            {
                continue;
            }
            let empty = ClubSquad::default();
            let squad = squads.get(team.id.as_str()).unwrap_or(&empty);
            if squad.seniors() >= MAX_SENIOR_SQUAD || !improves_the_side(squad, player) {
                continue;
            }
            let wage = expected_wage(player, team, today);
            let terms = OfferTerms {
                weekly_wage: wage,
                contract_years: expected_contract_years(player, today),
                signing_bonus: 0,
                promise_stepping_stone: false,
            };
            let facts = facts_for(player, team, squad, &terms, today);
            if appeal_of(&facts, &terms).verdict() != Verdict::Accept {
                continue;
            }
            let bill = *bills
                .entry(team.id.as_str())
                .or_insert_with(|| crate::finances::calc_wages(game, &team.id));
            let seniors = crate::squad_floor::senior_counts(game, &team.id);
            let wage_facts = crate::contract_wage_policy::BuyerWageFacts::new(bill, seniors);
            let books = crate::ai_finance_guard::SigningBooks::given(
                team,
                bill,
                home_matches.get(team.id.as_str()).copied().unwrap_or(0),
            );
            if !wage_facts.purchase_verdict(team, player, wage).permits()
                || !books.can_add_wage(i64::from(wage))
            {
                continue;
            }
            new_talks.push((index, new_talk(&player.id, &team.id, &terms, today, false)));
            break;
        }
    }

    for (index, talk) in new_talks {
        game.players[index].morale_core.free_agent_talks.push(talk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;

    fn player(ovr: u8, date_of_birth: &str) -> Player {
        let mut player = Player::new(
            "p".to_string(),
            "P".to_string(),
            "P".to_string(),
            date_of_birth.to_string(),
            "England".to_string(),
            domain::player::Position::Midfielder,
            uniform_attributes(50),
        );
        player.ovr = ovr;
        player.potential = ovr;
        player
    }

    fn club(reputation: u32) -> Team {
        let mut team = Team::new(
            "t".to_string(),
            "T".to_string(),
            "TTT".to_string(),
            "England".to_string(),
            "Town".to_string(),
            "Ground".to_string(),
            10_000,
        );
        team.reputation = reputation;
        team
    }

    /// Given an elite free agent and clubs of different standing
    /// When an AI club fills its squad on the spot
    /// Then a club far below him is passed over, and one near his level is not.
    #[test]
    fn a_squad_filling_club_far_below_an_elite_player_is_declined() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        let elite = player(82, "1998-01-01");

        assert!(declines_on_standing(&elite, &club(250), today));
        assert!(!declines_on_standing(&elite, &club(700), today));
        assert!(!declines_on_standing(
            &player(50, "1998-01-01"),
            &club(250),
            today
        ));
    }
}
