//! What the manager has handed to the staff, and what the staff has proposed in return.
//!
//! Pure data: the rules that act on it live in `ofm_core::staff_system`.

use serde::{Deserialize, Serialize};

use crate::team::{TrainingFocus, TrainingIntensity};

/// A responsibility the manager can hand over.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Duty {
    /// The weekly training plan.
    Training,
    /// Filling vacant staff posts.
    StaffRecruitment,
    /// Sending idle scouts to look at players.
    Scouting,
    /// The academy: who is ready for the first team.
    Youth,
    /// Resting tired players before a fixture.
    MatchPreparation,
    /// Renewing the contracts of staff that are running down.
    StaffRenewals,
    /// Naming the eleven (and resting the tired) the day before a match.
    Lineup,
    /// Renewing the contracts of players that are running down.
    PlayerRenewals,
    /// Signing players to fill the squad's needs, through the ordinary transfer negotiation.
    PlayerRecruitment,
    /// The tactical plan for the next match: the nine phase dials, set from the club's style, the
    /// squad's legs and pace, and the recent results.
    Tactics,
    /// Selling: answering the offers other clubs make for the manager's players, and putting the
    /// players the squad does not need on the transfer list. What the assistant may do alone
    /// depends on the mode: the routine (turning down an insult, keeping a player nobody
    /// replaces) is always his; accepting, countering and listing are his only with Total
    /// authority, and are proposals to the manager otherwise.
    Offers,
}

impl Duty {
    pub const ALL: [Duty; 11] = [
        Duty::Training,
        Duty::StaffRecruitment,
        Duty::Scouting,
        Duty::Youth,
        Duty::MatchPreparation,
        Duty::StaffRenewals,
        Duty::Lineup,
        Duty::PlayerRenewals,
        Duty::PlayerRecruitment,
        Duty::Tactics,
        Duty::Offers,
    ];

    /// Days between two runs of a delegated duty.
    pub fn period_days(self) -> i64 {
        match self {
            Duty::Training
            | Duty::MatchPreparation
            | Duty::Lineup
            | Duty::Tactics
            | Duty::Offers => 1,
            Duty::Scouting | Duty::PlayerRecruitment => 3,
            Duty::Youth | Duty::StaffRecruitment | Duty::StaffRenewals | Duty::PlayerRenewals => 7,
        }
    }
}

impl Duty {
    /// Days the assistant leaves a duty alone after the manager changed what he had set: a match
    /// is a day or two away for the match duties, and a training week is seven days.
    pub fn stand_back_days(self) -> i64 {
        match self {
            Duty::Training => 7,
            _ => 3,
        }
    }
}

/// How one duty is handled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DutySetting {
    pub duty: Duty,
    /// The staff acts on this duty on its own.
    pub delegated: bool,
    /// The staff only proposes; the manager decides each major step.
    pub confirm_major: bool,
}

/// One concrete step the staff wants to take.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProposalAction {
    HireStaff {
        staff_id: String,
        wage: u32,
        years: u8,
    },
    RenewStaff {
        staff_id: String,
        wage: u32,
        years: u8,
    },
    ReleaseStaff {
        staff_id: String,
    },
    /// Renew a player's contract on the terms the assistant worked out.
    RenewPlayer {
        player_id: String,
        wage: u32,
        years: u8,
    },
    SetTraining {
        focus: TrainingFocus,
        intensity: TrainingIntensity,
    },
    SendScout {
        scout_id: String,
        player_id: String,
    },
    PromoteYouth {
        player_id: String,
    },
    RestPlayers {
        player_ids: Vec<String>,
    },
    /// Open (or carry on) a negotiation for a player the squad needs, at a fee and wage the
    /// manager has not yet allowed the assistant to commit to. Accepting it makes the bid; the
    /// clubs and the player still have their say.
    SignPlayer {
        player_id: String,
        fee: u64,
        wage: u32,
        years: u8,
    },
    /// Accept a club's offer for one of the manager's players, at the fee offered.
    AcceptOffer {
        player_id: String,
        offer_id: String,
        fee: u64,
    },
    /// Ask a club for more than it offered.
    CounterOffer {
        player_id: String,
        offer_id: String,
        fee: u64,
    },
    /// Put a player the squad does not need on the transfer list.
    ListForSale {
        player_id: String,
    },
}

/// A step waiting for the manager's answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Proposal {
    pub id: String,
    pub duty: Duty,
    pub created: String,
    pub action: ProposalAction,
    pub title_key: String,
    pub params: Vec<(String, String)>,
    pub reason_key: String,
}

