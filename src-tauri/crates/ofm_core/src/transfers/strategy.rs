//! The operating strategy of an AI club.
//!
//! The market asks one question of every buyer ("which target do you want most?") and used to
//! answer it the same way for all of them: appeal and rating gain, weighted by geography. A club
//! that is built around its academy and a club that wants a title this year wanted the same
//! player. A club's *profile* is the disposition behind the answer. It is derived, never stored,
//! from what the club is: where its reputation puts it in its own league, how last season went
//! against that, what it can spend against what its squad is worth, and how young and how well
//! housed its squad is. Being a pure function of the state, it is the same after a reload and
//! under fast-forward, and a club's strategy changes when the club does.
//!
//! The profile only changes which option a buyer prefers and how much a short-of-money club will put
//! into one player. Who may be signed (reputation reach, the squad floor, the player's own consent, the wage and
//! budget checks, the price) is decided where it was, so a profile cannot make an illegal deal.

use std::collections::HashMap;

use domain::player::SquadRole;

use crate::game::Game;

/// How high a club aims, from its place in its own league.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ambition {
    /// Among the best of its league: buys to win now.
    Contender,
    /// The middle: buys where the squad is short.
    Established,
    /// Among the smallest: buys what it can afford and develops what it has.
    Fighter,
}

/// What a club can spend, against what its squad is worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Means {
    Wealthy,
    Comfortable,
    Constrained,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClubProfile {
    pub ambition: Ambition,
    pub means: Means,
    /// 0 to 1: how much the club builds around young players.
    pub youth_focus: f64,
}

/// What the profile is read from, per club.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClubFacts {
    /// Where its reputation puts it in its own league: 0 the best, 1 the smallest.
    pub standing_pct: f64,
    /// Where it finished last season, the same scale; `None` before it has a season behind it.
    pub finished_pct: Option<f64>,
    /// Cash it could spend on a signing: the smaller of its transfer budget and its balance.
    pub cash: i64,
    /// Market value of its seniors together.
    pub squad_value: u64,
    /// Share of its seniors aged 21 or under.
    pub young_share: f64,
    /// Training facility level.
    pub training_facility: u8,
    /// Its wage bill is past what it can carry.
    pub overstretched: bool,
}

/// The share of its wage budget a club's wage bill may reach before it counts as overstretched (the
/// line the market uses for a club that has not just been relegated).
const OVERSTRETCHED_WAGE_PCT: i64 = 120;

/// Top and bottom quarters of a league are its contenders and its fighters.
const CONTENDER_PCT: f64 = 0.25;
const FIGHTER_PCT: f64 = 0.65;
/// A finish this far up (or down) the table, against reputation, moves a club's ambition a step.
const OVERPERFORMED_PCT: f64 = 0.20;
const UNDERPERFORMED_PCT: f64 = 0.80;
/// Cash as a share of squad value: from this a club is wealthy, under this it is constrained.
const WEALTHY_CASH_SHARE: f64 = 0.20;
const CONSTRAINED_CASH_SHARE: f64 = 0.04;
/// A young share this size (a third of the squad) is as youthful as a squad gets.
const FULL_YOUTH_SHARE: f64 = 0.35;
/// Training facility level at which the academy side of youth focus is full.
const TOP_FACILITY: f64 = 5.0;

pub(crate) fn classify(facts: &ClubFacts) -> ClubProfile {
    let mut ambition = if facts.standing_pct <= CONTENDER_PCT {
        Ambition::Contender
    } else if facts.standing_pct <= FIGHTER_PCT {
        Ambition::Established
    } else {
        Ambition::Fighter
    };
    if let Some(finished) = facts.finished_pct {
        ambition = match (ambition, finished) {
            (Ambition::Fighter, f) if f <= OVERPERFORMED_PCT => Ambition::Established,
            (Ambition::Established, f) if f <= OVERPERFORMED_PCT => Ambition::Contender,
            (Ambition::Contender, f) if f >= UNDERPERFORMED_PCT => Ambition::Established,
            (kept, _) => kept,
        };
    }

    // A squad with no value on record says nothing about what the cash is worth: the club is not
    // broke for that.
    let share =
        (facts.squad_value > 0).then(|| facts.cash.max(0) as f64 / facts.squad_value as f64);
    let means = if facts.overstretched || share.is_some_and(|s| s < CONSTRAINED_CASH_SHARE) {
        Means::Constrained
    } else if share.is_some_and(|s| s >= WEALTHY_CASH_SHARE) {
        Means::Wealthy
    } else {
        Means::Comfortable
    };

    let squad = (facts.young_share / FULL_YOUTH_SHARE).clamp(0.0, 1.0);
    let academy = (f64::from(facts.training_facility) / TOP_FACILITY).clamp(0.0, 1.0);
    let patience = match ambition {
        Ambition::Contender => 0.6,
        Ambition::Established => 1.0,
        Ambition::Fighter => 1.2,
    };
    let youth_focus = ((0.6 * squad + 0.4 * academy) * patience).clamp(0.0, 1.0);

    ClubProfile {
        ambition,
        means,
        youth_focus,
    }
}

