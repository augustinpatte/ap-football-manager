//! Promotion and relegation between the divisions of a pyramid under authored rules: direct
//! places, promotion play-offs, relegation barrages, and the reserve-side (B team) constraints.
//!
//! The planner is pure. It reads the final tables and the rules, asks a [`TieResolver`] to play
//! every tie of a play-off or barrage, and returns who goes where; nothing is changed here. The
//! generic swap OpenFootManager shipped with (`promotion::apply_promotion_relegation`) still
//! serves a pyramid that carries no rules.
//!
//! Boundaries are settled from the top of the pyramid down, so that a club promoted at one
//! boundary is known when the next one asks whether a reserve side may follow its parent.

use std::collections::{HashMap, HashSet};

use domain::competition_rules::{
    Barrage, BoundaryMovements, BoundaryRule, PlayoffTie, PromotionPlayoff, ReserveSide,
    RuleStatus, WithdrawnSide,
};
use domain::league::League;

/// One tie to play.
#[derive(Debug, Clone, Copy)]
pub struct TieRequest<'a> {
    /// Name of the round ("Semi-final", "Final", "Barrage") for the season history.
    pub stage: &'a str,
    /// The club named first: it plays at home in the first (or only) leg.
    pub home: &'a str,
    pub away: &'a str,
    pub legs: u8,
}

/// Plays the ties a play-off asks for. The game's implementation simulates them with the clubs'
/// squads; a test supplies a fixed outcome.
pub trait TieResolver {
    fn resolve(&mut self, request: &TieRequest<'_>) -> PlayoffTie;
}

/// A resolver for a pyramid that has no play-offs: the better-ranked club (the one named first)
/// wins. Never reached by rules that need none.
pub struct HigherSeedWins;

impl TieResolver for HigherSeedWins {
    fn resolve(&mut self, request: &TieRequest<'_>) -> PlayoffTie {
        PlayoffTie {
            stage: request.stage.to_string(),
            home_team_id: request.home.to_string(),
            away_team_id: request.away.to_string(),
            legs: vec![(1, 0); usize::from(request.legs.max(1))],
            penalties: None,
            winner_id: request.home.to_string(),
        }
    }
}

/// What one pass over a pyramid decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LadderOutcome {
    /// The clubs registered with each division for the next season, in the order of `divisions`.
    pub participants: Vec<Vec<String>>,
    /// The reserve sides standing out of each division.
    pub withdrawn: Vec<Vec<WithdrawnSide>>,
    /// One record per boundary: `movements[i]` is the boundary between divisions `i` and `i + 1`.
    pub movements: Vec<BoundaryMovements>,
}

/// Whether any division of the run carries authored movement rules or the country has reserve
/// sides: the cases the generic swap cannot serve.
pub fn run_needs_rules(divisions: &[League]) -> bool {
    divisions.iter().any(|division| {
        division.rules.ladder.is_authored()
            || !division.rules.reserve_sides.is_empty()
            || !division.rules.withdrawn_sides.is_empty()
    })
}

/// The reserve sides of a run, from whichever division carries the list (they all repeat it).
fn reserve_sides(divisions: &[League]) -> Vec<ReserveSide> {
    let mut seen = HashSet::new();
    divisions
        .iter()
        .flat_map(|division| division.rules.reserve_sides.iter())
        .filter(|side| seen.insert(side.team_id.clone()))
        .cloned()
        .collect()
}

/// The finishing order of the clubs still registered with the division.
fn ranked(division: &League) -> Vec<String> {
    crate::promotion::ranked_participants(division)
}

/// The generic one-fifth swap, expressed as a rule, for a boundary that carries none.
fn generic_rule(upper: &League, lower: &League) -> BoundaryRule {
    let count = crate::promotion::relegation_count(
        upper.participant_ids.len(),
        lower.participant_ids.len(),
    ) as u32;
    BoundaryRule {
        promoted_direct: count,
        promotion_playoff: None,
        relegated_direct: count,
        barrage: None,
        status: RuleStatus::Assumed,
        note: "generic OpenFootManager swap".to_string(),
    }
}

