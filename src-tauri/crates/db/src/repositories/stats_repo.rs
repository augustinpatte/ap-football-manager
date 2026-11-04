use domain::league::FixtureCompetition;
use domain::stats::{
    PlayerMatchStatsRecord, PlayerStatsTotals, StatsState, TeamMatchStatsRecord, TeamStatsTotals,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;

use super::fixture_competition::{fixture_competition_to_string, parse_fixture_competition};

const GAME_PERSISTENCE_LOAD_ERROR: &str = "be.error.gamePersistence.loadFailed";
const GAME_PERSISTENCE_WRITE_ERROR: &str = "be.error.gamePersistence.writeFailed";

/// How match history is stored (v066): ids are written once into `stat_ids` and referred to by
/// number; a match's own facts (date, competition, clubs, score) are written once in
/// `match_stat_fixtures`; the player and team lines carry only what is theirs. A record reads back
/// exactly as it was written.
pub fn replace_stats_state(conn: &Connection, stats: &StatsState) -> Result<(), String> {
    for table in [
        "player_match_lines",
        "team_match_lines",
        "match_stat_fixtures",
        "player_stat_totals",
        "team_stat_totals",
        "stat_ids",
    ] {
        conn.execute(&format!("DELETE FROM {table}"), [])
            .map_err(|_| GAME_PERSISTENCE_WRITE_ERROR.to_string())?;
    }

    insert_player_records(conn, &stats.player_matches)?;
    insert_team_records(conn, &stats.team_matches)?;
    insert_totals(conn, stats)?;
    conn.execute(
        "UPDATE stat_totals_epoch SET epoch = ?1 WHERE slot = 1",
        [stats.totals_epoch as i64],
    )
    .map_err(write_error)?;
    Ok(())
}

fn insert_totals(conn: &Connection, stats: &StatsState) -> Result<(), String> {
    let mut ids = IdTable::new(conn)?;
    let mut player = conn
        .prepare(
            "INSERT INTO player_stat_totals (player_ref, minutes_played, shots, shots_on_target,
                passes_completed, passes_attempted, tackles_won, interceptions, fouls_committed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .map_err(write_error)?;
    for t in &stats.player_totals {
        player
            .execute(params![
                ids.reference(&t.player_id)?,
                t.minutes_played,
                t.shots,
                t.shots_on_target,
                t.passes_completed,
                t.passes_attempted,
                t.tackles_won,
                t.interceptions,
                t.fouls_committed,
            ])
            .map_err(write_error)?;
    }
    let mut team = conn
        .prepare(
            "INSERT INTO team_stat_totals (team_ref, matches_played, goals_for, goals_against,
                possession_total, shots, shots_on_target, passes_completed, passes_attempted,
                tackles_won, interceptions, fouls_committed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .map_err(write_error)?;
    for t in &stats.team_totals {
        team.execute(params![
            ids.reference(&t.team_id)?,
            t.matches_played,
            t.goals_for,
            t.goals_against,
            t.possession_total,
            t.shots,
            t.shots_on_target,
            t.passes_completed,
            t.passes_attempted,
            t.tackles_won,
            t.interceptions,
            t.fouls_committed,
        ])
        .map_err(write_error)?;
    }
    Ok(())
}

fn write_error<E>(_: E) -> String {
    GAME_PERSISTENCE_WRITE_ERROR.to_string()
}

fn load_error<E>(_: E) -> String {
    GAME_PERSISTENCE_LOAD_ERROR.to_string()
}

/// The reference of `id`, written the first time it is met.
struct IdTable<'conn> {
    cache: HashMap<String, i64>,
    find: rusqlite::Statement<'conn>,
    add: rusqlite::Statement<'conn>,
}

impl<'conn> IdTable<'conn> {
    fn new(conn: &'conn Connection) -> Result<Self, String> {
        Ok(Self {
            cache: HashMap::new(),
            find: conn
                .prepare("SELECT ref FROM stat_ids WHERE id = ?1")
                .map_err(write_error)?,
            add: conn
                .prepare("INSERT INTO stat_ids (id) VALUES (?1)")
                .map_err(write_error)?,
        })
    }

    fn reference(&mut self, id: &str) -> Result<i64, String> {
        if let Some(found) = self.cache.get(id) {
            return Ok(*found);
        }
        let stored: Option<i64> = self
            .find
            .query_row([id], |row| row.get(0))
            .optional()
            .map_err(write_error)?;
        let reference = match stored {
            Some(reference) => reference,
            None => {
                self.add.execute([id]).map_err(write_error)?;
                self.find
                    .query_row([id], |row| row.get(0))
                    .map_err(write_error)?
            }
        };
        self.cache.insert(id.to_string(), reference);
        Ok(reference)
    }
}

/// What a match's records say about the match itself.
#[derive(PartialEq, Eq, Hash, Clone)]
struct MatchFacts {
    fixture_ref: i64,
    season: u32,
    matchday: u32,
    date: String,
    competition: String,
    home_ref: i64,
    away_ref: i64,
    home_goals: u8,
    away_goals: u8,
}

struct Matches<'conn> {
    ids: IdTable<'conn>,
    cache: HashMap<MatchFacts, i64>,
    find: rusqlite::Statement<'conn>,
    add: rusqlite::Statement<'conn>,
}

impl<'conn> Matches<'conn> {
    fn new(conn: &'conn Connection) -> Result<Self, String> {
        Ok(Self {
            ids: IdTable::new(conn)?,
            cache: HashMap::new(),
            find: conn
                .prepare(
                    "SELECT fixture_no FROM match_stat_fixtures
                     WHERE fixture_ref = ?1 AND season = ?2 AND matchday = ?3 AND date = ?4
                       AND competition = ?5 AND home_ref = ?6 AND away_ref = ?7
                       AND home_goals = ?8 AND away_goals = ?9",
                )
                .map_err(write_error)?,
            add: conn
                .prepare(
                    "INSERT INTO match_stat_fixtures (fixture_ref, season, matchday, date,
                        competition, home_ref, away_ref, home_goals, away_goals)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )
                .map_err(write_error)?,
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the nine facts a record states about its match; owner=#589"
    )]
    fn number(
        &mut self,
        fixture_id: &str,
        season: u32,
        matchday: u32,
        date: &str,
        competition: &FixtureCompetition,
        (home_id, away_id): (&str, &str),
        (home_goals, away_goals): (u8, u8),
    ) -> Result<i64, String> {
        let facts = MatchFacts {
            fixture_ref: self.ids.reference(fixture_id)?,
            season,
            matchday,
            date: date.to_string(),
            competition: fixture_competition_to_string(competition),
            home_ref: self.ids.reference(home_id)?,
            away_ref: self.ids.reference(away_id)?,
            home_goals,
            away_goals,
        };
        if let Some(number) = self.cache.get(&facts) {
            return Ok(*number);
        }
        let parameters = params![
            facts.fixture_ref,
            facts.season,
            facts.matchday,
            facts.date,
            facts.competition,
            facts.home_ref,
            facts.away_ref,
            facts.home_goals,
            facts.away_goals,
        ];
        let stored: Option<i64> = self
            .find
            .query_row(parameters, |row| row.get(0))
            .optional()
            .map_err(write_error)?;
        let number = match stored {
            Some(number) => number,
            None => {
                self.add.execute(parameters).map_err(write_error)?;
                self.find
                    .query_row(parameters, |row| row.get(0))
                    .map_err(write_error)?
            }
        };
        self.cache.insert(facts, number);
        Ok(number)
    }
}

fn insert_player_records(
    conn: &Connection,
    records: &[PlayerMatchStatsRecord],
) -> Result<(), String> {
    let mut matches = Matches::new(conn)?;
    let mut insert = conn
        .prepare(
            "INSERT INTO player_match_lines (
                fixture_no, player_ref, team_ref, opponent_ref, minutes_played, goals, assists,
                shots, shots_on_target, passes_completed, passes_attempted, tackles_won,
                interceptions, fouls_committed, yellow_cards, red_cards, rating
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )
        .map_err(write_error)?;
    for record in records {
        let fixture_no = matches.number(
            &record.fixture_id,
            record.season,
            record.matchday,
            &record.date,
            &record.competition,
            (&record.home_team_id, &record.away_team_id),
            (record.home_goals, record.away_goals),
        )?;
        insert
            .execute(params![
                fixture_no,
                matches.ids.reference(&record.player_id)?,
                matches.ids.reference(&record.team_id)?,
                matches.ids.reference(&record.opponent_team_id)?,
                record.minutes_played,
                record.goals,
                record.assists,
                record.shots,
                record.shots_on_target,
                record.passes_completed,
                record.passes_attempted,
                record.tackles_won,
                record.interceptions,
                record.fouls_committed,
                record.yellow_cards,
                record.red_cards,
                record.rating,
            ])
            .map_err(write_error)?;
    }

    Ok(())
}

fn insert_team_records(conn: &Connection, records: &[TeamMatchStatsRecord]) -> Result<(), String> {
    let mut matches = Matches::new(conn)?;
    let mut insert = conn
        .prepare(
            "INSERT INTO team_match_lines (
                fixture_no, team_ref, opponent_ref, goals_for, goals_against, possession_pct,
                shots, shots_on_target, passes_completed, passes_attempted, tackles_won,
                interceptions, fouls_committed, yellow_cards, red_cards
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )
        .map_err(write_error)?;
    for record in records {
        // A team line states its goals as for and against; the match's score is home first.
        let at_home = record.team_id == record.home_team_id;
        let score = if at_home {
            (record.goals_for, record.goals_against)
        } else {
            (record.goals_against, record.goals_for)
        };
        let fixture_no = matches.number(
            &record.fixture_id,
            record.season,
            record.matchday,
            &record.date,
            &record.competition,
            (&record.home_team_id, &record.away_team_id),
            score,
        )?;
        insert
            .execute(params![
                fixture_no,
                matches.ids.reference(&record.team_id)?,
                matches.ids.reference(&record.opponent_team_id)?,
                record.goals_for,
                record.goals_against,
                record.possession_pct,
                record.shots,
                record.shots_on_target,
                record.passes_completed,
                record.passes_attempted,
                record.tackles_won,
                record.interceptions,
                record.fouls_committed,
                record.yellow_cards,
                record.red_cards,
            ])
            .map_err(write_error)?;
    }

    Ok(())
}

/// Writes the match records the file does not hold yet.
///
/// The history only ever grows by appending, and rows are written in order, so the file's newest
/// row (`MAX(rowid)`) says how many records it already holds: those are skipped and only the tail
/// is inserted. Before trusting that, the newest row must be the record it should be; a file that
/// holds more than memory, or a different history, is rewritten in full instead of appended to.
pub fn persist_stats_state(conn: &Connection, stats: &StatsState) -> Result<(), String> {
    // A fold removes lines from anywhere in the history, which an append-only prefix check cannot
    // see: the file is rewritten whenever its totals are from another epoch than memory's.
    let stored_epoch: i64 = conn
        .query_row(
            "SELECT epoch FROM stat_totals_epoch WHERE slot = 1",
            [],
            |row| row.get(0),
        )
        .map_err(write_error)?;
    if stored_epoch != stats.totals_epoch as i64 {
        return replace_stats_state(conn, stats);
    }
    let players_held = held_prefix(
        conn,
        "SELECT MAX(rowid) FROM player_match_lines",
        "SELECT fx.id, pl.id FROM player_match_lines l
         JOIN match_stat_fixtures f ON f.fixture_no = l.fixture_no
         JOIN stat_ids fx ON fx.ref = f.fixture_ref
         JOIN stat_ids pl ON pl.ref = l.player_ref
         WHERE l.rowid = ?1",
        &stats.player_matches,
        |r| (r.fixture_id.as_str(), r.player_id.as_str()),
    )?;
    let teams_held = held_prefix(
        conn,
        "SELECT MAX(rowid) FROM team_match_lines",
        "SELECT fx.id, tm.id FROM team_match_lines l
         JOIN match_stat_fixtures f ON f.fixture_no = l.fixture_no
         JOIN stat_ids fx ON fx.ref = f.fixture_ref
         JOIN stat_ids tm ON tm.ref = l.team_ref
         WHERE l.rowid = ?1",
        &stats.team_matches,
        |r| (r.fixture_id.as_str(), r.team_id.as_str()),
    )?;
    match (players_held, teams_held) {
        (Some(players), Some(teams)) => {
            insert_player_records(conn, &stats.player_matches[players..])?;
            insert_team_records(conn, &stats.team_matches[teams..])
        }
        _ => replace_stats_state(conn, stats),
    }
}

/// How many of `records` the table already holds, or `None` when the table is not a prefix of them.
///
/// The prefix is judged by its newest row alone: the table's `MAX(rowid)` is taken as the number of
/// rows it holds (a rewrite empties the table first, so the ids are contiguous from 1) and that row
/// must be the record at the same place in memory. That is enough because
/// saves are the only writer and only ever append, and the load reads the table in `rowid` order, so
/// memory keeps the order the file has. A file altered in the middle by something else would not be
/// noticed here.
fn held_prefix<T>(
    conn: &Connection,
    newest_row: &str,
    row_identity: &str,
    records: &[T],
    key: impl Fn(&T) -> (&str, &str),
) -> Result<Option<usize>, String> {
    let newest: Option<i64> = conn
        .query_row(newest_row, [], |row| row.get(0))
        .map_err(write_error)?;
    let Some(held) = newest else {
        return Ok(Some(0));
    };
    let held = usize::try_from(held).map_err(write_error)?;
    let Some(last_held) = held.checked_sub(1).and_then(|index| records.get(index)) else {
        return Ok(None);
    };
    let stored: Option<(String, String)> = conn
        .query_row(row_identity, [held as i64], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()
        .map_err(write_error)?;
    let (fixture, subject) = key(last_held);
    Ok(
        (stored.as_ref().map(|(f, s)| (f.as_str(), s.as_str())) == Some((fixture, subject)))
            .then_some(held),
    )
}

/// Every id the history refers to, by its reference.
fn load_ids(conn: &Connection) -> Result<HashMap<i64, String>, String> {
    let mut statement = conn
        .prepare("SELECT ref, id FROM stat_ids")
        .map_err(load_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(load_error)?;
    rows.collect::<Result<_, _>>().map_err(load_error)
}

/// What a match's row says about the match, as loaded.
struct LoadedMatch {
    fixture_id: String,
    season: u32,
    matchday: u32,
    date: String,
    competition: FixtureCompetition,
    home_team_id: String,
    away_team_id: String,
    home_goals: u8,
    away_goals: u8,
}

fn load_matches(
    conn: &Connection,
    ids: &HashMap<i64, String>,
) -> Result<HashMap<i64, LoadedMatch>, String> {
    let id = |reference: i64| ids.get(&reference).cloned().ok_or_else(|| load_error(()));
    let mut statement = conn
        .prepare(
            "SELECT fixture_no, fixture_ref, season, matchday, date, competition, home_ref,
                    away_ref, home_goals, away_goals
             FROM match_stat_fixtures",
        )
        .map_err(load_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, u8>(8)?,
                row.get::<_, u8>(9)?,
            ))
        })
        .map_err(load_error)?;
    let mut matches = HashMap::new();
    for row in rows {
        let (number, fixture, season, matchday, date, competition, home, away, hg, ag) =
            row.map_err(load_error)?;
        matches.insert(
            number,
            LoadedMatch {
                fixture_id: id(fixture)?,
                season,
                matchday,
                date,
                competition: parse_fixture_competition(&competition),
                home_team_id: id(home)?,
                away_team_id: id(away)?,
                home_goals: hg,
                away_goals: ag,
            },
        );
    }
    Ok(matches)
}