/// What became of a decision in the history.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DecisionOutcome {
    /// The assistant did it within what the manager allowed.
    #[default]
    Executed,
    /// The assistant proposed it and the manager said yes.
    Approved,
    /// The assistant proposed it and the manager said no.
    Declined,
    /// The assistant could not or would not do it by himself; it is left to the manager.
    LeftToManager,
}

/// A step the staff took on its own (or one the manager answered), kept for the report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DelegationLogEntry {
    pub date: String,
    pub duty: Duty,
    pub title_key: String,
    pub params: Vec<(String, String)>,
    #[serde(default)]
    pub outcome: DecisionOutcome,
}

/// How much of the job the manager has handed over, as a whole.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DelegationMode {
    /// The manager does everything; the assistant only advises.
    Manual,
    /// **Prudent.** The assistant runs the routine and brings every major decision (a signing, a
    /// sale, a renewal) to the manager as a proposal he can read and answer.
    Approval,
    /// **Total.** The assistant manages every duty himself: recruiting, negotiating, selling,
    /// renewing, releasing, hiring and the match-day duties. His authority is the club's finances
    /// and the rules (board wage policy, affordability, registration, the player's consent) and the
    /// manager's own instructions (protected players), not a ceiling the manager sets per deal.
    Full,
    /// Duties are set one by one and match none of the three presets.
    Custom,
}

/// What the assistant may commit without asking. Every figure is a ceiling on one decision.
///
/// Anything above a ceiling is proposed to the manager instead of being done, whatever the duty's
/// own setting says.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DelegationLimits {
    /// Highest weekly wage the assistant may sign a staff member (hire or renewal) at.
    pub max_staff_wage: u32,
    /// Highest weekly wage the assistant may renew a player at.
    pub max_player_wage: u32,
    /// Highest total a single contract may commit the club to: weekly wage x 52 x years.
    pub max_commitment: i64,
    /// Highest fee the assistant may pay for a transfer. Zero: every transfer is the manager's.
    /// Read through `ofm_core::staff_system::delegation_policy::authorize`; the transfer engine
    /// is the caller.
    pub max_transfer_fee: i64,
    /// Total authority: a delegated duty that acts alone is held to what the club can carry (its
    /// cash, its wage room, the board's wage policy and the transfer rules) instead of the
    /// ceilings above, and may sell and list players. False for every state saved before it
    /// existed, so nobody is given authority they did not ask for.
    #[serde(default)]
    pub finance_bound: bool,
}

impl Default for DelegationLimits {
    fn default() -> Self {
        Self {
            max_staff_wage: 5_000,
            max_player_wage: 15_000,
            max_commitment: 3_000_000,
            max_transfer_fee: 0,
            finance_bound: false,
        }
    }
}

pub const LOG_CAPACITY: usize = 120;
pub const PROPOSAL_CAPACITY: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DelegationState {
    pub settings: Vec<DutySetting>,
    /// Duty settings before the manager temporarily takes every responsibility back. Restored on
    /// the next preset so a manual pause does not erase deliberate per-duty choices.
    #[serde(default)]
    pub suspended_settings: Option<Vec<DutySetting>>,
    #[serde(default)]
    pub proposals: Vec<Proposal>,
    #[serde(default)]
    pub log: Vec<DelegationLogEntry>,
    /// Last day each duty ran, as `Duty` -> `YYYY-MM-DD`.
    #[serde(default)]
    pub last_run: Vec<(Duty, String)>,
    #[serde(default)]
    pub next_proposal: u64,
    #[serde(default)]
    pub limits: DelegationLimits,
    /// Day the last weekly digest covered up to; entries dated after it are still unreported.
    #[serde(default)]
    pub last_digest: Option<String>,
    /// Day the manager was last told proposals were waiting, so the notice is not daily.
    #[serde(default)]
    pub last_proposal_notice: Option<String>,
    /// What the assistant has in hand on the transfer market.
    #[serde(default)]
    pub recruitment: RecruitmentMemory,
    /// What each duty last wrote (training plan, eleven, tactical plan) as a fingerprint. If the
    /// club's setting no longer matches, the manager has changed it since, and the duty stands
    /// back: the assistant never overwrites a manager's own decision.
    #[serde(default)]
    pub applied: Vec<(Duty, String)>,
    /// Duties the assistant is leaving to the manager until a day (`YYYY-MM-DD`).
    #[serde(default)]
    pub stood_back_until: Vec<(Duty, String)>,
    /// What the assistant has in hand on the selling side.
    #[serde(default)]
    pub sales: SalesMemory,
    /// Players the manager has said are not to be sold: the assistant turns down every offer for
    /// them, never lists them and never proposes their sale.
    #[serde(default)]
    pub protected_players: Vec<String>,
}

