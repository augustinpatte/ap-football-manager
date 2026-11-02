//! How a free agent weighs a club's offer.
//!
//! One score per offer, a sum of small bounded terms, so the answer is explainable: the wage
//! against what he can command, how far the club sits below his level, the place he would have in
//! the side, the length of the contract, how long he has been out of work, and a promise. At zero
//! or above he signs; just below it he asks for different terms; further below he walks away.
//!
//! Nothing here is stored or random. The same offer judged twice on the same day is judged the
//! same way, which is what lets a saved game reload in the middle of a negotiation and come to the
//! same answer.

use super::consts::*;
use crate::contracts::MAX_CONTRACT_YEARS;
use crate::squad_floor::{STARTERS_PER_GROUP, group_index};
use chrono::NaiveDate;
use domain::free_agent_talk::FreeAgentTalkReason;
use domain::player::{Player, SquadRole};
use serde::{Deserialize, Serialize};

/// What the club offers, in the terms the player weighs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfferTerms {
    pub weekly_wage: u32,
    pub contract_years: u32,
    pub signing_bonus: u64,
    pub promise_stepping_stone: bool,
}

impl OfferTerms {
    /// What he weighs against his expectations: the wage plus the bonus spread over the contract.
    /// The board's wage policy is still held to the wage alone.
    pub fn worth_per_week(&self) -> u32 {
        let weeks = WEEKS_PER_YEAR * u64::from(self.contract_years.max(1));
        self.weekly_wage
            .saturating_add(u32::try_from(self.signing_bonus / weeks).unwrap_or(u32::MAX))
    }
}

/// The part he would play for the club, by where his rating puts him among its seniors of the
/// same position group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceInSide {
    /// Clearly the best in his position.
    Key,
    Starter,
    Rotation,
    Backup,
}

/// A club's senior squad as a player looking at it sees it: ratings by position group (best
/// first) and the level of the whole.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClubSquad {
    groups: [Vec<u8>; 4],
}

impl ClubSquad {
    pub(crate) fn of(players: &[Player], team_id: &str) -> Self {
        let mut squad = Self::default();
        for player in players {
            squad.add(player, team_id);
        }
        squad.finish();
        squad
    }

    pub(crate) fn add(&mut self, player: &Player, team_id: &str) {
        if player.team_id.as_deref() == Some(team_id)
            && player.squad_role == SquadRole::Senior
            && !player.retired
        {
            self.groups[group_index(&player.position)].push(player.ovr);
        }
    }

    /// A squad read off ratings already sorted best first, for a caller that has them by group.
    pub(crate) fn from_levels(levels: &[Vec<u8>; 4]) -> Self {
        let mut squad = Self {
            groups: levels.clone(),
        };
        squad.finish();
        squad
    }

    pub(crate) fn finish(&mut self) {
        for group in &mut self.groups {
            group.sort_unstable_by(|left, right| right.cmp(left));
        }
    }

    pub(crate) fn seniors(&self) -> usize {
        self.groups.iter().map(Vec::len).sum()
    }

    /// Mean rating of the club's best [`SQUAD_LEVEL_SAMPLE`] seniors: the level it plays at.
    pub(crate) fn level(&self) -> u8 {
        let mut ratings: Vec<u8> = self.groups.iter().flatten().copied().collect();
        ratings.sort_unstable_by(|left, right| right.cmp(left));
        ratings.truncate(SQUAD_LEVEL_SAMPLE);
        if ratings.is_empty() {
            return 0;
        }
        let total: u32 = ratings.iter().map(|ovr| u32::from(*ovr)).sum();
        (total / ratings.len() as u32) as u8
    }

    /// The part a member of this squad plays in it, judged against the others: he is not his own
    /// rival, which `place_for` would make him, since his own rating is on the list.
    pub(crate) fn place_among_others(&self, player: &Player) -> PlaceInSide {
        let mut others = self.clone();
        let group = group_index(&player.position);
        if let Some(own) = others.groups[group]
            .iter()
            .position(|ovr| *ovr == player.ovr)
        {
            others.groups[group].remove(own);
        }
        others.place_for(player)
    }

