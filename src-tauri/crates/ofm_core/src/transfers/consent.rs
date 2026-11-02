//! The third party to a paid transfer: the player.
//!
//! The clubs agreeing on a fee is only one of the three conditions of a move. The player must also
//! want to go, and the money must fit the buyer's books (`fund_block`, the board's wage policy,
//! the squad floor). This module answers the middle question with the same score a free agent
//! weighs an offer by (`contracts::free_agent_talks::appeal`), extended with what a player who
//! already has a club also weighs: where he stands now, the part he plays there, and anything that
//! is wrong there.
//!
//! The answer is explainable and has no random element: the same move judged twice on the same day
//! is judged the same way, which is what lets a saved game reload in the middle of a negotiation.
//! No rule here is a flat prohibition. A step down in stature costs points in proportion to the
//! drop and to his ambition; pay, a longer deal, a bigger role, a promise, and the circumstances
//! at his club (listed, unhappy, a contract running down, a club in debt) buy them back.

use super::*;
use crate::contracts::{
    ACCEPT_AT, AppealFacts, ClubSquad, Incumbent, InterestLevel, OfferTerms, PUSH_CLUB_IN_TROUBLE,
    PUSH_CONTRACT_ENDING, PUSH_CONTRACT_RUNNING_DOWN, PUSH_DISCONTENT, PUSH_LISTED, PUSH_UNHAPPY,
    PlaceInSide, Verdict, appeal_of, club_standing, counter_terms, facts_for,
};
use domain::free_agent_talk::FreeAgentTalkReason;

/// Morale at or under which a player is unhappy where he is.
const UNHAPPY_MORALE: u8 = 45;
/// Morale at or under which he is discontented, without being ready to leave over it.
const DISCONTENT_MORALE: u8 = 60;
/// A contract this close to its end, in days, makes a player open to a move.
const CONTRACT_ENDING_DAYS: i64 = 180;
const CONTRACT_RUNNING_DOWN_DAYS: i64 = 365;

/// What the player makes of a move, and what would change his mind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MoveJudgement {
    pub score: i32,
    pub verdict: Verdict,
    /// The wage and length that would bring him round, when pay and length can.
    pub counter: Option<(u32, u32)>,
    pub main_obstacle: Option<FreeAgentTalkReason>,
    pub terms: OfferTerms,
    pub expected_wage: u32,
    pub expected_years: u32,
    pub place_in_new_side: PlaceInSide,
}

impl MoveJudgement {
    pub(crate) fn interest(&self) -> InterestLevel {
        match self.verdict {
            Verdict::Accept => InterestLevel::Keen,
            Verdict::Counter => InterestLevel::Hesitant,
            Verdict::Refuse => InterestLevel::Unlikely,
        }
    }
}

/// How the player stands toward a paid move, for the screens that ask before the manager commits.
/// Not a promise: his mood, or the clubs' squads, can change before the deal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerOutlook {
    pub interest: InterestLevel,
    pub main_obstacle: Option<FreeAgentTalkReason>,
    /// The wage and length the buyer's standard terms come to, and so what he starts from.
    pub expected_wage: u32,
    pub expected_years: u32,
    /// The wage and length he would ask for if the offer falls short, when he would ask at all.
    pub asks_wage: Option<u32>,
    pub asks_years: Option<u32>,
    pub place_in_new_side: PlaceInSide,
}

impl From<&MoveJudgement> for PlayerOutlook {
    fn from(judgement: &MoveJudgement) -> Self {
        Self {
            interest: judgement.interest(),
            main_obstacle: judgement.main_obstacle,
            expected_wage: judgement.expected_wage,
            expected_years: judgement.expected_years,
            asks_wage: judgement.counter.map(|(wage, _)| wage),
            asks_years: judgement.counter.map(|(_, years)| years),
            place_in_new_side: judgement.place_in_new_side,
        }
    }
}

/// What the player knows of the club he is leaving, besides its squad: its name and whether it is
/// in debt. Cheap to copy, so the market sweep can judge a move without holding the whole team.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClubView {
    pub reputation: u32,
    pub in_debt: bool,
}

