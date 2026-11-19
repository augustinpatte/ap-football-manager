//! What the manager's club has learnt by scouting: the reports it holds on players, and the
//! shortlist of players it is following.
//!
//! A scout does not hand the manager a player's hidden numbers. He hands over an *estimate*: a
//! range for the player's current ability and, when he is good enough at judging potential, a wider
//! one for his potential. The true value always lies inside a range (that is what makes it a
//! range and not a guess), but the middle of the range is not the truth: the scout's error moves
//! the centre by up to 60 % of the half-width either way. A better scout gives a narrower range;
//! a second report on the same player in the same season narrows it again, because both ranges
//! contain the truth and the club keeps what they have in common.
//!
//! Nothing here stores a player's true potential or true attributes: the report holds the
//! estimate and the snapshot of attributes *as the scout saw them*, so a screen reading the
//! dossier cannot show more than the club has earned.

use crate::game::{Game, ScoutedPlayer};
use domain::player::{Player, PlayerAttributes};
use rand::RngExt;
use serde::{Deserialize, Serialize};

const ERR_PLAYER_NOT_FOUND: &str = "be.error.playerNotFound";
const ERR_SHORTLIST_OWN_PLAYER: &str = "be.error.scouting.shortlistOwnPlayer";
const ERR_SHORTLIST_ALREADY: &str = "be.error.scouting.shortlistAlready";
const ERR_SHORTLIST_FULL: &str = "be.error.scouting.shortlistFull";
const ERR_SHORTLIST_NOT_LISTED: &str = "be.error.scouting.shortlistNotListed";

/// Players a manager can follow at once. A shortlist is a working list, not a database.
pub const SHORTLIST_MAX: usize = 30;

/// Reports kept. A club that scouts a player a day for ten seasons does not carry four thousand
/// of them: the oldest are dropped first, and a dropped report only costs the club the estimate,
/// never a player.
const REPORTS_MAX: usize = 400;

/// A closed range of ratings, 1 to 99.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub low: u8,
    pub high: u8,
}

impl Range {
    pub fn mid(self) -> u8 {
        ((u16::from(self.low) + u16::from(self.high)) / 2) as u8
    }

    pub fn width(self) -> u8 {
        self.high - self.low
    }

    pub fn contains(self, value: u8) -> bool {
        (self.low..=self.high).contains(&value)
    }

    fn intersect(self, other: Range) -> Option<Range> {
        let low = self.low.max(other.low);
        let high = self.high.min(other.high);
        (low <= high).then_some(Range { low, high })
    }
}

/// How far the report can be trusted, from the width of the estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Moderate,
    High,
}

/// One report on one player: the latest, with how many have been made on him.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoutReport {
    pub player_id: String,
    pub scout_id: String,
    pub scout_name: String,
    pub scouted_on: String,
    /// What the scout could judge on the day, after the department's help
    /// (`staff_system::profile::effective_judging`).
    pub judging_ability: u8,
    pub judging_potential: u8,
    /// Reports on this player, counting the ones whose ranges this one was narrowed with.
    pub reports: u32,
    pub ability: Range,
    /// `None` when the scout cannot judge potential at all.
    pub potential: Option<Range>,
    pub age: i32,
    /// Attributes the scout rated well and poorly, by attribute name, from his own reading.
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
}

