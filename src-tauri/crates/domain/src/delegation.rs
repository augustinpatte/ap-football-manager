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
}

impl Duty {
    pub const ALL: [Duty; 8] = [
        Duty::Training,
        Duty::StaffRecruitment,
        Duty::Scouting,
        Duty::Youth,
        Duty::MatchPreparation,
        Duty::StaffRenewals,
        Duty::Lineup,
        Duty::PlayerRenewals,
    ];

    /// Days between two runs of a delegated duty.
    pub fn period_days(self) -> i64 {
        match self {
            Duty::Training | Duty::MatchPreparation | Duty::Lineup => 1,
            Duty::Scouting => 3,
            Duty::Youth | Duty::StaffRecruitment | Duty::StaffRenewals | Duty::PlayerRenewals => 7,
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
    /// The assistant runs the routine and brings every major decision to the manager.
    Approval,
    /// The assistant runs everything within the manager's spending limits.
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
}

impl Default for DelegationLimits {
    fn default() -> Self {
        Self {
            max_staff_wage: 5_000,
            max_player_wage: 15_000,
            max_commitment: 3_000_000,
            max_transfer_fee: 0,
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
        }
    }
}

impl DelegationState {
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

    /// Which preset the duty settings amount to. Derived, never stored, so it cannot disagree
    /// with the settings it summarises.
    pub fn mode(&self) -> DelegationMode {
        let delegated = |s: &&DutySetting| s.delegated;
        if !self.settings.iter().any(|s| s.delegated) {
            return DelegationMode::Manual;
        }
        let every_duty = Duty::ALL
            .iter()
            .all(|d| self.setting(*d).is_some_and(|s| s.delegated));
        let asks = self
            .settings
            .iter()
            .filter(delegated)
            .all(|s| s.confirm_major);
        let acts = self
            .settings
            .iter()
            .filter(delegated)
            .all(|s| !s.confirm_major);
        match (every_duty, asks, acts) {
            (true, true, _) => DelegationMode::Approval,
            (true, _, true) => DelegationMode::Full,
            _ => DelegationMode::Custom,
        }
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
        if let Some(settings) = self.suspended_settings.take() {
            self.settings = settings;
            return;
        }
        let confirm_major = match mode {
            DelegationMode::Approval => true,
            DelegationMode::Full => false,
            DelegationMode::Manual | DelegationMode::Custom => return,
        };
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
}