/// Most players the manager can protect at once.
pub const PROTECTED_CAPACITY: usize = 60;
/// Most offers the sales memory keeps.
pub const OFFER_RECORD_CAPACITY: usize = 60;
/// Most players the assistant has on the list at once.
pub const ASSISTANT_LISTING_CAPACITY: usize = 2;

/// What the assistant did about an offer, so that a day processed twice does nothing twice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfferRecord {
    pub offer_id: String,
    /// `YYYY-MM-DD` of the last step taken on it.
    pub date: String,
    /// Counter-offers made on it.
    #[serde(default)]
    pub counters: u8,
    /// Nothing more is to be done about it (accepted, turned down, left to the manager).
    #[serde(default)]
    pub settled: bool,
}

/// The assistant's working memory for selling: saved with the career, so a reload in the middle of
/// a negotiation carries on and nothing is done twice.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SalesMemory {
    /// Offers the assistant has acted on or left alone.
    pub offers: Vec<OfferRecord>,
    /// Players the assistant put on the list, and so may take off it. A player the manager listed
    /// himself is never here.
    pub listed: Vec<String>,
    /// Players whose sale was proposed and refused, or whom the manager took off the list again,
    /// until a day (`YYYY-MM-DD`): not raised again before then.
    pub shelved: Vec<ShelvedTarget>,
    /// Last day the assistant reviewed the squad for players to list.
    pub last_listing_review: Option<String>,
}

impl SalesMemory {
    pub fn record_for(&self, offer_id: &str) -> Option<&OfferRecord> {
        self.offers.iter().find(|r| r.offer_id == offer_id)
    }

    /// The record of an offer, created if there is none.
    pub fn record_mut(&mut self, offer_id: &str, today: &str) -> &mut OfferRecord {
        if !self.offers.iter().any(|r| r.offer_id == offer_id) {
            self.offers.push(OfferRecord {
                offer_id: offer_id.to_string(),
                date: today.to_string(),
                counters: 0,
                settled: false,
            });
            if self.offers.len() > OFFER_RECORD_CAPACITY {
                let excess = self.offers.len() - OFFER_RECORD_CAPACITY;
                self.offers.drain(..excess);
            }
        }
        let last = self.offers.len() - 1;
        let index = self
            .offers
            .iter()
            .position(|r| r.offer_id == offer_id)
            .unwrap_or(last);
        &mut self.offers[index]
    }

    pub fn is_shelved(&self, player_id: &str, today: &str) -> bool {
        self.shelved
            .iter()
            .any(|target| target.player_id == player_id && target.until.as_str() > today)
    }

    pub fn shelve(&mut self, player_id: &str, until: &str, reason: ShelveReason) {
        self.shelved.retain(|target| target.player_id != player_id);
        self.shelved.push(ShelvedTarget {
            player_id: player_id.to_string(),
            until: until.to_string(),
            reason,
        });
        if self.shelved.len() > SHELF_CAPACITY {
            let excess = self.shelved.len() - SHELF_CAPACITY;
            self.shelved.drain(..excess);
        }
    }

    pub fn forget_expired(&mut self, today: &str) {
        self.shelved.retain(|target| target.until.as_str() > today);
    }
}

/// Most targets the assistant puts aside at once, oldest dropped first.
pub const SHELF_CAPACITY: usize = 60;
/// Most negotiations the assistant has open for the manager at once.
pub const ACTIVE_APPROACH_CAPACITY: usize = 2;
/// Most approaches kept in the pacing record.
pub const APPROACH_RECORD_CAPACITY: usize = 24;

/// Why the assistant stopped pursuing a player.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ShelveReason {
    /// The manager answered a proposal for him with no.
    ManagerDeclined,
    /// A proposal for him was never answered.
    ProposalLapsed,
    /// His club would not sell at a price the assistant may pay.
    ClubRefused,
    /// The player would not join, whatever the assistant could offer.
    PlayerRefused,
    /// The deal could not be made at all (budget, board, squad list).
    NotPossible,
    /// He was signed, or the assistant dropped the idea because the squad had changed.
    Done,
}

/// A target the assistant has put aside until a date, so that the same offer is not made again
/// and again.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShelvedTarget {
    pub player_id: String,
    /// `YYYY-MM-DD`: the first day he may be looked at again.
    pub until: String,
    pub reason: ShelveReason,
}

