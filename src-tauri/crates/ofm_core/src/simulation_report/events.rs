//! The moments worth the manager's attention that the run itself recorded: a season rolled over,
//! a dismissal, and the board's own messages.

use domain::message::MessageCategory;

use crate::game::Game;

use super::tracker::Stretch;
use super::types::{ReportEvent, StretchEnd};

pub(super) fn events_of(
    game: &Game,
    stretch: &Stretch,
    until: &str,
    ended: StretchEnd,
    knockouts: Vec<ReportEvent>,
) -> Vec<ReportEvent> {
    let mut events = knockouts;
    for note in &stretch.rollovers {
        events.push(ReportEvent::SeasonCompleted {
            date: note.date.clone(),
            season: note.season,
            league_name: note.league_name.clone(),
            position: note.position,
            total_teams: note.total_teams,
            points: note.points,
            division_move: note.division_move.clone(),
        });
    }
    for message in game.messages.iter().filter(|message| {
        message.category == MessageCategory::BoardDirective
            && message.date.as_str() >= stretch.from.as_str()
            && message.date.as_str() < until
            && !stretch.pre_run.messages.contains(&message.id)
    }) {
        events.push(ReportEvent::BoardMessage {
            date: message.date.clone(),
            message_id: message.id.clone(),
            subject_key: message.subject_key.clone(),
            subject: message.subject.clone(),
        });
    }
    if ended == StretchEnd::Dismissed {
        // The day the board acted is the last day of the stretch.
        events.push(ReportEvent::Dismissed {
            date: last_day_before(until),
        });
    }
    events.sort_by(|a, b| a.date().cmp(b.date()));
    events
}

/// The day before `until` (`YYYY-MM-DD`), or `until` itself when it cannot be read.
fn last_day_before(until: &str) -> String {
    chrono::NaiveDate::parse_from_str(until, "%Y-%m-%d")
        .ok()
        .and_then(|day| day.pred_opt())
        .map_or_else(
            || until.to_string(),
            |day| day.format("%Y-%m-%d").to_string(),
        )
}