    pub(crate) fn place_for(&self, player: &Player) -> PlaceInSide {
        let group = group_index(&player.position);
        let ratings = &self.groups[group];
        let better = ratings.iter().filter(|ovr| **ovr > player.ovr).count();
        let starters = STARTERS_PER_GROUP[group];
        let clear_of_the_next = ratings
            .iter()
            .find(|ovr| **ovr <= player.ovr)
            .is_none_or(|next| player.ovr >= next.saturating_add(KEY_PLAYER_LEAD));
        if better == 0 && clear_of_the_next {
            PlaceInSide::Key
        } else if better < starters {
            PlaceInSide::Starter
        } else if better < starters + ROTATION_PLACES {
            PlaceInSide::Rotation
        } else {
            PlaceInSide::Backup
        }
    }
}

/// How high a player stands in the game, on the same 0–1000 scale as a club's reputation.
/// A young player with room to grow stands a little above his current rating.
pub(crate) fn player_standing(ovr: u8, potential: u8, age: i32) -> i32 {
    let ovr = i32::from(ovr);
    let effective = if age < 24 {
        ovr + (i32::from(potential) - ovr).max(0) / 3
    } else {
        ovr
    };
    ((effective - STANDING_FLOOR_OVR) * STANDING_PER_OVR).clamp(0, 1000)
}

/// How high a club stands: its reputation and the level it actually plays at, together.
pub(crate) fn club_standing(reputation: u32, squad_level: u8) -> i32 {
    let reputation = i32::try_from(reputation).unwrap_or(1000).clamp(0, 1000);
    // A club with no senior squad yet has no level to speak of: it stands on its name.
    if squad_level == 0 {
        return reputation;
    }
    let squad = ((i32::from(squad_level) - STANDING_FLOOR_OVR) * STANDING_PER_OVR).clamp(0, 1000);
    (reputation * CLUB_REPUTATION_WEIGHT_PCT + squad * (100 - CLUB_REPUTATION_WEIGHT_PCT)) / 100
}

/// How much a player minds dropping down a level, in per cent of the ordinary: the young and the
/// gifted want more from a career; a veteran has fewer years to be choosy.
pub(crate) fn ambition_pct(age: i32, ovr: u8, potential: u8) -> i32 {
    let mut pct = 100;
    if age <= 26 {
        pct += 15;
    }
    if i32::from(potential) >= i32::from(ovr) + 5 {
        pct += 15;
    }
    if age >= 31 {
        pct -= 30;
    }
    if age >= 34 {
        pct -= 15;
    }
    pct.clamp(40, 150)
}

/// Where a player who already has a club stands today. A free agent has none of this: he is
/// judged against his own level; a contracted player against what he would be leaving.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Incumbent {
    /// [`club_standing`] of the club he is at.
    pub club_standing: i32,
    /// His place in that club's side.
    pub place: PlaceInSide,
    /// Points toward the door from his circumstances there: see `appeal_of`.
    pub push: i32,
}

/// Everything about the player and the club the score needs, gathered by the caller.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AppealFacts {
    pub age: i32,
    /// [`ambition_pct`] of the player.
    pub ambition: i32,
    pub player_standing: i32,
    pub club_standing: i32,
    pub place: PlaceInSide,
    pub days_unattached: i64,
    pub wants_stepping_stone: bool,
    pub expected_wage: u32,
    pub expected_years: u32,
    /// `Some` for a player under contract, who weighs the move against staying.
    pub incumbent: Option<Incumbent>,
}

/// The score of an offer and the terms of it, kept apart so the interface can say what weighed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appeal {
    pub score: i32,
    pub wage: i32,
    pub sporting: i32,
    pub role: i32,
    pub duration: i32,
    pub situation: i32,
    pub promise: i32,
}

/// What the player makes of an offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    Counter,
    Refuse,
}

impl Appeal {
    pub fn verdict(&self) -> Verdict {
        if self.score >= ACCEPT_AT {
            Verdict::Accept
        } else if self.score >= COUNTER_AT {
            Verdict::Counter
        } else {
            Verdict::Refuse
        }
    }

    /// The term that hurt the offer most: the reason given when he says no or asks for more.
    pub fn main_obstacle(&self) -> FreeAgentTalkReason {
        let mut worst = (0, FreeAgentTalkReason::WageTooLow);
        for (term, reason) in [
            (self.wage, FreeAgentTalkReason::WageTooLow),
            (self.sporting, FreeAgentTalkReason::ClubBelowHisLevel),
            (self.role, FreeAgentTalkReason::NoPlaceInSideForHim),
            (self.duration, FreeAgentTalkReason::ContractTooShort),
        ] {
            if term < worst.0 {
                worst = (term, reason);
            }
        }
        worst.1
    }
}