impl From<&Team> for ClubView {
    fn from(team: &Team) -> Self {
        Self {
            reputation: team.reputation,
            in_debt: team.finance < 0,
        }
    }
}

/// What is wrong, from his point of view, at the club he is at. Bounded by `PUSH_MAX` in the score.
fn push_to_leave(player: &Player, seller: ClubView, today: NaiveDate) -> i32 {
    let mut push = 0;
    if player.transfer_listed {
        push += PUSH_LISTED;
    }
    if player.morale <= UNHAPPY_MORALE {
        push += PUSH_UNHAPPY;
    } else if player.morale <= DISCONTENT_MORALE {
        push += PUSH_DISCONTENT;
    }
    if let Some(days) = contract_days_remaining(today, player.contract_end()) {
        if days <= CONTRACT_ENDING_DAYS {
            push += PUSH_CONTRACT_ENDING;
        } else if days <= CONTRACT_RUNNING_DOWN_DAYS {
            push += PUSH_CONTRACT_RUNNING_DOWN;
        }
    }
    if seller.in_debt {
        push += PUSH_CLUB_IN_TROUBLE;
    }
    push
}

/// The terms the player weighs for these `TransferTerms`: the wage and length the manager put in
/// them, or the buyer's standard ones where he put nothing.
pub(crate) fn offer_terms_of(
    player: &Player,
    buyer: &Team,
    terms: &TransferTerms,
    today: NaiveDate,
) -> OfferTerms {
    let standard_wage = crate::contracts::expected_wage(player, buyer, today);
    let standard_years = crate::contracts::expected_contract_years(player, today);
    OfferTerms {
        weekly_wage: if terms.player_weekly_wage > 0 {
            terms.player_weekly_wage
        } else {
            standard_wage
        },
        contract_years: if terms.player_contract_years > 0 {
            u32::from(terms.player_contract_years)
        } else {
            standard_years
        },
        signing_bonus: terms.signing_bonus,
        promise_stepping_stone: terms.promise_stepping_stone,
    }
}

/// The two squads a judgement reads, as the player sees them.
pub(crate) struct Squads<'a> {
    pub seller: &'a ClubSquad,
    pub buyer: &'a ClubSquad,
}

/// Judges a move of `player` from `seller` to `buyer` on `terms`.
pub(crate) fn judge_move(
    player: &Player,
    seller: ClubView,
    buyer: &Team,
    squads: &Squads,
    terms: &TransferTerms,
    today: NaiveDate,
) -> MoveJudgement {
    let offer = offer_terms_of(player, buyer, terms, today);
    let mut facts = facts_for(player, buyer, squads.buyer, &offer, today);
    // The standard terms already stand for what he expects; a promise lowers it for the wish to
    // be promised a way up, as for a free agent.
    facts.days_unattached = 0;
    facts.incumbent = Some(Incumbent {
        club_standing: club_standing(seller.reputation, squads.seller.level()),
        place: squads.seller.place_among_others(player),
        push: push_to_leave(player, seller, today),
    });
    verdict_of(&facts, offer)
}

fn verdict_of(facts: &AppealFacts, offer: OfferTerms) -> MoveJudgement {
    let appeal = appeal_of(facts, &offer);
    let verdict = appeal.verdict();
    MoveJudgement {
        score: appeal.score,
        verdict,
        counter: (verdict != Verdict::Accept)
            .then(|| counter_terms(facts, &offer))
            .flatten(),
        main_obstacle: (appeal.score < ACCEPT_AT).then(|| appeal.main_obstacle()),
        terms: offer,
        expected_wage: facts.expected_wage,
        expected_years: facts.expected_years,
        place_in_new_side: facts.place,
    }
}

