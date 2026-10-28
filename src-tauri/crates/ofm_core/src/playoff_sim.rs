//! The game's [`TieResolver`]: plays a play-off or barrage tie from the two clubs' squads, with the
//! scoreline model the rest of the world's out-of-sight matches use, so a promotion decided by a
//! tie is as likely to go to the stronger side as any other match is.

use crate::ladder_rules::{TieRequest, TieResolver};
use crate::seed::rng_for_seed;
use domain::competition_rules::PlayoffTie;
use domain::player::Player;

pub struct SquadTies<'a> {
    pub players: &'a [Player],
    pub seed: u64,
    /// The date the ties are keyed on (the rollover), so a reloaded save plays them identically.
    pub date: String,
}

impl TieResolver for SquadTies<'_> {
    fn resolve(&mut self, request: &TieRequest<'_>) -> PlayoffTie {
        let tag = format!(
            "playoff/{}/{}/{}",
            request.stage, request.home, request.away
        );
        let mut rng = rng_for_seed(self.seed, &tag, &self.date);
        let first = crate::catchup::club_strength(self.players, request.home);
        let second = crate::catchup::club_strength(self.players, request.away);

        let mut legs: Vec<(u8, u8)> = Vec::new();
        // First leg at the first club's ground.
        legs.push(crate::national_team::simulate_scoreline(
            first, second, &mut rng,
        ));
        if request.legs >= 2 {
            // Second leg at the second club's ground: the scoreline is drawn for the host first.
            let (host, guest) = crate::national_team::simulate_scoreline(second, first, &mut rng);
            legs.push((guest, host));
        }
        let mut first_total: u32 = legs.iter().map(|leg| u32::from(leg.0)).sum();
        let mut second_total: u32 = legs.iter().map(|leg| u32::from(leg.1)).sum();
        let mut penalties = None;
        if first_total == second_total {
            // Extra time at the ground of the last leg, then penalties.
            let (extra_first, extra_second) = if request.legs >= 2 {
                let (host, guest) =
                    crate::national_team::simulate_extra_time(second, first, &mut rng);
                (guest, host)
            } else {
                crate::national_team::simulate_extra_time(first, second, &mut rng)
            };
            first_total += u32::from(extra_first);
            second_total += u32::from(extra_second);
            if let Some(last) = legs.last_mut() {
                last.0 = last.0.saturating_add(extra_first);
                last.1 = last.1.saturating_add(extra_second);
            }
            if first_total == second_total {
                penalties = Some(crate::national_team::simulate_shootout(
                    first, second, &mut rng,
                ));
            }
        }
        let first_wins = match penalties {
            Some((first_pens, second_pens)) => first_pens > second_pens,
            None => first_total > second_total,
        };
        PlayoffTie {
            stage: request.stage.to_string(),
            home_team_id: request.home.to_string(),
            away_team_id: request.away.to_string(),
            legs,
            penalties,
            winner_id: if first_wins {
                request.home
            } else {
                request.away
            }
            .to_string(),
        }
    }
}
