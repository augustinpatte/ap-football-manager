//! One AI club's reading of the market on one day: what it would pay for a target and whether
//! the case for him stands.
//!
//! This is the buyer's side of the sweep (`market.rs`) lifted out of its loop so that the same
//! judgement can be asked of a club that is not the one whose turn it is: a club that makes room
//! for a signing by selling a surplus player (`room_making.rs`) needs another club to *take* him, and
//! that club must want him, afford him and be allowed him by the same rules as for any purchase.
//! There is one set of rules for "would this club buy this player", and this is it.

use super::buyer_view::BuyerView;
use super::interest::{Funnel, count};
use super::*;

use crate::ai_finance_guard::SigningBooks;
use std::collections::HashMap;

/// How much further up the reputation ladder a club that has just come up looks for a signing:
/// its new division is above the reputation it earned in the old one.
const PROMOTED_REPUTATION_REACH: u32 = 50;

/// What a buyer's turn reads of the world and that does not change while the day's sweep runs
/// (the maps below are built once, before the first club shops).
pub(super) struct SweepFacts<'a> {
    pub current_date: NaiveDate,
    pub phase: WindowPhase,
    pub consent: &'a SweepConsent,
    pub team_country: &'a HashMap<String, String>,
    pub player_index: &'a HashMap<String, usize>,
    pub strategies: &'a HashMap<String, strategy::ClubProfile>,
    pub sale_view: &'a sale_view::SaleView,
    pub home_matches: &'a HashMap<String, i64>,
    pub promoted_clubs: &'a HashSet<&'a str>,
    pub relegated_clubs: &'a HashSet<&'a str>,
}

impl SweepFacts<'_> {
    fn same_country(&self, club_a: &str, club_b: &str) -> bool {
        self.team_country
            .get(club_a)
            .is_some_and(|country| self.team_country.get(club_b) == Some(country))
    }
}

/// One buyer's squad, books and standing, read once for its turn.
pub(super) struct BuyerTurn {
    pub(super) team: Team,
    /// The club's seniors' ratings per group, for loans (the squad as it is today).
    pub(super) loan_levels: [Vec<u8>; 4],
    /// The club's seniors' ratings per group, for purchases (the squad a year on).
    pub(super) levels: [Vec<u8>; 4],
    /// Seniors per group who are not out for long: what the club can field.
    pub(super) available_depths: [usize; 4],
    /// Registered seniors per group, for the board's wage rule.
    depths: [usize; 4],
    pub(super) books: SigningBooks,
    pub(super) profile: strategy::ClubProfile,
    pub(super) view: BuyerView,
    cash: i64,
    wage_bill: i64,
    overstretched: bool,
    promoted: bool,
    /// Worked out once for this buyer, and only if a target gets as far as needing it.
    wage_facts: Option<BuyerWageFacts>,
    /// Whether the harness's funnel counts this club's refusals. A club weighed as the taker of a
    /// player another club sells to make room is not one of the day's shoppers, and counting its
    /// refusals would blur the questions the funnel answers about the shoppers.
    pub(super) counted: bool,
}

impl BuyerTurn {
    pub(super) fn new(
        game: &Game,
        facts: &SweepFacts<'_>,
        wage_bills: &HashMap<String, i64>,
        club_players: &HashMap<String, Vec<usize>>,
        position_depths: &HashMap<String, [usize; 4]>,
        team: Team,
    ) -> Self {
        let squad = club_players.get(&team.id).map_or(&[][..], Vec::as_slice);
        let (loan_levels, levels) = crate::squad_outlook::club_outlook(
            squad
                .iter()
                .map(|&index| &game.players[index])
                .filter(|player| {
                    player.team_id.as_deref() == Some(team.id.as_str())
                        && crate::squad_outlook::counts_toward_squad(player)
                }),
            facts.current_date,
        );
        let mut available_depths = [0_usize; 4];
        for &index in squad {
            let player = &game.players[index];
            if player.team_id.as_deref() == Some(team.id.as_str())
                && player.squad_role == domain::player::SquadRole::Senior
                && player
                    .injury
                    .as_ref()
                    .is_none_or(|injury| injury.days_remaining < LONG_INJURY_DAYS)
            {
                available_depths[crate::squad_floor::group_index(&player.position)] += 1;
            }
        }
        let bill = wage_bills.get(&team.id).copied().unwrap_or(0);
        let books = SigningBooks::given(
            &team,
            bill,
            facts.home_matches.get(&team.id).copied().unwrap_or(0),
        );
        let profile = facts
            .strategies
            .get(&team.id)
            .copied()
            .unwrap_or(strategy::ClubProfile::NEUTRAL);
        let overstretched = is_overstretched_at(
            bill,
            team.wage_budget,
            team.finance,
            // A club that has just come down does not spend as the division it left did.
            if facts.relegated_clubs.contains(team.id.as_str()) {
                100
            } else {
                120
            },
        );
        // What he adds to the side this buyer fields (Squad Intelligence: the team that plays its
        // formation, not a broad group's first cover), and whether the buyer is short in his group.
        let view = BuyerView::new(game, squad, &team, profile, facts.current_date);
        Self {
            loan_levels,
            levels,
            available_depths,
            depths: position_depths.get(&team.id).copied().unwrap_or([0; 4]),
            books,
            profile,
            view,
            cash: team.transfer_budget.min(team.finance),
            wage_bill: bill,
            overstretched,
            promoted: facts.promoted_clubs.contains(team.id.as_str()),
            wage_facts: None,
            counted: true,
            team,
        }
    }