/// A negotiation the assistant is carrying for the manager, with what he may offer in it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveApproach {
    pub player_id: String,
    pub started: String,
    /// Why he was chosen, as a position name (`Defender`...), for the report.
    pub need: String,
    /// Highest fee the assistant is willing to pay for him, by his own judgement of the price.
    pub max_fee: u64,
    /// The fee of the next bid: his opening offer, then what the selling club asked for.
    #[serde(default)]
    pub next_fee: u64,
    /// The wage the player has asked for, once he has asked. Zero: the buyer's standard.
    #[serde(default)]
    pub wage: u32,
    /// The length he has asked for, in years. Zero: the standard.
    #[serde(default)]
    pub years: u8,
    /// Bids made so far.
    #[serde(default)]
    pub rounds: u8,
    /// The slot of the side he is to fill (`CenterBack`, `Striker`...), for the report.
    #[serde(default)]
    pub role: String,
    /// What he adds to the side, in rating points across the eleven, as the assistant read it
    /// when he chose him.
    #[serde(default)]
    pub gain: i32,
    /// Why he was chosen: `starter`, `depth` or `succession`.
    #[serde(default)]
    pub reason: String,
}

/// A bid the assistant made, kept for pacing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApproachRecord {
    pub player_id: String,
    pub date: String,
}

/// The assistant's working memory for [`Duty::PlayerRecruitment`]. Saved with the career, so a
/// reload in the middle of a negotiation carries on, and a season rollover forgets nothing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RecruitmentMemory {
    pub shelved: Vec<ShelvedTarget>,
    pub active: Vec<ActiveApproach>,
    pub approaches: Vec<ApproachRecord>,
}

impl RecruitmentMemory {
    /// Whether `player_id` is set aside on `today` (`YYYY-MM-DD`).
    pub fn is_shelved(&self, player_id: &str, today: &str) -> bool {
        self.shelved
            .iter()
            .any(|target| target.player_id == player_id && target.until.as_str() > today)
    }

    /// Sets a target aside until `until`, replacing an earlier entry for him.
    pub fn shelve(&mut self, player_id: &str, until: &str, reason: ShelveReason) {
        self.shelved.retain(|target| target.player_id != player_id);
        self.shelved.push(ShelvedTarget {
            player_id: player_id.to_string(),
            until: until.to_string(),
            reason,
        });
        if self.shelved.len() > SHELF_CAPACITY {
            let excess = self.shelved.len() - SHELF_CAPACITY;
            self.shelved.drain(..excess);
        }
    }

    /// Drops what has expired.
    pub fn forget_expired(&mut self, today: &str) {
        self.shelved.retain(|target| target.until.as_str() > today);
    }

    pub fn is_active(&self, player_id: &str) -> bool {
        self.active.iter().any(|a| a.player_id == player_id)
    }

    /// Stores the state of a negotiation, replacing the earlier one for the same player.
    pub fn replace(&mut self, approach: ActiveApproach) {
        match self
            .active
            .iter_mut()
            .find(|a| a.player_id == approach.player_id)
        {
            Some(existing) => *existing = approach,
            None => self.active.push(approach),
        }
    }

    pub fn finish(&mut self, player_id: &str) {
        self.active.retain(|a| a.player_id != player_id);
    }

    pub fn record_approach(&mut self, player_id: &str, date: &str) {
        self.approaches.push(ApproachRecord {
            player_id: player_id.to_string(),
            date: date.to_string(),
        });
        if self.approaches.len() > APPROACH_RECORD_CAPACITY {
            let excess = self.approaches.len() - APPROACH_RECORD_CAPACITY;
            self.approaches.drain(..excess);
        }
    }

    /// Approaches begun on or after `since` (`YYYY-MM-DD`).
    pub fn approaches_since(&self, since: &str) -> usize {
        self.approaches
            .iter()
            .filter(|a| a.date.as_str() >= since)
            .count()
    }
}

impl Default for DelegationState {
    fn default() -> Self {
        Self {
            settings: Duty::ALL
                .iter()
                .map(|duty| DutySetting {
                    duty: *duty,
                    delegated: false,
                    confirm_major: true,
                })
                .collect(),
            suspended_settings: None,
            proposals: Vec::new(),
            log: Vec::new(),
            last_run: Vec::new(),
            next_proposal: 0,
            limits: DelegationLimits::default(),
            last_digest: None,
            last_proposal_notice: None,
            recruitment: RecruitmentMemory::default(),
            applied: Vec::new(),
            stood_back_until: Vec::new(),
            sales: SalesMemory::default(),
            protected_players: Vec::new(),
        }
    }
}

/// Which preset a set of duty settings amounts to.
fn mode_of(settings: &[DutySetting]) -> DelegationMode {
    if !settings.iter().any(|s| s.delegated) {
        return DelegationMode::Manual;
    }
    let setting = |duty: Duty| settings.iter().find(|s| s.duty == duty);
    let every_duty = Duty::ALL
        .iter()
        .all(|d| setting(*d).is_some_and(|s| s.delegated));
    let delegated = || settings.iter().filter(|s| s.delegated);
    let asks = delegated().all(|s| s.confirm_major);
    let acts = delegated().all(|s| !s.confirm_major);
    match (every_duty, asks, acts) {
        (true, true, _) => DelegationMode::Approval,
        (true, _, true) => DelegationMode::Full,
        _ => DelegationMode::Custom,
    }
}