pub fn load_stats_state(conn: &Connection) -> Result<StatsState, String> {
    let ids = load_ids(conn)?;
    let matches = load_matches(conn, &ids)?;
    let id = |reference: i64| ids.get(&reference).cloned().ok_or_else(|| load_error(()));

    let mut player_stmt = conn
        .prepare(
            "SELECT fixture_no, player_ref, team_ref, opponent_ref, minutes_played, goals, assists,
                    shots, shots_on_target, passes_completed, passes_attempted, tackles_won,
                    interceptions, fouls_committed, yellow_cards, red_cards, rating
             FROM player_match_lines
             ORDER BY rowid",
        )
        .map_err(load_error)?;
    let mut player_matches = Vec::new();
    let mut rows = player_stmt.query([]).map_err(load_error)?;
    while let Some(row) = rows.next().map_err(load_error)? {
        let facts = matches
            .get(&row.get::<_, i64>(0).map_err(load_error)?)
            .ok_or_else(|| load_error(()))?;
        player_matches.push(PlayerMatchStatsRecord {
            fixture_id: facts.fixture_id.clone(),
            season: facts.season,
            matchday: facts.matchday,
            date: facts.date.clone(),
            competition: facts.competition.clone(),
            player_id: id(row.get(1).map_err(load_error)?)?,
            team_id: id(row.get(2).map_err(load_error)?)?,
            opponent_team_id: id(row.get(3).map_err(load_error)?)?,
            home_team_id: facts.home_team_id.clone(),
            away_team_id: facts.away_team_id.clone(),
            home_goals: facts.home_goals,
            away_goals: facts.away_goals,
            minutes_played: row.get(4).map_err(load_error)?,
            goals: row.get(5).map_err(load_error)?,
            assists: row.get(6).map_err(load_error)?,
            shots: row.get(7).map_err(load_error)?,
            shots_on_target: row.get(8).map_err(load_error)?,
            passes_completed: row.get(9).map_err(load_error)?,
            passes_attempted: row.get(10).map_err(load_error)?,
            tackles_won: row.get(11).map_err(load_error)?,
            interceptions: row.get(12).map_err(load_error)?,
            fouls_committed: row.get(13).map_err(load_error)?,
            yellow_cards: row.get(14).map_err(load_error)?,
            red_cards: row.get(15).map_err(load_error)?,
            rating: row.get(16).map_err(load_error)?,
        });
    }

    let mut team_stmt = conn
        .prepare(
            "SELECT fixture_no, team_ref, opponent_ref, goals_for, goals_against, possession_pct,
                    shots, shots_on_target, passes_completed, passes_attempted, tackles_won,
                    interceptions, fouls_committed, yellow_cards, red_cards
             FROM team_match_lines
             ORDER BY rowid",
        )
        .map_err(load_error)?;
    let mut team_matches = Vec::new();
    let mut rows = team_stmt.query([]).map_err(load_error)?;
    while let Some(row) = rows.next().map_err(load_error)? {
        let facts = matches
            .get(&row.get::<_, i64>(0).map_err(load_error)?)
            .ok_or_else(|| load_error(()))?;
        team_matches.push(TeamMatchStatsRecord {
            fixture_id: facts.fixture_id.clone(),
            season: facts.season,
            matchday: facts.matchday,
            date: facts.date.clone(),
            competition: facts.competition.clone(),
            team_id: id(row.get(1).map_err(load_error)?)?,
            opponent_team_id: id(row.get(2).map_err(load_error)?)?,
            home_team_id: facts.home_team_id.clone(),
            away_team_id: facts.away_team_id.clone(),
            goals_for: row.get(3).map_err(load_error)?,
            goals_against: row.get(4).map_err(load_error)?,
            possession_pct: row.get(5).map_err(load_error)?,
            shots: row.get(6).map_err(load_error)?,
            shots_on_target: row.get(7).map_err(load_error)?,
            passes_completed: row.get(8).map_err(load_error)?,
            passes_attempted: row.get(9).map_err(load_error)?,
            tackles_won: row.get(10).map_err(load_error)?,
            interceptions: row.get(11).map_err(load_error)?,
            fouls_committed: row.get(12).map_err(load_error)?,
            yellow_cards: row.get(13).map_err(load_error)?,
            red_cards: row.get(14).map_err(load_error)?,
        });
    }

    let (player_totals, team_totals, totals_epoch) = load_totals(conn, &ids)?;
    let mut stats = StatsState::with_lines(player_matches, team_matches);
    stats.player_totals = player_totals;
    stats.team_totals = team_totals;
    stats.totals_epoch = totals_epoch;
    Ok(stats)
}

