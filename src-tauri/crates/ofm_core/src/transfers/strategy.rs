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
use crate::manager_identity::ManagerIdentity;

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
    /// How much its manager builds around young players, 0 to 1 (0.5 is the neutral manager, who
    /// changes nothing). A manager's tilt on the club's own philosophy, never the philosophy.
    pub manager_youth_trust: f64,
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
/// How far a manager's trust in youth moves the club's youth focus either way. The club's own
/// squad and academy span the whole range; a manager shades them, so a youth-built club with a
/// sceptical manager is still more youth-minded than an old squad with a believer.
const MANAGER_YOUTH_TILT: f64 = 0.15;

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
    let manager_tilt = (facts.manager_youth_trust - 0.5) * 2.0 * MANAGER_YOUTH_TILT;
    let youth_focus = ((0.6 * squad + 0.4 * academy) * patience + manager_tilt).clamp(0.0, 1.0);

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
    let tiers = crate::end_of_season::division_tiers(game);
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
                    // A finish says how the club did in the division it was earned in; in another
                    // one (promoted, relegated) it says nothing about this league. A record with
                    // no tier, or a division the pyramid does not rank, is read as it always was.
                    let current_tier = tiers.get(league.id.as_str()).copied();
                    let finished = team
                        .history
                        .last()
                        .filter(|record| match (record.tier, current_tier) {
                            (Some(earned), Some(now)) => earned == now,
                            _ => true,
                        })
                        .map(|record| {
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
                manager_youth_trust: ManagerIdentity::for_club(game, Some(team)).youth_trust,
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
            manager_youth_trust: 0.5,
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

    /// Given a youth-built club, an old one, and each under a manager who trusts youth, doubts it,
    /// and the neutral one
    /// When they are classified
    /// Then the manager tilts the club's youth focus the way he leans and the neutral manager
    /// changes nothing, but the club's own squad and academy still outweigh him.
    #[test]
    fn a_manager_tilts_a_clubs_youth_focus_without_overriding_its_philosophy() {
        let club = |young_share, training_facility, manager_youth_trust| {
            classify(&ClubFacts {
                young_share,
                training_facility,
                manager_youth_trust,
                ..facts()
            })
            .youth_focus
        };
        let (middling, youthful, old) = ((0.15, 2), (0.30, 5), (0.02, 1));
        assert!(club(middling.0, middling.1, 1.0) > club(middling.0, middling.1, 0.5));
        assert!(club(middling.0, middling.1, 0.5) > club(middling.0, middling.1, 0.0));
        // The club's own figure: 0.6 of a squad 0.15/0.35 young and 0.4 of an academy 2 in 5, for
        // an established club (patience 1). The neutral manager adds nothing to it.
        let own = 0.6 * (0.15 / 0.35) + 0.4 * (2.0 / 5.0);
        assert!(
            (club(middling.0, middling.1, 0.5) - own).abs() < 1e-12,
            "the neutral manager moved the club"
        );
        assert!(
            club(youthful.0, youthful.1, 0.0) > club(old.0, old.1, 1.0),
            "a sceptical manager out-voted a youth-built club's academy"
        );
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

    // --- Seven clubs in seven situations ------------------------------------
    //
    // Each names a club by what has happened to it, not by a category: nothing here is a club type,
    // each is facts fed to the one derivation, and the assertion is what a market buyer then prefers
    // and why. They are the situations the strategy exists to tell apart.

    fn club_with(update: impl FnOnce(&mut ClubFacts)) -> ClubProfile {
        let mut facts = facts();
        update(&mut facts);
        classify(&facts)
    }

    /// An immediate improvement: a player in his prime who adds a lot to the side.
    fn proven(fee: u64) -> TargetFacts {
        TargetFacts {
            age: 27,
            potential_gap: 0,
            improvement: 6,
            fee,
            value: 20_000_000,
        }
    }

    /// Given a club at the top of its league with a fifth of its squad's value in cash
    /// When it classifies and weighs a proven player against a prospect
    /// Then it is a wealthy contender, prefers the player who helps now, and can pay his fee.
    #[test]
    fn a_wealthy_contender_prefers_the_player_who_helps_now() {
        let club = club_with(|f| {
            f.standing_pct = 0.05;
            f.cash = 30_000_000;
        });
        assert_eq!(
            (club.ambition, club.means),
            (Ambition::Contender, Means::Wealthy)
        );
        assert!(club.rank_weight(&proven(15_000_000)) > club.rank_weight(&prospect(2_000_000)));
        assert!(club.allows_spending(15_000_000, 30_000_000, false));
    }

    /// Given a club whose cash is a sliver of its squad's value
    /// When it weighs an expensive player against a free one, and a large fee for one player
    /// Then it is constrained, prefers the free player, and will not stake a quarter of its cash on
    /// one signing unless it has a hole to fill.
    #[test]
    fn a_financially_constrained_club_prefers_free_players_and_guards_its_cash() {
        let club = club_with(|f| f.cash = 2_000_000);
        assert_eq!(club.means, Means::Constrained);
        assert!(club.rank_weight(&proven(0)) > club.rank_weight(&proven(19_000_000)));
        assert!(!club.allows_spending(1_000_000, 2_000_000, false));
        assert!(
            club.allows_spending(1_000_000, 2_000_000, true),
            "a hole is the exception"
        );
    }

    /// Given a small club in the league it has just joined, with no season behind it there
    /// When it classifies
    /// Then it is a fighter that takes a free player over a priced one of the same quality.
    #[test]
    fn a_promoted_club_aims_at_what_it_can_afford() {
        let club = club_with(|f| {
            f.standing_pct = 0.9;
            f.finished_pct = None;
        });
        assert_eq!(club.ambition, Ambition::Fighter);
        assert!(club.rank_weight(&proven(0)) > club.rank_weight(&proven(15_000_000)));
    }

    /// Given a club built around its academy and young squad
    /// When it weighs a prospect against a veteran
    /// Then its youth focus is high, it favours the prospect and marks the veteran down.
    #[test]
    fn a_youth_built_club_prefers_the_prospect() {
        let club = club_with(|f| {
            f.young_share = 0.35;
            f.training_facility = 5;
        });
        assert!(club.youth_focus > 0.9);
        assert!(club.rank_weight(&prospect(1_000_000)) > 1.3);
        assert!(club.rank_weight(&veteran(1_000_000)) < 0.8);
    }

    /// Given a club that finished at the foot of the table although its reputation puts it among
    /// the best, and the same club a year earlier with no season behind it
    /// When each classifies
    /// Then poor results take it down a step, and only a step: it is still a club of its stature.
    #[test]
    fn poor_results_lower_a_clubs_ambition_by_one_step() {
        let before = club_with(|f| {
            f.standing_pct = 0.1;
            f.finished_pct = None;
        });
        let after = club_with(|f| {
            f.standing_pct = 0.1;
            f.finished_pct = Some(0.95);
        });
        assert_eq!(
            (before.ambition, after.ambition),
            (Ambition::Contender, Ambition::Established)
        );
    }

    /// Given one club under two managers, one who trusts youth and one who doubts it
    /// When it classifies under each
    /// Then the club's ambition and means (its philosophy and its books) are the same and only the
    /// youth focus, which a manager tilts, differs.
    #[test]
    fn a_new_manager_tilts_a_club_without_changing_what_it_is() {
        let under = |trust: f64| club_with(|f| f.manager_youth_trust = trust);
        let (believer, sceptic) = (under(1.0), under(0.0));
        assert_eq!(
            (believer.ambition, believer.means),
            (sceptic.ambition, sceptic.means)
        );
        assert!(believer.youth_focus > sceptic.youth_focus);
    }

    /// A world manager's id whose identity satisfies `wanted`.
    fn manager_like(wanted: impl Fn(&ManagerIdentity) -> bool) -> String {
        (0..)
            .map(|index| format!("manager-{index}"))
            .find(|id| wanted(&ManagerIdentity::of(id)))
            .expect("some manager fits")
    }

    /// Given the same AI club and the same user's club, each under a manager who trusts youth, a
    /// sceptic, and none
    /// When the market's profiles are read
    /// Then the AI club's youth focus follows its manager, and the user's club, whose manager is
    /// the player, reads no disposition of the world's.
    #[test]
    fn profiles_read_the_clubs_own_manager_and_never_the_users() {
        let believer = manager_like(|m| m.youth_trust > 0.9);
        let sceptic = manager_like(|m| m.youth_trust < 0.1);
        let youth_focus = |users: Option<&str>, ai: Option<&str>| {
            let mut game = crate::transfers::tests::acceptance_game("team1", false);
            game.teams[0].manager_id = users.map(str::to_string);
            game.teams[1].manager_id = ai.map(str::to_string);
            let profiles = profiles(&game, &HashMap::new());
            (profiles["team1"].youth_focus, profiles["team2"].youth_focus)
        };
        let (users_with, ai_with_believer) = youth_focus(Some(&believer), Some(&believer));
        let (users_without, ai_with_sceptic) = youth_focus(None, Some(&sceptic));
        assert!(
            ai_with_believer > ai_with_sceptic,
            "{ai_with_believer} vs {ai_with_sceptic}"
        );
        assert_eq!(
            users_with, users_without,
            "the user's club read a world manager's trust"
        );
    }

    // --- A finish is read for the division it was earned in ------------------------------------

    /// A world of one club under test and four peers in one league, with a second league above or
    /// below it so the pyramid has two tiers, and the club's last season on record as given.
    fn pyramid_with(
        club_reputation: u32,
        peers_reputation: u32,
        league_now: u32,
        last_season: (u32, Option<u32>),
    ) -> Game {
        use domain::league::League;
        use domain::team::{Team, TeamSeasonRecord};

        let mut game = crate::transfers::tests::acceptance_game("team1", false);
        let team = |id: &str, reputation: u32| {
            let mut team = Team::new(
                id.into(),
                id.into(),
                id.into(),
                "England".into(),
                "Town".into(),
                "Ground".into(),
                10_000,
            );
            team.reputation = reputation;
            team
        };
        let mut club = team("club", club_reputation);
        club.history.push(TeamSeasonRecord {
            season: 2025,
            league_position: last_season.0,
            tier: last_season.1,
            played: 8,
            won: 4,
            drawn: 2,
            lost: 2,
            goals_for: 10,
            goals_against: 8,
        });
        let mut teams = vec![club];
        teams.extend((0..4).map(|index| team(&format!("peer{index}"), peers_reputation)));
        let ids: Vec<String> = teams.iter().map(|t| t.id.clone()).collect();
        let league = |id: &str, priority: u32, participants: &[String]| {
            let mut league = League::new(id.into(), id.into(), 2026, participants);
            league.country_id = Some("ENG".to_string());
            league.priority = priority;
            league
        };
        let (here, elsewhere) = if league_now == 0 {
            ("top", "second")
        } else {
            ("second", "top")
        };
        let priority = |id: &str| if id == "top" { 0 } else { 1 };
        game.competitions = vec![
            league(here, priority(here), &ids),
            league(elsewhere, priority(elsewhere), &[]),
        ];
        game.teams = teams;
        game
    }

    fn ambition_of_club(game: &Game) -> Ambition {
        profiles(game, &HashMap::new())["club"].ambition
    }

    /// Given a small club that won the division below and now sits last by reputation in its new one
    /// When its strategy is read
    /// Then it is a fighter: first in the second tier says nothing about the first.
    #[test]
    fn a_promoted_champion_is_still_a_fighter_in_its_new_division() {
        let game = pyramid_with(300, 800, 0, (1, Some(1)));
        assert_eq!(ambition_of_club(&game), Ambition::Fighter);
    }

    /// Given the same club and the same finish, but earned in the division it still plays in
    /// When its strategy is read
    /// Then the finish counts: a small club that topped its own league aims a step higher.
    #[test]
    fn a_finish_in_the_same_division_still_lifts_a_club() {
        let game = pyramid_with(300, 800, 0, (1, Some(0)));
        assert_eq!(ambition_of_club(&game), Ambition::Established);
    }

    /// Given a club of high reputation that finished last in the top flight and now plays a tier
    /// below, first by reputation among its new peers
    /// When its strategy is read
    /// Then it is still a contender there: its relegation does not mark it down twice.
    #[test]
    fn a_relegated_club_is_still_a_contender_in_its_new_division() {
        let game = pyramid_with(900, 400, 1, (5, Some(0)));
        assert_eq!(ambition_of_club(&game), Ambition::Contender);
    }

    /// Given a record written before divisions were recorded
    /// When the club's strategy is read
    /// Then the finish is taken as earned in its current division, as it always was: an old save
    /// keeps the strategy it had.
    #[test]
    fn a_record_from_before_tiers_were_kept_reads_as_it_always_did() {
        let game = pyramid_with(300, 800, 0, (1, None));
        assert_eq!(ambition_of_club(&game), Ambition::Established);
    }
}