/// What a profile weighs in a target.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TargetFacts {
    pub age: i32,
    /// Potential minus rating.
    pub potential_gap: i32,
    /// Rating gain over the buyer's first cover.
    pub improvement: i32,
    pub fee: u64,
    pub value: u64,
}

/// Bounds of the weight, so that no profile turns a ranking into a rule.
const WEIGHT_MIN: f64 = 0.5;
const WEIGHT_MAX: f64 = 1.6;

impl ClubProfile {
    /// The profile of a club nothing is known about: the market's own ranking, unchanged.
    pub(crate) const NEUTRAL: ClubProfile = ClubProfile {
        ambition: Ambition::Established,
        means: Means::Comfortable,
        youth_focus: 0.0,
    };

    /// How much this club likes a target, as a multiplier on the market's own ranking: 1 is
    /// neutral. Bounded, so it tilts the choice between options; it never excludes one.
    pub(crate) fn rank_weight(&self, target: &TargetFacts) -> f64 {
        let mut weight = 1.0;
        if target.age <= 22 && target.potential_gap >= 8 {
            weight *= 1.0 + 0.5 * self.youth_focus;
        }
        if target.age >= 29 {
            weight *= 1.0 - 0.35 * self.youth_focus;
        }
        if self.ambition == Ambition::Contender {
            if target.improvement >= 3 && (23..=31).contains(&target.age) {
                weight *= 1.15;
            } else if target.improvement < 2 {
                weight *= 0.85;
            }
        }
        let price_ratio = target.fee as f64 / target.value.max(1) as f64;
        match self.means {
            Means::Constrained => {
                if target.fee == 0 {
                    weight *= 1.25;
                } else if price_ratio > 0.8 {
                    weight *= 0.7;
                }
            }
            Means::Comfortable | Means::Wealthy => {}
        }
        if self.ambition == Ambition::Fighter && target.fee == 0 {
            weight *= 1.2;
        }
        weight.clamp(WEIGHT_MIN, WEIGHT_MAX)
    }

    /// Whether the club's means allow a fee of this size for one player. A constrained club
    /// spends at most a quarter of its cash on one signing, unless it has a hole to fill.
    pub(crate) fn allows_spending(&self, fee: u64, cash: i64, group_is_thin: bool) -> bool {
        match self.means {
            Means::Constrained if !group_is_thin => {
                i128::from(fee) * 100 <= i128::from(cash.max(0)) * 25
            }
            Means::Constrained | Means::Comfortable | Means::Wealthy => true,
        }
    }
}

