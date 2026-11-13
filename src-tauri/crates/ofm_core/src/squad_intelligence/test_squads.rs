#![cfg(test)]

//! Squads for the scenario tests, written in football terms: "a keeper of 85", "a striker of 61".
//!
//! A player here is a specialist: an outfielder has no hands and a keeper no engine, so an
//! outfielder in goal is as bad as he would be in the game and a missing keeper is a real hole.

use chrono::NaiveDate;
use domain::player::{Player, Position, SquadRole};
use domain::team::Team;

use super::ClubSquad;
use super::appetite::Appetite;
use crate::player_rating::ovr_from_attributes;

pub(crate) const TODAY: (i32, u32, u32) = (2026, 8, 3);

pub(crate) fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(TODAY.0, TODAY.1, TODAY.2).expect("a real date")
}

/// A senior of `age` rated `ovr` in his natural `position`, with no room to grow.
pub(crate) fn player(id: &str, position: Position, ovr: u8, age: i32) -> Player {
    let attributes = crate::generator::attributes_rated(
        ovr,
        &position,
        crate::stable_hash::stable_hash(id.as_bytes(), 0),
    );
    let rating = ovr_from_attributes(&attributes, &position).round() as u8;
    let mut player = Player::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        format!("{}-01-01", TODAY.0 - age),
        "England".to_string(),
        position,
        attributes,
    );
    player.squad_role = SquadRole::Senior;
    player.ovr = rating;
    player.potential = rating;
    player.fitness = 90;
    player.team_id = Some("club".to_string());
    player.market_value = u64::from(rating) * 60_000;
    player
}

/// The same player with a different ceiling.
pub(crate) fn with_potential(mut player: Player, potential: u8) -> Player {
    player.potential = potential;
    player
}

pub(crate) fn team(formation: &str, cash: i64, wage_budget: i64) -> Team {
    let mut team = Team::new(
        "club".to_string(),
        "club".to_string(),
        "CLB".to_string(),
        "England".to_string(),
        "London".to_string(),
        "Ground".to_string(),
        20_000,
    );
    team.formation = formation.to_string();
    team.finance = cash;
    team.transfer_budget = cash;
    team.wage_budget = wage_budget;
    team
}

/// Builds a squad by position, best first within each: `squad().add(Goalkeeper, &[85, 83])`.
#[derive(Default)]
pub(crate) struct Squad {
    pub players: Vec<Player>,
}

impl Squad {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Players of `position` with these ratings, all aged 27.
    pub(crate) fn add(self, position: Position, ratings: &[u8]) -> Self {
        let aged: Vec<(u8, i32)> = ratings.iter().map(|&ovr| (ovr, 27)).collect();
        self.add_aged(position, &aged)
    }

    /// Players of `position` as `(rating, age)`.
    pub(crate) fn add_aged(mut self, position: Position, players: &[(u8, i32)]) -> Self {
        for &(ovr, age) in players {
            let id = format!("{position:?}{}-{}", self.players.len(), ovr);
            self.players.push(player(&id, position.clone(), ovr, age));
        }
        self
    }

    /// A sound squad of twenty for a 4-4-2 with every position doubled up except the ones a test
    /// then changes. Eleven starters between 70 and 74 and cover not far behind.
    pub(crate) fn balanced() -> Self {
        Self::new()
            .add(Position::Goalkeeper, &[74, 66])
            .add(Position::LeftBack, &[72, 66])
            .add(Position::RightBack, &[72, 66])
            .add(Position::CenterBack, &[74, 72, 68, 66])
            .add(Position::LeftMidfielder, &[72, 66])
            .add(Position::RightMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[74, 72, 68, 66])
            .add(Position::Striker, &[75, 73, 67])
    }

    pub(crate) fn club(&self, formation: &str, cash: i64, appetite: Appetite) -> ClubSquad {
        ClubSquad::from_players(
            self.players.iter(),
            &team(formation, cash, 600_000),
            today(),
            appetite,
        )
    }
}
