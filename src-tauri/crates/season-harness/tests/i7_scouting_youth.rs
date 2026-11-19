//! I7: scouting and youth development over long careers.
//!
//! Two things live here.
//!
//! * `i7_scouting_youth_study` (ignored; run it explicitly) plays the same generated world under
//!   two arms and prints what the scouting dossier and the academy did over the seasons:
//!   the club's administration is handed to the assistant in both, and the arms differ in who runs
//!   scouting and the academy: the assistant (the delegated duties) or a scripted manager who does
//!   by hand exactly what the screens let a human do (send scouts, shortlist, read the academy
//!   review, promote, list for loan, release). It measures the academy's population, the quality
//!   of each intake, what the academy's players became, promotions, scouting output, the senior
//!   limit, club finances and cost. Its assertions are the safety properties, never a margin.
//!
//!   ```text
//!   I7_SEASONS=10 I7_SEEDS=1 cargo test --release -p season-harness --test i7_scouting_youth \
//!     -- --ignored --nocapture i7_scouting_youth_study
//!   ```
//!
//! * The always-run tests prove the dossier is deterministic and survives a save and reload in the
//!   middle of a delegated career, and that the scripted manager's actions are the screens' own.

use std::collections::{BTreeMap, HashMap, HashSet};

use db::save_manager::SaveManager;
use domain::delegation::{DelegationMode, Duty};
use domain::player::{Player, SquadRole};
use domain::staff::{StaffPost, StaffRole};
use domain::stats::StatsState;
use ofm_core::academy_review::{Verdict, academy_review, promotion_place_refusal};
use ofm_core::end_of_season::is_season_complete;
use ofm_core::game::Game;
use ofm_core::scouting_view::{SquadImpact, targets_view};
use ofm_core::staff_system::{market, profile};
use season_harness::driver::{RunOptions, SeasonObserver, play_day, roll_season_over};
use season_harness::invariants::Invariants;
use season_harness::worlds::WorldSpec;

const SENIOR_LIMIT: usize = 30;

fn user_team(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn seniors_of<'a>(game: &'a Game, team_id: &str) -> Vec<&'a Player> {
    game.players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(team_id) && p.squad_role == SquadRole::Senior && !p.retired
        })
        .collect()
}

fn academy_of<'a>(game: &'a Game, team_id: &str) -> Vec<&'a Player> {
    game.players
        .iter()
        .filter(|p| {
            p.contract_club_id() == Some(team_id) && p.squad_role == SquadRole::Youth && !p.retired
        })
        .collect()
}

fn age_of(game: &Game, player: &Player) -> i32 {
    domain::age::age_on_or_unknown(game.clock.current_date.date_naive(), &player.date_of_birth)
}

fn ensure_assistant(game: &mut Game) {
    let team = user_team(game);
    if profile::team_staff(game, &team)
        .iter()
        .any(|s| s.post() == StaffPost::AssistantManager)
    {
        return;
    }
    let reputation = game
        .teams
        .iter()
        .find(|t| t.id == team)
        .map_or(0, |t| t.reputation);
    let (id, wage) = game
        .staff
        .iter()
        .filter(|s| s.team_id.is_none() && s.post() == StaffPost::AssistantManager)
        .max_by_key(|s| (s.overall(), s.id.clone()))
        .map(|s| (s.id.clone(), market::asking_wage(s, reputation)))
        .expect("a free assistant manager exists in the world");
    market::hire(game, &team, &id, wage, 3).expect("the assistant can be hired");
}

/// The shared club: an assistant and the whole administration handed over (not the staff
/// themselves, who are kept for the study), as in the I9 study.
fn administered(game: &mut Game) {
    ensure_assistant(game);
    game.delegation.apply_mode(DelegationMode::Full);
    game.delegation.set(Duty::StaffRecruitment, false, false);
    game.delegation.set(Duty::StaffRenewals, false, false);
    let team = user_team(game);
    for member in game
        .staff
        .iter_mut()
        .filter(|s| s.team_id.as_deref() == Some(team.as_str()))
    {
        member.contract_end = Some("2099-01-01".to_string());
    }
}