/// [`judge_move`] for the manager's club buying `player_id`, with the squads read off the game.
pub(crate) fn judge_in_game(
    game: &Game,
    player: &Player,
    buyer_team_id: &str,
    terms: &TransferTerms,
) -> Result<MoveJudgement, String> {
    let seller_id = player
        .team_id
        .as_deref()
        .ok_or("be.error.transfers.playerHasNoTeam")?;
    let team = |id: &str| {
        game.teams
            .iter()
            .find(|team| team.id == id)
            .ok_or_else(|| "be.error.teamNotFound".to_string())
    };
    let seller = team(seller_id)?;
    let buyer = team(buyer_team_id)?;
    let seller_squad = ClubSquad::of(&game.players, seller_id);
    let buyer_squad = ClubSquad::of(&game.players, buyer_team_id);
    Ok(judge_move(
        player,
        ClubView::from(seller),
        buyer,
        &Squads {
            seller: &seller_squad,
            buyer: &buyer_squad,
        },
        terms,
        game.clock.current_date.date_naive(),
    ))
}

pub const ERR_PLAYER_REFUSES_MOVE: &str = "be.error.transfers.playerRefusesMove";

/// The terms a club that bids for one of the manager's players signs him on, if he agrees to go:
/// the wage he is asked for is paid in full when he comes at his club's standard terms, and the
/// buyer pays what he asks when he is hesitant and the ask is in proportion. Refused with
/// [`ERR_PLAYER_REFUSES_MOVE`] when he will not go, whatever the clubs agreed.
pub(crate) fn incoming_deal_terms(
    game: &Game,
    player_id: &str,
    buyer_team_id: &str,
) -> Result<TransferTerms, String> {
    let player = game
        .players
        .iter()
        .find(|player| player.id == player_id)
        .ok_or("be.error.playerNotFound")?;
    let judgement = judge_in_game(game, player, buyer_team_id, &TransferTerms::default())?;
    terms_he_comes_on(&judgement).ok_or_else(|| ERR_PLAYER_REFUSES_MOVE.to_string())
}

/// Whether a move agreed some time ago still has the player's consent on the day it is carried
/// out. Only a refusal stops it: he said yes on the terms of the day, and what has changed since
/// is not allowed to turn a near thing into a veto. Used when a deal that waited for the window
/// registers.
pub(crate) fn still_agrees(
    game: &Game,
    player_id: &str,
    buyer_team_id: &str,
    terms: &TransferTerms,
) -> bool {
    game.players
        .iter()
        .find(|player| player.id == player_id)
        .and_then(|player| judge_in_game(game, player, buyer_team_id, terms).ok())
        .is_none_or(|judgement| judgement.verdict != Verdict::Refuse)
}

/// Every club's squad and standing as a player sees them, read once for a whole sweep of the
/// market so that judging a candidate move costs no more than a lookup.
pub(crate) struct SweepConsent {
    squads: std::collections::HashMap<String, ClubSquad>,
    views: std::collections::HashMap<String, ClubView>,
    empty: ClubSquad,
}

impl SweepConsent {
    pub(crate) fn new(
        game: &Game,
        levels: &std::collections::HashMap<String, [Vec<u8>; 4]>,
    ) -> Self {
        Self {
            squads: levels
                .iter()
                .map(|(club, groups)| (club.clone(), ClubSquad::from_levels(groups)))
                .collect(),
            views: game
                .teams
                .iter()
                .map(|team| (team.id.clone(), ClubView::from(team)))
                .collect(),
            empty: ClubSquad::default(),
        }
    }

    /// The terms on which `player` would join `buyer` of his own accord, or `None` when he would
    /// not.
    pub(crate) fn terms_for(
        &self,
        player: &Player,
        buyer: &Team,
        today: NaiveDate,
    ) -> Option<TransferTerms> {
        let seller_id = player.team_id.as_deref()?;
        let seller = *self.views.get(seller_id)?;
        let squads = Squads {
            seller: self.squads.get(seller_id).unwrap_or(&self.empty),
            buyer: self.squads.get(&buyer.id).unwrap_or(&self.empty),
        };
        terms_he_comes_on(&judge_move(
            player,
            seller,
            buyer,
            &squads,
            &TransferTerms::default(),
            today,
        ))
    }
}