impl DelegationState {
    /// Whether the manager has said `player_id` is not to be sold.
    pub fn is_protected(&self, player_id: &str) -> bool {
        self.protected_players.iter().any(|id| id == player_id)
    }

    /// Protects (or stops protecting) a player. Protecting beyond [`PROTECTED_CAPACITY`] drops the
    /// oldest, so the list never grows without end.
    pub fn set_protected(&mut self, player_id: &str, protected: bool) {
        self.protected_players.retain(|id| id != player_id);
        if protected {
            self.protected_players.push(player_id.to_string());
            if self.protected_players.len() > PROTECTED_CAPACITY {
                let excess = self.protected_players.len() - PROTECTED_CAPACITY;
                self.protected_players.drain(..excess);
            }
        }
    }

    pub fn setting(&self, duty: Duty) -> Option<&DutySetting> {
        self.settings.iter().find(|s| s.duty == duty)
    }

    /// Whether the staff acts on `duty` by itself.
    pub fn is_delegated(&self, duty: Duty) -> bool {
        self.setting(duty).is_some_and(|s| s.delegated)
    }

    /// Whether the manager wants to approve each major step of `duty`.
    pub fn needs_confirmation(&self, duty: Duty) -> bool {
        self.setting(duty).is_none_or(|s| s.confirm_major)
    }

    pub fn set(&mut self, duty: Duty, delegated: bool, confirm_major: bool) {
        if delegated && !self.is_delegated(duty) {
            // Handing a duty over again is the manager saying "yours": no memory of an old
            // override survives it.
            self.applied.retain(|(d, _)| *d != duty);
            self.stood_back_until.retain(|(d, _)| *d != duty);
        }
        match self.settings.iter_mut().find(|s| s.duty == duty) {
            Some(setting) => {
                setting.delegated = delegated;
                setting.confirm_major = confirm_major;
            }
            None => self.settings.push(DutySetting {
                duty,
                delegated,
                confirm_major,
            }),
        }
    }

    /// Gives a duty that a saved state predates the setting of the closest older duty, so a state
    /// that was "Full" is still "Full" and one that was "Manual" delegates nothing. Run when a
    /// state is loaded.
    pub fn complete_settings(&mut self) {
        for duty in Duty::ALL {
            if self.setting(duty).is_some() {
                continue;
            }
            let (delegated, confirm_major) = self
                .setting(Duty::Lineup)
                .map_or((false, true), |s| (s.delegated, s.confirm_major));
            self.set(duty, delegated, confirm_major);
        }
    }

    /// The fingerprint of what `duty` last wrote.
    pub fn applied(&self, duty: Duty) -> Option<&str> {
        self.applied
            .iter()
            .find(|(d, _)| *d == duty)
            .map(|(_, fingerprint)| fingerprint.as_str())
    }

    pub fn record_applied(&mut self, duty: Duty, fingerprint: &str) {
        match self.applied.iter_mut().find(|(d, _)| *d == duty) {
            Some(entry) => entry.1 = fingerprint.to_string(),
            None => self.applied.push((duty, fingerprint.to_string())),
        }
    }

    pub fn forget_applied(&mut self, duty: Duty) {
        self.applied.retain(|(d, _)| *d != duty);
    }

    /// Leaves `duty` to the manager until `until` (`YYYY-MM-DD`).
    pub fn stand_back(&mut self, duty: Duty, until: &str) {
        match self.stood_back_until.iter_mut().find(|(d, _)| *d == duty) {
            Some(entry) => entry.1 = until.to_string(),
            None => self.stood_back_until.push((duty, until.to_string())),
        }
    }

    /// Whether the assistant is leaving `duty` to the manager on `today`.
    pub fn is_standing_back(&self, duty: Duty, today: &str) -> bool {
        self.stood_back_until
            .iter()
            .any(|(d, until)| *d == duty && until.as_str() > today)
    }

    /// Which preset the duty settings amount to. Derived, never stored, so it cannot disagree
    /// with the settings it summarises.
    pub fn mode(&self) -> DelegationMode {
        mode_of(&self.settings)
    }