/// Who runs scouting and the academy.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    /// The assistant runs both duties.
    Delegated,
    /// A scripted manager does by hand what the screens let a human do.
    ScriptedManager,
}

impl Arm {
    fn name(self) -> &'static str {
        match self {
            Arm::Delegated => "delegated: the assistant runs scouting and the academy",
            Arm::ScriptedManager => "scripted manager: scouting and the academy by hand",
        }
    }

    fn setup(self, game: &mut Game) {
        administered(game);
        if self == Arm::ScriptedManager {
            game.delegation.set(Duty::Scouting, false, false);
            game.delegation.set(Duty::Youth, false, false);
        }
    }
}

#[derive(Default)]
struct ManagerLog {
    scouts_sent: usize,
    shortlisted: usize,
    promoted: usize,
    loan_listed: usize,
    released: usize,
}

/// What a human does on a Monday with the two screens: sends every idle scout to the best young
/// public rating the club has not looked at (the shortlist first), shortlists reports that would
/// start or cover for the side at a price the club can pay, promotes the academy players the
/// review calls ready while a senior place is free, lists loan candidates and releases players
/// the review calls unlikely to progress once they are twenty. Every call is the one a command
/// makes.
fn scripted_week(game: &mut Game, log: &mut ManagerLog) {
    let team_id = user_team(game);

    let fresh: Vec<String> = game
        .scouting_dossier
        .reports
        .iter()
        .map(|r| r.player_id.clone())
        .collect();
    let fits: Vec<String> = targets_view(game, &fresh)
        .into_iter()
        .filter(|v| !v.shortlisted)
        .filter(|v| {
            matches!(
                v.squad_impact,
                Some(SquadImpact::Starter | SquadImpact::Cover)
            )
        })
        .filter(|v| v.fee_affordable != Some(false) && v.wage_permitted)
        .map(|v| v.player_id)
        .take(2)
        .collect();
    for id in fits {
        if ofm_core::scouting_dossier::shortlist_add(game, &id).is_ok() {
            log.shortlisted += 1;
        }
    }

    let busy: HashSet<String> = game
        .scouting_assignments
        .iter()
        .map(|a| a.scout_id.clone())
        .collect();
    let idle: Vec<String> = profile::team_staff(game, &team_id)
        .iter()
        .filter(|s| s.role == StaffRole::Scout && s.post() != StaffPost::Analyst)
        .filter(|s| !busy.contains(&s.id))
        .map(|s| s.id.clone())
        .collect();
    let reported: HashSet<&str> = game
        .scouting_dossier
        .reports
        .iter()
        .map(|r| r.player_id.as_str())
        .collect();
    let watched: HashSet<&str> = game
        .scouting_assignments
        .iter()
        .map(|a| a.player_id.as_str())
        .collect();
    let mut targets: Vec<(String, u8)> = game
        .players
        .iter()
        .filter(|p| p.team_id.as_deref().is_some_and(|t| t != team_id) && !p.retired)
        .filter(|p| !reported.contains(p.id.as_str()) && !watched.contains(p.id.as_str()))
        .filter(|p| age_of(game, p) <= 23)
        .map(|p| (p.id.clone(), p.ovr))
        .collect();
    targets.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (scout, (target, _)) in idle.iter().zip(targets) {
        if ofm_core::scouting::send_scout(game, scout, &target).is_ok() {
            log.scouts_sent += 1;
        }
    }

    let Some(review) = academy_review(game) else {
        return;
    };
    for row in review.rows {
        match row.verdict {
            Verdict::ReadyForSenior if promotion_place_refusal(game, &team_id).is_none() => {
                if let Some(p) = game.players.iter_mut().find(|p| p.id == row.player_id) {
                    p.squad_role = SquadRole::Senior;
                    log.promoted += 1;
                }
            }
            Verdict::LoanCandidate if !row.loan_listed && !row.on_loan => {
                if let Some(p) = game.players.iter_mut().find(|p| p.id == row.player_id) {
                    p.loan_listed = true;
                    log.loan_listed += 1;
                }
            }
            Verdict::UnlikelyToProgress
                if row.age >= 20
                    && ofm_core::contracts::terminate_contract_now(game, &row.player_id)
                        .is_ok() =>
            {
                log.released += 1;
            }
            _ => {}
        }
    }
}

