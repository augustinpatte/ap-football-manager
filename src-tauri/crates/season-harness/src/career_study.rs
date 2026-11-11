//! What a long career does to the world, season by season (P4.5). Measurement only.
//!
//! The squad, finance and world-health logs already answer "what do the squads and books look like
//! at season N". This adds what they do not: who is managing, whether the pyramid keeps its shape,
//! how clubs set themselves up, and what happens to loans and prospects, each as one row per season
//! so that a drift shows as a trend and not as two snapshots.
//!
//! It reads the game through public API only and decides nothing.

use std::collections::{HashMap, HashSet};

use domain::league::{CompetitionFormat, CompetitionScope, CompetitionType, League};
use domain::player::{Player, Position, SquadRole};
use ofm_core::finances::wage_bills_by_club;
use ofm_core::game::Game;

use crate::driver::SeasonObserver;

/// A loan that lasts at least this many days and gives under [`IDLE_MINUTES`] is idle.
const IDLE_AFTER_DAYS: u32 = 60;
const IDLE_MINUTES: u32 = 180;

/// A club with fewer seniors than this is thin whatever its groups look like (the game's own floor
/// asks eighteen of a squad; this is the line below which a club is plainly short).
const THIN_SQUAD: usize = 16;

/// One season of one run, read just before its rollover.
#[derive(Debug, Clone, Default)]
pub struct SeasonRow {
    pub season: u32,
    pub clubs: usize,
    // Squads.
    pub seniors_min: usize,
    pub seniors_mean: f64,
    pub seniors_max: usize,
    pub thin_squads: usize,
    pub no_keeper: usize,
    pub one_keeper: usize,
    // Books.
    pub in_debt: usize,
    pub over_wage_budget_110: usize,
    pub wage_to_budget_median: f64,
    // Managers.
    /// Clubs with nobody in charge, and clubs an interim stand-in (an assistant) is covering.
    pub without_manager: usize,
    pub interim_clubs: usize,
    /// Permanent managers who left a club, and permanent managers appointed, this season. An
    /// interim stand-in is neither: counting him made every dismissal three events.
    pub dismissals: u32,
    pub appointments: u32,
    /// Mean days a club went without a permanent manager, for vacancies that ended this season.
    pub mean_vacancy_days: f64,
    // Set-up.
    pub formations: usize,
    pub commonest_formation_share: f64,
    pub formation_changes: u32,
    pub tactical_plans: usize,
    // Pyramid.
    pub leagues: usize,
    pub league_size_changes: usize,
    pub promoted: usize,
    pub relegated: usize,
    pub unfinished_leagues: usize,
    // Loans that ended this season.
    pub loans_ended: usize,
    pub idle_loans: usize,
    /// Loans of two months or more, and how many of them were idle, for the players a club lends
    /// for their minutes (22 or under, six points to grow: the lender's rule, copied here) and for
    /// everyone else.
    pub young_long: usize,
    pub young_idle: usize,
    pub other_long: usize,
    pub other_idle: usize,
    // Prospects followed over the season.
    pub prospects: usize,
    pub prospect_growth: f64,
    pub prospect_minutes: f64,
    pub prospects_under_450: f64,
}

#[derive(Debug, Clone)]
struct LoanTrack {
    young: bool,
    last_minutes: u32,
    minutes: u32,
    days: u32,
}

/// Follows a run and keeps one [`SeasonRow`] per season.
#[derive(Default)]
pub struct CareerLog {
    pub rows: Vec<SeasonRow>,
    manager_of: HashMap<String, Option<String>>,
    vacant_since: HashMap<String, u32>,
    interim_ids: HashSet<String>,
    day: u32,
    dismissals: u32,
    appointments: u32,
    vacancies: Vec<u32>,
    formation_of: HashMap<String, String>,
    formation_changes: u32,
    league_of: HashMap<String, String>,
    league_size: HashMap<String, usize>,
    loans: HashMap<String, LoanTrack>,
    loans_ended: Vec<LoanTrack>,
    cohort: HashMap<String, u8>,
}

fn is_ladder_league(league: &League) -> bool {
    league.scope == CompetitionScope::Domestic
        && league.kind == CompetitionType::League
        && league.rules.format == CompetitionFormat::LeagueTable
        && league.standings.len() >= 8
}