/// Every club's profile, read once for a sweep. `wage_bills` is the sweep's own reading of what
/// each club pays; one pass over the players gives the squads' ages and values.
pub(crate) fn profiles(
    game: &Game,
    wage_bills: &HashMap<String, i64>,
) -> HashMap<String, ClubProfile> {
    #[derive(Default)]
    struct Squad {
        seniors: u32,
        young: u32,
        value: u64,
    }
    let today = game.clock.current_date.date_naive();
    let mut squads: HashMap<&str, Squad> = HashMap::new();
    for player in &game.players {
        let Some(team_id) = player.team_id.as_deref() else {
            continue;
        };
        if player.squad_role != SquadRole::Senior {
            continue;
        }
        let squad = squads.entry(team_id).or_default();
        squad.seniors += 1;
        squad.value += player.market_value;
        if domain::age::age_on_or_unknown(today, &player.date_of_birth) <= 21 {
            squad.young += 1;
        }
    }

    // Each club's league: the competition with the most clubs that it plays in.
    let mut league_of: HashMap<&str, &domain::league::League> = HashMap::new();
    for competition in game.competitions_in_play() {
        for entry in &competition.standings {
            let current = league_of
                .entry(entry.team_id.as_str())
                .or_insert(competition);
            if competition.standings.len() > current.standings.len() {
                *current = competition;
            }
        }
    }
    let reputation: HashMap<&str, u32> = game
        .teams
        .iter()
        .map(|team| (team.id.as_str(), team.reputation))
        .collect();

    game.teams
        .iter()
        .map(|team| {
            let (standing_pct, finished_pct) = match league_of.get(team.id.as_str()) {
                Some(league) => {
                    let mut peers: Vec<(&str, u32)> = league
                        .standings
                        .iter()
                        .map(|e| {
                            (
                                e.team_id.as_str(),
                                reputation.get(e.team_id.as_str()).copied().unwrap_or(0),
                            )
                        })
                        .collect();
                    // Ties broken on id, so the order never depends on storage order.
                    peers.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
                    let rank = peers.iter().position(|(id, _)| *id == team.id).unwrap_or(0);
                    let pct = rank as f64 / peers.len().saturating_sub(1).max(1) as f64;
                    let finished = team.history.last().map(|record| {
                        f64::from(record.league_position.saturating_sub(1))
                            / peers.len().saturating_sub(1).max(1) as f64
                    });
                    (pct, finished.map(|f| f.clamp(0.0, 1.0)))
                }
                None => (0.5, None),
            };
            let squad = squads.get(team.id.as_str());
            let facts = ClubFacts {
                standing_pct,
                finished_pct,
                cash: team.transfer_budget.min(team.finance),
                squad_value: squad.map_or(0, |s| s.value),
                young_share: squad
                    .map_or(0.0, |s| f64::from(s.young) / f64::from(s.seniors.max(1))),
                training_facility: team.facilities.training,
                overstretched: super::is_overstretched_at(
                    wage_bills.get(&team.id).copied().unwrap_or(0),
                    team.wage_budget,
                    team.finance,
                    OVERSTRETCHED_WAGE_PCT,
                ),
            };
            (team.id.clone(), classify(&facts))
        })
        .collect()
}

/// One club's profile, read as the sweep reads it: for a caller that works for a single club (the
/// manager's assistant) and has no sweep to share the reading with.
pub(crate) fn profile_of(game: &Game, team_id: &str) -> ClubProfile {
    let wage_bills = crate::finances::wage_bills_by_club(game);
    profiles(game, &wage_bills)
        .remove(team_id)
        .unwrap_or(ClubProfile::NEUTRAL)
}

/// A club's strategy in words, for the harness and for any screen that wants to say what a club is
/// about. Read-only: nothing is stored.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ClubStrategyReport {
    pub team_id: String,
    pub ambition: &'static str,
    pub means: &'static str,
    pub youth_focus: f64,
}

