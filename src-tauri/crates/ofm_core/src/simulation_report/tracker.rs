//! What the run itself watches while it plays, because the finished game cannot say it later:
//! which club the manager held and for which days, and the injuries seen while it played (including
//! one a player already carried). Everything else in the report is read from the game's records when the run ends.
//!
//! The watch costs one pass over the player list per day for the club's own players, and no copy
//! of the game: a stretch holds its opening cash, a list of the squad's ids and the injuries seen.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::end_of_season::UserDivisionMove;
use crate::game::Game;
use crate::match_day_eligibility::club_players;

use super::types::StretchEnd;

/// What the game already held on the run's first day when the run began. The first day is shared
/// between what the player did before pressing the button and what the run does, and every record is
/// dated by day only, so the report sets the earlier ones aside: it reports the run, not the day.
#[derive(Debug, Clone, Default)]
pub(super) struct PreRun {
    /// Ids of the club's cash-journal posts dated the first day.
    pub posts: HashSet<String>,
    /// Ids of the club's completed fixtures dated the first day.
    pub fixtures: HashSet<String>,
    /// Per player, how many of his ledger entries involving the club are dated the first day.
    pub ledger_entries: HashMap<String, usize>,
    /// How many delegation log entries are dated the first day.
    pub delegation_entries: usize,
    /// Ids of the inbox messages dated the first day.
    pub messages: HashSet<String>,
}

impl PreRun {
    fn capture(game: &Game, club_id: &str, from: &str) -> Self {
        let mut pre = Self::default();
        for post in game.cash_journal.iter() {
            if post.club_id == club_id && post.date == from {
                pre.posts.insert(post.id.clone());
            }
        }
        for fixture in game.competitions.iter().flat_map(|c| &c.fixtures) {
            let involved = fixture.home_team_id == club_id || fixture.away_team_id == club_id;
            if involved
                && fixture.date == from
                && fixture.status == domain::league::FixtureStatus::Completed
            {
                pre.fixtures.insert(fixture.id.clone());
            }
        }
        for player in &game.players {
            let same_day = player
                .movement_history
                .iter()
                .filter(|entry| {
                    entry.date == from
                        && (entry.from_team_id.as_deref() == Some(club_id)
                            || entry.to_team_id.as_deref() == Some(club_id))
                })
                .count();
            if same_day > 0 {
                pre.ledger_entries.insert(player.id.clone(), same_day);
            }
        }
        pre.delegation_entries = game
            .delegation
            .log
            .iter()
            .filter(|entry| entry.date == from)
            .count();
        for message in game.messages.iter().filter(|m| m.date == from) {
            pre.messages.insert(message.id.clone());
        }
        pre
    }
}

/// One injury the watch saw begin.
#[derive(Debug, Clone)]
pub(super) struct InjurySeen {
    pub player_id: String,
    pub player_name: String,
    pub injury: String,
    pub days_remaining: u32,
    pub first_seen: String,
    pub recovered_by: Option<String>,
}

/// A season rollover the run performed, kept small: the report needs the result, not the awards.
#[derive(Debug, Clone)]
pub(super) struct RolloverNote {
    pub date: String,
    pub season: u32,
    pub league_name: String,
    pub position: u32,
    pub total_teams: u32,
    pub points: u32,
    pub division_move: String,
}

/// How a stretch ended, with the club's books and squad at that moment.
#[derive(Debug, Clone)]
pub(super) struct StretchClose {
    pub until: String,
    pub ended: StretchEnd,
    pub closing_cash: i64,
    pub squad_after: BTreeSet<String>,
}

/// The days one club was managed.
#[derive(Debug, Clone)]
pub(super) struct Stretch {
    pub club_id: String,
    pub club_name: String,
    pub from: String,
    pub opening_cash: i64,
    pub squad_before: BTreeSet<String>,
    pub pre_run: PreRun,
    pub injuries: Vec<InjurySeen>,
    pub rollovers: Vec<RolloverNote>,
    pub days_played: u32,
    pub close: Option<StretchClose>,
    /// Index into `injuries` of each injury still open, by player id.
    open_injuries: BTreeMap<String, usize>,
}

fn clock_day(game: &Game) -> String {
    game.clock.current_date.format("%Y-%m-%d").to_string()
}

fn squad_ids(game: &Game, club_id: &str) -> BTreeSet<String> {
    club_players(&game.players, club_id)
        .into_iter()
        .map(|player| player.id.clone())
        .collect()
}

fn club_cash(game: &Game, club_id: &str) -> i64 {
    game.teams
        .iter()
        .find(|team| team.id == club_id)
        .map_or(0, |team| team.finance)
}