fn natural_keepers(players: &[&Player]) -> usize {
    players
        .iter()
        .filter(|p| p.squad_role == SquadRole::Senior && p.natural_position == Position::Goalkeeper)
        .count()
}

impl CareerLog {
    /// The log of a run that starts from `game`, so that the first season's changes have a start.
    pub fn starting_from(game: &Game) -> Self {
        let mut log = Self::default();
        log.remember_set_up(game);
        log.remember_pyramid(game);
        log.remember_prospects(game);
        log
    }

    fn ai_clubs(game: &Game) -> impl Iterator<Item = &domain::team::Team> {
        let user = game.manager.team_id.clone();
        game.teams
            .iter()
            .filter(move |team| Some(&team.id) != user.as_ref())
    }

    fn remember_set_up(&mut self, game: &Game) {
        for team in Self::ai_clubs(game) {
            let permanent = team.manager_id.clone().filter(|id| {
                !game
                    .managers
                    .iter()
                    .any(|manager| manager.interim && manager.id == *id)
            });
            self.manager_of.insert(team.id.clone(), permanent);
            self.formation_of
                .insert(team.id.clone(), team.formation.clone());
        }
    }

    fn remember_pyramid(&mut self, game: &Game) {
        self.league_of.clear();
        self.league_size.clear();
        for league in game.competitions.iter().filter(|l| is_ladder_league(l)) {
            self.league_size
                .insert(league.id.clone(), league.standings.len());
            for entry in &league.standings {
                self.league_of
                    .insert(entry.team_id.clone(), league.id.clone());
            }
        }
    }

    fn remember_prospects(&mut self, game: &Game) {
        let today = game.clock.current_date.date_naive();
        self.cohort.clear();
        for player in &game.players {
            let Some(team) = player.team_id.as_deref() else {
                continue;
            };
            if Some(team) == game.manager.team_id.as_deref() || player.retired {
                continue;
            }
            let age = domain::age::age_on_or_unknown(today, &player.date_of_birth);
            if age <= 21 && i32::from(player.potential) >= i32::from(player.ovr) + 6 {
                self.cohort.insert(player.id.clone(), player.ovr);
            }
        }
    }

    fn follow_managers_and_set_up(&mut self, game: &Game) {
        self.interim_ids = game
            .managers
            .iter()
            .filter(|manager| manager.interim)
            .map(|manager| manager.id.clone())
            .collect();
        for team in Self::ai_clubs(game) {
            // The permanent manager in charge, if any: an interim stand-in is not one.
            let now = team
                .manager_id
                .clone()
                .filter(|id| !self.interim_ids.contains(id));
            let before = self.manager_of.get(&team.id).cloned().flatten();
            if before.is_some() && before != now {
                self.dismissals += 1;
                self.vacant_since.insert(team.id.clone(), self.day);
            }
            if now.is_some() && before != now {
                self.appointments += 1;
                if let Some(since) = self.vacant_since.remove(&team.id) {
                    self.vacancies.push(self.day.saturating_sub(since));
                }
            }
            self.manager_of.insert(team.id.clone(), now);
            if self
                .formation_of
                .get(&team.id)
                .is_some_and(|f| *f != team.formation)
            {
                self.formation_changes += 1;
            }
            self.formation_of
                .insert(team.id.clone(), team.formation.clone());
        }
    }

    fn follow_loans(&mut self, game: &Game) {
        let user = game.manager.team_id.as_deref();
        let mut seen: HashSet<&str> = HashSet::new();
        for player in &game.players {
            let Some(loan) = &player.active_loan else {
                continue;
            };
            if Some(loan.loan_team_id.as_str()) == user
                || Some(loan.parent_team_id.as_str()) == user
            {
                continue;
            }
            seen.insert(player.id.as_str());
            let minutes = player.stats.minutes_played;
            let age = domain::age::age_on_or_unknown(
                game.clock.current_date.date_naive(),
                &player.date_of_birth,
            );
            let young = age <= 22 && i32::from(player.potential) >= i32::from(player.ovr) + 6;
            let track = self.loans.entry(player.id.clone()).or_insert(LoanTrack {
                young,
                last_minutes: minutes,
                minutes: 0,
                days: 0,
            });
            // The counter restarts each season, so a drop is a new season's first minutes.
            track.minutes += if minutes >= track.last_minutes {
                minutes - track.last_minutes
            } else {
                minutes
            };
            track.last_minutes = minutes;
            track.days += 1;
        }
        let finished: Vec<String> = self
            .loans
            .keys()
            .filter(|id| !seen.contains(id.as_str()))
            .cloned()
            .collect();
        for id in finished {
            if let Some(track) = self.loans.remove(&id) {
                self.loans_ended.push(track);
            }
        }
    }