/// Scores `terms` for a player with `facts`.
pub(crate) fn appeal_of(facts: &AppealFacts, terms: &OfferTerms) -> Appeal {
    let expected = i64::from(facts.expected_wage.max(1));
    let worth = i64::from(terms.worth_per_week());
    // Per cent over or under what he expects, halved: a quarter more pay is twelve points.
    let wage = (((worth - expected) * 100 / expected) / 2)
        .clamp(i64::from(WAGE_TERM_MIN), i64::from(WAGE_TERM_MAX)) as i32;

    let sporting = match facts.incumbent {
        // A contracted player is not judged against his own level but against the club he would
        // leave: a lateral move costs nothing, a step up is a draw, a step down is a price.
        Some(now) => {
            let step = facts.club_standing - now.club_standing;
            if step >= 0 {
                (step / STEP_UP_POINTS_PER_STANDING).min(STEP_UP_BONUS_MAX)
            } else if -step <= MOVE_DOWN_FREE_GAP {
                0
            } else {
                -((-step - MOVE_DOWN_FREE_GAP) * facts.ambition
                    / 100
                    / STEP_DOWN_POINTS_PER_STANDING)
            }
        }
        None => {
            let gap = facts.player_standing - facts.club_standing;
            if gap <= 0 {
                ((-gap) / STEP_UP_POINTS_PER_STANDING).min(STEP_UP_BONUS_MAX)
            } else if gap <= STEP_DOWN_FREE_GAP {
                0
            } else {
                -((gap - STEP_DOWN_FREE_GAP) * facts.ambition / 100 / STEP_DOWN_POINTS_PER_STANDING)
            }
        }
    };

    let role = match facts.incumbent {
        // What he would be against what he is: a key man does not become a backup for nothing,
        // a backup is glad of a place.
        Some(now) => (role_points(facts.place) - role_points(now.place))
            .clamp(ROLE_DELTA_MIN, ROLE_DELTA_MAX),
        None => role_points(facts.place),
    };

    let duration = if terms.contract_years < facts.expected_years {
        -DURATION_POINTS_PER_YEAR * (facts.expected_years - terms.contract_years) as i32
    } else if facts.age >= 33 && terms.contract_years > facts.expected_years.max(2) {
        -OLD_PLAYER_LONG_DEAL_POINTS * (terms.contract_years - facts.expected_years.max(2)) as i32
    } else {
        0
    };

    let situation = match facts.incumbent {
        // Under contract he is not desperate; what moves him is what is wrong where he is.
        Some(now) => now.push.clamp(0, PUSH_MAX),
        None => ((facts.days_unattached - DESPERATION_STARTS_AFTER_DAYS).max(0)
            / DESPERATION_DAYS_PER_POINT)
            .min(i64::from(DESPERATION_MAX)) as i32,
    };

    let promise = if terms.promise_stepping_stone && facts.wants_stepping_stone {
        PROMISE_POINTS
    } else {
        0
    };

    Appeal {
        score: wage + sporting + role + duration + situation + promise,
        wage,
        sporting,
        role,
        duration,
        situation,
        promise,
    }
}

fn role_points(place: PlaceInSide) -> i32 {
    match place {
        PlaceInSide::Key => ROLE_KEY,
        PlaceInSide::Starter => ROLE_STARTER,
        PlaceInSide::Rotation => 0,
        PlaceInSide::Backup => ROLE_BACKUP,
    }
}

/// The terms he asks for in place of an offer that fell short, or `None` when no terms the market
/// would bear can make him sign: the wage he would need is more than he can ever command there.
pub(crate) fn counter_terms(facts: &AppealFacts, terms: &OfferTerms) -> Option<(u32, u32)> {
    let years = terms
        .contract_years
        .max(facts.expected_years)
        .min(MAX_CONTRACT_YEARS);
    let with_years = OfferTerms {
        contract_years: years,
        ..*terms
    };
    let appeal = appeal_of(facts, &with_years);
    if appeal.score >= ACCEPT_AT {
        return Some((terms.weekly_wage, years));
    }
    // One point of score is two per cent of what he expects, and the point past the line is his
    // margin of comfort.
    let missing = i64::from(ACCEPT_AT - appeal.score + 1);
    let raise = (i64::from(facts.expected_wage) * missing * 2 + 99) / 100;
    let wage = terms.weekly_wage.saturating_add(u32::try_from(raise).ok()?);
    let wage = crate::contracts::round_up_wage(wage);
    let countered = appeal_of(
        facts,
        &OfferTerms {
            weekly_wage: wage,
            ..with_years
        },
    );
    (countered.score >= ACCEPT_AT).then_some((wage, years))
}

