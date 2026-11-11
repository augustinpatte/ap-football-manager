//! Run F3: players develop with what their seasons gave them, in every league of the world.
//!
//! Two properties are checked on the gate world, which has a followed league and dormant ones:
//!
//! * Outside the followed league matches are resolved by scoreline, yet the players of those
//!   leagues leave a playing record (appearances and minutes), so the rules that read minutes see
//!   the same kind of data everywhere.
//! * A young senior with room to grow develops more when he plays than when he does not, in a
//!   whole simulated season and not only in the unit that bends the curve.

use std::collections::HashMap;

use domain::player::SquadRole;
use ofm_core::end_of_season::EndOfSeasonSummary;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

#[derive(Clone, Copy)]
struct Before {
    ovr: u8,
    gap: u8,
    age: i32,
    minutes: u32,
    appearances: u32,
    dormant: bool,
}

#[derive(Default)]
struct Season {
    before: HashMap<String, Before>,
    /// Every senior of every season played: his season before the rollover and what the rollover
    /// did to his rating.
    outcomes: Vec<(Before, i32)>,
}

impl SeasonObserver for Season {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        let today = game.clock.current_date.date_naive();
        let active = game.active_team_ids().expect("a followed league");
        self.before.clear();
        for p in game
            .players
            .iter()
            .filter(|p| p.squad_role == SquadRole::Senior)
        {
            let Some(team) = p.team_id.as_deref() else {
                continue;
            };
            self.before.insert(
                p.id.clone(),
                Before {
                    ovr: p.ovr,
                    gap: p.potential.saturating_sub(p.ovr),
                    age: domain::age::age_on_or_unknown(today, &p.date_of_birth),
                    minutes: p.stats.minutes_played,
                    appearances: p.stats.appearances,
                    dormant: !active.contains(team),
                },
            );
        }
    }

    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, _season: u32) {
        for p in &game.players {
            if let Some(before) = self.before.get(&p.id) {
                self.outcomes
                    .push((*before, i32::from(p.ovr) - i32::from(before.ovr)));
            }
        }
    }
}

fn play(seasons: u32) -> Season {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    let mut season = Season::default();
    run_seasons(&mut game, RunOptions::seasons(seasons), &mut season).expect("the seasons play");
    season
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0.0), |(s, n), v| (s + v, n + 1.0));
    sum / f64::max(n, 1.0)
}

/// Given the gate world, with dormant leagues, played for a season
/// When the season is about to turn over
/// Then most senior players of the dormant leagues have played, with a record of the same kind as
///      the followed league's, and the benches are not credited.
#[test]
fn dormant_leagues_leave_a_playing_record() {
    let season = play(1);
    let dormant: Vec<&Before> = season
        .outcomes
        .iter()
        .map(|(b, _)| b)
        .filter(|b| b.dormant)
        .collect();
    assert!(dormant.len() > 200, "the world has dormant leagues");

    let played = dormant.iter().filter(|b| b.minutes > 0).count() as f64 / dormant.len() as f64;
    let appearances = mean(dormant.iter().map(|b| f64::from(b.appearances)));
    assert!(played > 0.5, "{played:.2} of dormant seniors have played");
    assert!(
        appearances > 10.0,
        "{appearances:.1} appearances on average"
    );
    assert!(
        played < 0.9,
        "{played:.2}: the benches are not credited, so not everyone has played"
    );
}

/// Given young seniors with room to grow, over four seasons
/// When the seasons are played and they develop at each one's end
/// Then, among players of the same age band and with the same room to grow, those who played most
///      gain clearly more than those who barely played. A raw comparison is misleading (a player
///      who plays is usually better and so nearer his ceiling, which slows his growth), so the
///      comparison is made inside bands of age and room to grow and averaged by how many played.
#[test]
fn young_players_who_play_develop_more_than_those_who_do_not() {
    let season = play(4);
    const MIN_PER_GROUP: usize = 20;
    let mut weighted_difference = 0.0;
    let mut weight = 0.0;
    for (age_lo, age_hi) in [(16, 18), (19, 21), (22, 23)] {
        for (gap_lo, gap_hi) in [(6, 12), (13, 20), (21, 60)] {
            let gains = |keep: &dyn Fn(&Before) -> bool| -> Vec<f64> {
                season
                    .outcomes
                    .iter()
                    .filter(|(b, _)| {
                        (age_lo..=age_hi).contains(&b.age)
                            && (gap_lo..=gap_hi).contains(&b.gap)
                            && keep(b)
                    })
                    .map(|(_, gain)| f64::from(*gain))
                    .collect()
            };
            let played = gains(&|b| b.minutes >= 1_500);
            let sat = gains(&|b| b.minutes < 300);
            if played.len() < MIN_PER_GROUP || sat.len() < MIN_PER_GROUP {
                continue;
            }
            let share = played.len() as f64;
            weighted_difference += share * (mean(played.into_iter()) - mean(sat.into_iter()));
            weight += share;
        }
    }
    assert!(weight >= 100.0, "enough comparable players: {weight}");
    let difference = weighted_difference / weight;
    println!("same age and room to grow: players gain {difference:+.2} points more");
    assert!(difference > 0.3, "{difference:+.2}");
}