    fn target_player<'g>(
        game: &'g Game,
        facts: &SweepFacts<'_>,
        target: &MarketTarget,
    ) -> Option<&'g Player> {
        facts
            .player_index
            .get(&target.player_id)
            .map(|&index| &game.players[index])
    }

    /// What the target adds to this buyer's side, and whether the buyer is short in his group.
    pub(super) fn fit(
        &self,
        game: &Game,
        facts: &SweepFacts<'_>,
        target: &MarketTarget,
    ) -> (i32, bool) {
        let levels = &self.levels[target.position_group_index];
        (
            Self::target_player(game, facts, target)
                .map_or(0, |player| self.view.improvement(player)),
            group_is_thin(levels, target.position_group_index),
        )
    }

    /// The fee this buyer would pay: the seller's price when it is within his ceiling, a little
    /// under it when the ceiling is close, none otherwise. A user's player has no asking price
    /// to haggle over: the incoming offer is the buyer's own.
    pub(super) fn price(
        &self,
        game: &Game,
        facts: &SweepFacts<'_>,
        target: &MarketTarget,
    ) -> Option<u64> {
        if !target.needs_a_case() {
            return Some(target.fee);
        }
        let (improvement, thin) = self.fit(game, facts, target);
        agreed_fee(
            target.fee,
            buyer_ceiling(&CeilingFacts {
                value: target.value,
                improvement,
                thin,
                age: target.age,
                phase: facts.phase,
                alternatives: 1,
                budget: self.cash,
                // A club that has come up wants to be ready for its new division.
                ambition: if self.promoted { 0.04 } else { 0.0 },
            }),
        )
    }

    /// The rank this buyer gives a target it can accept at `fee`: appeal, what he adds priced at
    /// the fee, the country he is in, and the club's own strategy.
    pub(super) fn rank(
        &self,
        game: &Game,
        facts: &SweepFacts<'_>,
        target: &MarketTarget,
        fee: u64,
    ) -> f64 {
        let (improvement, _) = self.fit(game, facts, target);
        let priced_points = Self::target_player(game, facts, target)
            .and_then(|player| self.view.appraise(player, fee))
            .map_or(0, |case| case.utility_points());
        let same_country = facts.same_country(&target.owner_team_id, &self.team.id);
        target_rank(
            target.score,
            priced_points,
            geographic_affinity(&GeoFacts {
                same_country,
                buyer_reputation: self.team.reputation,
                improvement,
                fee,
                buyer_budget: self.cash,
                value: target.value,
            }),
        ) * self.profile.rank_weight(&strategy::TargetFacts {
            age: target.age,
            potential_gap: target.potential_gap,
            improvement,
            fee,
            value: target.value,
        })
    }

    /// Whether the seller, which would not sell on the usual terms, is better off for an
    /// exceptional offer: it cannot lose a player nobody replaces, and it can mend more than he is
    /// worth.
    fn exceptional_sale_is_rational(
        &self,
        game: &Game,
        facts: &SweepFacts<'_>,
        club_players: &HashMap<String, Vec<usize>>,
        target: &MarketTarget,
        fee: u64,
    ) -> bool {
        let (Some(&index), Some(seller_team), Some(squad)) = (
            facts.player_index.get(&target.player_id),
            game.teams
                .iter()
                .find(|team| team.id == target.owner_team_id),
            club_players.get(&target.owner_team_id),
        ) else {
            return false;
        };
        facts.sale_view.rational_sale(
            game,
            &sale_view::Seller {
                team: seller_team,
                squad,
                profile: facts
                    .strategies
                    .get(&target.owner_team_id)
                    .copied()
                    .unwrap_or(strategy::ClubProfile::NEUTRAL),
                today: facts.current_date,
            },
            &game.players[index],
            fee,
        )
    }

    /// Counts a refusal in the harness's funnel (a target owned by the manager is the manager's
    /// question, any other the AI's) and answers "no".
    fn refuse(&self, target: &MarketTarget, stage: Funnel) -> bool {
        self.note(if target.is_user_owned {
            stage
        } else {
            stage.for_ai_owner()
        });
        false
    }

    fn note(&self, stage: Funnel) {
        if self.counted {
            count(stage);
        }
    }

    /// Whether this club would sign `target`: every test a purchase has to pass between two AI
    /// clubs, or from an AI club to the manager's, in the order they are cheapest. The sweep's own
    /// day limits (the cap on completed deals, the inbox throttle) are the caller's. Each refusal
    /// is counted in the harness's funnel where it happens.
    pub(super) fn would_sign(
        &mut self,
        game: &Game,
        facts: &SweepFacts<'_>,
        club_players: &HashMap<String, Vec<usize>>,
        target: &MarketTarget,
    ) -> bool {
        // A club that would not sell him on the usual terms still hears an exceptional offer
        // (settled with the fee, in `sporting_case`).
        let usual_terms = target.is_user_owned
            || willing_to_sell(
                target.status,
                target.is_listed,
                target.score,
                facts.phase,
                target.contract_days,
            );
        if !usual_terms && !sale_view::is_exceptional(target.fee, target.value) {
            self.note(Funnel::AiSellerUnwilling);
            return false;
        }
        let Some(price) = self.sporting_case(game, facts, club_players, target, usual_terms) else {
            return false;
        };
        self.can_close(game, facts, target, price)
    }

    /// The fee at which the buyer wants `target` for the side it fields, or `None`: the club's
    /// stature and need, the case for the fee, and whether the seller, if it was not selling, is
    /// better off for the offer. Nothing here is about whether the buyer has the money.
    fn sporting_case(
        &self,
        game: &Game,
        facts: &SweepFacts<'_>,
        club_players: &HashMap<String, Vec<usize>>,
        target: &MarketTarget,
        usual_terms: bool,
    ) -> Option<u64> {
        let phase = facts.phase;
        // The cheap tests come first, in front of anything that reads the buyer's side: a sweep
        // puts thousands of targets in front of every buyer, and only the few that pass these
        // are worth the evaluation of what they would add.
        let group = target.position_group_index;
        let thin = group_is_thin(&self.levels[group], group);
        // Below the floor the club is not choosing a signing, it is fielding a side: any body
        // will do. At the floor it has the bodies it needs and is choosing again.
        let below_floor =
            self.levels[group].len() < crate::squad_floor::MIN_PLAYERS_PER_GROUP[group].1;
        // Clubs only chase players that fit their stature and a position they
        // actually need, so a single star doesn't draw the whole division. A club with a hole
        // to fill near the close will look a little higher.
        let reach = if thin {
            (60.0 * phase.pressure()) as u32
        } else {
            0
        } + if self.promoted {
            PROMOTED_REPUTATION_REACH
        } else {
            0
        };
        let player = Self::target_player(game, facts, target);
        let fills_a_starting_need =
            player.is_some_and(|player| self.view.has_starter_need_for(player));
        if !buyer_has_genuine_interest(
            self.team.reputation,
            target.owner_reputation.saturating_sub(reach),
            self.available_depths[group],
            fills_a_starting_need,
        ) {
            self.refuse(target, Funnel::RejectedNoInterest);
            return None;
        }
        // Nobody pays a fee for a player at the end of his career, a goalkeeper's coming later.
        if target.needs_a_case() && target.age >= retirement_horizon_age(group) {
            self.refuse(target, Funnel::RejectedNoCase);
            return None;
        }
        // A club that cannot carry what it already pays sells and lends before it buys.
        if target.needs_a_case() && self.overstretched && target.fee > 0 {
            self.refuse(target, Funnel::RejectedSpending);
            return None;
        }
        // The first cover usually screens weak targets cheaply, but broad groups cannot see
        // a specialist for a weak starting slot. The side appraises that exception below.
        if target.needs_a_case()
            && !target_fills_a_need(&self.levels[group], group, target.planned_ovr())
            && !fills_a_starting_need
        {
            self.refuse(target, Funnel::RejectedNoCase);
            return None;
        }
        // A cheap foreign filler is not worth the trip.
        if target.needs_a_case() {
            let same_country = facts.same_country(&target.owner_team_id, &self.team.id);
            let improvement = if same_country || thin || target.value >= CROSS_BORDER_FILLER_VALUE {
                0
            } else {
                self.fit(game, facts, target).0
            };
            if !worth_the_border(same_country, target.value, improvement, thin) {
                self.refuse(target, Funnel::RejectedNoCase);
                return None;
            }
        }
        let Some(price) = self.price(game, facts, target) else {
            self.refuse(target, Funnel::RejectedPrice);
            return None;
        };
        // A club short of money spends little on one player unless it has a hole to fill.
        if target.needs_a_case() && !self.profile.allows_spending(price, self.cash, thin) {
            self.refuse(target, Funnel::RejectedSpending);
            return None;
        }
        if !usual_terms
            && !self.exceptional_sale_is_rational(game, facts, club_players, target, price)
        {
            self.note(Funnel::AiSellerDeclinedExceptional);
            return None;
        }
        // An AI club buys what improves the side by more than the money is worth to it. A club
        // short of bodies may waive the price test, but never the question of whether the
        // player adds anything at all: that is the third keeper a club with two already has,
        // and the fee for a spectator. The floor is the model's own (`MIN_GAIN`), the same one
        // the loan path asks of a borrower, so a fee is not a way round it. Only a club under
        // the legal squad floor, which must find a body to field a side, is excused entirely.
        //
        // A thin club's waiver is for a player who makes the side better *now*. A player whose
        // value is all in a later season is a succession signing, and a succession signing has
        // to be worth its fee on the club's own scale like any other: without that, a club at
        // the floor bought future bench cover with the price test waived, and the cover never
        // started even in its own model.
        if target.needs_a_case() {
            let valued = player
                .and_then(|player| self.view.appraise(player, price))
                .is_some_and(|case| match (below_floor, thin) {
                    (true, _) => true,
                    (false, true) => case.is_worth_it() || case.helps_now(),
                    (false, false) => case.is_worth_it(),
                });
            if !valued {
                self.refuse(target, Funnel::RejectedNoCase);
                return None;
            }
        }
        Some(price)
    }

    /// Whether the buyer can close the deal at `price`: its budget, the player's terms, the wage
    /// the board allows and the club's books.
    fn can_close(
        &mut self,
        game: &Game,
        facts: &SweepFacts<'_>,
        target: &MarketTarget,
        price: u64,
    ) -> bool {
        if !(self.team.transfer_budget >= price as i64 && self.team.finance >= price as i64) {
            return self.refuse(target, Funnel::RejectedBudget);
        }
        let Some(player) = Self::target_player(game, facts, target) else {
            return false;
        };
        let Some(deal_terms) = facts
            .consent
            .terms_for(player, &self.team, facts.current_date)
        else {
            return self.refuse(target, Funnel::RejectedTerms);
        };
        let wage_facts = self
            .wage_facts
            .get_or_insert_with(|| BuyerWageFacts::new(self.wage_bill, self.depths));
        if !buyer_can_pay_standard_wage(
            player,
            &self.team,
            wage_facts,
            facts.current_date,
            &deal_terms,
        ) {
            return self.refuse(target, Funnel::RejectedWage);
        }
        if target.needs_a_case()
            && !self.books.can_carry(
                i64::from(
                    standard_contract_terms(
                        player,
                        &self.team,
                        facts.current_date,
                        deal_terms.player_weekly_wage,
                    )
                    .map_or(0, |(wage, _)| wage),
                ),
                price as i64,
            )
        {
            return self.refuse(target, Funnel::RejectedBooks);
        }
        self.note(if target.is_user_owned {
            Funnel::Passed
        } else {
            Funnel::AiPassed
        });
        true
    }
}