/// What an AI club has to pay for the player to come on its own initiative: its standard terms
/// when he is keen, the wage he asks when he is hesitant and the ask is not out of proportion,
/// and nothing (he does not come) otherwise. `None` is a refusal; `Some(terms)` carries the wage
/// to sign him on, so the buyer's board is asked about the wage he will really be paid.
pub(crate) fn terms_he_comes_on(judgement: &MoveJudgement) -> Option<TransferTerms> {
    /// An AI club will not pay more than this share over its standard wage to win a player round.
    const AI_PREMIUM_PCT: u32 = 30;
    match judgement.verdict {
        Verdict::Accept => Some(TransferTerms::default()),
        Verdict::Refuse => None,
        Verdict::Counter => {
            let (wage, years) = judgement.counter?;
            let ceiling = judgement.terms.weekly_wage / 100 * (100 + AI_PREMIUM_PCT);
            (wage <= ceiling.max(judgement.terms.weekly_wage)).then(|| TransferTerms {
                player_weekly_wage: wage,
                player_contract_years: u8::try_from(years).unwrap_or(0),
                ..TransferTerms::default()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::uniform_attributes;
    use domain::player::Position;

    fn star(ovr: u8, age_born: &str) -> Player {
        let mut player = Player::new(
            "star".to_string(),
            "S".to_string(),
            "Star".to_string(),
            age_born.to_string(),
            "England".to_string(),
            Position::Midfielder,
            uniform_attributes(60),
        );
        player.ovr = ovr;
        player.potential = ovr;
        player.morale = 75;
        player.team_id = Some("seller".to_string());
        player.stage_wage(20_000);
        player.stage_contract_end(Some("2031-06-30".to_string()));
        player
    }

    fn club(id: &str, reputation: u32) -> Team {
        let mut team = Team::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "England".to_string(),
            "X".to_string(),
            "X".to_string(),
            10_000,
        );
        team.reputation = reputation;
        team.finance = 5_000_000;
        team
    }

    fn squad_of(team_id: &str, level: u8, with: Option<&Player>) -> ClubSquad {
        let mut players = Vec::new();
        for index in 0..9 {
            let mut member = star(60, "1996-01-01");
            member.id = format!("{team_id}-{index}");
            member.team_id = Some(team_id.to_string());
            member.ovr = level.saturating_sub(index as u8 % 3);
            players.push(member);
        }
        if let Some(player) = with {
            players.push(player.clone());
        }
        ClubSquad::of(&players, team_id)
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()
    }

    fn judge(
        player: &Player,
        seller_rep: u32,
        buyer_rep: u32,
        terms: &TransferTerms,
    ) -> MoveJudgement {
        let seller = club("seller", seller_rep);
        let buyer = club("buyer", buyer_rep);
        let seller_squad = squad_of("seller", 74, Some(player));
        let buyer_squad = squad_of("buyer", 62, None);
        judge_move(
            player,
            ClubView::from(&seller),
            &buyer,
            &Squads {
                seller: &seller_squad,
                buyer: &buyer_squad,
            },
            terms,
            today(),
        )
    }

    /// Given a first-team player of a prestigious club and a much weaker buyer
    /// When the buyer offers the standard terms
    /// Then he refuses or at best asks for more: affordability alone does not take him.
    #[test]
    fn an_elite_player_does_not_drop_to_a_much_weaker_club_on_standard_terms() {
        let player = star(82, "1998-01-01");
        let judgement = judge(&player, 850, 250, &TransferTerms::default());
        assert_ne!(
            judgement.verdict,
            Verdict::Accept,
            "score {}",
            judgement.score
        );
    }

    /// Given the same move
    /// When his club has listed him, he is unhappy and his contract is nearly over
    /// Then the move becomes possible: credible exceptional circumstances.
    #[test]
    fn exceptional_circumstances_make_the_same_drop_possible() {
        let mut player = star(82, "1998-01-01");
        let before = judge(&player, 850, 450, &TransferTerms::default());
        assert_eq!(before.verdict, Verdict::Refuse, "score {}", before.score);
        player.transfer_listed = true;
        player.morale = 30;
        player.stage_contract_end(Some("2026-12-31".to_string()));
        let after = judge(&player, 850, 450, &TransferTerms::default());
        assert!(after.score >= before.score + 20);
        assert_ne!(after.verdict, Verdict::Refuse, "score {}", after.score);
    }

    /// Given a player stepping up to a bigger club
    /// When he is offered the standard terms
    /// Then he is keen.
    #[test]
    fn a_step_up_is_welcome() {
        let player = star(70, "1998-01-01");
        let judgement = judge(&player, 300, 800, &TransferTerms::default());
        assert_eq!(
            judgement.verdict,
            Verdict::Accept,
            "score {}",
            judgement.score
        );
    }

    /// Given a hesitant player
    /// When the wage offered rises
    /// Then his score never falls, and a counter names the wage that brings him round.
    #[test]
    fn pay_buys_back_a_step_down_and_the_counter_names_the_price() {
        let player = star(78, "1998-01-01");
        let standard = judge(&player, 700, 450, &TransferTerms::default());
        let mut last = standard.score;
        for raise in [1_000_u32, 5_000, 10_000, 20_000] {
            let more = judge(
                &player,
                700,
                450,
                &TransferTerms {
                    player_weekly_wage: standard.terms.weekly_wage + raise,
                    ..TransferTerms::default()
                },
            );
            assert!(more.score >= last);
            last = more.score;
        }
        if let (Verdict::Counter, Some((wage, _))) = (standard.verdict, standard.counter) {
            let met = judge(
                &player,
                700,
                450,
                &TransferTerms {
                    player_weekly_wage: wage,
                    ..TransferTerms::default()
                },
            );
            assert_eq!(met.verdict, Verdict::Accept);
        }
    }

    /// Given a key man at his club
    /// When the buyer would make him a backup
    /// Then the role costs him more than the same move into a starting place.
    #[test]
    fn a_smaller_part_in_the_new_side_weighs_against_the_move() {
        let seller = club("seller", 500);
        let buyer = club("buyer", 500);
        let player = star(70, "1998-01-01");
        let seller_squad = squad_of("seller", 64, Some(&player));
        let weak_side = squad_of("buyer", 60, None);
        let strong_side = squad_of("buyer", 80, None);
        let go = |buyer_squad: &ClubSquad| {
            judge_move(
                &player,
                ClubView::from(&seller),
                &buyer,
                &Squads {
                    seller: &seller_squad,
                    buyer: buyer_squad,
                },
                &TransferTerms::default(),
                today(),
            )
        };
        let into_weak = go(&weak_side);
        let into_strong = go(&strong_side);
        assert_eq!(into_weak.place_in_new_side, PlaceInSide::Key);
        assert_eq!(into_strong.place_in_new_side, PlaceInSide::Backup);
        assert!(into_weak.score > into_strong.score);
    }

    /// Given the same facts judged twice
    /// Then the answer is identical: nothing is random or stored.
    #[test]
    fn the_judgement_is_deterministic() {
        let player = star(75, "1999-01-01");
        assert_eq!(
            judge(&player, 600, 400, &TransferTerms::default()),
            judge(&player, 600, 400, &TransferTerms::default())
        );
    }

    /// Given a hesitant player whose ask is modest
    /// Then an AI club can bring him round by paying it; a refusal stays a refusal.
    #[test]
    fn an_ai_club_pays_a_modest_ask_but_not_a_refusal() {
        let hesitant = MoveJudgement {
            score: -10,
            verdict: Verdict::Counter,
            counter: Some((11_000, 3)),
            main_obstacle: None,
            terms: OfferTerms {
                weekly_wage: 10_000,
                contract_years: 3,
                signing_bonus: 0,
                promise_stepping_stone: false,
            },
            expected_wage: 10_000,
            expected_years: 3,
            place_in_new_side: PlaceInSide::Starter,
        };
        let terms = terms_he_comes_on(&hesitant).expect("a ten per cent ask is met");
        assert_eq!(terms.player_weekly_wage, 11_000);
        let greedy = MoveJudgement {
            counter: Some((20_000, 3)),
            ..hesitant.clone()
        };
        assert_eq!(terms_he_comes_on(&greedy), None);
        let refusal = MoveJudgement {
            verdict: Verdict::Refuse,
            counter: None,
            ..hesitant
        };
        assert_eq!(terms_he_comes_on(&refusal), None);
    }
}