/// A seeded elimination among `seeds` (best first). Byes go to the best seeds when the field is
/// not a power of two; each round the survivors are re-seeded and the best meets the worst.
/// Returns the winner.
fn play_bracket(
    seeds: &[String],
    round_legs: u8,
    final_legs: u8,
    label: &str,
    resolver: &mut dyn TieResolver,
    ties: &mut Vec<PlayoffTie>,
) -> Option<String> {
    let rank: HashMap<&str, usize> = seeds
        .iter()
        .enumerate()
        .map(|(index, team)| (team.as_str(), index))
        .collect();
    let mut field: Vec<String> = seeds.to_vec();
    while field.len() > 1 {
        field.sort_by_key(|team| rank[team.as_str()]);
        let size = field.len().next_power_of_two();
        let byes = size - field.len();
        let (waiting, playing) = field.split_at(byes);
        let is_final = size == 2;
        let stage = match size {
            2 => format!("{label} · final"),
            4 => format!("{label} · semi-finals"),
            _ => format!("{label} · round of {size}"),
        };
        let legs = if is_final { final_legs } else { round_legs }.max(1);
        let mut winners: Vec<String> = waiting.to_vec();
        let pairs = playing.len() / 2;
        for index in 0..pairs {
            let better = &playing[index];
            let worse = &playing[playing.len() - 1 - index];
            // One leg: the better seed hosts. Two legs: the better seed hosts the second.
            let (home, away) = if legs == 1 {
                (better, worse)
            } else {
                (worse, better)
            };
            let tie = resolver.resolve(&TieRequest {
                stage: &stage,
                home,
                away,
                legs,
            });
            winners.push(tie.winner_id.clone());
            ties.push(tie);
        }
        field = winners;
    }
    field.pop()
}