#[derive(Default, Clone)]
struct SeasonLine {
    user_academy: usize,
    ai_academy_mean: f64,
    ai_academy_max: usize,
    user_seniors: usize,
    intake_user: Vec<(u8, u8)>,
    intake_ai_ovr: f64,
    intake_ai_pot: f64,
    intake_ai_n: usize,
    /// Of the AI clubs' intake this season, how many were given a potential of 80 or more.
    intake_ai_elite: usize,
    promotions_user: usize,
    promotions_ai: usize,
    reports_delivered: usize,
}

struct Recorder {
    team_id: String,
    seasons: Vec<SeasonLine>,
    current: SeasonLine,
    role_by_id: HashMap<String, (Option<String>, SquadRole)>,
    seen: HashSet<String>,
    cohort: HashMap<String, (usize, i32, u8, u8, bool)>,
    senior_limit_breaches: usize,
    floor_breach_days: usize,
    reports_seen: HashSet<(String, String)>,
    ever_shortlisted: HashSet<String>,
    promoted_user_ids: Vec<(String, u8)>,
    min_finance: i64,
    ai_negative_finance_seasons: usize,
    season: usize,
}

impl Recorder {
    fn new(game: &Game) -> Self {
        let team_id = user_team(game);
        Self {
            role_by_id: game
                .players
                .iter()
                .map(|p| (p.id.clone(), (p.team_id.clone(), p.squad_role)))
                .collect(),
            seen: game.players.iter().map(|p| p.id.clone()).collect(),
            team_id,
            seasons: Vec::new(),
            current: SeasonLine::default(),
            cohort: HashMap::new(),
            senior_limit_breaches: 0,
            floor_breach_days: 0,
            reports_seen: HashSet::new(),
            ever_shortlisted: HashSet::new(),
            promoted_user_ids: Vec::new(),
            min_finance: i64::MAX,
            ai_negative_finance_seasons: 0,
            season: 0,
        }
    }
}

impl SeasonObserver for Recorder {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        let n = seniors_of(game, &self.team_id).len();
        if n > SENIOR_LIMIT {
            self.senior_limit_breaches += 1;
        }
        if n < 15 {
            self.floor_breach_days += 1;
        }
        if let Some(team) = game.teams.iter().find(|t| t.id == self.team_id) {
            self.min_finance = self.min_finance.min(team.finance);
        }
        for report in &game.scouting_dossier.reports {
            if self
                .reports_seen
                .insert((report.player_id.clone(), report.scouted_on.clone()))
            {
                self.current.reports_delivered += 1;
            }
        }
        for entry in &game.scouting_dossier.shortlist {
            self.ever_shortlisted.insert(entry.player_id.clone());
        }
        // Promotions: academy to senior at the same club, seen day to day.
        for player in &game.players {
            let now = (player.team_id.clone(), player.squad_role);
            if let Some(before) = self.role_by_id.get(&player.id)
                && before.1 == SquadRole::Youth
                && now.1 == SquadRole::Senior
                && before.0 == now.0
                && now.0.is_some()
            {
                if now.0.as_deref() == Some(self.team_id.as_str()) {
                    self.current.promotions_user += 1;
                    self.promoted_user_ids.push((player.id.clone(), player.ovr));
                } else {
                    self.current.promotions_ai += 1;
                }
            }
            self.role_by_id.insert(player.id.clone(), now);
        }
    }

    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        let clubs: Vec<&str> = game.teams.iter().map(|t| t.id.as_str()).collect();
        let sizes: Vec<usize> = clubs
            .iter()
            .filter(|c| **c != self.team_id)
            .map(|c| academy_of(game, c).len())
            .collect();
        self.current.user_academy = academy_of(game, &self.team_id).len();
        self.current.user_seniors = seniors_of(game, &self.team_id).len();
        self.current.ai_academy_mean =
            sizes.iter().sum::<usize>() as f64 / sizes.len().max(1) as f64;
        self.current.ai_academy_max = sizes.iter().copied().max().unwrap_or(0);
        self.ai_negative_finance_seasons += game
            .teams
            .iter()
            .filter(|t| t.id != self.team_id && t.finance < 0)
            .count();
    }

    fn on_season_end(
        &mut self,
        game: &Game,
        _summary: &ofm_core::end_of_season::EndOfSeasonSummary,
        _season: u32,
    ) {
        // The intake takes place in the rollover: whoever is new and in an academy joined it.
        let mut ai_ovr = 0.0;
        let mut ai_pot = 0.0;
        let mut ai_n = 0usize;
        let mut ai_elite = 0usize;
        for player in game.players.iter().filter(|p| !self.seen.contains(&p.id)) {
            if player.squad_role != SquadRole::Youth {
                continue;
            }
            let age = age_of(game, player);
            self.cohort.insert(
                player.id.clone(),
                (self.season, age, player.ovr, player.potential, false),
            );
            if player.contract_club_id() == Some(self.team_id.as_str()) {
                self.current
                    .intake_user
                    .push((player.ovr, player.potential));
            } else {
                ai_ovr += f64::from(player.ovr);
                ai_pot += f64::from(player.potential);
                ai_n += 1;
                ai_elite += usize::from(player.potential >= 80);
            }
        }
        if ai_n > 0 {
            self.current.intake_ai_ovr = ai_ovr / ai_n as f64;
            self.current.intake_ai_pot = ai_pot / ai_n as f64;
            self.current.intake_ai_n = ai_n;
            self.current.intake_ai_elite = ai_elite;
        }
        self.seen = game.players.iter().map(|p| p.id.clone()).collect();
        self.seasons.push(std::mem::take(&mut self.current));
        self.season += 1;
    }
}

