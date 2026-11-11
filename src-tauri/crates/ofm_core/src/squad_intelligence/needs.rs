//! What a squad is short of, in order.
//!
//! A need is a place where the side is weaker than the club's own standard, measured in expected
//! rating points lost. Three kinds, because a squad can be short in three different ways:
//!
//! * **Starter** — a slot whose first choice is clearly below the level of the rest of the eleven
//!   (a club with excellent forwards and poor centre-backs has a need at centre-back, not in
//!   attack);
//! * **Cover** — a slot whose replacement, when its starter is absent, is far below the standard
//!   (a squad of fourteen reliable seniors has several of these);
//! * **Succession** — a slot whose first choice is expected to be worse, or gone, in a year or two
//!   (a striker nearing retirement).
//!
//! Needs are read from the formation the club plays, slot by slot, so a change of formation or the
//! loss of a player changes them at once; nothing is cached across a squad change.

use domain::player::Position;

use super::model::{Evaluation, SLOTS, SquadModel};

/// Rating points a bench player is expected to be below the eleven's own standard before the club
/// calls the cover weak.
const COVER_ALLOWANCE: f32 = 10.0;

/// Needs smaller than this many expected points are ordinary variation, not worth a signing.
const MIN_SEVERITY: f32 = 0.5;

/// How a slot falls short.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NeedKind {
    Starter,
    Cover,
    Succession,
}

/// One place a squad is short.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Need {
    pub kind: NeedKind,
    pub slot: usize,
    pub position: Position,
    /// Broad group, 0 goalkeeper to 3 forward.
    pub group: usize,
    /// Expected rating points lost against the club's own standard.
    pub severity: f32,
}

/// The squad's needs, most severe first (ties by slot, then kind, so the order is stable).
pub(crate) fn needs(model: &SquadModel, evaluation: &Evaluation) -> Vec<Need> {
    let now = &evaluation.years[0];
    let standard = now.starter.iter().sum::<f32>() / SLOTS as f32;
    let future_weight = model.weights[1] + model.weights[2];
    let mut found = Vec::new();
    for slot in 0..SLOTS {
        let position = &model.slots[slot];
        let group = crate::squad_floor::group_index(position);
        let absence = now.absence[slot];
        let starter_gap = (1.0 - absence) * (standard - now.starter[slot]);
        let cover_gap = absence * (standard - COVER_ALLOWANCE - now.cover[slot]);
        let succession_gap = if future_weight > 0.0 {
            let later = (model.weights[1] * evaluation.years[1].starter[slot]
                + model.weights[2] * evaluation.years[2].starter[slot])
                / future_weight;
            (1.0 - absence) * (now.starter[slot] - later)
        } else {
            0.0
        };
        for (kind, severity) in [
            (NeedKind::Starter, starter_gap),
            (NeedKind::Cover, cover_gap),
            (NeedKind::Succession, succession_gap),
        ] {
            if severity >= MIN_SEVERITY {
                found.push(Need {
                    kind,
                    slot,
                    position: position.clone(),
                    group,
                    severity,
                });
            }
        }
    }
    found.sort_by(|a, b| {
        b.severity
            .total_cmp(&a.severity)
            .then(a.slot.cmp(&b.slot))
            .then((a.kind as u8).cmp(&(b.kind as u8)))
    });
    found
}