type LoadedTotals = (Vec<PlayerStatsTotals>, Vec<TeamStatsTotals>, u64);

fn load_totals(conn: &Connection, ids: &HashMap<i64, String>) -> Result<LoadedTotals, String> {
    let id = |reference: i64| ids.get(&reference).cloned().ok_or_else(|| load_error(()));
    let mut player_totals = Vec::new();
    let mut statement = conn
        .prepare(
            "SELECT player_ref, minutes_played, shots, shots_on_target, passes_completed,
                    passes_attempted, tackles_won, interceptions, fouls_committed
             FROM player_stat_totals ORDER BY player_ref",
        )
        .map_err(load_error)?;
    let mut rows = statement.query([]).map_err(load_error)?;
    while let Some(row) = rows.next().map_err(load_error)? {
        player_totals.push(PlayerStatsTotals {
            player_id: id(row.get(0).map_err(load_error)?)?,
            minutes_played: row.get(1).map_err(load_error)?,
            shots: row.get(2).map_err(load_error)?,
            shots_on_target: row.get(3).map_err(load_error)?,
            passes_completed: row.get(4).map_err(load_error)?,
            passes_attempted: row.get(5).map_err(load_error)?,
            tackles_won: row.get(6).map_err(load_error)?,
            interceptions: row.get(7).map_err(load_error)?,
            fouls_committed: row.get(8).map_err(load_error)?,
        });
    }
    let mut team_totals = Vec::new();
    let mut statement = conn
        .prepare(
            "SELECT team_ref, matches_played, goals_for, goals_against, possession_total, shots,
                    shots_on_target, passes_completed, passes_attempted, tackles_won,
                    interceptions, fouls_committed
             FROM team_stat_totals ORDER BY team_ref",
        )
        .map_err(load_error)?;
    let mut rows = statement.query([]).map_err(load_error)?;
    while let Some(row) = rows.next().map_err(load_error)? {
        team_totals.push(TeamStatsTotals {
            team_id: id(row.get(0).map_err(load_error)?)?,
            matches_played: row.get(1).map_err(load_error)?,
            goals_for: row.get(2).map_err(load_error)?,
            goals_against: row.get(3).map_err(load_error)?,
            possession_total: row.get(4).map_err(load_error)?,
            shots: row.get(5).map_err(load_error)?,
            shots_on_target: row.get(6).map_err(load_error)?,
            passes_completed: row.get(7).map_err(load_error)?,
            passes_attempted: row.get(8).map_err(load_error)?,
            tackles_won: row.get(9).map_err(load_error)?,
            interceptions: row.get(10).map_err(load_error)?,
            fouls_committed: row.get(11).map_err(load_error)?,
        });
    }
    let epoch: i64 = conn
        .query_row(
            "SELECT epoch FROM stat_totals_epoch WHERE slot = 1",
            [],
            |row| row.get(0),
        )
        .map_err(load_error)?;
    player_totals.sort_by(|a, b| a.player_id.cmp(&b.player_id));
    team_totals.sort_by(|a, b| a.team_id.cmp(&b.team_id));
    Ok((player_totals, team_totals, epoch as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_database::GameDatabase;
    use domain::league::FixtureCompetition;
    use rusqlite::Connection;

    fn test_db() -> GameDatabase {
        GameDatabase::open_in_memory().unwrap()
    }

    fn sample_stats_state() -> StatsState {
        StatsState {
            player_matches: vec![PlayerMatchStatsRecord {
                fixture_id: "fixture-001".to_string(),
                season: 2025,
                matchday: 1,
                date: "2025-08-10".to_string(),
                competition: FixtureCompetition::League,
                player_id: "player-001".to_string(),
                team_id: "team-001".to_string(),
                opponent_team_id: "team-002".to_string(),
                home_team_id: "team-001".to_string(),
                away_team_id: "team-002".to_string(),
                home_goals: 2,
                away_goals: 1,
                minutes_played: 90,
                goals: 1,
                assists: 0,
                shots: 3,
                shots_on_target: 2,
                passes_completed: 18,
                passes_attempted: 22,
                tackles_won: 1,
                interceptions: 0,
                fouls_committed: 1,
                yellow_cards: 0,
                red_cards: 0,
                rating: 7.4,
            }],
            team_matches: vec![TeamMatchStatsRecord {
                fixture_id: "fixture-001".to_string(),
                season: 2025,
                matchday: 1,
                date: "2025-08-10".to_string(),
                competition: FixtureCompetition::League,
                team_id: "team-001".to_string(),
                opponent_team_id: "team-002".to_string(),
                home_team_id: "team-001".to_string(),
                away_team_id: "team-002".to_string(),
                goals_for: 2,
                goals_against: 1,
                possession_pct: 54,
                shots: 12,
                shots_on_target: 5,
                passes_completed: 310,
                passes_attempted: 360,
                tackles_won: 14,
                interceptions: 8,
                fouls_committed: 10,
                yellow_cards: 1,
                red_cards: 0,
            }],
            ..StatsState::default()
        }
    }

    #[test]
    fn test_replace_stats_state_roundtrip() {
        let db = test_db();
        let stats = sample_stats_state();

        replace_stats_state(db.conn(), &stats).unwrap();
        let loaded = load_stats_state(db.conn()).unwrap();

        assert_eq!(loaded.player_matches, stats.player_matches);
        assert_eq!(loaded.team_matches, stats.team_matches);
    }

    fn player_row(fixture: &str, player: &str) -> PlayerMatchStatsRecord {
        PlayerMatchStatsRecord {
            fixture_id: fixture.to_string(),
            player_id: player.to_string(),
            ..sample_stats_state().player_matches[0].clone()
        }
    }

    fn team_row(fixture: &str, team: &str) -> TeamMatchStatsRecord {
        TeamMatchStatsRecord {
            fixture_id: fixture.to_string(),
            team_id: team.to_string(),
            ..sample_stats_state().team_matches[0].clone()
        }
    }

    /// One played match, 2-1 to the home side: 22 players and the two teams, all of them saying the
    /// same about the match, as the records a real match produces do.
    fn a_match(fixture: &str) -> StatsState {
        let (home, away) = (format!("{fixture}-home"), format!("{fixture}-away"));
        let mut played = StatsState {
            player_matches: (0..22)
                .map(|n| player_row(fixture, &format!("{fixture}-p{n}")))
                .collect(),
            team_matches: vec![team_row(fixture, &home), team_row(fixture, &away)],
            ..StatsState::default()
        };
        for record in &mut played.player_matches {
            record.home_team_id = home.clone();
            record.away_team_id = away.clone();
            record.team_id = home.clone();
            record.opponent_team_id = away.clone();
        }
        for (record, (goals_for, goals_against, opponent)) in played
            .team_matches
            .iter_mut()
            .zip([(2, 1, &away), (1, 2, &home)])
        {
            record.home_team_id = home.clone();
            record.away_team_id = away.clone();
            record.opponent_team_id = opponent.clone();
            record.goals_for = goals_for;
            record.goals_against = goals_against;
        }
        played
    }

    fn history_of(matches: usize) -> StatsState {
        let mut stats = StatsState::default();
        for n in 0..matches {
            stats.append(a_match(&format!("fixture-{n:04}")));
        }
        stats
    }

    /// The records of two states, whatever order a load returns them in.
    fn assert_same_records(left: &StatsState, right: &StatsState) {
        let players = |s: &StatsState| {
            let mut keys: Vec<_> = s
                .player_matches
                .iter()
                .map(|r| (r.fixture_id.clone(), r.player_id.clone()))
                .collect();
            keys.sort();
            keys
        };
        let teams = |s: &StatsState| {
            let mut keys: Vec<_> = s
                .team_matches
                .iter()
                .map(|r| (r.fixture_id.clone(), r.team_id.clone()))
                .collect();
            keys.sort();
            keys
        };
        assert_eq!(players(left), players(right));
        assert_eq!(teams(left), teams(right));
    }

    fn rows_written_by(conn: &Connection, save: impl FnOnce()) -> u64 {
        let before = conn.total_changes();
        save();
        conn.total_changes() - before
    }

    /// Given a history already saved, when the same history is saved again,
    /// then nothing is written.
    #[test]
    fn saving_an_unchanged_history_writes_nothing() {
        let db = test_db();
        let stats = history_of(40);
        persist_stats_state(db.conn(), &stats).unwrap();

        let written = rows_written_by(db.conn(), || {
            persist_stats_state(db.conn(), &stats).unwrap();
        });

        assert_eq!(written, 0);
    }

    fn table_rows(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    /// What saving one more match adds, table by table.
    fn growth_from_one_more_match(history: usize) -> [i64; 4] {
        let db = test_db();
        let mut stats = history_of(history);
        persist_stats_state(db.conn(), &stats).unwrap();
        let tables = [
            "player_match_lines",
            "team_match_lines",
            "match_stat_fixtures",
            "stat_ids",
        ];
        let before = tables.map(|t| table_rows(db.conn(), t));
        stats.append(a_match("fixture-new"));
        persist_stats_state(db.conn(), &stats).unwrap();
        assert_same_records(&load_stats_state(db.conn()).unwrap(), &stats);
        let after = tables.map(|t| table_rows(db.conn(), t));
        [0, 1, 2, 3].map(|i| after[i] - before[i])
    }

    /// Given a saved history and one more match played, when it is saved, then only that match is
    /// written: its 22 player lines, its 2 team lines, its one row of match facts and the ids it is
    /// the first to name (its fixture, its 22 players and its 2 clubs) — the same however long the
    /// history is.
    #[test]
    fn saving_one_more_match_writes_only_that_match() {
        let short = growth_from_one_more_match(10);
        assert_eq!(short, [22, 2, 1, 1 + 22 + 2]);
        assert_eq!(growth_from_one_more_match(60), short);
    }

    /// Given a history saved, loaded, extended and saved again, when the file is read through a
    /// second connection, then every record is there exactly once.
    #[test]
    fn nothing_is_lost_or_duplicated_across_save_reload_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.db");
        let writer = GameDatabase::open(&path).unwrap();
        persist_stats_state(writer.conn(), &history_of(10)).unwrap();

        let mut reloaded = load_stats_state(writer.conn()).unwrap();
        reloaded.append(a_match("fixture-after-reload"));
        persist_stats_state(writer.conn(), &reloaded).unwrap();
        persist_stats_state(writer.conn(), &reloaded).unwrap();

        let reader = Connection::open(&path).unwrap();
        let count = |sql: &str| {
            reader
                .query_row(sql, [], |row| row.get::<_, i64>(0))
                .unwrap()
        };
        assert_eq!(count("SELECT COUNT(*) FROM player_match_lines"), 11 * 22);
        assert_eq!(
            count(
                "SELECT COUNT(*) FROM (SELECT DISTINCT fixture_no, player_ref FROM player_match_lines)"
            ),
            11 * 22
        );
        assert_eq!(count("SELECT COUNT(*) FROM team_match_lines"), 11 * 2);
        assert_eq!(
            count(
                "SELECT COUNT(*) FROM (SELECT DISTINCT fixture_no, team_ref FROM team_match_lines)"
            ),
            11 * 2
        );
    }

    /// Given a file holding different records than memory, when memory is saved, then the file
    /// ends up holding exactly memory: a file that disagrees is rewritten, not appended to.
    #[test]
    fn a_file_that_disagrees_with_memory_is_rewritten_in_full() {
        let db = test_db();
        persist_stats_state(db.conn(), &history_of(5)).unwrap();
        let mut elsewhere = StatsState::default();
        for n in 0..8 {
            elsewhere.append(a_match(&format!("other-{n}")));
        }

        persist_stats_state(db.conn(), &elsewhere).unwrap();

        let loaded = load_stats_state(db.conn()).unwrap();
        assert_eq!(loaded.player_matches.len(), elsewhere.player_matches.len());
        assert!(
            loaded
                .player_matches
                .iter()
                .all(|r| r.fixture_id.starts_with("other-"))
        );
    }

    /// A match played on `date`, with the fixture named so that it sorts by name the same way.
    fn a_match_on(fixture: &str, date: &str) -> StatsState {
        let mut played = a_match(fixture);
        for record in &mut played.player_matches {
            record.date = date.to_string();
        }
        for record in &mut played.team_matches {
            record.date = date.to_string();
        }
        played
    }

    /// Given ten matches saved in the order they were played, the last one a postponed fixture
    /// dated before the others,
    /// When the file is loaded and the loaded state is saved again,
    /// Then nothing is written, and one more match writes only its own rows (24 lines, its match
    /// row and the 25 ids it is the first to name).
    #[test]
    fn the_first_save_after_a_load_writes_nothing() {
        let db = test_db();
        let mut stats = StatsState::default();
        for day in 10..19 {
            stats.append(a_match_on(
                &format!("fixture-{day}"),
                &format!("2025-08-{day}"),
            ));
        }
        stats.append(a_match_on("fixture-postponed", "2025-08-01"));
        persist_stats_state(db.conn(), &stats).unwrap();

        let mut loaded = load_stats_state(db.conn()).unwrap();
        let after_load = rows_written_by(db.conn(), || {
            persist_stats_state(db.conn(), &loaded).unwrap();
        });
        loaded.append(a_match_on("fixture-next", "2025-08-20"));
        let after_one_more = rows_written_by(db.conn(), || {
            persist_stats_state(db.conn(), &loaded).unwrap();
        });

        assert_eq!(
            after_load, 0,
            "the first save after a load rewrote the history"
        );
        assert_eq!(after_one_more, 24 + 1 + 25);
    }

    /// Given a file holding more than memory, when memory is saved, then the extra is removed.
    #[test]
    fn a_shorter_history_removes_what_it_no_longer_holds() {
        let db = test_db();
        persist_stats_state(db.conn(), &history_of(9)).unwrap();
        let shorter = history_of(4);

        persist_stats_state(db.conn(), &shorter).unwrap();

        assert_same_records(&load_stats_state(db.conn()).unwrap(), &shorter);
    }

    /// Given a file already holding a history of 400, 2,000 and 10,000 matches (24 rows each),
    /// When one more match is saved the old way (rewrite everything) and the new way,
    /// Then the time of each is printed: the new one stays flat as the history grows.
    ///
    /// `cargo test --release -p db --lib a_save_costs_the_new_match_not_the_history -- --ignored --nocapture`
    #[test]
    #[ignore = "measurement, run by hand in release mode"]
    fn a_save_costs_the_new_match_not_the_history() {
        use std::time::Instant;

        for held in [400, 2_000, 10_000] {
            let mut stats = history_of(held);
            let rewriting = test_db();
            let incremental = test_db();
            replace_stats_state(rewriting.conn(), &stats).unwrap();
            persist_stats_state(incremental.conn(), &stats).unwrap();
            stats.append(a_match("fixture-newest"));

            let started = Instant::now();
            replace_stats_state(rewriting.conn(), &stats).unwrap();
            let rewrite = started.elapsed();
            let started = Instant::now();
            persist_stats_state(incremental.conn(), &stats).unwrap();
            let incremental_time = started.elapsed();

            eprintln!(
                "{held} matches held: rewrite {rewrite:.2?}, incremental {incremental_time:.2?}"
            );
        }
    }

    #[test]
    fn test_replace_stats_state_returns_backend_key_when_schema_is_missing() {
        let conn = Connection::open_in_memory().unwrap();
        let stats = StatsState::default();

        let result = replace_stats_state(&conn, &stats);

        assert_eq!(result.unwrap_err(), GAME_PERSISTENCE_WRITE_ERROR);
    }

    #[test]
    fn test_load_stats_state_returns_backend_key_when_schema_is_missing() {
        let conn = Connection::open_in_memory().unwrap();

        let result = load_stats_state(&conn);

        assert_eq!(result.unwrap_err(), GAME_PERSISTENCE_LOAD_ERROR);
    }

    /// A match of `season`, in the shape `a_match` plays one.
    fn a_match_of_season(season: u32, number: usize) -> StatsState {
        let mut played = a_match(&format!("fixture-{season}-{number}"));
        // The same twenty-two players and two clubs every match, as a career has: a player with
        // few lines keeps them whatever their age.
        for (index, record) in played.player_matches.iter_mut().enumerate() {
            record.season = season;
            record.player_id = format!("player-{index}");
            record.team_id = "home".to_string();
            record.opponent_team_id = "away".to_string();
            record.home_team_id = "home".to_string();
            record.away_team_id = "away".to_string();
        }
        for (record, (team, opponent)) in played
            .team_matches
            .iter_mut()
            .zip([("home", "away"), ("away", "home")])
        {
            record.season = season;
            record.team_id = team.to_string();
            record.opponent_team_id = opponent.to_string();
            record.home_team_id = "home".to_string();
            record.away_team_id = "away".to_string();
        }
        played
    }

    /// Given a career saved while it was playing its seasons, one match after another, so that the
    /// seasons beyond the five kept were folded into totals along the way,
    /// When it is saved and loaded,
    /// Then the lines, the totals and the epoch come back as they were, and a second save writes
    /// nothing; the file holds no line of a folded season.
    #[test]
    fn folded_totals_survive_save_and_load() {
        let db = test_db();
        let mut stats = StatsState::default();
        for season in 2030..2038 {
            for number in 0..3 {
                stats.append(a_match_of_season(season, number));
                // Saved after every match, as a career that is played is.
                persist_stats_state(db.conn(), &stats).unwrap();
            }
        }
        assert!(!stats.player_totals.is_empty() && stats.totals_epoch > 0);

        let loaded = load_stats_state(db.conn()).unwrap();

        assert_eq!(loaded.player_matches, stats.player_matches);
        assert_eq!(loaded.team_matches, stats.team_matches);
        assert_eq!(loaded.player_totals, stats.player_totals);
        assert_eq!(loaded.team_totals, stats.team_totals);
        assert_eq!(loaded.totals_epoch, stats.totals_epoch);
        assert!(loaded.player_matches.iter().all(|r| r.season >= 2033));
        let again = rows_written_by(db.conn(), || {
            persist_stats_state(db.conn(), &loaded).unwrap();
        });
        assert_eq!(again, 0, "nothing new to write after a load");
    }

    /// Given a file written before a fold,
    /// When memory has folded (lines gone from the middle of its history) and is saved,
    /// Then the file is rewritten to match, not appended to: no folded line is left behind and no
    /// line is written twice.
    #[test]
    fn a_fold_makes_the_next_save_a_full_rewrite() {
        let db = test_db();
        let mut stats = StatsState::default();
        for season in 2030..2035 {
            stats.append(a_match_of_season(season, 0));
        }
        persist_stats_state(db.conn(), &stats).unwrap();
        assert_eq!(table_rows(db.conn(), "player_match_lines"), 5 * 22);

        stats.append(a_match_of_season(2035, 0));
        persist_stats_state(db.conn(), &stats).unwrap();

        let loaded = load_stats_state(db.conn()).unwrap();
        assert_eq!(loaded.player_matches, stats.player_matches);
        assert_eq!(loaded.player_totals, stats.player_totals);
        assert_eq!(
            table_rows(db.conn(), "player_match_lines") as usize,
            stats.player_matches.len()
        );
        assert!(loaded.player_matches.iter().all(|r| r.season >= 2031));
    }

    /// Given a file written at one epoch of folding and a memory at a later one whose lines are the
    /// same ones (the totals are what the fold changed),
    /// When memory is saved,
    /// Then the file's totals are rewritten: the append path alone would leave the old totals behind.
    #[test]
    fn totals_of_a_later_epoch_are_written_even_when_the_lines_did_not_move() {
        let db = test_db();
        let mut stats = history_of(3);
        persist_stats_state(db.conn(), &stats).unwrap();

        stats.player_totals = vec![PlayerStatsTotals {
            player_id: "fixture-0000-p0".to_string(),
            minutes_played: 4_500,
            ..PlayerStatsTotals::default()
        }];
        stats.totals_epoch = 1;
        persist_stats_state(db.conn(), &stats).unwrap();

        let loaded = load_stats_state(db.conn()).unwrap();
        assert_eq!(loaded.player_totals, stats.player_totals);
        assert_eq!(loaded.totals_epoch, 1);
        assert_eq!(loaded.player_matches.len(), stats.player_matches.len());
    }
}
