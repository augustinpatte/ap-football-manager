//! The formation an AI club plays, and when it changes it.
//!
//! Every AI club used to take the field in the 4-4-2 the world was built with and keep it for ever:
//! nothing in the backend wrote `Team::formation` for a club the player does not manage, so a club
//! with three international centre backs and no left back played a back four for thirty seasons.
//!
//! Three different things decide a club's shape, and they are kept apart:
//!
//! - **The club's formation** is the persisted `Team::formation`: what the club plays today. It has
//!   inertia, because a change costs a margin (below), so it is the club's habit and not a daily
//!   opinion.
//! - **The manager's taste** is derived from his id ([`ManagerIdentity`] and a personal liking for
//!   each shape) and travels with him from club to club. A new manager arrives with a mandate to
//!   put his own stamp on the side, so the margin shrinks while he beds in.
//! - **The club's philosophy** is its play style: an attacking club leans to shapes with more
//!   forwards, a defensive one to shapes with more defenders.
//!
//! A temporary adjustment for the match (injuries, bans) is not a change of the club's formation and
//! lives with the line-up, not here.
//!
//! # How a change is decided
//!
//! Each candidate formation is scored by the squad model the market already prices signings by: the
//! squad built for that formation, planned over the club's own horizon, with absences allowed for.
//! That one number already knows depth (cover when a starter is out), ageing, development and
//! contracts that run out, so there is no second squad-quality model here. The score is the model's
//! mean rating of the eleven slots, plus a *taste* of at most a few tenths of a point (the manager's
//! and the club's), so taste decides between formations the squad is indifferent between and cannot
//! make a squad play a shape it cannot fill.
//!
//! The club moves only when the best formation beats the current one by a **margin**. The margin
//! falls while a new manager beds in and when results have been poor for a manager who adapts
//! readily, and it rises after a run of good results. Poor form never changes the formation by
//! itself: it only makes a manager readier to take an improvement that is really there, so a defeat
//! is not a reason and a squad that suits its shape stays in it.
//!
//! Nothing here is stored. The formation is the state; everything else is read from the squad, the
//! results and the manager's id, so a reload and a fast-forward decide the same way.

use chrono::NaiveDate;
use domain::player::{Player, Position};
use domain::team::{PlayStyle, Team};

use crate::manager_identity::ManagerIdentity;
use crate::player_rating::{FORMATIONS, formation_slots};
use crate::squad_intelligence::appetite::Appetite;
use crate::squad_intelligence::model::{Availability, SquadModel};
use crate::stable_hash::{mixed, stable_hash, unit_interval};

/// Set by a study that assigns formations itself and must not have the clubs undo it (see
/// [`freeze_reviews_for_study`]). Nothing in the game sets it.
static REVIEWS_FROZEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// AI clubs do not review their formation while this is alive: a controlled experiment's hold on
/// them. Dropping it (including by a panic that unwinds past it) lets them review again, so a
/// study that fails cannot leave every later game in the process with frozen formations.
#[must_use = "reviews resume as soon as the guard is dropped"]
pub struct ReviewsFrozen(());

