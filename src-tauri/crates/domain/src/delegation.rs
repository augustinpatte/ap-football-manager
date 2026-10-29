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
}

impl Duty {
    pub const ALL: [Duty; 6] = [
        Duty::Training,
        Duty::StaffRecruitment,
        Duty::Scouting,
        Duty::Youth,
        Duty::MatchPreparation,
        Duty::StaffRenewals,
    ];

    /// Days between two runs of a delegated duty.
    pub fn period_days(self) -> i64 {
        match self {
            Duty::Training | Duty::MatchPreparation => 1,
            Duty::Scouting => 3,
            Duty::Youth | Duty::StaffRecruitment | Duty::StaffRenewals => 7,
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

/// A step the staff took on its own (or one the manager confirmed), kept for the report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DelegationLogEntry {
    pub date: String,
    pub duty: Duty,
    pub title_key: String,
    pub params: Vec<(String, String)>,
}

pub const LOG_CAPACITY: usize = 60;
pub const PROPOSAL_CAPACITY: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DelegationState {
    pub settings: Vec<DutySetting>,
    #[serde(default)]
    pub proposals: Vec<Proposal>,
    #[serde(default)]
    pub log: Vec<DelegationLogEntry>,
    /// Last day each duty ran, as `Duty` -> `YYYY-MM-DD`.
    #[serde(default)]
    pub last_run: Vec<(Duty, String)>,
    #[serde(default)]
    pub next_proposal: u64,
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
            proposals: Vec::new(),
            log: Vec::new(),
            last_run: Vec::new(),
            next_proposal: 0,
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
            });
        }
        assert_eq!(state.log.len(), LOG_CAPACITY);
        assert_eq!(state.log[0].date, "d5");
    }
}