/// Days since he last left a club, or 0 when nothing says when he did.
pub(crate) fn days_unattached(player: &Player, today: NaiveDate) -> i64 {
    player
        .movement_history
        .iter()
        .filter_map(|entry| NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").ok())
        .filter(|date| *date <= today)
        .max()
        .map_or(0, |left| (today - left).num_days().max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;

    fn facts() -> AppealFacts {
        AppealFacts {
            age: 28,
            ambition: 100,
            player_standing: 500,
            club_standing: 500,
            place: PlaceInSide::Starter,
            days_unattached: 0,
            wants_stepping_stone: false,
            expected_wage: 10_000,
            expected_years: 3,
            incumbent: None,
        }
    }

    fn offer(weekly_wage: u32, contract_years: u32) -> OfferTerms {
        OfferTerms {
            weekly_wage,
            contract_years,
            signing_bonus: 0,
            promise_stepping_stone: false,
        }
    }

    fn midfielder(ovr: u8) -> Player {
        let mut player = Player::new(
            format!("p{ovr}"),
            "P".to_string(),
            "P".to_string(),
            "1998-01-01".to_string(),
            "England".to_string(),
            domain::player::Position::Midfielder,
            uniform_attributes(50),
        );
        player.ovr = ovr;
        player.team_id = Some("club".to_string());
        player
    }

    /// Given the same player and club
    /// When the wage offered rises from nothing to ten times his ask
    /// Then the score never falls, and the wage term stays within its bounds.
    #[test]
    fn the_wage_term_rises_with_the_wage_and_is_bounded() {
        let mut last = i32::MIN;
        for wage in (0..=100_000).step_by(1_000) {
            let appeal = appeal_of(&facts(), &offer(wage, 3));
            assert!(appeal.score >= last, "score fell at wage {wage}");
            assert!((WAGE_TERM_MIN..=WAGE_TERM_MAX).contains(&appeal.wage));
            last = appeal.score;
        }
    }

    /// Given a club at, a little below, and far below the player
    /// When he weighs it
    /// Then a small gap costs nothing, a wide one costs points, and a bigger club earns a bonus.
    #[test]
    fn dropping_down_costs_only_beyond_a_small_gap() {
        let at = |club_standing| {
            appeal_of(
                &AppealFacts {
                    club_standing,
                    ..facts()
                },
                &offer(10_000, 3),
            )
        };

        assert_eq!(at(500).sporting, 0);
        assert_eq!(at(450).sporting, 0, "within the free gap");
        assert!(at(300).sporting < 0);
        assert!(at(100).sporting < at(300).sporting);
        assert!(at(900).sporting > 0, "a bigger club is a draw");
        assert!(at(900).sporting <= STEP_UP_BONUS_MAX);
    }

    /// Given two players the same distance above a club
    /// When one is young and gifted and the other a veteran
    /// Then the young one minds the drop more.
    #[test]
    fn ambition_makes_a_drop_cost_more() {
        let young = ambition_pct(21, 70, 80);
        let veteran = ambition_pct(35, 70, 70);
        let at = |ambition| {
            appeal_of(
                &AppealFacts {
                    ambition,
                    club_standing: 100,
                    ..facts()
                },
                &offer(10_000, 3),
            )
            .sporting
        };

        assert!(young > 100 && veteran < 100);
        assert!(at(young) < at(100) && at(100) < at(veteran));
    }

    /// Given a player out of work for different lengths of time
    /// When he weighs the same offer
    /// Then nothing changes for three weeks, then he softens a point every three days, up to a cap.
    #[test]
    fn a_long_wait_softens_him_up_to_a_cap() {
        let after = |days_unattached| {
            appeal_of(
                &AppealFacts {
                    days_unattached,
                    ..facts()
                },
                &offer(10_000, 3),
            )
            .situation
        };

        assert_eq!(after(0), 0);
        assert_eq!(after(DESPERATION_STARTS_AFTER_DAYS), 0);
        assert_eq!(after(DESPERATION_STARTS_AFTER_DAYS + 3), 1);
        assert_eq!(after(10_000), DESPERATION_MAX);
    }

    /// Given an offer a few points short
    /// When the player counters
    /// Then his terms reach the line, never ask for less than was offered, and a hopeless offer
    ///      gets no counter at all.
    #[test]
    fn a_counter_reaches_the_line_or_there_is_none() {
        let short = AppealFacts {
            club_standing: 300,
            ..facts()
        };
        let offered = offer(8_500, 3);
        assert_eq!(appeal_of(&short, &offered).verdict(), Verdict::Counter);

        let (wage, years) = counter_terms(&short, &offered).expect("within reach");
        assert!(wage >= offered.weekly_wage);
        let asked = appeal_of(
            &short,
            &OfferTerms {
                weekly_wage: wage,
                contract_years: years,
                ..offered
            },
        );
        assert!(asked.score >= ACCEPT_AT);

        let hopeless = AppealFacts {
            club_standing: 0,
            player_standing: 1_000,
            ..facts()
        };
        assert_eq!(counter_terms(&hopeless, &offer(10_000, 3)), None);
    }

    /// Given a contract shorter than he wants
    /// When he counters
    /// Then he asks for the length, not for more pay, if the length is all that stands in the way.
    #[test]
    fn a_short_contract_alone_is_countered_with_the_length() {
        let facts = facts();
        let offered = offer(10_000, 1);
        assert_eq!(appeal_of(&facts, &offered).verdict(), Verdict::Counter);

        assert_eq!(counter_terms(&facts, &offered), Some((10_000, 3)));
    }

    /// Given an offer that falls short on several counts
    /// When the player explains himself
    /// Then he names the term that hurt most.
    #[test]
    fn the_stated_reason_is_the_worst_term() {
        let appeal = appeal_of(
            &AppealFacts {
                club_standing: 100,
                place: PlaceInSide::Backup,
                ..facts()
            },
            &offer(10_000, 3),
        );

        assert!(appeal.sporting < appeal.role);
        assert_eq!(
            appeal.main_obstacle(),
            FreeAgentTalkReason::ClubBelowHisLevel
        );
    }

    /// Given a signing bonus
    /// When the player weighs the offer
    /// Then the bonus counts toward his pay spread over the contract.
    #[test]
    fn a_signing_bonus_counts_as_pay_over_the_contract() {
        let plain = offer(10_000, 2);
        let with_bonus = OfferTerms {
            signing_bonus: 104_000,
            ..plain
        };

        assert_eq!(plain.worth_per_week(), 10_000);
        assert_eq!(with_bonus.worth_per_week(), 11_000);
    }

    /// Given a squad with a clear best, two others and a deep bench in his position
    /// When players of different ratings are placed
    /// Then a clear lead is key, the next few start, then rotation, then the bench.
    #[test]
    fn a_players_place_follows_his_rating_among_the_club_seniors() {
        let mut squad = ClubSquad::default();
        for ovr in [70, 68, 66, 64, 62, 60, 58, 56] {
            squad.add(&midfielder(ovr), "club");
        }
        squad.finish();

        assert_eq!(squad.place_for(&midfielder(80)), PlaceInSide::Key);
        assert_eq!(
            squad.place_for(&midfielder(71)),
            PlaceInSide::Starter,
            "a lead under three points"
        );
        assert_eq!(squad.place_for(&midfielder(65)), PlaceInSide::Starter);
        assert_eq!(squad.place_for(&midfielder(63)), PlaceInSide::Rotation);
        assert_eq!(squad.place_for(&midfielder(59)), PlaceInSide::Backup);
        assert_eq!(squad.level(), 63);
    }

    /// A club with no senior squad stands on its reputation; a player's standing is bounded.
    #[test]
    fn standings_are_bounded_and_an_empty_squad_counts_for_nothing() {
        assert_eq!(club_standing(400, 0), 400);
        assert_eq!(club_standing(5_000, 99), 1_000);
        assert_eq!(player_standing(99, 99, 28), 1_000);
        assert_eq!(player_standing(30, 30, 28), 0);
        assert!(player_standing(60, 80, 19) > player_standing(60, 80, 28));
    }
}