impl Drop for ReviewsFrozen {
    fn drop(&mut self) {
        REVIEWS_FROZEN.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Stops AI clubs reviewing their formation until the returned guard is dropped.
pub fn freeze_reviews_for_study() -> ReviewsFrozen {
    REVIEWS_FROZEN.store(true, std::sync::atomic::Ordering::Relaxed);
    ReviewsFrozen(())
}

/// How many weekly reviews apart a settled club looks at its formation: about once a quarter. A
/// club does not reconsider its shape every week (and each look builds the squad model eight times,
/// so it is not free); a manager new in post does (see [`BEDDING_IN_LOOKS`]).
const REVIEW_EVERY_WEEKS: u64 = 12;

/// A manager in post looks at the shape he inherited on his first weekly review, and again a month
/// in, when he has seen the squad play: he came to change things, but each look builds the squad
/// model eight times. The two windows are a week wide, the weekly review's own cadence, so each
/// is one look.
const BEDDING_IN_LOOKS: [std::ops::Range<i64>; 2] = [0..7, 28..35];

/// Days over which a new manager's mandate to change the side fades to none.
const MANDATE_DAYS: f64 = 90.0;

/// The most a manager's mandate, in his first day, takes off the margin.
const MANDATE_DISCOUNT: f64 = 0.6;

/// What the best formation must beat the current one by, in rating points of the mean of the
/// eleven slots (about 2.5 points of rating over a whole side), before a settled club moves.
const BASE_MARGIN: f64 = 0.25;

/// What a club's play style asks of the shape, a manager's risk asks of it, and what a manager
/// simply likes, in the same rating points. Each moves a formation's score by at most this much.
const STYLE_TASTE: f64 = 0.20;
const RISK_TASTE: f64 = 0.20;
const PERSONAL_TASTE: f64 = 0.20;

/// Results over this many matches are what a manager looks back on.
pub(crate) const FORM_MATCHES: usize = 8;
/// Fewer results than this say nothing about whether the shape is working.
const FORM_NEEDED: usize = 5;
/// Points a game below which a run of results is a worry, and above which it is a vindication.
const WORRYING_PPG: f64 = 1.3;
const VINDICATING_PPG: f64 = 1.8;
/// How far a worrying run takes the margin down for the most adaptable manager.
const ADAPTABILITY_DISCOUNT: f64 = 0.6;
/// How far a vindicating run raises it.
const VINDICATION_PREMIUM: f64 = 0.5;

const PERSONAL_SALT: u64 = 0x666f_726d_6174_696f; // "formatio"

/// How attacking one slot is, from -1 (a centre back) to +1 (a striker).
fn slot_boldness(position: &Position) -> f64 {
    match position {
        Position::CenterBack | Position::Defender => -1.0,
        Position::LeftBack | Position::RightBack => -0.6,
        Position::LeftWingBack | Position::RightWingBack => -0.2,
        Position::DefensiveMidfielder => -0.4,
        Position::LeftMidfielder | Position::RightMidfielder => 0.2,
        Position::AttackingMidfielder => 0.6,
        Position::LeftWinger | Position::RightWinger => 0.8,
        Position::Striker | Position::Forward => 1.0,
        Position::Goalkeeper | Position::CentralMidfielder | Position::Midfielder => 0.0,
    }
}

/// How attacking a formation is: the mean of its ten outfield slots.
fn raw_boldness(formation: &str) -> f64 {
    let slots = formation_slots(formation);
    let outfield = slots.iter().filter(|s| **s != Position::Goalkeeper);
    let (sum, count) = outfield.fold((0.0, 0.0), |(sum, count), slot| {
        (sum + slot_boldness(slot), count + 1.0)
    });
    if count == 0.0 { 0.0 } else { sum / count }
}

/// A formation's boldness stretched over the formations the game offers to run from -1 (the most
/// cautious) to +1 (the boldest), so a taste for boldness is a taste for where a shape sits among
/// the options and not for an accident of the slot table.
fn boldness(formation: &str) -> f64 {
    let (low, high) = FORMATIONS
        .iter()
        .map(|f| raw_boldness(f))
        .fold((f64::MAX, f64::MIN), |(low, high), b| {
            (low.min(b), high.max(b))
        });
    if (high - low).abs() < f64::EPSILON {
        return 0.0;
    }
    2.0 * (raw_boldness(formation) - low) / (high - low) - 1.0
}

/// How bold a club's philosophy wants its shape, from -1 to +1.
pub(crate) fn style_boldness(style: &PlayStyle) -> f64 {
    match style {
        PlayStyle::Attacking => 0.7,
        PlayStyle::HighPress => 0.4,
        PlayStyle::Possession => 0.2,
        PlayStyle::Balanced => 0.0,
        PlayStyle::Counter => -0.3,
        PlayStyle::Defensive => -0.7,
    }
}

/// How bold a manager wants his shape: his risk, from -1 (cautious) to +1 (bold).
pub(crate) fn risk_boldness(manager: &ManagerIdentity) -> f64 {
    2.0 * manager.risk - 1.0
}

/// A manager's own liking for one formation, from -1 to +1, the same on every call and in every
/// save. Independent of his risk: two bold managers do not like the same shape.
fn personal_liking(manager_id: Option<&str>, formation: &str) -> f64 {
    let Some(id) = manager_id else {
        return 0.0;
    };
    let seed = stable_hash(id.as_bytes(), PERSONAL_SALT);
    2.0 * unit_interval(mixed(stable_hash(formation.as_bytes(), seed))) - 1.0
}

/// What the club and its manager like about `formation`, in the squad score's rating points.
pub(crate) fn taste(team: &Team, manager: &ManagerIdentity, formation: &str) -> f64 {
    let shape = boldness(formation);
    -STYLE_TASTE * (shape - style_boldness(&team.play_style)).abs()
        - RISK_TASTE * (shape - risk_boldness(manager)).abs()
        + PERSONAL_TASTE * personal_liking(team.manager_id.as_deref(), formation)
}

/// What the squad is worth in `formation`: the market's own squad model, planned over its horizon.
/// The mean expected rating of the eleven slots.
pub(crate) fn squad_score(squad: &[&Player], formation: &str, today: NaiveDate) -> f64 {
    let model = SquadModel::build(
        squad.iter().copied(),
        formation,
        today,
        Appetite::NEUTRAL.weights,
        Availability::Planning,
    );
    f64::from(model.evaluate_now().value)
}

/// Everything a club's formation review reads besides the squad.
pub(crate) struct Circumstances {
    pub today: NaiveDate,
    pub manager: ManagerIdentity,
    /// Days the manager has been in post at this club, when known.
    pub tenure_days: Option<i64>,
    /// Points a game over the last [`FORM_MATCHES`] matches, when there are enough of them.
    pub points_per_game: Option<f64>,
}

/// The margin the best formation must beat the current one by.
pub(crate) fn margin(circumstances: &Circumstances) -> f64 {
    let mandate = circumstances.tenure_days.map_or(0.0, |days| {
        MANDATE_DISCOUNT * (1.0 - (days as f64 / MANDATE_DAYS).clamp(0.0, 1.0))
    });
    let mut margin = BASE_MARGIN * (1.0 - mandate);
    if let Some(ppg) = circumstances.points_per_game {
        let worry = ((WORRYING_PPG - ppg) / WORRYING_PPG).clamp(0.0, 1.0);
        margin *= 1.0 - ADAPTABILITY_DISCOUNT * circumstances.manager.adaptability * worry;
        let vindication = ((ppg - VINDICATING_PPG) / (3.0 - VINDICATING_PPG)).clamp(0.0, 1.0);
        margin *= 1.0 + VINDICATION_PREMIUM * vindication;
    }
    margin
}

/// One candidate formation, scored.
#[derive(Debug, Clone, PartialEq)]
pub struct FormationScore {
    pub formation: &'static str,
    /// What the squad is worth in it (the squad model's mean rating of the eleven slots).
    pub squad: f64,
    /// What the club and its manager like about it.
    pub taste: f64,
}

impl FormationScore {
    pub fn total(&self) -> f64 {
        self.squad + self.taste
    }
}

/// Every formation the game offers, scored for this club.
pub(crate) fn score_all(
    team: &Team,
    squad: &[&Player],
    circumstances: &Circumstances,
) -> Vec<FormationScore> {
    FORMATIONS
        .iter()
        .map(|formation| FormationScore {
            formation,
            squad: squad_score(squad, formation, circumstances.today),
            taste: taste(team, &circumstances.manager, formation),
        })
        .collect()
}

/// The formation `team` should move to, or `None` to stay as it is.
///
/// The best-scoring formation wins only if it beats the current one by [`margin`]; ties on score
/// go to the formation earlier in [`FORMATIONS`], so the answer never depends on anything but the
/// inputs. A current formation the game does not offer (a package's, an old save's) is left to be
/// replaced by any formation that scores at all.
pub(crate) fn formation_to_move_to(
    team: &Team,
    squad: &[&Player],
    circumstances: &Circumstances,
) -> Option<&'static str> {
    if squad.len() < 11 {
        return None;
    }
    let scores = score_all(team, squad, circumstances);
    let best = scores.iter().max_by(|a, b| {
        a.total()
            .total_cmp(&b.total())
            .then(std::cmp::Ordering::Greater)
    })?;
    let current = scores.iter().find(|s| s.formation == team.formation);
    let needed = current.map_or(f64::NEG_INFINITY, |c| c.total() + margin(circumstances));
    (best.formation != team.formation && best.total() > needed).then_some(best.formation)
}

/// Days the club's manager has been in post here, from his career record, or `None` for a club
/// with no manager on record.
pub(crate) fn tenure_days(game: &crate::game::Game, team: &Team) -> Option<i64> {
    let manager_id = team.manager_id.as_deref()?;
    let manager = game.managers.iter().find(|m| m.id == manager_id)?;
    let started = manager
        .career_history
        .iter()
        .find(|entry| entry.end_date.is_none() && entry.team_id == team.id)?;
    let started = NaiveDate::parse_from_str(&started.start_date, "%Y-%m-%d").ok()?;
    Some((game.clock.current_date.date_naive() - started).num_days())
}

/// Moves `team` to `formation`: the formation itself, no separate out-of-possession shape (the
/// club's own is gone with the old formation), and the saved eleven re-laid for the new slots so
/// nothing reads a player's deployed position against a shape he no longer plays.
pub(crate) fn adopt(team: &mut Team, formation: &str, squad: &[&Player]) {
    team.formation = formation.to_string();
    team.out_of_possession_formation = String::new();
    // The pool a match is picked from (healthy seniors, youth only to cover a shortage), so an
    // academy player is never saved into a first eleven that match day would not field.
    let available: Vec<&Player> = crate::match_day_eligibility::eligible_among(squad)
        .into_iter()
        .filter(|p| p.can_play())
        .collect();
    team.starting_xi_ids = crate::turn::squad::first_choice_eleven(&available, formation)
        .into_iter()
        .map(|player| player.id.clone())
        .collect();
}

/// One AI club's formation review as it would be read today, for the studies: the current
/// formation, every candidate scored, and the margin a move must clear. Read-only.
#[derive(Debug, Clone)]
pub struct ClubFormationReading {
    pub team_id: String,
    pub current: String,
    pub scores: Vec<FormationScore>,
    pub margin: f64,
    pub would_move_to: Option<&'static str>,
}

/// Every AI club's formation review as of today. Measurement only: nothing is changed.
pub fn read_all_clubs(game: &crate::game::Game) -> Vec<ClubFormationReading> {
    let by_club = crate::match_day_eligibility::players_by_club(&game.players);
    let today = game.clock.current_date.date_naive();
    crate::ai_tactics::ai_clubs(game)
        .into_iter()
        .filter_map(|team_id| {
            let team = game.teams.iter().find(|t| t.id == team_id)?;
            let squad = by_club.get(team_id.as_str())?;
            let circumstances = Circumstances {
                today,
                manager: ManagerIdentity::of_club_manager(team.manager_id.as_deref()),
                tenure_days: tenure_days(game, team),
                points_per_game: None,
            };
            Some(ClubFormationReading {
                team_id,
                current: team.formation.clone(),
                scores: score_all(team, squad, &circumstances),
                margin: margin(&circumstances),
                would_move_to: formation_to_move_to(team, squad, &circumstances),
            })
        })
        .collect()
}

/// Whether a club looks at its formation on the weekly review that falls on `today`.
pub(crate) fn is_review_week(team_id: &str, today: NaiveDate, tenure_days: Option<i64>) -> bool {
    if REVIEWS_FROZEN.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    if tenure_days.is_some_and(|days| BEDDING_IN_LOOKS.iter().any(|look| look.contains(&days))) {
        return true;
    }
    let week = today.signed_duration_since(NaiveDate::default()).num_days() as u64 / 7;
    let phase = stable_hash(team_id.as_bytes(), PERSONAL_SALT) % REVIEW_EVERY_WEEKS;
    week % REVIEW_EVERY_WEEKS == phase
}

/// Points a game over the most recent `results` (points from each match, most recent first), or
/// `None` when there are too few to say anything.
pub(crate) fn points_per_game(points_most_recent_first: &[u32]) -> Option<f64> {
    if points_most_recent_first.len() < FORM_NEEDED {
        return None;
    }
    let window = &points_most_recent_first[..FORM_MATCHES.min(points_most_recent_first.len())];
    Some(f64::from(window.iter().sum::<u32>()) / window.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::squad_intelligence::test_squads::{Squad, team, today};

    fn circumstances(manager: ManagerIdentity) -> Circumstances {
        Circumstances {
            today: today(),
            manager,
            tenure_days: Some(400),
            points_per_game: None,
        }
    }

    fn refs(squad: &Squad) -> Vec<&Player> {
        squad.players.iter().collect()
    }

    /// A manager with none of the dispositions or likings that would tilt a decision: a club of
    /// no manager reads no liking, and a neutral risk asks for the middle of the shapes.
    fn neutral_team(formation: &str) -> Team {
        let mut team = team(formation, 1_000_000, 600_000);
        team.manager_id = None;
        team.play_style = PlayStyle::Balanced;
        team
    }

    /// A squad built for a back three: three outstanding centre backs, ordinary full backs.
    fn back_three_squad() -> Squad {
        Squad::new()
            .add(Position::Goalkeeper, &[72, 60])
            .add(Position::CenterBack, &[84, 83, 82, 60])
            .add(Position::LeftBack, &[55, 50])
            .add(Position::RightBack, &[55, 50])
            .add(Position::LeftWingBack, &[70, 60])
            .add(Position::RightWingBack, &[70, 60])
            .add(Position::CentralMidfielder, &[72, 70, 68, 62])
            .add(Position::LeftMidfielder, &[55])
            .add(Position::RightMidfielder, &[55])
            .add(Position::Striker, &[75, 73, 67])
    }

    /// Given a squad with three outstanding centre backs and poor full backs, playing a back four
    /// When its formation is reviewed
    /// Then it moves to a back three, because the squad model values the side higher in one.
    #[test]
    fn a_squad_with_three_great_centre_backs_moves_to_a_back_three() {
        let squad = back_three_squad();
        let team = neutral_team("4-4-2");
        let moved = formation_to_move_to(
            &team,
            &refs(&squad),
            &circumstances(ManagerIdentity::NEUTRAL),
        );
        let moved = moved.expect("the squad is built for a back three");
        let centre_backs = formation_slots(moved)
            .iter()
            .filter(|slot| **slot == Position::CenterBack)
            .count();
        assert!(centre_backs >= 3, "moved to {moved}");
    }

    /// Given a balanced squad that suits the club's current 4-4-2
    /// When its formation is reviewed by a manager with no tilt
    /// Then it stays: a squad the shape fits has no reason to change it.
    #[test]
    fn a_squad_that_suits_its_shape_stays_in_it() {
        let squad = Squad::balanced();
        let team = neutral_team("4-4-2");
        let moved = formation_to_move_to(
            &team,
            &refs(&squad),
            &circumstances(ManagerIdentity::NEUTRAL),
        );
        assert_eq!(moved, None);
    }

    /// Given a club playing the second-best shape for its squad, so another shape is a little
    /// better, a run of eight defeats and a manager as adaptable as any
    /// When the formation is reviewed
    /// Then it stays: the defeats lowered the bar but a small gain does not clear it, so a run of
    /// defeats alone is not a reason to change.
    #[test]
    fn a_run_of_defeats_alone_does_not_change_the_formation() {
        let squad = versatile_squad();
        let squad = refs(&squad);
        let adaptable = ManagerIdentity {
            adaptability: 1.0,
            ..ManagerIdentity::NEUTRAL
        };
        let lowest = margin(&Circumstances {
            points_per_game: Some(0.0),
            ..circumstances(adaptable)
        });
        let review = |id: &str, current: &str, ppg: Option<f64>| {
            let mut team = neutral_team(current);
            team.manager_id = Some(id.to_string());
            let manager = ManagerIdentity {
                adaptability: 1.0,
                ..ManagerIdentity::of(id)
            };
            let circumstances = Circumstances {
                points_per_game: ppg,
                ..circumstances(manager)
            };
            let mut ranked = score_all(&team, &squad, &circumstances);
            ranked.sort_by(|a, b| b.total().total_cmp(&a.total()));
            (ranked, formation_to_move_to(&team, &squad, &circumstances))
        };
        let (id, second) = (0..4000)
            .find_map(|i| {
                let id = format!("manager-{i}");
                let (ranked, _) = review(&id, "4-4-2", None);
                let gap = ranked[0].total() - ranked[1].total();
                (gap > 0.02 && gap < lowest * 0.8).then(|| (id, ranked[1].formation))
            })
            .expect("some manager's top two shapes are close");
        assert_eq!(review(&id, second, Some(0.0)).1, None);
    }

    /// Given the same worrying run for a manager who adapts and one who does not
    /// When the margin each asks of an alternative is read
    /// Then the adaptable manager asks less, and neither asks nothing.
    #[test]
    fn a_poor_run_lowers_the_margin_for_an_adaptable_manager_but_never_to_zero() {
        let worried = |adaptability| {
            let mut c = circumstances(ManagerIdentity {
                adaptability,
                ..ManagerIdentity::NEUTRAL
            });
            c.points_per_game = Some(0.4);
            margin(&c)
        };
        assert!(worried(1.0) < worried(0.0));
        assert!(worried(1.0) > 0.0);
        assert!((worried(0.0) - BASE_MARGIN).abs() < 1e-9);
    }

    /// Given a run of wins
    /// When the margin is read
    /// Then it is higher than for a run of ordinary results: a vindicated shape is kept.
    #[test]
    fn a_winning_run_raises_the_margin() {
        let at = |ppg| {
            let mut c = circumstances(ManagerIdentity::NEUTRAL);
            c.points_per_game = Some(ppg);
            margin(&c)
        };
        assert!(at(2.8) > at(1.5));
    }

    /// A squad that has two good players in every position the formations ask for, so that no
    /// formation is much better than another for it and taste is what is left to decide.
    fn versatile_squad() -> Squad {
        Squad::new()
            .add(Position::Goalkeeper, &[72, 60])
            .add(Position::CenterBack, &[72, 71, 70, 66])
            .add(Position::LeftBack, &[71, 65])
            .add(Position::RightBack, &[71, 65])
            .add(Position::LeftWingBack, &[71, 65])
            .add(Position::RightWingBack, &[71, 65])
            .add(Position::DefensiveMidfielder, &[72, 66])
            .add(Position::CentralMidfielder, &[72, 71, 66])
            .add(Position::AttackingMidfielder, &[72, 66])
            .add(Position::LeftMidfielder, &[71, 65])
            .add(Position::RightMidfielder, &[71, 65])
            .add(Position::LeftWinger, &[72, 66])
            .add(Position::RightWinger, &[72, 66])
            .add(Position::Striker, &[73, 72, 66])
    }

    /// The best gain over 4-4-2, in score, a manager with this id sees in the versatile squad, and
    /// whether he moves.
    fn best_gain_for(id: &str, tenure: i64) -> (f64, Option<&'static str>) {
        let squad = versatile_squad();
        let squad = refs(&squad);
        let mut team = neutral_team("4-4-2");
        team.manager_id = Some(id.to_string());
        let circumstances = Circumstances {
            tenure_days: Some(tenure),
            ..circumstances(ManagerIdentity::of(id))
        };
        let scores = score_all(&team, &squad, &circumstances);
        let current = scores
            .iter()
            .find(|s| s.formation == "4-4-2")
            .unwrap()
            .total();
        let gain = scores
            .iter()
            .map(|s| s.total() - current)
            .fold(f64::MIN, f64::max);
        (gain, formation_to_move_to(&team, &squad, &circumstances))
    }

    /// Given a manager just arrived and one long in post, and a manager whose taste for another
    /// shape lands between the new man's margin and the settled one's
    /// When each reviews the same squad
    /// Then the new manager changes the shape and the established one leaves it.
    #[test]
    fn a_new_manager_changes_what_an_established_one_would_leave() {
        let fresh = margin(&Circumstances {
            tenure_days: Some(0),
            ..circumstances(ManagerIdentity::NEUTRAL)
        });
        let settled = margin(&circumstances(ManagerIdentity::NEUTRAL));
        assert!(fresh < settled);
        let id = (0..4000)
            .map(|i| format!("manager-{i}"))
            .find(|id| {
                let (gain, _) = best_gain_for(id, 400);
                gain > fresh + 0.02 && gain < settled - 0.02
            })
            .expect("some manager's taste lands between the two margins");
        assert!(best_gain_for(&id, 0).1.is_some(), "the new manager stayed");
        assert_eq!(best_gain_for(&id, 400).1, None, "the established one moved");
    }

    /// Given one versatile squad and a world of managers
    /// When each, brand new in post, reviews it
    /// Then they do not all choose the same formation: it is the manager who differs.
    #[test]
    fn different_managers_choose_different_formations_from_the_same_squad() {
        let chosen: std::collections::BTreeSet<&str> = (0..200)
            .map(|i| {
                best_gain_for(&format!("manager-{i}"), 0)
                    .1
                    .unwrap_or("4-4-2")
            })
            .collect();
        assert!(chosen.len() >= 3, "only {chosen:?}");
    }

    /// Given an attacking club and a defensive one with the same balanced squad and no manager
    /// When their shapes are scored
    /// Then the attacking club likes a bolder shape than the defensive one does.
    #[test]
    fn a_clubs_philosophy_tilts_which_shape_it_likes() {
        let liked = |style: PlayStyle| {
            let mut team = neutral_team("4-4-2");
            team.play_style = style;
            FORMATIONS
                .iter()
                .max_by(|a, b| {
                    taste(&team, &ManagerIdentity::NEUTRAL, a).total_cmp(&taste(
                        &team,
                        &ManagerIdentity::NEUTRAL,
                        b,
                    ))
                })
                .map(|f| boldness(f))
                .unwrap()
        };
        assert!(liked(PlayStyle::Attacking) > liked(PlayStyle::Defensive));
    }

    /// Given the formations the game offers
    /// When their boldness is read
    /// Then it runs the whole of -1 to +1 and a back five is more cautious than a front three.
    #[test]
    fn boldness_spans_the_options_and_orders_them() {
        let values: Vec<f64> = FORMATIONS.iter().map(|f| boldness(f)).collect();
        assert!((values.iter().cloned().fold(f64::MIN, f64::max) - 1.0).abs() < 1e-9);
        assert!((values.iter().cloned().fold(f64::MAX, f64::min) + 1.0).abs() < 1e-9);
        assert!(boldness("5-3-2") < boldness("3-4-3"));
    }

    /// Given the formations the game offers a club
    /// When each is laid out for the match
    /// Then every one has geometry the match can be played on, and is one the formation review
    /// considers: a formation added to the geometry but not here would never be adopted.
    #[test]
    fn every_formation_a_club_may_adopt_can_be_played() {
        for formation in FORMATIONS {
            assert!(
                crate::spatial_adapter::formation_geometry(formation).is_ok(),
                "{formation} has no geometry"
            );
            assert_eq!(formation_slots(formation).len(), 11, "{formation}");
        }
    }

    /// Given an AI club with an academy goalkeeper better than any senior and enough fit seniors
    /// When it adopts a new formation
    /// Then its saved eleven names no academy player: it is the eleven match day would field.
    #[test]
    fn a_formation_change_never_saves_an_academy_player_into_the_eleven() {
        let squad = Squad::balanced();
        let mut players = squad.players.clone();
        let mut prodigy = crate::squad_intelligence::test_squads::player(
            "academy-striker",
            Position::Striker,
            95,
            18,
        );
        prodigy.squad_role = domain::player::SquadRole::Youth;
        players.push(prodigy);
        let refs: Vec<&Player> = players.iter().collect();
        let mut team = neutral_team("4-4-2");
        adopt(&mut team, "4-3-3", &refs);
        assert_eq!(team.starting_xi_ids.len(), 11);
        assert!(
            !team
                .starting_xi_ids
                .contains(&"academy-striker".to_string())
        );
    }

    /// Given a short squad
    /// When its formation is reviewed
    /// Then nothing changes: ten players cannot say what shape suits them.
    #[test]
    fn a_squad_too_short_to_judge_keeps_its_shape() {
        let squad = back_three_squad();
        let few: Vec<&Player> = squad.players.iter().take(10).collect();
        let team = neutral_team("4-4-2");
        assert_eq!(
            formation_to_move_to(&team, &few, &circumstances(ManagerIdentity::NEUTRAL)),
            None
        );
    }

    /// Given a manager new in post and a settled one
    /// When the weeks of a year are walked
    /// Then the new manager looks on his first review and again a month in, and the settled club
    /// about one week in twelve.
    #[test]
    fn settled_clubs_review_every_few_weeks_and_new_managers_look_twice() {
        let start = today();
        let weeks = |tenure_at_start: i64| {
            (0..52)
                .filter(|week| {
                    is_review_week(
                        "club",
                        start + chrono::Duration::days(week * 7),
                        Some(tenure_at_start + week * 7),
                    )
                })
                .count()
        };
        // A manager hired today: weeks 0 and 4 are his looks, and the quarterly cycle adds
        // its own three or four.
        let settled = weeks(400);
        assert!((4..=5).contains(&settled), "{settled}");
        assert!(weeks(0) > settled, "{} vs {settled}", weeks(0));
        assert!(is_review_week("club", start, Some(0)));
        assert!(is_review_week("club", start, Some(30)));
    }

    /// Given results as points from each match
    /// When the run is read
    /// Then too few results say nothing and enough give points a game over the last eight.
    #[test]
    fn form_needs_enough_matches_and_looks_back_a_fixed_number() {
        assert_eq!(points_per_game(&[3, 3, 3, 3]), None);
        assert_eq!(points_per_game(&[3, 3, 3, 3, 3]), Some(3.0));
        let long = [0, 0, 0, 0, 0, 0, 0, 0, 3, 3, 3, 3, 3, 3, 3, 3];
        assert_eq!(points_per_game(&long), Some(0.0));
    }

    /// A two-club world: the player's club and an AI club "club" with `squad`, on a review day.
    fn world_with(squad: &Squad, formation: &str) -> crate::game::Game {
        use chrono::TimeZone;
        let manager = {
            let mut manager = domain::manager::Manager::new(
                "user-manager".into(),
                "Test".into(),
                "Manager".into(),
                "1980-01-01".into(),
                "England".into(),
            );
            manager.hire("user".into());
            manager
        };
        let mut ai = team(formation, 1_000_000, 600_000);
        ai.manager_id = None;
        ai.play_style = PlayStyle::Balanced;
        let mut user = team(formation, 1_000_000, 600_000);
        user.id = "user".into();
        user.name = "user".into();
        let day = today();
        // Walk forward to a day this club's formation review falls on.
        let review_day = (0..84)
            .map(|offset| day + chrono::Duration::days(offset))
            .find(|d| is_review_week("club", *d, None))
            .expect("a settled club reviews within twelve weeks");
        let clock = crate::clock::GameClock::new(
            chrono::Utc.from_utc_datetime(&review_day.and_hms_opt(12, 0, 0).unwrap()),
        );
        crate::game::Game::new(
            clock,
            manager,
            vec![ai, user],
            squad.players.clone(),
            vec![],
            vec![],
        )
    }

    /// Given an AI club built for a back three and a user club with the same squad shape
    /// When the weekly tactical review falls on the club's day
    /// Then the AI club's formation changes and its saved eleven fits the new slots, and the
    /// player's club is left alone.
    #[test]
    fn the_weekly_review_moves_an_ai_club_and_never_the_users() {
        let squad = back_three_squad();
        let mut game = world_with(&squad, "4-4-2");
        // The user's club holds a copy of the same players under its own id.
        for player in squad.players.clone() {
            let mut copy = player;
            copy.id = format!("user-{}", copy.id);
            copy.team_id = Some("user".into());
            game.players.push(copy);
        }
        let weekday = crate::ai_tactics::review_weekday("club");
        crate::ai_tactics::apply_ai_tactical_reviews(&mut game, weekday);
        let ai = game.teams.iter().find(|t| t.id == "club").unwrap();
        let user = game.teams.iter().find(|t| t.id == "user").unwrap();
        assert_ne!(ai.formation, "4-4-2", "the AI club kept its shape");
        assert_eq!(user.formation, "4-4-2", "the user's club was changed");
        assert_eq!(ai.starting_xi_ids.len(), 11);
        let slots = formation_slots(&ai.formation);
        let centre_backs = slots.iter().filter(|s| **s == Position::CenterBack).count();
        assert!(centre_backs >= 3);
    }

    /// Given a club whose current formation the game does not offer
    /// When its formation is reviewed
    /// Then it moves to whichever offered formation the squad is worth most in.
    #[test]
    fn a_formation_the_game_does_not_offer_is_replaced() {
        let squad = Squad::balanced();
        let team = neutral_team("4-1-2-3");
        assert!(
            formation_to_move_to(
                &team,
                &refs(&squad),
                &circumstances(ManagerIdentity::NEUTRAL)
            )
            .is_some()
        );
    }
}