    /// Applies a preset to every duty. A temporary manual pause retains the prior individual
    /// choices so the next preset resumes them. `Custom` is a description, not a preset.
    pub fn apply_mode(&mut self, mode: DelegationMode) {
        if mode == DelegationMode::Custom {
            return;
        }
        if mode == DelegationMode::Manual {
            if self.mode() != DelegationMode::Manual {
                self.suspended_settings = Some(self.settings.clone());
            }
            for duty in Duty::ALL {
                self.set(duty, false, true);
            }
            return;
        }
        // Resuming after a pause restores what was there: the manager's own per-duty choices, or
        // the preset he is asking for again. A different preset than the one paused is a new
        // choice and is applied as one (pausing Total and then picking Prudent gives Prudent).
        if let Some(settings) = self.suspended_settings.take() {
            let paused = mode_of(&settings);
            if paused == DelegationMode::Custom || paused == mode {
                self.settings = settings;
                if paused == mode {
                    self.limits.finance_bound = mode == DelegationMode::Full;
                }
                return;
            }
        }
        let confirm_major = match mode {
            DelegationMode::Approval => true,
            DelegationMode::Full => false,
            DelegationMode::Manual | DelegationMode::Custom => return,
        };
        // Total authority comes with the Total preset and goes with any other: choosing Prudent
        // takes it back.
        self.limits.finance_bound = mode == DelegationMode::Full;
        for duty in Duty::ALL {
            self.set(duty, true, confirm_major);
        }
    }

    pub fn last_run_on(&self, duty: Duty) -> Option<&str> {
        self.last_run
            .iter()
            .find(|(d, _)| *d == duty)
            .map(|(_, date)| date.as_str())
    }

    pub fn mark_run(&mut self, duty: Duty, date: &str) {
        match self.last_run.iter_mut().find(|(d, _)| *d == duty) {
            Some(entry) => entry.1 = date.to_string(),
            None => self.last_run.push((duty, date.to_string())),
        }
    }

    pub fn push_log(&mut self, entry: DelegationLogEntry) {
        self.log.push(entry);
        if self.log.len() > LOG_CAPACITY {
            let excess = self.log.len() - LOG_CAPACITY;
            self.log.drain(..excess);
        }
    }

