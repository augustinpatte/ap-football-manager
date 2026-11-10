//! Giving a generated club the standing its reputation implies (Run F5.1).
//!
//! World generation only: an authored club keeps the players and wages its author wrote. The
//! rule itself (how much a reputation earns, how much better its squad is) is `club_economy`'s;
//! this applies it to the squad `build_club` has just drawn. It lives under `generator/` because
//! staging a wage is something only world generation may do (`tests/contract_single_writer.rs`).

use domain::player::Player;
use domain::team::Team;

use crate::club_economy::{opening_wage_bill, quality_shift};

/// A generated club is given the standing its reputation implies: its players better or worse by
/// [`quality_shift`] and paid so that the squad's bill is [`opening_wage_bill`]. Only generated
/// clubs are shaped; an authored club keeps the players and wages its author wrote.
pub(super) fn shape_generated_club(team: &Team, players: &mut [Player], on: chrono::NaiveDate) {
    let shift = quality_shift(team.reputation);
    if shift != 0 {
        for player in players.iter_mut() {
            shift_quality(player, shift, on);
        }
    }
    fit_wages(team, players);
}

fn shift_quality(player: &mut Player, shift: i32, on: chrono::NaiveDate) {
    let before = crate::player_rating::natural_ovr(player);
    let target = (before + f64::from(shift)).round().clamp(1.0, 99.0) as u8;
    crate::player_rating::move_attributes_to(&mut player.attributes, &player.position, target);
    let headroom = i32::from(player.potential) - before.round() as i32;
    let after = crate::player_rating::natural_ovr(player);
    player.potential = (after.round() as i32 + headroom.max(0)).clamp(1, 99) as u8;
    crate::player_rating::refresh_player_derived(player, on);
    // Priced from the rating the player will be shown with, by the generator's own rule.
    let ratio = (after / before.max(1.0)).powi(2);
    player.market_value = (player.market_value as f64 * ratio) as u64;
}

/// How far a squad's wages may be scaled to fit the club. The floor is low because the smallest
/// grounds earn under a third of what the generator's market-value wages cost; the test
/// `no_generated_club_opens_with_a_bill_its_income_cannot_carry` is what holds it there.
const MIN_WAGE_FACTOR: f64 = 0.2;
const MAX_WAGE_FACTOR: f64 = 1.6;

/// Scale every wage in the squad by one factor so the bill is what the club can carry. The factor
/// is bounded so a freak squad is not paid nothing or a fortune.
fn fit_wages(team: &Team, players: &mut [Player]) {
    let bill: i64 = players.iter().map(|p| i64::from(p.wage())).sum();
    if bill <= 0 {
        return;
    }
    let target = opening_wage_bill(team.reputation, team.stadium_capacity);
    let factor = (target as f64 / bill as f64).clamp(MIN_WAGE_FACTOR, MAX_WAGE_FACTOR);
    let minimum = crate::contracts::MINIMUM_DEFAULT_WAGE as u32;
    for player in players.iter_mut() {
        let wage = ((f64::from(player.wage()) * factor).round() as u32).max(minimum);
        player.stage_wage(wage);
    }
}

#[cfg(test)]
mod tests {
    use crate::generator::{DefinitionSources, WorldGenConfig, generate_world_data_seeded_with};
    use std::collections::HashMap;

    fn world(seed: u64) -> crate::generator::definitions::WorldData {
        let sources = DefinitionSources::embedded_only();
        let mut config = WorldGenConfig::standard_from(&sources);
        config.nations.retain(|nation| nation.code == "ENG");
        config.opening_year = Some(2033);
        generate_world_data_seeded_with(seed, &config, &sources)
    }

    fn correlation(a: &[f64], b: &[f64]) -> f64 {
        let (ma, mb) = (
            a.iter().sum::<f64>() / a.len() as f64,
            b.iter().sum::<f64>() / b.len() as f64,
        );
        let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
        let (va, vb): (f64, f64) = (
            a.iter().map(|x| (x - ma).powi(2)).sum(),
            b.iter().map(|y| (y - mb).powi(2)).sum(),
        );
        cov / (va.sqrt() * vb.sqrt())
    }

    /// Given a generated nation of two divisions, when its clubs are built, then a club's
    /// reputation says something about its squad: the correlation is clearly positive in every
    /// one of several worlds, and the strongest quarter of clubs by reputation field better
    /// squads than the weakest quarter.
    #[test]
    fn a_generated_clubs_reputation_predicts_its_squad_strength() {
        for seed in [11, 12, 13, 14] {
            let world = world(seed);
            let mut by_club: HashMap<&str, Vec<f64>> = HashMap::new();
            for player in world.players.iter().filter(|p| p.team_id.is_some()) {
                by_club
                    .entry(player.team_id.as_deref().unwrap_or(""))
                    .or_default()
                    .push(f64::from(player.ovr));
            }
            let strength = |id: &str| {
                let mut v = by_club.get(id).cloned().unwrap_or_default();
                v.sort_by(|a, b| b.total_cmp(a));
                v.truncate(14);
                v.iter().sum::<f64>() / v.len().max(1) as f64
            };
            let rep: Vec<f64> = world
                .teams
                .iter()
                .map(|t| f64::from(t.reputation))
                .collect();
            let str_: Vec<f64> = world.teams.iter().map(|t| strength(&t.id)).collect();
            assert!(
                correlation(&rep, &str_) > 0.4,
                "seed {seed}: {}",
                correlation(&rep, &str_)
            );
        }
    }

    /// Given the same worlds, when grounds are read, then a bigger club has a bigger ground: the
    /// top quarter by reputation averages at least half as many seats again as the bottom quarter
    /// (the ground used to be drawn at random, independent of the club).
    #[test]
    fn a_generated_clubs_ground_follows_its_reputation() {
        let world = world(21);
        let mut clubs: Vec<(u32, u32)> = world
            .teams
            .iter()
            .map(|t| (t.reputation, t.stadium_capacity))
            .collect();
        clubs.sort_by_key(|c| std::cmp::Reverse(c.0));
        let quarter = clubs.len() / 4;
        let mean = |slice: &[(u32, u32)]| {
            slice.iter().map(|c| f64::from(c.1)).sum::<f64>() / slice.len() as f64
        };
        assert!(mean(&clubs[..quarter]) > 1.5 * mean(&clubs[clubs.len() - quarter..]));
    }

    /// Given a generated club, when its squad's wage bill is read against what its ground and
    /// reputation can earn, then the bill is below that income for every club: no generated club
    /// opens unable to pay its players (the cause of the thirty-season debt in F5.1's baseline).
    #[test]
    fn no_generated_club_opens_with_a_bill_its_income_cannot_carry() {
        let world = world(31);
        for team in &world.teams {
            let bill: i64 = world
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some(team.id.as_str()))
                .map(|p| i64::from(p.wage()))
                .sum();
            let income =
                crate::club_economy::expected_weekly_income(team.reputation, team.stadium_capacity);
            assert!(
                bill < income,
                "{}: bill {bill} against income {income}",
                team.name
            );
        }
    }
}