    fn row(&mut self, game: &Game, season: u32) -> SeasonRow {
        let mut row = SeasonRow {
            season,
            ..SeasonRow::default()
        };
        let bills = wage_bills_by_club(game);
        let mut by_club: HashMap<&str, Vec<&Player>> = HashMap::new();
        for player in &game.players {
            if let Some(team) = player.team_id.as_deref() {
                by_club.entry(team).or_default().push(player);
            }
        }
        let mut seniors: Vec<usize> = Vec::new();
        let mut ratios: Vec<f64> = Vec::new();
        let mut formations: HashMap<&str, usize> = HashMap::new();
        let mut plans: HashSet<String> = HashSet::new();
        for team in Self::ai_clubs(game) {
            row.clubs += 1;
            let squad = by_club.get(team.id.as_str()).map_or(&[][..], Vec::as_slice);
            let count = squad
                .iter()
                .filter(|p| p.squad_role == SquadRole::Senior)
                .count();
            seniors.push(count);
            if count < THIN_SQUAD {
                row.thin_squads += 1;
            }
            match natural_keepers(squad) {
                0 => row.no_keeper += 1,
                1 => row.one_keeper += 1,
                _ => {}
            }
            if team.finance < 0 {
                row.in_debt += 1;
            }
            if team.wage_budget > 0 {
                let ratio =
                    bills.get(&team.id).copied().unwrap_or(0) as f64 / team.wage_budget as f64;
                ratios.push(ratio);
                if ratio > 1.1 {
                    row.over_wage_budget_110 += 1;
                }
            }
            match &team.manager_id {
                None => row.without_manager += 1,
                Some(id) if self.interim_ids.contains(id) => row.interim_clubs += 1,
                Some(_) => {}
            }
            *formations.entry(team.formation.as_str()).or_default() += 1;
            plans.insert(format!("{:?}", team.tactics_phase));
        }
        row.seniors_min = seniors.iter().copied().min().unwrap_or(0);
        row.seniors_max = seniors.iter().copied().max().unwrap_or(0);
        row.seniors_mean = seniors.iter().sum::<usize>() as f64 / seniors.len().max(1) as f64;
        ratios.sort_by(f64::total_cmp);
        row.wage_to_budget_median = ratios.get(ratios.len() / 2).copied().unwrap_or(0.0);
        row.formations = formations.len();
        row.commonest_formation_share =
            formations.values().copied().max().unwrap_or(0) as f64 / row.clubs.max(1) as f64;
        row.tactical_plans = plans.len();
        row.dismissals = std::mem::take(&mut self.dismissals);
        row.appointments = std::mem::take(&mut self.appointments);
        row.formation_changes = std::mem::take(&mut self.formation_changes);
        row.mean_vacancy_days = if self.vacancies.is_empty() {
            0.0
        } else {
            self.vacancies.iter().map(|d| f64::from(*d)).sum::<f64>() / self.vacancies.len() as f64
        };
        self.vacancies.clear();

        // The pyramid, against last season's.
        let priority: HashMap<&str, u32> = game
            .competitions
            .iter()
            .map(|l| (l.id.as_str(), l.priority))
            .collect();
        let mut now_of: HashMap<String, String> = HashMap::new();
        for league in game.competitions.iter().filter(|l| is_ladder_league(l)) {
            row.leagues += 1;
            if self
                .league_size
                .get(&league.id)
                .is_some_and(|size| *size != league.standings.len())
            {
                row.league_size_changes += 1;
            }
            if league
                .fixtures
                .iter()
                .any(|f| f.status == domain::league::FixtureStatus::Scheduled)
            {
                row.unfinished_leagues += 1;
            }
            for entry in &league.standings {
                now_of.insert(entry.team_id.clone(), league.id.clone());
            }
        }
        for (club, league) in &now_of {
            let Some(before) = self.league_of.get(club) else {
                continue;
            };
            if before == league {
                continue;
            }
            let (then, now) = (
                priority.get(before.as_str()).copied().unwrap_or(0),
                priority.get(league.as_str()).copied().unwrap_or(0),
            );
            if now < then {
                row.promoted += 1;
            } else {
                row.relegated += 1;
            }
        }

        // Loans ended, and prospects.
        row.loans_ended = self.loans_ended.len();
        row.idle_loans = self
            .loans_ended
            .iter()
            .filter(|l| l.minutes < IDLE_MINUTES)
            .count();
        let long: Vec<&LoanTrack> = self
            .loans_ended
            .iter()
            .filter(|l| l.days >= IDLE_AFTER_DAYS)
            .collect();
        for track in &long {
            let idle = track.minutes < IDLE_MINUTES;
            if track.young {
                row.young_long += 1;
                row.young_idle += usize::from(idle);
            } else {
                row.other_long += 1;
                row.other_idle += usize::from(idle);
            }
        }
        self.loans_ended.clear();

        let mut growth = 0.0;
        let mut minutes = 0.0;
        let mut under = 0.0;
        for player in &game.players {
            if let Some(before) = self.cohort.get(&player.id) {
                row.prospects += 1;
                growth += f64::from(player.ovr) - f64::from(*before);
                minutes += f64::from(player.stats.minutes_played);
                if player.stats.minutes_played < 450 {
                    under += 1.0;
                }
            }
        }
        let n = row.prospects.max(1) as f64;
        row.prospect_growth = growth / n;
        row.prospect_minutes = minutes / n;
        row.prospects_under_450 = 100.0 * under / n;
        row
    }
}