impl ScoutReport {
    /// Trust in the ability reading, from the width of its range: a scout rated 80 or more gives
    /// high, 60 to 79 moderate, and a second report on the same player can lift either a step.
    /// The potential range is shown as it is, wider and honest about it; it does not move this.
    pub fn confidence(&self) -> Confidence {
        match self.ability.width() {
            0..=5 => Confidence::High,
            6..=10 => Confidence::Moderate,
            _ => Confidence::Low,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortlistEntry {
    pub player_id: String,
    pub added_on: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoutingDossier {
    pub reports: Vec<ScoutReport>,
    pub shortlist: Vec<ShortlistEntry>,
    /// The day through which the staff in charge of the scouting duty have read the reports, so a
    /// report is judged once: a player the manager takes off the shortlist is not put back.
    pub delegate_reviewed_through: String,
}

impl ScoutingDossier {
    pub fn report_on(&self, player_id: &str) -> Option<&ScoutReport> {
        self.reports
            .iter()
            .find(|report| report.player_id == player_id)
    }

    /// The report on him if it is from the current season: not before `season_start` (an ISO date,
    /// which orders like the days it names). With no season start every report is current. The one
    /// statement of "current", read by the screens, the search and the delegated duty alike.
    pub fn current_report_on(
        &self,
        player_id: &str,
        season_start: Option<&str>,
    ) -> Option<&ScoutReport> {
        self.report_on(player_id)
            .filter(|report| season_start.is_none_or(|start| report.scouted_on.as_str() >= start))
    }

    pub fn is_shortlisted(&self, player_id: &str) -> bool {
        self.shortlist
            .iter()
            .any(|entry| entry.player_id == player_id)
    }
}

/// Half the width of the ability range a scout with `judging` reports.
fn ability_half_width(judging: u8) -> u8 {
    match judging {
        80.. => 2,
        60..=79 => 4,
        40..=59 => 6,
        _ => 9,
    }
}

/// Half the width of the potential range, or `None` for a scout who cannot judge it. A young
/// player's potential is harder to read than a veteran's, whose future is mostly his present.
fn potential_half_width(judging_potential: u8, age: i32) -> Option<u8> {
    let base = match judging_potential {
        80.. => 4,
        60..=79 => 7,
        40..=59 => 10,
        30..=39 => 14,
        _ => return None,
    };
    Some(if age <= 19 { base + 2 } else { base })
}

/// A range of half-width `half_width` that contains `truth` and is not centred on it: the centre
/// is off by a deterministic amount of up to 60 % of the half-width, drawn from `key`. Always
/// exactly `2 * half_width` wide.
fn estimate(truth: u8, half_width: u8, key: &str) -> Range {
    let truth = truth.clamp(1, 99);
    let reach = (u16::from(half_width) * 6 / 10) as i16;
    let offset = crate::seed::rng_from_key(key).random_range(-reach..=reach);
    let centre = i16::from(truth) + offset;
    let half = i16::from(half_width);
    // Against the ends of the scale the whole range slides rather than being cut, so its width
    // stays what the scout's skill makes it (a clipped range would be narrower, and so look
    // more certain, exactly for the players at the extremes) and still holds the truth.
    let low = (centre - half).clamp(1, 99 - 2 * half);
    Range {
        low: low as u8,
        high: (low + 2 * half) as u8,
    }
}

/// Largest error, in points, a scout of `judging` makes in reading one attribute.
fn attribute_noise(judging: u8) -> i16 {
    match judging {
        80.. => 2,
        60..=79 => 5,
        40..=59 => 8,
        _ => 12,
    }
}

/// The attributes as the scout read them: each moved by up to his error, deterministically.
fn read_attributes(attributes: &PlayerAttributes, judging: u8, key: &str) -> PlayerAttributes {
    let noise = attribute_noise(judging);
    let mut rng = crate::seed::rng_from_key(key);
    let mut read = attributes.clone();
    for value in attribute_slots(&mut read) {
        let delta = rng.random_range(-noise..=noise);
        *value = (i16::from(*value) + delta).clamp(1, 99) as u8;
    }
    read
}

/// Every attribute with its name, in one order, so reading, naming and ranking agree.
fn attribute_slots(attributes: &mut PlayerAttributes) -> [&mut u8; 19] {
    let PlayerAttributes {
        pace,
        stamina,
        strength,
        agility,
        passing,
        shooting,
        tackling,
        dribbling,
        defending,
        positioning,
        vision,
        decisions,
        composure,
        aggression,
        teamwork,
        leadership,
        handling,
        reflexes,
        aerial,
    } = attributes;
    [
        pace,
        stamina,
        strength,
        agility,
        passing,
        shooting,
        tackling,
        dribbling,
        defending,
        positioning,
        vision,
        decisions,
        composure,
        aggression,
        teamwork,
        leadership,
        handling,
        reflexes,
        aerial,
    ]
}

const ATTRIBUTE_NAMES: [&str; 19] = [
    "pace",
    "stamina",
    "strength",
    "agility",
    "passing",
    "shooting",
    "tackling",
    "dribbling",
    "defending",
    "positioning",
    "vision",
    "decisions",
    "composure",
    "aggression",
    "teamwork",
    "leadership",
    "handling",
    "reflexes",
    "aerial",
];

/// Strengths and weaknesses of the player as read: the attributes that matter for his position,
/// the three highest named as strengths when clearly above his own average, the two lowest as
/// weaknesses when clearly below it.
fn judge_attributes(player: &Player, read: &PlayerAttributes) -> (Vec<String>, Vec<String>) {
    let keeper =
        player.natural_position.to_group_position() == domain::player::Position::Goalkeeper;
    let mut scratch = read.clone();
    let rated: Vec<(&str, u8)> = ATTRIBUTE_NAMES
        .iter()
        .zip(attribute_slots(&mut scratch))
        .filter(|(name, _)| match **name {
            "handling" | "reflexes" => keeper,
            "shooting" | "dribbling" | "tackling" => !keeper,
            _ => true,
        })
        .map(|(name, value)| (*name, *value))
        .collect();
    let mean = rated.iter().map(|(_, v)| i32::from(*v)).sum::<i32>() / rated.len().max(1) as i32;
    let mut ranked = rated;
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let strengths = ranked
        .iter()
        .take(3)
        .filter(|(_, v)| i32::from(*v) >= mean + 6)
        .map(|(name, _)| (*name).to_string())
        .collect();
    let weaknesses = ranked
        .iter()
        .rev()
        .take(2)
        .filter(|(_, v)| i32::from(*v) <= mean - 8)
        .map(|(name, _)| (*name).to_string())
        .collect();
    (strengths, weaknesses)
}

/// Writes a finished scouting job into the dossier and returns the report and the attributes as
/// the scout read them (the report's inbox message shows the same numbers the profile will).
///
/// A report from the same season as the one it replaces is narrowed with it; one from an earlier
/// season replaces it, because a player changes between seasons.
pub(crate) fn record_report(
    game: &mut Game,
    assignment_id: &str,
    scout_id: &str,
    player_id: &str,
) -> Option<(ScoutReport, PlayerAttributes)> {
    let scout = game.staff.iter().find(|s| s.id == scout_id)?;
    let (judging_ability, judging_potential) =
        crate::staff_system::profile::effective_judging(game, scout);
    let scout_name = format!("{} {}", scout.first_name, scout.last_name);
    let player = game.players.iter().find(|p| p.id == player_id)?;
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let age =
        domain::age::age_on_or_unknown(game.clock.current_date.date_naive(), &player.date_of_birth);

    let key = format!("scout-estimate/{assignment_id}/{today}");
    let mut ability = estimate(
        player.ovr,
        ability_half_width(judging_ability),
        &format!("{key}/ovr"),
    );
    let mut potential = potential_half_width(judging_potential, age).map(|half| {
        estimate(
            player.potential.max(player.ovr),
            half,
            &format!("{key}/pot"),
        )
    });
    let read = read_attributes(&player.attributes, judging_ability, &format!("{key}/attrs"));
    let (strengths, weaknesses) = judge_attributes(player, &read);

    let season_start = game.season_context.season_start.as_deref();
    let previous = game
        .scouting_dossier
        .current_report_on(player_id, season_start)
        .cloned();
    let mut reports = 1;
    if let Some(old) = &previous {
        reports = old.reports + 1;
        ability = old.ability.intersect(ability).unwrap_or(ability);
        potential = match (old.potential, potential) {
            (Some(a), Some(b)) => Some(a.intersect(b).unwrap_or(b)),
            (a, b) => b.or(a),
        };
    }
    let report = ScoutReport {
        player_id: player_id.to_string(),
        scout_id: scout_id.to_string(),
        scout_name,
        scouted_on: today.clone(),
        judging_ability,
        judging_potential,
        reports,
        ability,
        potential,
        age,
        strengths,
        weaknesses,
    };

    let dossier = &mut game.scouting_dossier;
    dossier.reports.retain(|old| old.player_id != player_id);
    dossier.reports.push(report.clone());
    if dossier.reports.len() > REPORTS_MAX {
        dossier
            .reports
            .sort_by(|a, b| a.scouted_on.cmp(&b.scouted_on));
        let excess = dossier.reports.len() - REPORTS_MAX;
        dossier.reports.drain(..excess);
    }
    game.scouted_players.retain(|s| s.player_id != player_id);
    game.scouted_players.push(ScoutedPlayer {
        player_id: player_id.to_string(),
        scouted_on: today,
        attributes: read.clone(),
    });
    Some((report, read))
}

fn users_club(game: &Game) -> Option<&str> {
    game.manager.team_id.as_deref()
}

/// Puts a player on the manager's shortlist.
pub fn shortlist_add(game: &mut Game, player_id: &str) -> Result<(), String> {
    let player = game
        .players
        .iter()
        .find(|p| p.id == player_id)
        .ok_or(ERR_PLAYER_NOT_FOUND)?;
    if player.team_id.as_deref() == users_club(game) && player.team_id.is_some() {
        return Err(ERR_SHORTLIST_OWN_PLAYER.to_string());
    }
    if game.scouting_dossier.is_shortlisted(player_id) {
        return Err(ERR_SHORTLIST_ALREADY.to_string());
    }
    if game.scouting_dossier.shortlist.len() >= SHORTLIST_MAX {
        return Err(ERR_SHORTLIST_FULL.to_string());
    }
    let added_on = game.clock.current_date.format("%Y-%m-%d").to_string();
    game.scouting_dossier.shortlist.push(ShortlistEntry {
        player_id: player_id.to_string(),
        added_on,
    });
    Ok(())
}

pub fn shortlist_remove(game: &mut Game, player_id: &str) -> Result<(), String> {
    let before = game.scouting_dossier.shortlist.len();
    game.scouting_dossier
        .shortlist
        .retain(|entry| entry.player_id != player_id);
    if game.scouting_dossier.shortlist.len() == before {
        return Err(ERR_SHORTLIST_NOT_LISTED.to_string());
    }
    Ok(())
}

/// Takes off the shortlist the players the club can no longer pursue: retired, gone from the
/// world, or now at the manager's own club (signed, or loaned in). Daily, and free when the
/// shortlist is empty.
pub(crate) fn prune_shortlist(game: &mut Game) {
    if game.scouting_dossier.shortlist.is_empty() {
        return;
    }
    let own = game.manager.team_id.clone();
    let pursuable: std::collections::HashSet<&str> = game
        .players
        .iter()
        .filter(|p| !p.retired && (p.team_id.is_none() || p.team_id != own))
        .map(|p| p.id.as_str())
        .collect();
    let kept: Vec<ShortlistEntry> = game
        .scouting_dossier
        .shortlist
        .iter()
        .filter(|entry| pursuable.contains(entry.player_id.as_str()))
        .cloned()
        .collect();
    game.scouting_dossier.shortlist = kept;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given any true rating and any half-width
    /// When an estimate is made, with many different keys
    /// Then the truth is always inside the range, the range is never wider than twice the
    ///      half-width, and the centre is not always the truth.
    #[test]
    fn an_estimate_contains_the_truth_without_being_centred_on_it() {
        let mut off_centre = 0;
        for truth in [1u8, 5, 40, 63, 94, 99] {
            for half in [2u8, 4, 6, 9, 14] {
                for n in 0..40 {
                    let range = estimate(truth, half, &format!("k{n}"));
                    assert!(range.contains(truth), "{truth} not in {range:?} (±{half})");
                    assert_eq!(range.width(), 2 * half);
                    if range.mid() != truth {
                        off_centre += 1;
                    }
                }
            }
        }
        assert!(off_centre > 200, "the estimate is centred on the truth");
    }

    /// Given the same key
    /// When an estimate is made twice
    /// Then it is the same, so a replayed day reads the same report.
    #[test]
    fn an_estimate_is_deterministic() {
        assert_eq!(estimate(70, 6, "same"), estimate(70, 6, "same"));
    }

    /// Given a scout who cannot judge potential, a mediocre one and a good one
    /// When the half-widths are read
    /// Then the first reports none, and a better scout always reports a narrower range, with the
    ///      young player harder to read than the old.
    #[test]
    fn better_judging_means_narrower_ranges() {
        assert_eq!(potential_half_width(20, 18), None);
        let mut last = u8::MAX;
        for judging in [35u8, 45, 65, 85] {
            let half = potential_half_width(judging, 25).unwrap();
            assert!(half < last);
            last = half;
        }
        assert!(potential_half_width(65, 18) > potential_half_width(65, 27));
        assert!(ability_half_width(90) < ability_half_width(70));
        assert!(ability_half_width(70) < ability_half_width(50));
        assert!(ability_half_width(50) < ability_half_width(10));
    }

    /// Given two ranges that both contain the truth
    /// When they are intersected
    /// Then the truth is still inside, and the result is no wider than either.
    #[test]
    fn a_second_report_narrows_without_losing_the_truth() {
        for n in 0..60 {
            let a = estimate(72, 9, &format!("a{n}"));
            let b = estimate(72, 9, &format!("b{n}"));
            let both = a.intersect(b).expect("both contain 72");
            assert!(both.contains(72));
            assert!(both.width() <= a.width().min(b.width()));
        }
    }

    /// Given reports whose ability ranges are narrow, middling and wide
    /// When their confidence is read
    /// Then it falls with the width of the ability range, and the potential range (or its
    ///      absence) does not change it.
    #[test]
    fn confidence_follows_the_ability_range() {
        let base = ScoutReport {
            player_id: "p".into(),
            scout_id: "s".into(),
            scout_name: "S".into(),
            scouted_on: "2026-08-01".into(),
            judging_ability: 80,
            judging_potential: 80,
            reports: 1,
            ability: Range { low: 68, high: 72 },
            potential: Some(Range { low: 70, high: 76 }),
            age: 20,
            strengths: vec![],
            weaknesses: vec![],
        };
        assert_eq!(base.confidence(), Confidence::High);
        let middling = ScoutReport {
            ability: Range { low: 64, high: 72 },
            ..base.clone()
        };
        assert_eq!(middling.confidence(), Confidence::Moderate);
        let wide = ScoutReport {
            ability: Range { low: 58, high: 74 },
            ..base.clone()
        };
        assert_eq!(wide.confidence(), Confidence::Low);
        let blind = ScoutReport {
            potential: None,
            ..base
        };
        assert_eq!(blind.confidence(), Confidence::High);
    }
}
