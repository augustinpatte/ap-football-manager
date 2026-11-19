//! The assistant manager's recommendations.
//!
//! Deterministic: every recommendation is computed from the state of the game and carries the
//! figures it was computed from, so the justification shown is the reason it was made. How many
//! the assistant raises, and how subtle a problem he notices, depend on his own quality: a poor
//! assistant misses the small things; an analyst on the staff is what lets him size up the
//! opponent.

use chrono::NaiveDate;
use domain::league::FixtureStatus;
use domain::player::{Player, Position, SquadRole};
use domain::staff::StaffPost;
use domain::team::{PlayStyle, TrainingFocus, TrainingIntensity};
use serde::Serialize;

use super::market;
use super::profile::{StaffProfile, team_staff};
use crate::game::Game;
use crate::training::{FitnessStatus, fitness_status};

/// Fewest players a position group should have fit and available.
const GROUP_MINIMUM: [(Position, usize); 4] = [
    (Position::Goalkeeper, 2),
    (Position::Defender, 5),
    (Position::Midfielder, 5),
    (Position::Forward, 3),
];
/// The expected rating points a slot of the side must be below the club's own standard before an
/// assistant of 0 calls the position weak, and one of 100.
const WEAK_SEVERITY_AT_ZERO: f64 = 8.0;
const WEAK_SEVERITY_AT_100: f64 = 4.0;
/// A starter this far below the match in condition is worth resting.
const TIRED_BELOW: u8 = 60;
/// A replacement must be this fit.
const FRESH_ABOVE: u8 = 75;
/// A replacement may be this many OVR points below the player he replaces.
const REPLACEMENT_GAP: u8 = 6;
/// Matches within a week that make the schedule heavy.
const BUSY_WEEK: usize = 3;
/// Days ahead the next match must be for a tired starter to be flagged.
const MATCH_WINDOW_DAYS: i64 = 3;
/// Tilt of the opponent's best eleven that moves the tactical advice.
const TACTICAL_GAP: f64 = 4.0;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RecKind {
    RestPlayer,
    ReplaceStarter,
    WeakPosition,
    BusySchedule,
    TacticalAdjustment,
    LowMorale,
    PoorForm,
    PromoteYouth,
    StaffVacancy,
    /// The manager's training plan is harder than the squad's condition can carry.
    TrainingLoad,
    /// The coaches teach one session clearly better than the one being run.
    CoachingStrength,
    /// Injured players, how long they are out and what the medical department makes of it.
    InjuryRecovery,
}