struct Result {
    arm: Arm,
    recorder: Recorder,
    manager: ManagerLog,
    invariants_clean: bool,
    invariants_report: String,
    seconds: f64,
    final_game: Game,
}

fn play_arm(arm: Arm, seed: u64, seasons: u32) -> Result {
    let started = std::time::Instant::now();
    let mut game = WorldSpec::gate(seed).build().expect("the world builds");
    arm.setup(&mut game);
    let options = RunOptions::seasons(seasons);
    let mut recorder = Recorder::new(&game);
    let mut invariants = Invariants::new(&game);
    let mut manager = ManagerLog::default();
    for season in 1..=seasons {
        let mut day = 0;
        while !is_season_complete(&game) {
            day += 1;
            assert!(
                day < 470,
                "{}: season {season} did not complete",
                arm.name()
            );
            if arm == Arm::ScriptedManager && day % 7 == 1 {
                scripted_week(&mut game, &mut manager);
            }
            play_day(
                &mut game,
                options,
                season,
                day,
                &mut (&mut recorder, &mut invariants),
            )
            .unwrap_or_else(|e| panic!("{}: {e}", arm.name()));
        }
        roll_season_over(
            &mut game,
            options,
            season,
            &mut (&mut recorder, &mut invariants),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", arm.name()));
    }
    Result {
        arm,
        recorder,
        manager,
        invariants_clean: invariants.is_clean(),
        invariants_report: invariants.report(),
        seconds: started.elapsed().as_secs_f64(),
        final_game: game,
    }
}

fn mean_of(values: impl Iterator<Item = f64>) -> f64 {
    let v: Vec<f64> = values.collect();
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn print_arm(result: &Result, seasons: u32) {
    let r = &result.recorder;
    println!("\n== {} ==", result.arm.name());
    println!(
        "  user academy by season end: {:?}; seniors {:?}",
        r.seasons.iter().map(|s| s.user_academy).collect::<Vec<_>>(),
        r.seasons.iter().map(|s| s.user_seniors).collect::<Vec<_>>()
    );
    println!(
        "  AI academies: mean {:?}; largest {:?}",
        r.seasons
            .iter()
            .map(|s| (s.ai_academy_mean * 10.0).round() / 10.0)
            .collect::<Vec<_>>(),
        r.seasons
            .iter()
            .map(|s| s.ai_academy_max)
            .collect::<Vec<_>>()
    );
    println!(
        "  intake, user (n / mean ovr / mean potential) by season: {:?}",
        r.seasons
            .iter()
            .map(|s| (
                s.intake_user.len(),
                mean_of(s.intake_user.iter().map(|(o, _)| f64::from(*o))).round(),
                mean_of(s.intake_user.iter().map(|(_, p)| f64::from(*p))).round()
            ))
            .collect::<Vec<_>>()
    );
    println!(
        "  intake, AI clubs (n / mean ovr / mean potential) by season: {:?}",
        r.seasons
            .iter()
            .map(|s| (
                s.intake_ai_n,
                s.intake_ai_ovr.round(),
                s.intake_ai_pot.round()
            ))
            .collect::<Vec<_>>()
    );
    println!(
        "  AI intake given a potential of 80 or more, share by season: {:?}",
        r.seasons
            .iter()
            .map(
                |s| (s.intake_ai_elite as f64 / s.intake_ai_n.max(1) as f64 * 1000.0).round()
                    / 10.0
            )
            .collect::<Vec<_>>()
    );
    println!(
        "  promotions to the senior squad, user {:?}; all AI clubs {:?}",
        r.seasons
            .iter()
            .map(|s| s.promotions_user)
            .collect::<Vec<_>>(),
        r.seasons
            .iter()
            .map(|s| s.promotions_ai)
            .collect::<Vec<_>>()
    );
    println!(
        "  scouting reports delivered by season {:?}; ever shortlisted {}; of those now at the club {}",
        r.seasons
            .iter()
            .map(|s| s.reports_delivered)
            .collect::<Vec<_>>(),
        r.ever_shortlisted.len(),
        r.ever_shortlisted
            .iter()
            .filter(|id| result
                .final_game
                .players
                .iter()
                .any(|p| &p.id == *id && p.team_id.as_deref() == Some(r.team_id.as_str())))
            .count()
    );
    if result.arm == Arm::ScriptedManager {
        let m = &result.manager;
        println!(
            "  scripted manager: {} scouts sent, {} shortlisted, {} promoted, {} listed for loan, {} released",
            m.scouts_sent, m.shortlisted, m.promoted, m.loan_listed, m.released
        );
    }
    development_report(result);
    println!(
        "  breaches: seniors>30 on {} days, below 15 on {} days; lowest cash {}; AI club-seasons with negative cash {}; invariants clean: {}",
        r.senior_limit_breaches,
        r.floor_breach_days,
        r.min_finance,
        r.ai_negative_finance_seasons,
        result.invariants_clean
    );
    println!(
        "  user promotions' mean ovr at promotion {:.1} (n={}); cost {:.0}s for {seasons} seasons",
        mean_of(r.promoted_user_ids.iter().map(|(_, o)| f64::from(*o))),
        r.promoted_user_ids.len(),
        result.seconds
    );
}

/// What the academy's intake became: for the cohorts of the first seasons, at the end of the
/// study, by age, how far their rating got against the potential they were given, and how many
/// were senior players somewhere.
fn development_report(result: &Result) {
    let game = &result.final_game;
    let r = &result.recorder;
    let today = game.clock.current_date.date_naive();
    let mut rows: Vec<(i32, u8, u8, u8, SquadRole, bool)> = Vec::new();
    for (id, (_, _, ovr_at, pot_at, _)) in &r.cohort {
        let Some(p) = game.players.iter().find(|p| &p.id == id) else {
            continue;
        };
        let age = domain::age::age_on_or_unknown(today, &p.date_of_birth);
        rows.push((age, *ovr_at, *pot_at, p.ovr, p.squad_role, p.retired));
    }
    let grown: Vec<_> = rows.iter().filter(|r| r.0 >= 24).collect();
    let all_ovr: Vec<u8> = game
        .players
        .iter()
        .filter(|p| p.squad_role == SquadRole::Senior && !p.retired)
        .map(|p| p.ovr)
        .collect();
    let mut sorted = all_ovr.clone();
    sorted.sort_unstable();
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0);
    let useful = grown.iter().filter(|r| r.3 >= median).count();
    println!(
        "  cohorts in the study still alive: {}; of those aged 24+: {} — mean potential given at intake {:.1}, mean rating now {:.1}, share at or above the world's median senior ({median}) {:.0}%, share who reached 5 points of their ceiling {:.0}%",
        rows.len(),
        grown.len(),
        mean_of(grown.iter().map(|r| f64::from(r.2))),
        mean_of(grown.iter().map(|r| f64::from(r.3))),
        if grown.is_empty() {
            0.0
        } else {
            useful as f64 / grown.len() as f64 * 100.0
        },
        if grown.is_empty() {
            0.0
        } else {
            grown.iter().filter(|r| r.3 + 5 >= r.2).count() as f64 / grown.len() as f64 * 100.0
        },
    );
    let mut gaps: Vec<f64> = grown
        .iter()
        .map(|r| f64::from(r.3) - f64::from(r.2))
        .collect();
    gaps.sort_by(f64::total_cmp);
    if !gaps.is_empty() {
        let at = |q: f64| gaps[((gaps.len() - 1) as f64 * q).round() as usize];
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        let sd = (gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / gaps.len() as f64).sqrt();
        println!(
            "  rating now minus the potential given at intake, 24+ cohort: mean {mean:+.1}, sd {sd:.1}, p10 {:+.0}, median {:+.0}, p90 {:+.0}; worst {:+.0}",
            at(0.1),
            at(0.5),
            at(0.9),
            gaps[0]
        );
    }
    println!(
        "  of the 24+ cohort, those whose potential at intake was 75 or more: {} (now {:.1} on average)",
        grown.iter().filter(|r| r.2 >= 75).count(),
        mean_of(grown.iter().filter(|r| r.2 >= 75).map(|r| f64::from(r.3))),
    );
}

/// Given the same generated world under both arms
/// When each plays the configured seasons
/// Then no arm ever puts the user's club above 30 seniors, and the world's invariants hold.
#[test]
#[ignore = "long: set I7_SEASONS and I7_SEEDS, run with --nocapture"]
fn i7_scouting_youth_study() {
    let seasons: u32 = std::env::var("I7_SEASONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let seeds: Vec<u64> = std::env::var("I7_SEEDS")
        .unwrap_or_else(|_| "1".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    for seed in seeds {
        println!("\n######## seed {seed}, {seasons} seasons ########");
        for arm in [Arm::Delegated, Arm::ScriptedManager] {
            let result = play_arm(arm, seed, seasons);
            print_arm(&result, seasons);
            assert_eq!(
                result.recorder.senior_limit_breaches,
                0,
                "{}: the 30-senior hold was broken",
                arm.name()
            );
            assert!(
                result.invariants_clean,
                "{}: {}",
                arm.name(),
                result.invariants_report
            );
        }
    }
}

struct Quiet;
impl SeasonObserver for Quiet {}

/// The dossier and everything the scouting duties wrote, in an order that does not depend on how a
/// table was read back: the snapshots and the assignments are stored as sets.
fn dossier_fingerprint(game: &Game) -> String {
    let mut snapshots: Vec<String> = game
        .scouted_players
        .iter()
        .map(|s| serde_json::to_string(s).expect("snapshot serializes"))
        .collect();
    snapshots.sort();
    let mut assignments: Vec<String> = game
        .scouting_assignments
        .iter()
        .map(|a| serde_json::to_string(a).expect("assignment serializes"))
        .collect();
    assignments.sort();
    format!(
        "{}|{}|{}|{}",
        serde_json::to_string(&game.scouting_dossier).expect("dossier serializes"),
        snapshots.join(";"),
        assignments.join(";"),
        serde_json::to_string(&game.delegation.log).expect("log serializes"),
    )
}

fn play_days(game: &mut Game, from: u32, to: u32) {
    let options = RunOptions::seasons(1);
    for day in from..=to {
        play_day(game, options, 1, day, &mut Quiet).unwrap_or_else(|e| panic!("{e}"));
    }
}

/// Given the same world and the same delegated administration, built twice
/// When 120 days are played in each
/// Then the scouting dossier, the scouted snapshots, the assignments and the decisions logged are
///      identical, and the dossier is not empty (otherwise the comparison would be vacuous).
#[test]
fn a_delegated_career_writes_the_same_dossier_every_time() {
    let run = || {
        let mut game = WorldSpec::gate(1).build().expect("the world builds");
        Arm::Delegated.setup(&mut game);
        play_days(&mut game, 1, 120);
        game
    };
    let first = run();
    assert!(
        !first.scouting_dossier.reports.is_empty(),
        "the delegated scouting duty delivered no report in 120 days"
    );
    assert_eq!(dossier_fingerprint(&first), dossier_fingerprint(&run()));
}

/// Given a delegated career that has scouted for a while and the manager has shortlisted a player
/// When it is saved to SQLite and reloaded, and the same days are then played from the reloaded
///      game and from the game that was never saved
/// Then the dossier, the shortlist, the snapshots and the assignments are identical before and
///      after, so a reload neither repeats a report nor loses one.
#[test]
fn the_dossier_and_shortlist_survive_a_reload_in_the_middle_of_a_career() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    Arm::Delegated.setup(&mut game);
    play_days(&mut game, 1, 70);
    let wanted = game
        .players
        .iter()
        .find(|p| {
            p.team_id
                .as_deref()
                .is_some_and(|t| Some(t) != game.manager.team_id.as_deref())
                && !p.retired
        })
        .map(|p| p.id.clone())
        .expect("another club's player");
    ofm_core::scouting_dossier::shortlist_add(&mut game, &wanted).expect("shortlisted");
    assert!(
        !game.scouting_dossier.reports.is_empty(),
        "nothing scouted yet"
    );

    let dir = tempfile::tempdir().unwrap();
    let mut saves = SaveManager::init(dir.path()).unwrap();
    let save_id = saves
        .create_save_with_stats(&game, &StatsState::default(), "i7")
        .unwrap();
    saves
        .save_game_with_stats(&game, &StatsState::default(), &save_id)
        .unwrap();
    let mut reloaded = SaveManager::init(dir.path())
        .unwrap()
        .load_game(&save_id)
        .unwrap();
    assert_eq!(
        dossier_fingerprint(&reloaded),
        dossier_fingerprint(&game),
        "the round trip changed the dossier"
    );
    assert!(reloaded.scouting_dossier.is_shortlisted(&wanted));

    play_days(&mut game, 71, 130);
    play_days(&mut reloaded, 71, 130);
    assert_eq!(
        dossier_fingerprint(&reloaded),
        dossier_fingerprint(&game),
        "the reloaded career scouted differently from the one that was never saved"
    );
    let by_player: BTreeMap<&str, usize> =
        reloaded
            .scouting_dossier
            .reports
            .iter()
            .fold(BTreeMap::new(), |mut m, r| {
                *m.entry(r.player_id.as_str()).or_default() += 1;
                m
            });
    assert!(
        by_player.values().all(|n| *n == 1),
        "a player has two reports in the dossier"
    );
}

/// Given the scripted manager's week on a delegated-administration club
/// When twelve weeks are played
/// Then it sends scouts and keeps the club within the senior limit on every day: the manual path
///      the screens expose works end to end.
#[test]
fn a_scripted_manager_can_run_scouting_and_the_academy_by_hand() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    Arm::ScriptedManager.setup(&mut game);
    let team = user_team(&game);
    let mut log = ManagerLog::default();
    let options = RunOptions::seasons(1);
    for day in 1..=120u32 {
        if day % 7 == 1 {
            scripted_week(&mut game, &mut log);
        }
        play_day(&mut game, options, 1, day, &mut Quiet).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            seniors_of(&game, &team).len() <= SENIOR_LIMIT,
            "day {day}: above the senior limit"
        );
    }
    assert!(
        log.scouts_sent > 0,
        "the scripted manager never sent a scout"
    );
    assert!(
        !game.scouting_dossier.reports.is_empty(),
        "the scouts the manager sent delivered nothing"
    );
}
