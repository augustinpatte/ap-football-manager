//! Contract talks between a club and a player who has no club.
//!
//! A free agent does not sign the moment an offer is made: the offer is a *talk* that sits on
//! the player until he answers, a few days later. Talks are kept on the player (inside the JSON
//! column that already holds his morale record) so a save made mid-negotiation reloads with every
//! offer, answer and deadline intact, and so a club can only ever have one open talk per player.

use serde::{Deserialize, Serialize};

/// Where a talk stands.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FreeAgentTalkStatus {
    /// The offer is with the player; he answers on `respond_on`.
    #[default]
    AwaitingResponse,
    /// The player wants other terms (`counter_*`); the club must answer or let it lapse.
    CounterOffered,
    /// Signed: the talk ended in a contract.
    Signed,
    /// The player turned the club down.
    Rejected,
    /// The club took its offer back.
    Withdrawn,
    /// Another club got him first.
    LostToRival,
    /// Nobody answered in time, or the club could no longer afford the deal.
    Lapsed,
}

impl FreeAgentTalkStatus {
    /// Still being negotiated: the player and club are talking, nothing has ended it.
    pub fn is_open(self) -> bool {
        matches!(self, Self::AwaitingResponse | Self::CounterOffered)
    }
}

/// Why a talk went the way it did: the single biggest factor in the player's answer, so the
/// interface can say it in words instead of leaving the manager to guess. Translation keys are
/// derived from the snake_case name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FreeAgentTalkReason {
    /// The pay did not match what he can command.
    WageTooLow,
    /// He is a clear step above the club's level and ambition.
    ClubBelowHisLevel,
    /// He would not be a regular there.
    NoPlaceInSideForHim,
    /// The contract is shorter than he wants.
    ContractTooShort,
    /// The offer was an insult; he will not talk to the club for a while.
    OfferInsulting,
    /// He has had enough rounds without agreement.
    TalksBrokenOff,
    /// A rival club's offer was better.
    BetterOfferElsewhere,
    /// The club can no longer afford the deal it offered.
    ClubCannotAfford,
    /// The club's senior list had no place left for him when he was ready to sign: it filled up,
    /// or its last place was promised to a purchase agreed and waiting to register. A capacity
    /// problem, not a money one. Appended after the existing variants; old saves never hold it.
    NoSeniorPlace,
    /// The manager took the offer back.
    ManagerWithdrew,
    /// The offer sat unanswered past its time.
    CounterExpired,
}

/// One club's offer to one free agent, and where the talk stands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct FreeAgentTalk {
    pub id: String,
    pub club_id: String,
    pub status: FreeAgentTalkStatus,
    pub weekly_wage: u32,
    pub contract_years: u32,
    pub signing_bonus: u64,
    /// The club promises not to stand in the way of a later move (see the stepping-stone promise).
    pub promise_stepping_stone: bool,
    /// How many offers the club has made in this talk; the player walks away after a few.
    pub round: u8,
    pub opened_on: String,
    pub updated_on: String,
    /// While awaiting a response: the day he answers. Always within a few days of the offer.
    pub respond_on: Option<String>,
    /// While a counter is on the table: the terms he asks for, and the day they stop standing.
    pub counter_wage: Option<u32>,
    pub counter_years: Option<u32>,
    pub counter_expires_on: Option<String>,
    pub closed_on: Option<String>,
    pub reason: Option<FreeAgentTalkReason>,
    /// The club in charge of the user's side of the table is the manager's; every other club's
    /// talk is made by the game on that club's behalf.
    pub made_by_manager: bool,
}

impl Default for FreeAgentTalk {
    fn default() -> Self {
        Self {
            id: String::new(),
            club_id: String::new(),
            status: FreeAgentTalkStatus::AwaitingResponse,
            weekly_wage: 0,
            contract_years: 1,
            signing_bonus: 0,
            promise_stepping_stone: false,
            round: 1,
            opened_on: String::new(),
            updated_on: String::new(),
            respond_on: None,
            counter_wage: None,
            counter_years: None,
            counter_expires_on: None,
            closed_on: None,
            reason: None,
            made_by_manager: false,
        }
    }
}

impl FreeAgentTalk {
    pub fn is_open(&self) -> bool {
        self.status.is_open()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given a talk saved before the squad-place reason existed, carrying an older reason
    /// When it is loaded
    /// Then it keeps that reason: the new variant changes no stored value.
    #[test]
    fn a_talk_saved_with_an_older_reason_still_loads_unchanged() {
        let saved = r#"{"id":"t","status":"lapsed","reason":"club_cannot_afford"}"#;

        let talk: FreeAgentTalk = serde_json::from_str(saved).expect("an old save loads");

        assert_eq!(talk.reason, Some(FreeAgentTalkReason::ClubCannotAfford));
    }

    /// Given the squad-place reason
    /// When it is saved and loaded again
    /// Then it round-trips under the snake_case name the interface derives its translation key from.
    #[test]
    fn the_squad_place_reason_round_trips_under_its_translation_key_name() {
        let encoded = serde_json::to_string(&FreeAgentTalkReason::NoSeniorPlace).unwrap();

        assert_eq!(encoded, "\"no_senior_place\"");
        assert_eq!(
            serde_json::from_str::<FreeAgentTalkReason>(&encoded).unwrap(),
            FreeAgentTalkReason::NoSeniorPlace
        );
    }
}