/// What accepting a recommendation does.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RecAction {
    /// Informational only.
    None,
    SwapStarter {
        out_player_id: String,
        in_player_id: String,
    },
    SetTraining {
        focus: TrainingFocus,
        intensity: TrainingIntensity,
    },
    SetPlayStyle {
        style: PlayStyle,
    },
    PromoteYouth {
        player_id: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Recommendation {
    pub id: String,
    pub kind: RecKind,
    /// 1 (worth a look) to 3 (act now).
    pub severity: u8,
    pub player_id: Option<String>,
    pub title_key: String,
    pub reason_key: String,
    /// The figures behind the reason, named for the translation.
    pub params: Vec<(String, String)>,
    pub action: RecAction,
}

/// What the assistant can see of the next match.
pub(crate) struct NextMatch {
    days: i64,
    opponent_id: String,
    away: bool,
}

fn group(player: &Player) -> Position {
    player.natural_position.to_group_position()
}

fn available(player: &Player) -> bool {
    player.can_play() && !player.retired
}

fn p(key: &str, value: impl ToString) -> (String, String) {
    (key.to_string(), value.to_string())
}

pub(crate) fn next_match_and_load(game: &Game, team_id: &str) -> (Option<NextMatch>, usize) {
    let today = game.clock.current_date.date_naive();
    let mut next: Option<(NaiveDate, NextMatch)> = None;
    let mut week = 0;
    for competition in game.competitions_in_play() {
        if !game.competition_in_active_scope(competition) {
            continue;
        }
        for fixture in competition
            .fixtures
            .iter()
            .filter(|f| f.status == FixtureStatus::Scheduled)
            .filter(|f| f.home_team_id == team_id || f.away_team_id == team_id)
        {
            let Ok(date) = NaiveDate::parse_from_str(&fixture.date, "%Y-%m-%d") else {
                continue;
            };
            let days = (date - today).num_days();
            if !(0..=7).contains(&days) {
                continue;
            }
            week += 1;
            let away = fixture.away_team_id == team_id;
            let opponent = if away {
                &fixture.home_team_id
            } else {
                &fixture.away_team_id
            };
            if next.as_ref().is_none_or(|(d, _)| date < *d) {
                next = Some((
                    date,
                    NextMatch {
                        days,
                        opponent_id: opponent.clone(),
                        away,
                    },
                ));
            }
        }
    }
    (next.map(|(_, m)| m), week)
}

/// The best eleven the club could field today by OVR, as a strength figure.
fn best_eleven_strength(game: &Game, team_id: &str) -> Option<f64> {
    let mut ovrs: Vec<u8> = game
        .players
        .iter()
        .filter(|pl| {
            pl.team_id.as_deref() == Some(team_id)
                && pl.squad_role == SquadRole::Senior
                && available(pl)
        })
        .map(|pl| pl.ovr)
        .collect();
    if ovrs.len() < 11 {
        return None;
    }
    ovrs.sort_unstable_by(|a, b| b.cmp(a));
    Some(ovrs[..11].iter().map(|o| f64::from(*o)).sum::<f64>() / 11.0)
}

/// The recommendations for `team_id`, most pressing first. Nothing is changed.
pub fn recommendations(game: &Game, team_id: &str) -> Vec<Recommendation> {
    recommendations_read_by(game, team_id, None)
}

/// The recommendations as read by a member of staff rated `reader`: what a duty handed to
/// someone other than the assistant notices is limited by that person's eye, not the assistant's.
/// `None` is the assistant manager himself.
pub(super) fn recommendations_read_by(
    game: &Game,
    team_id: &str,
    reader: Option<f64>,
) -> Vec<Recommendation> {
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return Vec::new();
    };
    let staff = team_staff(game, team_id);
    let mut profile = StaffProfile::from_staff(&staff);
    if let Some(rating) = reader {
        profile.assistant = rating;
    }
    let has_assistant = staff
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager);
    let has_analyst = staff.iter().any(|s| s.post() == StaffPost::Analyst);

    // A poor assistant misses the small things; with none, nothing is raised but the gap.
    let min_severity: u8 = if profile.assistant >= 60.0 {
        1
    } else if profile.assistant >= 40.0 {
        2
    } else {
        3
    };
    let max_items = 3 + (profile.assistant / 100.0 * 5.0).round() as usize;
    let mut out: Vec<Recommendation> = Vec::new();

    // With no assistant the club's own advice is only the gap. A member the manager named to read a
    // duty (a head of youth, a coach) reads the squad regardless: a named owner does not depend on
    // an assistant being in post.
    if !has_assistant && reader.is_none() {
        out.push(Recommendation {
            id: "no-assistant".into(),
            kind: RecKind::StaffVacancy,
            severity: 3,
            player_id: None,
            title_key: "assistant.rec.noAssistant.title".into(),
            reason_key: "assistant.rec.noAssistant.reason".into(),
            params: vec![],
            action: RecAction::None,
        });
        return out;
    }

    for status in market::structure(game, team_id)
        .into_iter()
        .filter(|s| s.is_vacant())
    {
        out.push(Recommendation {
            id: format!("vacancy-{:?}", status.post),
            kind: RecKind::StaffVacancy,
            severity: 2,
            player_id: None,
            title_key: "assistant.rec.vacancy.title".into(),
            reason_key: "assistant.rec.vacancy.reason".into(),
            params: vec![p("post", format!("{:?}", status.post))],
            action: RecAction::None,
        });
    }

    let squad: Vec<&Player> = game
        .players
        .iter()
        .filter(|pl| {
            pl.team_id.as_deref() == Some(team_id)
                && pl.squad_role == SquadRole::Senior
                && !pl.retired
        })
        .collect();
    let (next, week_load) = next_match_and_load(game, team_id);

    // The eleven in the line-up, or the best eleven when none is set.
    let mut starters: Vec<&Player> = team
        .starting_xi_ids
        .iter()
        .filter_map(|id| squad.iter().copied().find(|pl| &pl.id == id))
        .collect();
    if starters.len() < 11 {
        let mut by_ovr: Vec<&Player> = squad.iter().copied().filter(|pl| available(pl)).collect();
        by_ovr.sort_by(|a, b| b.ovr.cmp(&a.ovr).then(a.id.cmp(&b.id)));
        starters = by_ovr.into_iter().take(11).collect();
    }
    let starter_ids: std::collections::HashSet<&str> =
        starters.iter().map(|pl| pl.id.as_str()).collect();
    let bench: Vec<&Player> = squad
        .iter()
        .copied()
        .filter(|pl| !starter_ids.contains(pl.id.as_str()) && available(pl))
        .collect();

    // The replacement for a starter: same group, fresh, close in level, the best of those.
    let replacement_for = |starter: &Player, taken: &[String]| -> Option<&Player> {
        bench
            .iter()
            .copied()
            .filter(|b| group(b) == group(starter) && !taken.contains(&b.id))
            .filter(|b| b.condition >= FRESH_ABOVE && b.ovr + REPLACEMENT_GAP >= starter.ovr)
            .max_by(|a, b| a.ovr.cmp(&b.ovr).then(b.id.cmp(&a.id)))
    };
    let mut taken: Vec<String> = Vec::new();
    let soon = next.as_ref().is_some_and(|m| m.days <= MATCH_WINDOW_DAYS);

    // Injured starters first, then tired ones.
    for starter in &starters {
        if let Some(injury) = &starter.injury {
            let replacement = replacement_for(starter, &taken);
            let action = replacement.map_or(RecAction::None, |r| RecAction::SwapStarter {
                out_player_id: starter.id.clone(),
                in_player_id: r.id.clone(),
            });
            if let Some(r) = replacement {
                taken.push(r.id.clone());
            }
            out.push(Recommendation {
                id: format!("injured-{}", starter.id),
                kind: RecKind::ReplaceStarter,
                severity: 3,
                player_id: Some(starter.id.clone()),
                title_key: "assistant.rec.injuredStarter.title".into(),
                reason_key: "assistant.rec.injuredStarter.reason".into(),
                params: vec![
                    p("player", &starter.match_name),
                    p("days", injury.days_remaining),
                    p(
                        "replacement",
                        replacement.map_or("-", |r| r.match_name.as_str()),
                    ),
                    p(
                        "replacementCondition",
                        replacement.map_or(0, |r| r.condition),
                    ),
                ],
                action,
            });
        }
    }
    if soon {
        let m = next.as_ref().expect("soon implies a next match");
        for starter in starters
            .iter()
            .filter(|s| s.injury.is_none() && s.condition < TIRED_BELOW)
        {
            let severity = match fitness_status(starter.condition) {
                FitnessStatus::Critical => 3,
                FitnessStatus::Exhausted => 2,
                FitnessStatus::Fit => 1,
            };
            let replacement = replacement_for(starter, &taken);
            let action = replacement.map_or(RecAction::None, |r| RecAction::SwapStarter {
                out_player_id: starter.id.clone(),
                in_player_id: r.id.clone(),
            });
            if let Some(r) = replacement {
                taken.push(r.id.clone());
            }
            out.push(Recommendation {
                id: format!("tired-{}", starter.id),
                kind: if replacement.is_some() {
                    RecKind::ReplaceStarter
                } else {
                    RecKind::RestPlayer
                },
                severity,
                player_id: Some(starter.id.clone()),
                title_key: "assistant.rec.tiredStarter.title".into(),
                reason_key: if replacement.is_some() {
                    "assistant.rec.tiredStarter.reasonWithCover".into()
                } else {
                    "assistant.rec.tiredStarter.reason".into()
                },
                params: vec![
                    p("player", &starter.match_name),
                    p("condition", starter.condition),
                    p("days", m.days),
                    p(
                        "replacement",
                        replacement.map_or("-", |r| r.match_name.as_str()),
                    ),
                    p(
                        "replacementCondition",
                        replacement.map_or(0, |r| r.condition),
                    ),
                ],
                action,
            });
        }
    }

    // Morale and form of the eleven.
    for starter in starters
        .iter()
        .filter(|s| s.injury.is_none() && s.morale < 40)
    {
        out.push(Recommendation {
            id: format!("morale-{}", starter.id),
            kind: RecKind::LowMorale,
            severity: if starter.morale < 25 { 2 } else { 1 },
            player_id: Some(starter.id.clone()),
            title_key: "assistant.rec.lowMorale.title".into(),
            reason_key: "assistant.rec.lowMorale.reason".into(),
            params: vec![
                p("player", &starter.match_name),
                p("morale", starter.morale),
            ],
            action: RecAction::None,
        });
    }
    let rated: Vec<f32> = squad
        .iter()
        .filter(|pl| pl.stats.rated_appearances >= 5)
        .map(|pl| pl.stats.avg_rating)
        .collect();
    if rated.len() >= 8 {
        let mean = rated.iter().sum::<f32>() / rated.len() as f32;
        for starter in starters.iter().filter(|s| {
            s.injury.is_none() && s.stats.rated_appearances >= 5 && s.stats.avg_rating + 0.6 < mean
        }) {
            let replacement = replacement_for(starter, &taken)
                .filter(|r| r.stats.avg_rating >= starter.stats.avg_rating);
            let action = replacement.map_or(RecAction::None, |r| RecAction::SwapStarter {
                out_player_id: starter.id.clone(),
                in_player_id: r.id.clone(),
            });
            if let Some(r) = replacement {
                taken.push(r.id.clone());
            }
            out.push(Recommendation {
                id: format!("form-{}", starter.id),
                kind: RecKind::PoorForm,
                severity: 1,
                player_id: Some(starter.id.clone()),
                title_key: "assistant.rec.poorForm.title".into(),
                reason_key: "assistant.rec.poorForm.reason".into(),
                params: vec![
                    p("player", &starter.match_name),
                    p("rating", format!("{:.1}", starter.stats.avg_rating)),
                    p("squad", format!("{mean:.1}")),
                    p(
                        "replacement",
                        replacement.map_or("-", |r| r.match_name.as_str()),
                    ),
                ],
                action,
            });
        }
    }

    // Weak positions: too few fit players, or a group far below the rest of the squad.
    let squad_best: f64 = {
        let mut o: Vec<u8> = squad
            .iter()
            .filter(|pl| available(pl))
            .map(|pl| pl.ovr)
            .collect();
        o.sort_unstable_by(|a, b| b.cmp(a));
        o.iter().take(11).map(|v| f64::from(*v)).sum::<f64>() / 11.0_f64.max(o.len().min(11) as f64)
    };
    // Where the side the club fields is weakest: the same reading the recruiter and the AI clubs
    // make (Squad Intelligence). A sharper assistant calls a smaller shortfall weak: the severity he
    // reacts to falls smoothly from [`WEAK_SEVERITY_AT_ZERO`] to [`WEAK_SEVERITY_AT_100`].
    let weakness_threshold = WEAK_SEVERITY_AT_ZERO
        - (WEAK_SEVERITY_AT_ZERO - WEAK_SEVERITY_AT_100)
            * (profile.assistant / 100.0).clamp(0.0, 1.0);
    let needs = crate::squad_intelligence::ClubSquad::of(
        game,
        team_id,
        crate::squad_intelligence::appetite::Appetite::NEUTRAL.seen_by_assistant(profile.assistant),
    )
    .map(|squad| squad.needs())
    .unwrap_or_default();
    for (group_index, (position, minimum)) in GROUP_MINIMUM.into_iter().enumerate() {
        let fit: Vec<&&Player> = squad
            .iter()
            .filter(|pl| group(pl) == position && available(pl))
            .collect();
        let best = fit.iter().map(|pl| pl.ovr).max().unwrap_or(0);
        let too_few = fit.len() < minimum;
        let weakest = needs.iter().find(|need| need.group == group_index);
        let too_weak = !fit.is_empty()
            && weakest.is_some_and(|need| f64::from(need.severity) >= weakness_threshold);
        if too_few || too_weak {
            out.push(Recommendation {
                id: format!("weak-{position:?}"),
                kind: RecKind::WeakPosition,
                severity: if fit.len() + 1 < minimum { 3 } else { 2 },
                player_id: None,
                title_key: "assistant.rec.weakPosition.title".into(),
                reason_key: if too_few {
                    "assistant.rec.weakPosition.reasonDepth".into()
                } else {
                    "assistant.rec.weakPosition.reasonLevel".into()
                },
                params: vec![
                    p("position", format!("{position:?}")),
                    p("fit", fit.len()),
                    p("needed", minimum),
                    p("best", best),
                    p("squad", format!("{squad_best:.0}")),
                    p(
                        "role",
                        weakest.map_or(String::new(), |need| format!("{:?}", need.position)),
                    ),
                ],
                action: RecAction::None,
            });
        }
    }

    // A heavy week: ease the training.
    if week_load >= BUSY_WEEK {
        let hard = !matches!(team.training_intensity, TrainingIntensity::Low);
        out.push(Recommendation {
            id: "busy-week".into(),
            kind: RecKind::BusySchedule,
            severity: if week_load > BUSY_WEEK { 3 } else { 2 },
            player_id: None,
            title_key: "assistant.rec.busySchedule.title".into(),
            reason_key: "assistant.rec.busySchedule.reason".into(),
            params: vec![p("matches", week_load)],
            action: if hard {
                RecAction::SetTraining {
                    focus: TrainingFocus::Recovery,
                    intensity: TrainingIntensity::Low,
                }
            } else {
                RecAction::None
            },
        });
    }

    // The opponent: sized up only with an analyst, or by a sharp assistant.
    if let Some(m) = &next
        && (has_analyst || profile.assistant >= 60.0)
        && let (Some(mine), Some(theirs)) = (
            best_eleven_strength(game, team_id),
            best_eleven_strength(game, &m.opponent_id),
        )
    {
        let gap = theirs - mine;
        let opponent = game
            .teams
            .iter()
            .find(|t| t.id == m.opponent_id)
            .map_or("?", |t| t.name.as_str());
        let style = if gap >= TACTICAL_GAP
            && !matches!(team.play_style, PlayStyle::Defensive | PlayStyle::Counter)
        {
            Some(PlayStyle::Counter)
        } else if gap <= -TACTICAL_GAP
            && !m.away
            && !matches!(
                team.play_style,
                PlayStyle::Attacking | PlayStyle::Possession | PlayStyle::HighPress
            )
        {
            Some(PlayStyle::Possession)
        } else {
            None
        };
        if let Some(style) = style {
            out.push(Recommendation {
                id: format!("tactics-{}", m.opponent_id),
                kind: RecKind::TacticalAdjustment,
                severity: 2,
                player_id: None,
                title_key: "assistant.rec.tactics.title".into(),
                reason_key: "assistant.rec.tactics.reason".into(),
                params: vec![
                    p("opponent", opponent),
                    p("mine", format!("{mine:.1}")),
                    p("theirs", format!("{theirs:.1}")),
                    p("style", format!("{style:?}")),
                    p("days", m.days),
                ],
                action: RecAction::SetPlayStyle { style },
            });
        }
    }

    training_advice(
        game, team_id, &staff, &profile, &starters, week_load, &mut out,
    );
    injury_advice(&squad, &profile, &mut out);

    // Academy players ready for the first team.
    let today = game.clock.current_date.date_naive();
    let senior_median = {
        let mut o: Vec<u8> = squad.iter().map(|pl| pl.ovr).collect();
        o.sort_unstable();
        o.get(o.len() / 2).copied().unwrap_or(0)
    };
    for kid in game.players.iter().filter(|pl| {
        pl.team_id.as_deref() == Some(team_id) && pl.squad_role == SquadRole::Youth && available(pl)
    }) {
        let age = domain::age::age_on(today, &kid.date_of_birth).unwrap_or(99);
        if age <= 20 && kid.ovr >= senior_median {
            out.push(Recommendation {
                id: format!("promote-{}", kid.id),
                kind: RecKind::PromoteYouth,
                severity: 1,
                player_id: Some(kid.id.clone()),
                title_key: "assistant.rec.promote.title".into(),
                reason_key: "assistant.rec.promote.reason".into(),
                params: vec![
                    p("player", &kid.match_name),
                    p("age", age),
                    p("ovr", kid.ovr),
                    p("median", senior_median),
                ],
                action: RecAction::PromoteYouth {
                    player_id: kid.id.clone(),
                },
            });
        }
    }

    out.retain(|r| r.severity >= min_severity);
    out.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.id.cmp(&b.id)));
    out.truncate(max_items);
    out
}