/// Every club's strategy as the market would read it today.
pub fn strategy_reports(game: &Game) -> Vec<ClubStrategyReport> {
    let wage_bills = crate::finances::wage_bills_by_club(game);
    let profiles = profiles(game, &wage_bills);
    game.teams
        .iter()
        .filter_map(|team| {
            let profile = profiles.get(&team.id)?;
            Some(ClubStrategyReport {
                team_id: team.id.clone(),
                ambition: match profile.ambition {
                    Ambition::Contender => "contender",
                    Ambition::Established => "established",
                    Ambition::Fighter => "fighter",
                },
                means: match profile.means {
                    Means::Wealthy => "wealthy",
                    Means::Comfortable => "comfortable",
                    Means::Constrained => "constrained",
                },
                youth_focus: profile.youth_focus,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> ClubFacts {
        ClubFacts {
            standing_pct: 0.5,
            finished_pct: None,
            cash: 10_000_000,
            squad_value: 100_000_000,
            young_share: 0.15,
            training_facility: 2,
            overstretched: false,
        }
    }

    fn prospect(fee: u64) -> TargetFacts {
        TargetFacts {
            age: 19,
            potential_gap: 15,
            improvement: 1,
            fee,
            value: 2_000_000,
        }
    }

    fn veteran(fee: u64) -> TargetFacts {
        TargetFacts {
            age: 30,
            potential_gap: 0,
            improvement: 5,
            fee,
            value: 2_000_000,
        }
    }

    /// Given the same state twice
    /// When a profile is made
    /// Then it is the same.
    #[test]
    fn a_profile_is_a_function_of_the_state() {
        assert_eq!(classify(&facts()), classify(&facts()));
    }

    /// Given clubs at the top, the middle and the bottom of a league
    /// When they are classified
    /// Then they are contenders, established and fighters.
    #[test]
    fn ambition_follows_the_place_in_the_league() {
        let at = |standing_pct| {
            classify(&ClubFacts {
                standing_pct,
                ..facts()
            })
            .ambition
        };
        assert_eq!(at(0.0), Ambition::Contender);
        assert_eq!(at(0.5), Ambition::Established);
        assert_eq!(at(0.95), Ambition::Fighter);
    }

    /// Given a small club that finished at the top, and a big one that finished at the bottom
    /// When they are classified
    /// Then the first aims a step higher and the second a step lower.
    #[test]
    fn last_season_moves_ambition_one_step() {
        let small = classify(&ClubFacts {
            standing_pct: 0.9,
            finished_pct: Some(0.05),
            ..facts()
        });
        let big = classify(&ClubFacts {
            standing_pct: 0.0,
            finished_pct: Some(0.95),
            ..facts()
        });
        assert_eq!(small.ambition, Ambition::Established);
        assert_eq!(big.ambition, Ambition::Established);
    }

    /// Given a club whose squad has no value on record
    /// When it is classified
    /// Then it is not called broke: there is nothing to compare its cash with.
    #[test]
    fn a_squad_with_no_value_on_record_does_not_make_a_club_broke() {
        let club = classify(&ClubFacts {
            squad_value: 0,
            cash: 0,
            ..facts()
        });
        assert_eq!(club.means, Means::Comfortable);
    }

    /// Given rich, ordinary and broke clubs, and one over its wage budget
    /// When they are classified
    /// Then their means are wealthy, comfortable, constrained and constrained.
    #[test]
    fn means_follow_cash_against_squad_value_and_the_wage_bill() {
        let with = |cash, overstretched| {
            classify(&ClubFacts {
                cash,
                overstretched,
                ..facts()
            })
            .means
        };
        assert_eq!(with(40_000_000, false), Means::Wealthy);
        assert_eq!(with(10_000_000, false), Means::Comfortable);
        assert_eq!(with(1_000_000, false), Means::Constrained);
        assert_eq!(with(40_000_000, true), Means::Constrained);
    }

    /// Given a youth-built club and a veteran-built one at the same level
    /// When each weighs a teenage prospect and a thirty-year-old
    /// Then the first prefers the prospect and the second the veteran.
    #[test]
    fn a_youth_club_prefers_prospects_and_a_win_now_club_veterans() {
        let youthful = classify(&ClubFacts {
            young_share: 0.35,
            training_facility: 5,
            ..facts()
        });
        let contender = classify(&ClubFacts {
            standing_pct: 0.0,
            young_share: 0.0,
            training_facility: 1,
            ..facts()
        });
        assert!(youthful.rank_weight(&prospect(500_000)) > youthful.rank_weight(&veteran(500_000)));
        assert!(
            contender.rank_weight(&veteran(500_000)) > contender.rank_weight(&prospect(500_000))
        );
    }

    /// Given a constrained club
    /// When it weighs a free player and a dear one
    /// Then it prefers the free one, and it will not put a quarter of its cash into one signing
    /// unless it has a hole to fill.
    #[test]
    fn a_constrained_club_prefers_free_players_and_spends_little() {
        let broke = classify(&ClubFacts {
            cash: 1_000_000,
            ..facts()
        });
        assert!(broke.rank_weight(&veteran(0)) > broke.rank_weight(&veteran(2_000_000)));
        assert!(!broke.allows_spending(400_000, 1_000_000, false));
        assert!(broke.allows_spending(200_000, 1_000_000, false));
        assert!(broke.allows_spending(900_000, 1_000_000, true));
    }

    /// Given a wealthy contender
    /// When it considers a dear signing
    /// Then its means do not restrict it.
    #[test]
    fn a_wealthy_contender_is_not_held_back() {
        let rich = classify(&ClubFacts {
            standing_pct: 0.0,
            cash: 60_000_000,
            young_share: 0.1,
            ..facts()
        });
        assert!(rich.allows_spending(30_000_000, 60_000_000, false));
    }

    /// Given any profile
    /// When it weighs any target
    /// Then the weight stays inside its bounds: a profile tilts a choice, never decides it.
    #[test]
    fn the_weight_is_bounded() {
        for standing in [0.0, 0.5, 1.0] {
            for cash in [0, 5_000_000, 90_000_000] {
                let profile = classify(&ClubFacts {
                    standing_pct: standing,
                    cash,
                    young_share: 0.4,
                    training_facility: 5,
                    ..facts()
                });
                for target in [
                    prospect(0),
                    prospect(9_000_000),
                    veteran(0),
                    veteran(9_000_000),
                ] {
                    let w = profile.rank_weight(&target);
                    assert!((WEIGHT_MIN..=WEIGHT_MAX).contains(&w), "{w}");
                }
            }
        }
    }
}