struct BoundaryDraft {
    promoted_direct: Vec<String>,
    promoted_playoff: Vec<String>,
    relegated_direct: Vec<String>,
    relegated_barrage: Vec<String>,
    ties: Vec<PlayoffTie>,
    reserves_blocked: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn settle_boundary(
    upper_rank: &[String],
    lower_rank: &[String],
    rule: &BoundaryRule,
    tier: usize,
    tier_of: &HashMap<String, usize>,
    parent_of: &HashMap<&str, &str>,
    resolver: &mut dyn TieResolver,
    labels: (&str, &str),
) -> BoundaryDraft {
    let mut draft = BoundaryDraft {
        promoted_direct: Vec::new(),
        promoted_playoff: Vec::new(),
        relegated_direct: Vec::new(),
        relegated_barrage: Vec::new(),
        ties: Vec::new(),
        reserves_blocked: Vec::new(),
    };

    // The clubs that go down directly, worst last in the table.
    let direct_down = (rule.relegated_direct as usize).min(upper_rank.len());
    let relegated_now: HashSet<&str> = upper_rank[upper_rank.len() - direct_down..]
        .iter()
        .map(String::as_str)
        .collect();

    // A reserve side may only go up when its parent will be strictly above the division it
    // joins: the parent stays where it is (so it is in or above that division) or comes down.
    let parent_tier_after = |parent: &str| -> usize {
        if relegated_now.contains(parent) {
            tier + 1
        } else {
            tier_of.get(parent).copied().unwrap_or(usize::MAX)
        }
    };
    let mut eligible: Vec<String> = Vec::with_capacity(lower_rank.len());
    for club in lower_rank {
        let blocked = parent_of
            .get(club.as_str())
            .is_some_and(|parent| parent_tier_after(parent) >= tier);
        if blocked {
            draft.reserves_blocked.push(club.clone());
        } else {
            eligible.push(club.clone());
        }
    }

    // Direct promotion.
    let direct_up = (rule.promoted_direct as usize).min(eligible.len());
    draft.promoted_direct = eligible[..direct_up].to_vec();

    // Promotion play-off among the next eligible clubs.
    if let Some(PromotionPlayoff {
        from_rank,
        to_rank,
        round_legs,
        final_legs,
    }) = rule.promotion_playoff
    {
        let start = (from_rank as usize).saturating_sub(1).max(direct_up);
        let end = (to_rank as usize).min(eligible.len());
        if end > start
            && let Some(winner) = if end - start == 1 {
                Some(eligible[start].clone())
            } else {
                play_bracket(
                    &eligible[start..end],
                    round_legs,
                    final_legs,
                    labels.0,
                    resolver,
                    &mut draft.ties,
                )
            }
        {
            draft.promoted_playoff.push(winner);
        }
    }

    // Direct relegation, then the barrage.
    draft.relegated_direct = upper_rank[upper_rank.len() - direct_down..]
        .iter()
        .rev()
        .cloned()
        .collect();
    if let Some(Barrage {
        upper_rank: upper_place,
        lower_from_rank,
        lower_to_rank,
        bracket_legs,
        tie_legs,
    }) = rule.barrage
    {
        let upper_index = (upper_place as usize).saturating_sub(1);
        let candidate = upper_rank
            .get(upper_index)
            .filter(|club| !relegated_now.contains(club.as_str()));
        let start = (lower_from_rank as usize)
            .saturating_sub(1)
            .max(draft.promoted_direct.len());
        let end = (lower_to_rank as usize).min(eligible.len());
        if let Some(upper_club) = candidate
            && end > start
        {
            let challenger = if end - start == 1 {
                Some(eligible[start].clone())
            } else {
                play_bracket(
                    &eligible[start..end],
                    bracket_legs,
                    bracket_legs,
                    labels.1,
                    resolver,
                    &mut draft.ties,
                )
            };
            if let Some(challenger) = challenger {
                // Two legs: the upper club hosts the second. One leg: it hosts.
                let (home, away) = if tie_legs <= 1 {
                    (upper_club, &challenger)
                } else {
                    (&challenger, upper_club)
                };
                let tie = resolver.resolve(&TieRequest {
                    stage: &format!("{} · barrage", labels.1),
                    home,
                    away,
                    legs: tie_legs.max(1),
                });
                let challenger_won = tie.winner_id == challenger;
                draft.ties.push(tie);
                if challenger_won {
                    draft.promoted_playoff.push(challenger);
                    draft.relegated_barrage.push(upper_club.clone());
                }
            }
        }
    }

    // Keep both sides the same size: a short table or too few eligible clubs must not let one
    // division grow and the other shrink.
    let promoted = draft.promoted_direct.len() + draft.promoted_playoff.len();
    let relegated = draft.relegated_direct.len() + draft.relegated_barrage.len();
    if relegated > promoted {
        // Spare the best-placed of those who would have gone down.
        let surplus = relegated - promoted;
        let spared: Vec<String> = draft
            .relegated_direct
            .iter()
            .rev()
            .take(surplus.min(draft.relegated_direct.len()))
            .cloned()
            .collect();
        draft.relegated_direct.retain(|club| !spared.contains(club));
    } else if promoted > relegated {
        let mut surplus = promoted - relegated;
        while surplus > 0 && draft.promoted_playoff.pop().is_some() {
            surplus -= 1;
        }
        while surplus > 0 && draft.promoted_direct.pop().is_some() {
            surplus -= 1;
        }
    }
    draft
}

/// Plans the movements of one pyramid, divisions ordered highest first, each having finished its
/// season. Panics-free: a boundary that cannot be settled (an empty table) simply moves nobody.
pub fn plan_ruled_ladder(divisions: &[League], resolver: &mut dyn TieResolver) -> LadderOutcome {
    let reserves = reserve_sides(divisions);
    let parent_of: HashMap<&str, &str> = reserves
        .iter()
        .map(|side| (side.team_id.as_str(), side.parent_id.as_str()))
        .collect();

    let mut tier_of: HashMap<String, usize> = HashMap::new();
    for (tier, division) in divisions.iter().enumerate() {
        for club in &division.participant_ids {
            tier_of.insert(club.clone(), tier);
        }
    }

    let mut movements: Vec<BoundaryMovements> = Vec::new();
    let mut moved_up_from: Vec<HashSet<String>> = vec![HashSet::new(); divisions.len()];

    for tier in 0..divisions.len().saturating_sub(1) {
        let upper = &divisions[tier];
        let lower = &divisions[tier + 1];
        let mut upper_rank = ranked(upper);
        // Clubs the boundary above already took out of this division cannot go down as well.
        upper_rank.retain(|club| !moved_up_from[tier].contains(club));
        let lower_rank = ranked(lower);

        let mut record = BoundaryMovements {
            upper_competition_id: upper.id.clone(),
            lower_competition_id: lower.id.clone(),
            season: upper.season,
            ..Default::default()
        };
        if upper_rank.is_empty() || lower_rank.is_empty() {
            movements.push(record);
            continue;
        }
        let authored = upper.rules.ladder.rule_for(upper.season);
        let rule = match authored {
            Some(rule) if rule.is_consistent() => rule.clone(),
            Some(rule) => {
                log::warn!(
                    "[ladder] {}: the rule for season {} does not hold sizes ({} up, {} down); the generic swap is used",
                    upper.id,
                    upper.season,
                    rule.guaranteed_exchanges(),
                    rule.relegated_direct
                );
                generic_rule(upper, lower)
            }
            None => generic_rule(upper, lower),
        };

        let draft = settle_boundary(
            &upper_rank,
            &lower_rank,
            &rule,
            tier,
            &tier_of,
            &parent_of,
            resolver,
            (&format!("{}", lower.name), &format!("{}", upper.name)),
        );
        for club in draft.promoted_direct.iter().chain(&draft.promoted_playoff) {
            tier_of.insert(club.clone(), tier);
            moved_up_from[tier + 1].insert(club.clone());
        }
        for club in draft
            .relegated_direct
            .iter()
            .chain(&draft.relegated_barrage)
        {
            tier_of.insert(club.clone(), tier + 1);
        }
        record.promoted_direct = draft.promoted_direct;
        record.promoted_playoff = draft.promoted_playoff;
        record.relegated_direct = draft.relegated_direct;
        record.relegated_barrage = draft.relegated_barrage;
        record.ties = draft.ties;
        record.reserves_blocked = draft.reserves_blocked;
        movements.push(record);
    }

    // The registrations the movements leave behind.
    let mut participants: Vec<Vec<String>> = divisions
        .iter()
        .map(|division| division.participant_ids.clone())
        .collect();
    for (tier, record) in movements.iter().enumerate() {
        let up: HashSet<&String> = record.promoted().collect();
        let down: HashSet<&String> = record.relegated().collect();
        participants[tier].retain(|club| !down.contains(club));
        participants[tier + 1].retain(|club| !up.contains(club));
        participants[tier].extend(record.promoted().cloned());
        participants[tier + 1].extend(record.relegated().cloned());
    }

    let mut withdrawn: Vec<Vec<WithdrawnSide>> = divisions
        .iter()
        .map(|division| division.rules.withdrawn_sides.clone())
        .collect();
    let season = divisions.first().map_or(0, |division| division.season);
    settle_reserve_sides(
        &reserves,
        &mut participants,
        &mut withdrawn,
        &mut movements,
        divisions,
        season,
    );

    LadderOutcome {
        participants,
        withdrawn,
        movements,
    }
}

/// Where each club is registered after the movements.
fn tiers_now(participants: &[Vec<String>]) -> HashMap<String, usize> {
    participants
        .iter()
        .enumerate()
        .flat_map(|(tier, clubs)| clubs.iter().map(move |club| (club.clone(), tier)))
        .collect()
}

/// Brings back the reserve sides whose parent has left their division, then deals with the ones
/// whose parent has arrived in it: a reserve may never share a division with its parent club or
/// play above it. It goes down a division, taking the place of the best club of that division,
/// or, when there is no division below in the world, is withdrawn until the parent has left.
fn settle_reserve_sides(
    reserves: &[ReserveSide],
    participants: &mut [Vec<String>],
    withdrawn: &mut [Vec<WithdrawnSide>],
    movements: &mut [BoundaryMovements],
    divisions: &[League],
    season: u32,
) {
    if reserves.is_empty() && withdrawn.iter().all(Vec::is_empty) {
        return;
    }
    let parent_of: HashMap<&str, &str> = reserves
        .iter()
        .map(|side| (side.team_id.as_str(), side.parent_id.as_str()))
        .collect();

    // Readmission: the parent is now strictly above the division the reserve left.
    let tiers = tiers_now(participants);
    for tier in 0..withdrawn.len() {
        let waiting = std::mem::take(&mut withdrawn[tier]);
        for side in waiting {
            let parent_tier = parent_of
                .get(side.team_id.as_str())
                .and_then(|parent| tiers.get(*parent).copied());
            match parent_tier {
                Some(parent_tier) if parent_tier < tier => {
                    participants[tier].push(side.team_id.clone());
                    // The boundary above the division it comes back to carries the record.
                    let boundary = tier
                        .saturating_sub(1)
                        .min(movements.len().saturating_sub(1));
                    if let Some(record) = movements.get_mut(boundary) {
                        record.reserves_readmitted.push(side.team_id.clone());
                    }
                }
                _ => withdrawn[tier].push(side),
            }
        }
    }

    // Conflicts: a reserve level with, or above, its parent.
    let last_tier = participants.len() - 1;
    for side in reserves {
        let tiers = tiers_now(participants);
        let (Some(&reserve_tier), Some(&parent_tier)) =
            (tiers.get(&side.team_id), tiers.get(&side.parent_id))
        else {
            continue;
        };
        if reserve_tier > parent_tier {
            continue;
        }
        // The reserve must end up strictly below the parent.
        if reserve_tier < last_tier {
            // Down one division, exchanging with that division's best club that is not a
            // reserve itself on the way up and not already moved this season.
            let lower = &divisions[reserve_tier + 1];
            let moved: HashSet<&String> = movements
                .iter()
                .flat_map(|record| record.promoted().chain(record.relegated()))
                .collect();
            let replacement = ranked(lower).into_iter().find(|club| {
                participants[reserve_tier + 1].contains(club)
                    && !moved.contains(club)
                    && !parent_of.contains_key(club.as_str())
            });
            if let Some(replacement) = replacement {
                participants[reserve_tier].retain(|club| club != &side.team_id);
                participants[reserve_tier + 1].retain(|club| club != &replacement);
                participants[reserve_tier].push(replacement);
                participants[reserve_tier + 1].push(side.team_id.clone());
                if let Some(record) = movements.get_mut(reserve_tier) {
                    record.reserves_forced_down.push(side.team_id.clone());
                }
                continue;
            }
        }
        // No division below to receive it: out of the division until the parent leaves.
        participants[reserve_tier].retain(|club| club != &side.team_id);
        withdrawn[reserve_tier].push(WithdrawnSide {
            team_id: side.team_id.clone(),
            since_season: season,
        });
        let boundary = reserve_tier.min(movements.len().saturating_sub(1));
        if let Some(record) = movements.get_mut(boundary) {
            record.reserves_forced_down.push(side.team_id.clone());
        }
    }
}

#[cfg(test)]
#[path = "ladder_rules_tests.rs"]
mod tests;