/// Starters this far below full condition, on average, are not ready for a hard session.
const LOAD_ADVICE_BELOW: f64 = 65.0;
/// Below this average the advice is a recovery session, not just a lighter one.
const LOAD_ADVICE_CRISIS: f64 = 45.0;
/// A session must be taught this much better than the one run (ratio) to be worth suggesting.
const STRENGTH_ADVICE_RATIO: f64 = 1.07;
/// An injury this long (days) is a long-term absence.
const LONG_INJURY_DAYS: u32 = 21;

/// Advice on the manager's training plan: too hard for the squad as it stands, or leaving the
/// coaches' best session unused. Read from the starters' condition and the coaches' real
/// teaching figures, the same ones `training` applies.
fn training_advice(
    game: &Game,
    team_id: &str,
    staff: &[&domain::staff::Staff],
    profile: &StaffProfile,
    starters: &[&Player],
    week_load: usize,
    out: &mut Vec<Recommendation>,
) {
    let Some(team) = game.teams.iter().find(|t| t.id == team_id) else {
        return;
    };
    let fit: Vec<&&Player> = starters.iter().filter(|pl| pl.injury.is_none()).collect();
    if fit.is_empty() || team.training_focus == TrainingFocus::Recovery {
        return;
    }
    let mean = fit.iter().map(|pl| f64::from(pl.condition)).sum::<f64>() / fit.len() as f64;
    let hard = team.training_intensity == TrainingIntensity::High;
    // A congested week has its own advice (and `training` already runs recovery in it).
    if week_load < BUSY_WEEK && hard && mean < LOAD_ADVICE_BELOW {
        let crisis = mean < LOAD_ADVICE_CRISIS;
        out.push(Recommendation {
            id: "training-load".into(),
            kind: RecKind::TrainingLoad,
            severity: if crisis { 3 } else { 2 },
            player_id: None,
            title_key: "assistant.rec.trainingLoad.title".into(),
            reason_key: "assistant.rec.trainingLoad.reason".into(),
            params: vec![
                p("condition", format!("{mean:.0}")),
                p("intensity", format!("{:?}", team.training_intensity)),
                p("schedule", format!("{:?}", team.training_schedule)),
            ],
            action: if crisis {
                RecAction::SetTraining {
                    focus: TrainingFocus::Recovery,
                    intensity: TrainingIntensity::Low,
                }
            } else {
                RecAction::SetTraining {
                    focus: team.training_focus.clone(),
                    intensity: TrainingIntensity::Medium,
                }
            },
        });
    }
    // The coaches' strengths: only worth saying when the club is not in a heavy spell.
    if !hard || mean >= LOAD_ADVICE_BELOW {
        let coaches = staff
            .iter()
            .filter(|s| {
                matches!(
                    s.post(),
                    StaffPost::AssistantManager | StaffPost::FirstTeamCoach
                )
            })
            .count();
        let (best, best_effect) = super::delegation_training::best_taught(game, team_id);
        let current =
            super::delegation_training::teaching_effect(game, team_id, &team.training_focus);
        if coaches > 0
            && profile.coaching >= 40.0
            && best != team.training_focus
            && team.training_focus != TrainingFocus::Physical
            && best_effect >= current * STRENGTH_ADVICE_RATIO
        {
            out.push(Recommendation {
                id: "coaching-strength".into(),
                kind: RecKind::CoachingStrength,
                severity: 1,
                player_id: None,
                title_key: "assistant.rec.coachingStrength.title".into(),
                reason_key: "assistant.rec.coachingStrength.reason".into(),
                params: vec![
                    p("focus", format!("{best:?}")),
                    p("currentFocus", format!("{:?}", team.training_focus)),
                    p(
                        "gain",
                        format!("{:.0}", (best_effect / current - 1.0) * 100.0),
                    ),
                ],
                action: RecAction::SetTraining {
                    focus: best,
                    intensity: team.training_intensity.clone(),
                },
            });
        }
    }
}