impl Stretch {
    fn open(game: &Game, club_id: &str) -> Self {
        let from = clock_day(game);
        Self {
            club_id: club_id.to_string(),
            club_name: game.team_name_or_id(club_id),
            pre_run: PreRun::capture(game, club_id, &from),
            from,
            opening_cash: club_cash(game, club_id),
            squad_before: squad_ids(game, club_id),
            injuries: Vec::new(),
            rollovers: Vec::new(),
            days_played: 0,
            close: None,
            open_injuries: BTreeMap::new(),
        }
    }

    fn close(&mut self, game: &Game, ended: StretchEnd) {
        self.close = Some(StretchClose {
            until: clock_day(game),
            ended,
            closing_cash: club_cash(game, &self.club_id),
            squad_after: squad_ids(game, &self.club_id),
        });
    }

    /// Notes who is injured at the end of a played day, and who has stopped being.
    fn watch_injuries(&mut self, game: &Game, day_end: &str) {
        let mut still_injured: BTreeSet<&str> = BTreeSet::new();
        for player in club_players(&game.players, &self.club_id) {
            let Some(injury) = &player.injury else {
                continue;
            };
            still_injured.insert(player.id.as_str());
            if self.open_injuries.contains_key(&player.id) {
                continue;
            }
            self.open_injuries
                .insert(player.id.clone(), self.injuries.len());
            self.injuries.push(InjurySeen {
                player_id: player.id.clone(),
                player_name: player.match_name.clone(),
                injury: injury.name.clone(),
                days_remaining: injury.days_remaining,
                first_seen: day_end.to_string(),
                recovered_by: None,
            });
        }
        let recovered: Vec<String> = self
            .open_injuries
            .keys()
            .filter(|id| !still_injured.contains(id.as_str()))
            .cloned()
            .collect();
        for id in recovered {
            if let Some(index) = self.open_injuries.remove(&id) {
                // A player who left the club while injured has not recovered at the club.
                let at_club = game
                    .players
                    .iter()
                    .any(|p| p.id == id && p.team_id.as_deref() == Some(self.club_id.as_str()));
                if at_club {
                    self.injuries[index].recovered_by = Some(clock_day(game));
                }
            }
        }
    }
}

/// The run's watch. Built before the first day; told after each day and each rollover.
#[derive(Debug, Clone)]
pub struct ReportTracker {
    pub(super) stretches: Vec<Stretch>,
}

impl ReportTracker {
    /// Opens a stretch for the club the manager holds, if any.
    pub fn begin(game: &Game) -> Self {
        let stretches = game
            .manager
            .team_id
            .as_deref()
            .map(|club| Stretch::open(game, club))
            .into_iter()
            .collect();
        Self { stretches }
    }

    fn current(&mut self) -> Option<&mut Stretch> {
        self.stretches
            .last_mut()
            .filter(|stretch| stretch.close.is_none())
    }

    /// Follows the manager's club: closes the open stretch when it ended and opens the next.
    fn follow_club(&mut self, game: &Game) {
        let now = game.manager.team_id.as_deref();
        let open_club = self.current().map(|stretch| stretch.club_id.clone());
        if open_club.as_deref() == now {
            return;
        }
        if let Some(stretch) = self.current() {
            let ended = if now.is_none() {
                StretchEnd::Dismissed
            } else {
                StretchEnd::Moved
            };
            stretch.close(game, ended);
        }
        if let Some(club) = now {
            self.stretches.push(Stretch::open(game, club));
        }
    }

    /// After a played day: the clock already reads the next day.
    pub fn after_day(&mut self, game: &Game) {
        let day_end = clock_day(game);
        if let Some(stretch) = self.current() {
            stretch.days_played += 1;
            stretch.watch_injuries(game, &day_end);
        }
        self.follow_club(game);
    }

    /// After the run rolled a season over (nothing of the day the clock reads has been played).
    pub fn after_rollover(
        &mut self,
        game: &Game,
        summary: &crate::end_of_season::EndOfSeasonSummary,
    ) {
        let date = clock_day(game);
        if let Some(stretch) = self.current() {
            stretch.rollovers.push(RolloverNote {
                date,
                season: summary.season,
                league_name: summary.league_name.clone(),
                position: summary.user_position,
                total_teams: summary.total_teams,
                points: summary.user_points,
                division_move: match summary.user_division_move {
                    UserDivisionMove::Stayed => "stayed",
                    UserDivisionMove::Promoted => "promoted",
                    UserDivisionMove::Relegated => "relegated",
                }
                .to_string(),
            });
        }
        self.follow_club(game);
    }

    /// The run ended; closes the open stretch with the books and squad as they stand.
    pub(super) fn close_open_stretch(&mut self, game: &Game) {
        if let Some(stretch) = self.current() {
            stretch.close(game, StretchEnd::Continues);
        }
    }
}