    /// Queues a proposal unless an identical step is already waiting. Returns whether it was
    /// queued.
    pub fn propose(
        &mut self,
        duty: Duty,
        date: &str,
        action: ProposalAction,
        title_key: &str,
        params: Vec<(String, String)>,
        reason_key: &str,
    ) -> bool {
        if self.proposals.iter().any(|p| p.action == action)
            || self.proposals.len() >= PROPOSAL_CAPACITY
        {
            return false;
        }
        self.next_proposal += 1;
        self.proposals.push(Proposal {
            id: format!("prop-{}", self.next_proposal),
            duty,
            created: date.to_string(),
            action,
            title_key: title_key.to_string(),
            params,
            reason_key: reason_key.to_string(),
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_delegated_by_default_and_the_manager_confirms_everything() {
        let state = DelegationState::default();
        for duty in Duty::ALL {
            assert!(!state.is_delegated(duty));
            assert!(state.needs_confirmation(duty));
        }
    }

    #[test]
    fn an_identical_proposal_is_not_queued_twice() {
        let mut state = DelegationState::default();
        let action = ProposalAction::ReleaseStaff {
            staff_id: "s1".into(),
        };
        assert!(state.propose(
            Duty::StaffRecruitment,
            "2026-08-01",
            action.clone(),
            "t",
            vec![],
            "r"
        ));
        assert!(!state.propose(
            Duty::StaffRecruitment,
            "2026-08-02",
            action,
            "t",
            vec![],
            "r"
        ));
        assert_eq!(state.proposals.len(), 1);
    }

    #[test]
    fn the_log_keeps_only_the_latest_entries() {
        let mut state = DelegationState::default();
        for i in 0..(LOG_CAPACITY + 5) {
            state.push_log(DelegationLogEntry {
                date: format!("d{i}"),
                duty: Duty::Training,
                title_key: "t".into(),
                params: vec![],
                outcome: DecisionOutcome::Executed,
            });
        }
        assert_eq!(state.log.len(), LOG_CAPACITY);
        assert_eq!(state.log[0].date, "d5");
    }

    /// Given a fresh state, when each preset is applied, then `mode()` reads it back; editing one
    /// duty afterwards makes the state `Custom` rather than pretending to be a preset.
    #[test]
    fn the_mode_is_derived_from_the_duty_settings() {
        let mut state = DelegationState::default();
        assert_eq!(state.mode(), DelegationMode::Manual);
        state.apply_mode(DelegationMode::Approval);
        assert_eq!(state.mode(), DelegationMode::Approval);
        state.apply_mode(DelegationMode::Full);
        assert_eq!(state.mode(), DelegationMode::Full);
        assert!(Duty::ALL.iter().all(|d| state.is_delegated(*d)));
        assert!(Duty::ALL.iter().all(|d| !state.needs_confirmation(*d)));
        state.set(Duty::PlayerRenewals, true, true);
        assert_eq!(state.mode(), DelegationMode::Custom);
        state.apply_mode(DelegationMode::Manual);
        assert_eq!(state.mode(), DelegationMode::Manual);
    }

    /// Given individual delegation choices, when manual mode is only a temporary pause and full
    /// delegation is selected again, then the original choices are restored unchanged.
    #[test]
    fn a_temporary_manual_pause_preserves_individual_duty_settings() {
        let mut state = DelegationState::default();
        state.set(Duty::Training, true, false);
        state.set(Duty::StaffRenewals, true, true);
        let before = state.settings.clone();

        state.apply_mode(DelegationMode::Manual);
        state.apply_mode(DelegationMode::Full);

        assert_eq!(state.settings, before);
    }

    /// Given a state saved before modes, limits and outcomes existed, when it is read, then it
    /// loads with the defaults and keeps what the manager had delegated.
    #[test]
    fn a_state_from_before_the_assistant_coach_still_loads() {
        let old = r#"{"settings":[{"duty":"Training","delegated":true,"confirm_major":false}],
            "log":[{"date":"2026-08-01","duty":"Training","title_key":"k","params":[]}]}"#;
        let state: DelegationState = serde_json::from_str(old).unwrap();
        assert!(state.is_delegated(Duty::Training));
        assert_eq!(state.limits, DelegationLimits::default());
        assert_eq!(state.log[0].outcome, DecisionOutcome::Executed);
        assert_eq!(state.mode(), DelegationMode::Custom);
    }

    /// Given a "Full" state saved before the tactical duty existed
    /// When it is completed
    /// Then the new duty follows the closest older one and the state is still "Full".
    #[test]
    fn a_full_state_from_before_the_tactics_duty_is_still_full() {
        let mut state = DelegationState::default();
        state.apply_mode(DelegationMode::Full);
        state
            .settings
            .retain(|s| !matches!(s.duty, Duty::Tactics | Duty::Offers));
        assert_eq!(
            state.mode(),
            DelegationMode::Custom,
            "the gap shows as Custom"
        );

        state.complete_settings();

        assert_eq!(state.mode(), DelegationMode::Full);
        assert!(state.is_delegated(Duty::Tactics) && state.is_delegated(Duty::Offers));
    }

    /// Given a manual state saved before the duty existed
    /// When it is completed
    /// Then nothing becomes delegated.
    #[test]
    fn completing_a_manual_state_delegates_nothing() {
        let mut state = DelegationState::default();
        state.settings.retain(|s| s.duty != Duty::Tactics);
        state.complete_settings();
        assert_eq!(state.mode(), DelegationMode::Manual);
    }

    /// Given the assistant stands back from a duty
    /// When the stand-back date passes
    /// Then he acts again; until then he does not.
    #[test]
    fn standing_back_lasts_until_its_date() {
        let mut state = DelegationState::default();
        state.stand_back(Duty::Lineup, "2026-08-10");
        assert!(state.is_standing_back(Duty::Lineup, "2026-08-09"));
        assert!(!state.is_standing_back(Duty::Lineup, "2026-08-10"));
        assert!(!state.is_standing_back(Duty::Training, "2026-08-09"));
    }

    /// Given the assistant stood back from a duty and applied settings earlier
    /// When the manager hands the duty over again
    /// Then the stand-back and the remembered settings are forgotten: it is the assistant's again.
    #[test]
    fn delegating_a_duty_again_takes_it_back_from_the_manager() {
        let mut state = DelegationState::default();
        state.set(Duty::Training, true, false);
        state.record_applied(Duty::Training, "Physical|High");
        state.stand_back(Duty::Training, "2026-08-20");
        state.set(Duty::Training, false, false);
        state.set(Duty::Training, true, false);
        assert!(!state.is_standing_back(Duty::Training, "2026-08-01"));
        assert_eq!(state.applied(Duty::Training), None);
    }

    /// Given a state saved before overrides were tracked
    /// When it is read
    /// Then it loads with nothing applied and nobody standing back.
    #[test]
    fn a_state_from_before_overrides_loads_with_none() {
        let old = r#"{"settings":[]}"#;
        let state: DelegationState = serde_json::from_str(old).unwrap();
        assert!(state.applied.is_empty() && state.stood_back_until.is_empty());
    }

    /// Given a state saved before sales authority, protected players and the sales memory existed
    /// When it is read
    /// Then nothing is protected, the assistant has no authority to sell and its memory is empty.
    #[test]
    fn a_state_from_before_total_authority_gives_none() {
        let old = r#"{"settings":[{"duty":"Offers","delegated":true,"confirm_major":false}],
            "limits":{"max_staff_wage":5000,"max_player_wage":15000,"max_commitment":3000000,
            "max_transfer_fee":0}}"#;
        let state: DelegationState = serde_json::from_str(old).unwrap();
        assert!(!state.limits.finance_bound);
        assert!(state.protected_players.is_empty());
        assert_eq!(state.sales, SalesMemory::default());
        assert!(state.is_delegated(Duty::Offers) && !state.needs_confirmation(Duty::Offers));
    }

    /// Given each preset
    /// When it is applied
    /// Then Total (Full) carries authority over the club's finances and Prudent (Approval) takes it
    /// back; a manual pause does not change it.
    #[test]
    fn only_the_total_preset_carries_financial_authority() {
        let mut state = DelegationState::default();
        state.apply_mode(DelegationMode::Full);
        assert!(state.limits.finance_bound);
        state.apply_mode(DelegationMode::Approval);
        assert!(!state.limits.finance_bound);
        assert_eq!(state.mode(), DelegationMode::Approval);
        state.apply_mode(DelegationMode::Full);
        state.apply_mode(DelegationMode::Manual);
        assert!(state.limits.finance_bound, "pausing is not revoking");
    }

    /// Given Total paused for a while
    /// When the manager then picks Prudent
    /// Then he gets Prudent: choosing a different preset is a new choice, not a resume.
    #[test]
    fn pausing_total_and_picking_prudent_gives_prudent() {
        let mut state = DelegationState::default();
        state.apply_mode(DelegationMode::Full);
        state.apply_mode(DelegationMode::Manual);
        state.apply_mode(DelegationMode::Approval);
        assert_eq!(state.mode(), DelegationMode::Approval);
        assert!(!state.limits.finance_bound);
        assert!(Duty::ALL.iter().all(|d| state.needs_confirmation(*d)));
    }

    /// Given Total paused and then picked again
    /// When it is resumed
    /// Then it is Total again, with its authority.
    #[test]
    fn resuming_total_restores_total() {
        let mut state = DelegationState::default();
        state.apply_mode(DelegationMode::Full);
        state.apply_mode(DelegationMode::Manual);
        state.apply_mode(DelegationMode::Full);
        assert_eq!(state.mode(), DelegationMode::Full);
        assert!(state.limits.finance_bound);
    }

    /// Given a protected player
    /// When protection is set, repeated and withdrawn
    /// Then the list holds him once and then not at all, and never outgrows its capacity.
    #[test]
    fn protecting_a_player_is_idempotent_and_bounded() {
        let mut state = DelegationState::default();
        state.set_protected("p1", true);
        state.set_protected("p1", true);
        assert_eq!(state.protected_players, vec!["p1".to_string()]);
        assert!(state.is_protected("p1"));
        state.set_protected("p1", false);
        assert!(!state.is_protected("p1"));
        for i in 0..PROTECTED_CAPACITY + 10 {
            state.set_protected(&format!("p{i}"), true);
        }
        assert_eq!(state.protected_players.len(), PROTECTED_CAPACITY);
        assert!(
            !state.is_protected("p0")
                && state.is_protected(&format!("p{}", PROTECTED_CAPACITY + 9))
        );
    }

    /// Given the sales memory asked about the same offer twice
    /// When its record is read each time
    /// Then it is one record, and a shelved player is back after his date.
    #[test]
    fn the_sales_memory_keeps_one_record_per_offer_and_forgets_on_time() {
        let mut memory = SalesMemory::default();
        memory.record_mut("o1", "2026-08-01").counters = 1;
        assert_eq!(memory.record_mut("o1", "2026-08-02").counters, 1);
        assert_eq!(memory.offers.len(), 1);
        memory.shelve("p1", "2026-09-01", ShelveReason::ManagerDeclined);
        assert!(memory.is_shelved("p1", "2026-08-31"));
        assert!(!memory.is_shelved("p1", "2026-09-01"));
        memory.forget_expired("2026-09-01");
        assert!(memory.shelved.is_empty());
    }

    /// Given a negotiation saved in the middle, with what the assistant had worked out
    /// When it is serialised and read back
    /// Then every figure and the reason are kept: a reload carries the negotiation on.
    #[test]
    fn an_open_negotiation_survives_a_reload_with_its_reasons() {
        let mut state = DelegationState::default();
        state.recruitment.replace(ActiveApproach {
            player_id: "p9".into(),
            started: "2026-08-01".into(),
            need: "Forward".into(),
            max_fee: 9_000_000,
            next_fee: 8_100_000,
            wage: 20_000,
            years: 3,
            rounds: 2,
            role: "Striker".into(),
            gain: 14,
            reason: "starter".into(),
        });
        state.sales.record_mut("o7", "2026-08-02").counters = 2;
        state.set_protected("keeper", true);
        let again: DelegationState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(again, state);
    }
}