impl SeasonObserver for CareerLog {
    fn on_day_end(&mut self, game: &Game, _season: u32, day: u32) {
        self.day = day;
        self.follow_managers_and_set_up(game);
        self.follow_loans(game);
    }

    fn on_before_rollover(&mut self, game: &Game, season: u32) {
        let row = self.row(game, season);
        self.rows.push(row);
        self.remember_pyramid(game);
        self.remember_prospects(game);
    }
}

/// The rows as a table, one line a season, with the headings the first time.
pub fn report(log: &CareerLog) -> String {
    let mut out = String::new();
    out.push_str(
        "season | clubs | seniors min/mean/max | thin | no GK | one GK | in debt | >110% wages | \
         wage/budget P50 | no manager / interim | left | hired | vacancy d | formations (top share, \
         changes) | plans | leagues | size chg | up | down | unfinished | loans ended (idle) | \
         2m+ loans, for-minutes idle/n, others idle/n | prospects (growth, min, <450%)\n",
    );
    for r in &log.rows {
        out.push_str(&format!(
            "{:>3} | {:>3} | {:>2}/{:>5.1}/{:>2} | {:>2} | {:>2} | {:>2} | {:>2} | {:>2} | {:.2} | \
             {:>2} / {:>2} | {:>3} | {:>3} | {:>5.1} | {} ({:.0}%, {}) | {:>3} | {} | {} | {:>2} | \
             {:>2} | {} | {} ({}) | {}/{}, {}/{} | {} ({:+.2}, {:.0}, {:.0}%)\n",
            r.season,
            r.clubs,
            r.seniors_min,
            r.seniors_mean,
            r.seniors_max,
            r.thin_squads,
            r.no_keeper,
            r.one_keeper,
            r.in_debt,
            r.over_wage_budget_110,
            r.wage_to_budget_median,
            r.without_manager,
            r.interim_clubs,
            r.dismissals,
            r.appointments,
            r.mean_vacancy_days,
            r.formations,
            100.0 * r.commonest_formation_share,
            r.formation_changes,
            r.tactical_plans,
            r.leagues,
            r.league_size_changes,
            r.promoted,
            r.relegated,
            r.unfinished_leagues,
            r.loans_ended,
            r.idle_loans,
            r.young_idle,
            r.young_long,
            r.other_idle,
            r.other_long,
            r.prospects,
            r.prospect_growth,
            r.prospect_minutes,
            r.prospects_under_450,
        ));
    }
    out
}