/// One note on the injury list when it matters: several players out, or somebody out for weeks.
/// It names the medical department because that is what the club can change.
fn injury_advice(squad: &[&Player], profile: &StaffProfile, out: &mut Vec<Recommendation>) {
    let injured: Vec<&&Player> = squad.iter().filter(|pl| pl.injury.is_some()).collect();
    let longest = injured
        .iter()
        .filter_map(|pl| pl.injury.as_ref().map(|i| i.days_remaining))
        .max()
        .unwrap_or(0);
    if injured.len() < 2 && longest < LONG_INJURY_DAYS {
        return;
    }
    let weak = profile.medical < 40.0;
    out.push(Recommendation {
        id: "injury-recovery".into(),
        kind: RecKind::InjuryRecovery,
        severity: if weak { 2 } else { 1 },
        player_id: None,
        title_key: "assistant.rec.injuryRecovery.title".into(),
        reason_key: "assistant.rec.injuryRecovery.reason".into(),
        params: vec![
            p("count", injured.len()),
            p("days", longest),
            p("medical", format!("{:.0}", profile.medical)),
            p(
                "duration",
                format!("{:+.0}", (profile.injury_duration_mult() - 1.0) * 100.0),
            ),
        ],
        action: RecAction::None,
    });
}

/// Carries out a recommendation's action for `team_id`. The one path for the manager's click
/// and for delegated duties.
pub fn apply_action(game: &mut Game, team_id: &str, action: &RecAction) -> Result<(), String> {
    match action {
        RecAction::None => Ok(()),
        RecAction::SwapStarter {
            out_player_id,
            in_player_id,
        } => {
            let incoming_ok = game.players.iter().any(|pl| {
                &pl.id == in_player_id && pl.team_id.as_deref() == Some(team_id) && available(pl)
            });
            if !incoming_ok {
                return Err("be.error.playerNotInSquad".into());
            }
            let team = game
                .teams
                .iter_mut()
                .find(|t| t.id == team_id)
                .ok_or("be.error.teamNotFound")?;
            let slot = team
                .starting_xi_ids
                .iter()
                .position(|id| id == out_player_id);
            match slot {
                Some(i) => team.starting_xi_ids[i] = in_player_id.clone(),
                None => return Err("be.error.playerNotInLineup".into()),
            }
            Ok(())
        }
        RecAction::SetTraining { focus, intensity } => {
            let team = game
                .teams
                .iter_mut()
                .find(|t| t.id == team_id)
                .ok_or("be.error.teamNotFound")?;
            team.training_focus = focus.clone();
            team.training_intensity = intensity.clone();
            Ok(())
        }
        RecAction::SetPlayStyle { style } => {
            let team = game
                .teams
                .iter_mut()
                .find(|t| t.id == team_id)
                .ok_or("be.error.teamNotFound")?;
            team.play_style = style.clone();
            Ok(())
        }
        RecAction::PromoteYouth { player_id } => {
            // A promotion takes a senior place as any signing does. The manager's own club can no
            // more field a 31st senior than an AI club: the action is refused with the same
            // translated squad-full sentence a signing gets, so the list never goes above the limit.
            if let Some(refusal) = crate::transfers::signing_place_refusal(game, team_id, "") {
                return Err(refusal);
            }
            let player = game
                .players
                .iter_mut()
                .find(|pl| &pl.id == player_id && pl.team_id.as_deref() == Some(team_id))
                .ok_or("be.error.playerNotInSquad")?;
            player.squad_role = SquadRole::Senior;
            Ok(())
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::world::test_fixtures::{manager_for, nation_team, player_at};
    use chrono::{TimeZone, Utc};
    use domain::league::{Fixture, League};
    use domain::staff::{Staff, StaffAttributes};

    pub(crate) fn staff_member(id: &str, post: StaffPost, level: u8, team: &str) -> Staff {
        let mut s = Staff::new(
            id.into(),
            "A".into(),
            id.into(),
            "1980-01-01".into(),
            post.role(),
            StaffAttributes {
                coaching: level,
                judging_ability: level,
                judging_potential: level,
                physiotherapy: level,
            },
        );
        s.specialization = post.specialization();
        s.post = Some(post);
        s.team_id = Some(team.into());
        s.wage = 1000;
        s
    }

    /// "team1" with 16 midfielders (s0..s15, the first eleven in the line-up, the others on the
    /// bench), an assistant of the given quality, and a league match against "team2" in `days`.
    pub(crate) fn game(assistant: Option<u8>, days: i64) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        let mut team1 = nation_team("team1", "England", 500);
        let team2 = nation_team("team2", "England", 400);
        let mut players = Vec::new();
        for i in 0..16 {
            let mut p = player_at(&format!("s{i}"), "team1", "2030-06-30");
            p.ovr = 70 - i as u8; // the line-up is the strongest eleven
            p.condition = 90;
            players.push(p);
        }
        for i in 0..14 {
            let mut p = player_at(&format!("o{i}"), "team2", "2030-06-30");
            p.ovr = 60;
            players.push(p);
        }
        team1.starting_xi_ids = (0..11).map(|i| format!("s{i}")).collect();
        // A full staff, so that vacancies do not crowd the advice under test.
        let mut staff: Vec<Staff> = StaffPost::ALL
            .iter()
            .filter(|p| p.required() > 0 && **p != StaffPost::AssistantManager)
            .map(|p| staff_member(&format!("have-{p:?}"), *p, 60, "team1"))
            .collect();
        staff.extend(
            assistant.map(|a| staff_member("assist", StaffPost::AssistantManager, a, "team1")),
        );
        let mut game = Game::new(
            clock,
            manager_for("team1"),
            vec![team1, team2],
            players,
            staff,
            vec![],
        );
        let date = (Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()
            + chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();
        let mut league = League::new(
            "l".into(),
            "L".into(),
            2026,
            &["team1".to_string(), "team2".to_string()],
        );
        league.fixtures.push(Fixture {
            id: "f1".into(),
            competition_id: "l".into(),
            date,
            home_team_id: "team1".into(),
            away_team_id: "team2".into(),
            ..Default::default()
        });
        game.competitions = vec![league];
        game
    }

    fn of_kind(recs: &[Recommendation], kind: RecKind) -> Vec<&Recommendation> {
        recs.iter().filter(|r| r.kind == kind).collect()
    }

    #[test]
    fn without_an_assistant_the_only_advice_is_to_hire_one() {
        let g = game(None, 2);
        let recs = recommendations(&g, "team1");
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].kind, RecKind::StaffVacancy);
    }

    #[test]
    fn a_tired_starter_with_no_close_cover_is_to_be_rested() {
        let mut g = game(Some(80), 2);
        g.players
            .iter_mut()
            .find(|p| p.id == "s3")
            .unwrap()
            .condition = 30;
        let recs = recommendations(&g, "team1");
        // The best of the bench is s11 (OVR 59 against 67): more than the allowed gap, so there
        // is no cover and the advice is to rest him.
        assert!(of_kind(&recs, RecKind::ReplaceStarter).is_empty());
        let rest = of_kind(&recs, RecKind::RestPlayer);
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].player_id.as_deref(), Some("s3"));
        assert_eq!(rest[0].severity, 2);
        assert_eq!(rest[0].action, RecAction::None);
    }

    #[test]
    fn the_justification_carries_the_figures_it_was_made_from() {
        let mut g = game(Some(80), 2);
        // Bench level close to the starters: s11 has OVR 70.
        g.players.iter_mut().find(|p| p.id == "s11").unwrap().ovr = 66;
        g.players
            .iter_mut()
            .find(|p| p.id == "s3")
            .unwrap()
            .condition = 30;
        let recs = recommendations(&g, "team1");
        let rec = of_kind(&recs, RecKind::ReplaceStarter)[0];
        let get = |k: &str| {
            rec.params
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(get("condition"), "30");
        assert_eq!(get("days"), "2");
        assert_eq!(get("replacement"), "s11");
        assert_eq!(
            rec.action,
            RecAction::SwapStarter {
                out_player_id: "s3".into(),
                in_player_id: "s11".into()
            }
        );
        assert!(rec.reason_key.ends_with("reasonWithCover"));
    }

    #[test]
    fn accepting_a_swap_changes_the_line_up() {
        let mut g = game(Some(80), 2);
        g.players.iter_mut().find(|p| p.id == "s11").unwrap().ovr = 66;
        g.players
            .iter_mut()
            .find(|p| p.id == "s3")
            .unwrap()
            .condition = 30;
        let rec = of_kind(&recommendations(&g, "team1"), RecKind::ReplaceStarter)[0].clone();
        apply_action(&mut g, "team1", &rec.action).unwrap();
        assert!(g.teams[0].starting_xi_ids.contains(&"s11".to_string()));
        assert!(!g.teams[0].starting_xi_ids.contains(&"s3".to_string()));
    }

    #[test]
    fn no_one_is_flagged_when_the_match_is_far_away() {
        let mut g = game(Some(80), 6);
        g.players
            .iter_mut()
            .find(|p| p.id == "s3")
            .unwrap()
            .condition = 30;
        assert!(of_kind(&recommendations(&g, "team1"), RecKind::ReplaceStarter).is_empty());
        assert!(of_kind(&recommendations(&g, "team1"), RecKind::RestPlayer).is_empty());
    }

    #[test]
    fn an_injured_starter_is_always_raised_with_his_cover() {
        let mut g = game(Some(80), 6);
        g.players.iter_mut().find(|p| p.id == "s0").unwrap().injury =
            Some(domain::player::Injury {
                name: "x".into(),
                days_remaining: 12,
            });
        g.players.iter_mut().find(|p| p.id == "s11").unwrap().ovr = 68;
        let recs = recommendations(&g, "team1");
        let rec = of_kind(&recs, RecKind::ReplaceStarter)[0];
        assert_eq!(rec.severity, 3);
        assert_eq!(rec.reason_key, "assistant.rec.injuredStarter.reason");
    }

    #[test]
    fn a_poor_assistant_misses_what_a_sharp_one_notices() {
        let mut sharp = game(Some(80), 2);
        let mut poor = game(Some(20), 2);
        for g in [&mut sharp, &mut poor] {
            // A mildly tired starter: severity 1.
            g.players
                .iter_mut()
                .find(|p| p.id == "s4")
                .unwrap()
                .condition = 58;
        }
        assert!(
            !of_kind(&recommendations(&sharp, "team1"), RecKind::RestPlayer).is_empty()
                || !of_kind(&recommendations(&sharp, "team1"), RecKind::ReplaceStarter).is_empty()
        );
        let poor_recs = recommendations(&poor, "team1");
        assert!(poor_recs.iter().all(|r| r.severity >= 3));
        assert!(recommendations(&sharp, "team1").len() > poor_recs.len());
    }

    #[test]
    fn a_goalkeeping_gap_is_reported_as_a_weak_position_with_its_numbers() {
        let g = game(Some(80), 6);
        let recs = recommendations(&g, "team1");
        let gk = recs
            .iter()
            .find(|r| r.id == "weak-Goalkeeper")
            .expect("no keeper at all");
        assert_eq!(gk.kind, RecKind::WeakPosition);
        assert!(gk.params.iter().any(|(k, v)| k == "needed" && v == "2"));
    }

    #[test]
    fn a_heavy_week_suggests_easing_training() {
        let mut g = game(Some(80), 1);
        let league = &mut g.competitions[0];
        for (i, d) in [2, 4, 6].iter().enumerate() {
            let date = (Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()
                + chrono::Duration::days(*d))
            .format("%Y-%m-%d")
            .to_string();
            league.fixtures.push(Fixture {
                id: format!("x{i}"),
                competition_id: "l".into(),
                date,
                home_team_id: "team1".into(),
                away_team_id: "team2".into(),
                ..Default::default()
            });
        }
        g.teams[0].training_intensity = TrainingIntensity::High;
        let recs = recommendations(&g, "team1");
        let busy = of_kind(&recs, RecKind::BusySchedule)[0];
        assert!(matches!(busy.action, RecAction::SetTraining { .. }));
        assert!(busy.params.iter().any(|(k, v)| k == "matches" && v == "4"));
    }

    #[test]
    fn a_much_stronger_opponent_calls_for_a_counter_attacking_style_only_with_an_analyst() {
        let mut g = game(Some(40), 2);
        for p in g
            .players
            .iter_mut()
            .filter(|p| p.team_id.as_deref() == Some("team2"))
        {
            p.ovr = 85;
        }
        g.teams[0].play_style = PlayStyle::Attacking;
        // A middling assistant on his own does not size up the opponent...
        assert!(of_kind(&recommendations(&g, "team1"), RecKind::TacticalAdjustment).is_empty());
        // ...an analyst lets him.
        g.staff
            .push(staff_member("an", StaffPost::Analyst, 70, "team1"));
        let recs = recommendations(&g, "team1");
        let tactic = of_kind(&recs, RecKind::TacticalAdjustment)[0];
        assert_eq!(
            tactic.action,
            RecAction::SetPlayStyle {
                style: PlayStyle::Counter
            }
        );
    }

    #[test]
    fn an_academy_player_at_the_first_teams_level_is_proposed_for_promotion() {
        let mut g = game(Some(80), 6);
        let mut kid = player_at("kid", "team1", "2030-06-30");
        kid.squad_role = SquadRole::Youth;
        kid.date_of_birth = "2007-01-01".into();
        kid.ovr = 70;
        g.players.push(kid);
        let recs = recommendations(&g, "team1");
        let rec = of_kind(&recs, RecKind::PromoteYouth)[0];
        apply_action(&mut g, "team1", &rec.action).unwrap();
        assert_eq!(
            g.players.iter().find(|p| p.id == "kid").unwrap().squad_role,
            SquadRole::Senior
        );
    }

    /// Given a club with no assistant manager, a head of youth named in charge of the Youth duty,
    ///   and an academy player at the first team's level
    /// When the duty is delegated and a day is played
    /// Then the head of youth reads the squad himself and promotes the player: a named owner does
    ///   not depend on an assistant being in post, and the duty is not marked run for nothing.
    #[test]
    fn a_named_head_of_youth_promotes_when_the_club_has_no_assistant() {
        let mut g = game(None, 6);
        let mut kid = player_at("kid", "team1", "2030-06-30");
        kid.squad_role = SquadRole::Youth;
        kid.date_of_birth = "2007-01-01".into();
        kid.ovr = 70;
        g.players.push(kid);
        g.staff.retain(|s| {
            s.post() != StaffPost::HeadOfYouth && s.post() != StaffPost::AssistantManager
        });
        g.staff
            .push(staff_member("hoy", StaffPost::HeadOfYouth, 70, "team1"));
        assert!(
            of_kind(&recommendations(&g, "team1"), RecKind::PromoteYouth).is_empty(),
            "with no assistant the club's own advice is only the vacancy"
        );
        super::super::delegation_owner::assign(
            &mut g,
            "team1",
            domain::delegation::Duty::Youth,
            Some("hoy"),
        )
        .expect("a head of youth can run the youth duty");
        g.delegation
            .set(domain::delegation::Duty::Youth, true, false);

        super::super::delegation::run_after_day(&mut g);

        assert_eq!(
            g.players.iter().find(|p| p.id == "kid").unwrap().squad_role,
            SquadRole::Senior,
            "the named head of youth never read the academy"
        );
    }

    /// Given a club at its senior limit with an academy player the manager asks to promote
    /// When the recommendation is applied
    /// Then it is refused with the translated squad-full sentence and the player stays a youth;
    ///   with one senior gone the same action promotes him.
    #[test]
    fn a_club_at_its_senior_limit_refuses_a_delegated_promotion() {
        let mut g = game(Some(80), 6);
        let mut kid = player_at("kid", "team1", "2030-06-30");
        kid.squad_role = SquadRole::Youth;
        kid.date_of_birth = "2007-01-01".into();
        kid.ovr = 70;
        g.players.push(kid);
        let mut filled = 0;
        while crate::contracts::squad_capacity(&g, "team1").registered_seniors < 30 {
            let mut extra = player_at(&format!("filler{filled}"), "team1", "2030-06-30");
            extra.squad_role = SquadRole::Senior;
            g.players.push(extra);
            filled += 1;
        }
        let action = RecAction::PromoteYouth {
            player_id: "kid".into(),
        };

        let refusal = apply_action(&mut g, "team1", &action).unwrap_err();
        assert!(
            refusal.starts_with("be.error.freeAgents.squadFull"),
            "{refusal}"
        );
        assert_eq!(
            g.players.iter().find(|p| p.id == "kid").unwrap().squad_role,
            SquadRole::Youth,
            "a full club cannot promote the youngster"
        );

        g.players.retain(|p| p.id != "filler0");
        apply_action(&mut g, "team1", &action).unwrap();
        assert_eq!(
            g.players.iter().find(|p| p.id == "kid").unwrap().squad_role,
            SquadRole::Senior,
            "the same action promotes him once a place frees"
        );
    }

    fn set_condition(g: &mut Game, condition: u8) {
        for pl in g
            .players
            .iter_mut()
            .filter(|pl| pl.team_id.as_deref() == Some("team1"))
        {
            pl.condition = condition;
        }
    }

    /// Given the manager runs High sessions and his starters are at 50
    /// When the assistant advises
    /// Then he says the plan is too hard and offers Medium on the same focus; at 30 it is a
    ///      recovery session; at 90 there is nothing to say.
    #[test]
    fn a_plan_too_hard_for_the_squad_is_flagged_with_the_condition_that_shows_it() {
        let mut g = game(Some(80), 6);
        g.teams[0].training_intensity = TrainingIntensity::High;
        g.teams[0].training_focus = TrainingFocus::Technical;
        set_condition(&mut g, 50);
        let recs = recommendations(&g, "team1");
        let rec = of_kind(&recs, RecKind::TrainingLoad)[0];
        assert_eq!(rec.severity, 2);
        assert_eq!(
            rec.action,
            RecAction::SetTraining {
                focus: TrainingFocus::Technical,
                intensity: TrainingIntensity::Medium
            }
        );
        assert!(rec.params.contains(&p("condition", "50")));

        set_condition(&mut g, 30);
        let rec = recommendations(&g, "team1")
            .into_iter()
            .find(|r| r.kind == RecKind::TrainingLoad)
            .expect("a crisis is flagged");
        assert_eq!(rec.severity, 3);
        assert_eq!(
            rec.action,
            RecAction::SetTraining {
                focus: TrainingFocus::Recovery,
                intensity: TrainingIntensity::Low
            }
        );

        set_condition(&mut g, 90);
        assert!(of_kind(&recommendations(&g, "team1"), RecKind::TrainingLoad).is_empty());
    }

    /// Given a tired squad on High in a week of three matches
    /// When the assistant advises
    /// Then the heavy-week advice is given, not a second note about the same problem.
    #[test]
    fn a_congested_week_is_not_advised_twice() {
        let mut g = game(Some(80), 1);
        for day in [3, 5] {
            let date = (g.clock.current_date + chrono::Duration::days(day))
                .format("%Y-%m-%d")
                .to_string();
            g.competitions[0].fixtures.push(Fixture {
                id: format!("x{day}"),
                competition_id: "l".into(),
                date,
                home_team_id: "team1".into(),
                away_team_id: "team2".into(),
                ..Default::default()
            });
        }
        g.teams[0].training_intensity = TrainingIntensity::High;
        set_condition(&mut g, 50);
        let recs = recommendations(&g, "team1");
        assert!(!of_kind(&recs, RecKind::BusySchedule).is_empty());
        assert!(of_kind(&recs, RecKind::TrainingLoad).is_empty());
    }

    fn with_defending_specialist(g: &mut Game) {
        g.staff.retain(|s| !s.id.starts_with("have-First"));
        let mut coach = staff_member("def", StaffPost::FirstTeamCoach, 60, "team1");
        coach.specialization = Some(domain::staff::CoachingSpecialization::Defending);
        g.staff.push(coach);
    }

    /// Given coaches who teach Defending best and a squad running Technical
    /// When the assistant advises
    /// Then he suggests Defending, naming the gain; if Defending is already the plan, or the
    ///      assistant is too weak to read the coaches, he says nothing.
    #[test]
    fn the_coaches_strongest_session_is_suggested_when_it_is_not_the_one_run() {
        let mut g = game(Some(80), 6);
        with_defending_specialist(&mut g);
        g.teams[0].training_focus = TrainingFocus::Technical;
        g.teams[0].training_intensity = TrainingIntensity::Medium;
        let recs = recommendations(&g, "team1");
        let rec = of_kind(&recs, RecKind::CoachingStrength)[0];
        assert_eq!(
            rec.action,
            RecAction::SetTraining {
                focus: TrainingFocus::Defending,
                intensity: TrainingIntensity::Medium
            }
        );
        assert!(rec.params.contains(&p("focus", "Defending")));

        g.teams[0].training_focus = TrainingFocus::Defending;
        assert!(of_kind(&recommendations(&g, "team1"), RecKind::CoachingStrength).is_empty());

        let mut weak = game(Some(25), 6);
        with_defending_specialist(&mut weak);
        weak.staff.iter_mut().for_each(|s| {
            if s.id == "def" {
                s.attributes.coaching = 25;
            }
        });
        assert!(of_kind(&recommendations(&weak, "team1"), RecKind::CoachingStrength).is_empty());
    }

    fn injure(g: &mut Game, id: &str, days: u32) {
        g.players
            .iter_mut()
            .find(|pl| pl.id == id)
            .expect("a player of team1")
            .injury = Some(domain::player::Injury {
            name: "Strain".into(),
            days_remaining: days,
        });
    }

    /// Given one short injury, then two, then one long one
    /// When the assistant advises
    /// Then a single short injury is not worth a note; two injured or one long absence is, and a
    ///      club with no medical department is told it matters more.
    #[test]
    fn injuries_are_flagged_when_there_are_several_or_one_is_long() {
        let mut g = game(Some(80), 6);
        injure(&mut g, "s14", 5);
        assert!(of_kind(&recommendations(&g, "team1"), RecKind::InjuryRecovery).is_empty());
        injure(&mut g, "s15", 9);
        let recs = recommendations(&g, "team1");
        let rec = of_kind(&recs, RecKind::InjuryRecovery)[0];
        assert_eq!(rec.severity, 1);
        assert!(rec.params.contains(&p("count", 2)));

        let mut long = game(Some(80), 6);
        injure(&mut long, "s14", 30);
        long.staff
            .retain(|s| !matches!(s.post(), StaffPost::Physio | StaffPost::HeadOfMedical));
        let recs = recommendations(&long, "team1");
        let rec = of_kind(&recs, RecKind::InjuryRecovery)[0];
        assert_eq!(rec.severity, 2, "no medical department");
        assert!(rec.params.contains(&p("days", 30)));
    }
}
