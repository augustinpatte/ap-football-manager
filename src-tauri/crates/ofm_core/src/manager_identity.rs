//! Who a manager is, as opposed to how good he is and as opposed to what his club stands for.
//!
//! An AI manager used to be a rating: `Manager::rating` fed the rest threshold and the misjudgement
//! of a team sheet, and nothing else about him mattered, so every manager of the same rating made the
//! same decisions and a club's play was its badge's alone. A manager's *identity* is four dispositions
//! that stay with him from club to club:
//!
//! - **risk**: how far he leans away from his club's blueprint towards boldness (a higher line, more
//!   pressing, faster breaks) or caution (a deeper block, less pressing).
//! - **adaptability**: how quickly a run of results moves him to change the plan.
//! - **rotation**: how readily he rests a tired player.
//! - **youth trust**: how much he builds around young players, over what his club's academy and age
//!   structure already suggest.
//!
//! The club's own philosophy (its play style, its squad's age, its academy) is a different thing and
//! lives with the club; a manager *tilts* it. A dismissed manager takes his disposition to the next
//! club, which is the point of calling it his.
//!
//! # Derived, not stored
//!
//! Each disposition is a pure function of the manager's id, drawn uniformly from 0 to 1, so the world's
//! average manager is the neutral one and no club category is hardcoded. Nothing is saved: a save
//! made before this module existed, and a reload of one made after it, give every manager the same
//! identity, and fast-forward cannot disagree with day-by-day play about who a manager is. The price
//! is that an identity does not develop over a career; what develops is what he does with it, because
//! every decision reads it together with the squad, the results and the club's means.

use crate::stable_hash::stable_hash;

/// MurmurHash3's 64-bit finaliser. `stable_hash` folds its seed into the start of the hash, so two
/// seeds over the same id give outputs that move together (measured here at r = 0.96); mixing the
/// output makes each salt's draw independent of the others.
fn finalise(mut hash: u64) -> u64 {
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 33)
}

/// Four dispositions, each from 0 to 1, 0.5 being the neutral manager.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ManagerIdentity {
    pub risk: f64,
    pub adaptability: f64,
    pub rotation: f64,
    pub youth_trust: f64,
}

// Salts distinct per disposition, so one id's four draws are independent of each other.
const RISK_SALT: u64 = 0x6d67_7220_7269_736b; // "mgr risk"
const ADAPTABILITY_SALT: u64 = 0x6d67_7220_6164_6170; // "mgr adap"
const ROTATION_SALT: u64 = 0x6d67_7220_726f_7461; // "mgr rota"
const YOUTH_SALT: u64 = 0x6d67_7220_796f_7574; // "mgr yout"

impl ManagerIdentity {
    /// The manager nothing is known about, who changes nothing: what a club with no manager gets,
    /// and what the user's assistant plans with.
    pub const NEUTRAL: ManagerIdentity = ManagerIdentity {
        risk: 0.5,
        adaptability: 0.5,
        rotation: 0.5,
        youth_trust: 0.5,
    };

    /// The identity of the manager with this id: the same on every call, in every save.
    pub fn of(manager_id: &str) -> Self {
        let draw = |salt: u64| {
            let hash = finalise(stable_hash(manager_id.as_bytes(), salt));
            (hash >> 11) as f64 / (1u64 << 53) as f64
        };
        Self {
            risk: draw(RISK_SALT),
            adaptability: draw(ADAPTABILITY_SALT),
            rotation: draw(ROTATION_SALT),
            youth_trust: draw(YOUTH_SALT),
        }
    }

    /// The identity of a club's manager, neutral for a club with none.
    pub fn of_club_manager(manager_id: Option<&str>) -> Self {
        manager_id.map_or(Self::NEUTRAL, Self::of)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(count: usize) -> Vec<ManagerIdentity> {
        (0..count)
            .map(|index| ManagerIdentity::of(&format!("manager-{index}")))
            .collect()
    }

    /// Given a manager id
    /// When his identity is read twice, and a different id's once
    /// Then the same id gives the same identity and a different id a different one.
    #[test]
    fn an_identity_is_the_same_every_time_and_differs_between_managers() {
        assert_eq!(ManagerIdentity::of("a"), ManagerIdentity::of("a"));
        assert_ne!(ManagerIdentity::of("a"), ManagerIdentity::of("b"));
    }

    /// Given five hundred managers
    /// When each disposition is looked at across them
    /// Then each spreads over the whole range and the world's average manager is the neutral one,
    /// so the identities tilt individual clubs without moving the league.
    #[test]
    fn dispositions_spread_across_the_range_around_a_neutral_mean() {
        let managers = world(500);
        let columns: [(&str, Vec<f64>); 4] = [
            ("risk", managers.iter().map(|m| m.risk).collect()),
            (
                "adaptability",
                managers.iter().map(|m| m.adaptability).collect(),
            ),
            ("rotation", managers.iter().map(|m| m.rotation).collect()),
            ("youth", managers.iter().map(|m| m.youth_trust).collect()),
        ];
        for (name, values) in &columns {
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            assert!((mean - 0.5).abs() < 0.05, "{name} mean {mean}");
            for quarter in 0..4 {
                let low = f64::from(quarter) / 4.0;
                let count = values
                    .iter()
                    .filter(|v| **v >= low && **v < low + 0.25)
                    .count();
                assert!(count > 80, "{name} quarter {quarter} holds only {count}");
            }
        }
    }

    /// Given five hundred managers
    /// When one disposition is set against another
    /// Then they are not tied together (a bold manager is no more or less likely to rotate), so
    /// the combinations that make a recognisable character all occur.
    #[test]
    fn dispositions_are_independent_of_each_other() {
        let managers = world(500);
        let correlation = |a: &dyn Fn(&ManagerIdentity) -> f64,
                           b: &dyn Fn(&ManagerIdentity) -> f64| {
            let n = managers.len() as f64;
            let (ma, mb) = (
                managers.iter().map(a).sum::<f64>() / n,
                managers.iter().map(b).sum::<f64>() / n,
            );
            let covariance = managers
                .iter()
                .map(|m| (a(m) - ma) * (b(m) - mb))
                .sum::<f64>();
            let (va, vb) = (
                managers.iter().map(|m| (a(m) - ma).powi(2)).sum::<f64>(),
                managers.iter().map(|m| (b(m) - mb).powi(2)).sum::<f64>(),
            );
            covariance / (va * vb).sqrt()
        };
        let fields: [&dyn Fn(&ManagerIdentity) -> f64; 4] =
            [&|m| m.risk, &|m| m.adaptability, &|m| m.rotation, &|m| {
                m.youth_trust
            }];
        for first in 0..fields.len() {
            for second in first + 1..fields.len() {
                let r = correlation(fields[first], fields[second]);
                assert!(
                    r.abs() < 0.15,
                    "dispositions {first} and {second} correlate at {r}"
                );
            }
        }
    }
}
