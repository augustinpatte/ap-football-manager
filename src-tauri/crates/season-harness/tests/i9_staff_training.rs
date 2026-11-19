//! I9: staff, delegation and training over long careers.
//!
//! Two things live here.
//!
//! * `i9_staff_training_study` (ignored; run it explicitly) plays the same generated world under
//!   seven arms and prints what each did to the user's club. In every arm the club's
//!   administration is handed to the assistant (renewals, recruitment, the academy, offers, the
//!   lineup and match preparation), because a club in which nothing is ever signed or renewed
//!   simply decays over ten seasons and would drown the effect being measured. The arms then vary
//!   the staff (none, weak, average, strong coaching and medical departments, with the manager's
//!   own default training plan) and who plans the training and the tactics (the manager; the
//!   assistant acting alone; the assistant asking before raising the load, answered by a policy
//!   that accepts every other proposal; and the assistant on tactics as well). It measures squad
//!   fitness, injuries, points, staff cost, attribute progression by age band (the user's club
//!   against the AI clubs), the decisions taken and the rules that must never break (the
//!   30-senior hold, the squad floor, an overdrawn club). Its assertions are the safety
//!   properties, never a margin of improvement.
//!
//!   ```text
//!   I9_SEASONS=10 I9_SEEDS=1 cargo test --release -p season-harness --test i9_staff_training \
//!     -- --ignored --nocapture i9_staff_training_study
//!   ```
//!
//! * `a_reloaded_delegated_career_does_not_repeat_what_it_did` (always run) proves that a save and
//!   reload in the middle of a delegated season changes nothing: the same days played from the
//!   reloaded game and from the game that was never saved leave identical delegation state, and
//!   the named owner of training survives the reload and the rollover.

use std::collections::BTreeMap;

use db::save_manager::SaveManager;
use domain::delegation::{DecisionOutcome, DelegationMode, Duty};
use domain::player::{Player, SquadRole};
use domain::staff::{Staff, StaffPost};
use domain::stats::StatsState;
use ofm_core::end_of_season::{EndOfSeasonSummary, is_season_complete};
use ofm_core::game::Game;
use ofm_core::staff_system::delegation::resolve_proposal;
use ofm_core::staff_system::{delegation_owner, market, profile};
use season_harness::ageing::{AGE_BANDS, Snapshot, snapshot_of};
use season_harness::driver::{RunOptions, SeasonObserver, play_day, roll_season_over};
use season_harness::invariants::Invariants;
use season_harness::worlds::WorldSpec;

/// Most seniors a club may register (I4A).
const SENIOR_LIMIT: usize = 30;
/// Fewest seniors a club must keep (the squad floor).
const SENIOR_FLOOR: usize = 15;

fn user_team(game: &Game) -> String {
    game.manager
        .team_id
        .clone()
        .expect("the manager has a club")
}

fn seniors<'a>(game: &'a Game, team_id: &str) -> Vec<&'a Player> {
    game.players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(team_id) && p.squad_role == SquadRole::Senior && !p.retired
        })
        .collect()
}

/// Mean condition of the eleven best healthy seniors: the starters' reading the planners use.
fn starters_condition(game: &Game, team_id: &str) -> Option<f64> {
    let mut healthy: Vec<&Player> = seniors(game, team_id)
        .into_iter()
        .filter(|p| p.injury.is_none())
        .collect();
    if healthy.len() < 11 {
        return None;
    }
    healthy.sort_by(|a, b| b.ovr.cmp(&a.ovr).then(a.id.cmp(&b.id)));
    Some(
        healthy[..11]
            .iter()
            .map(|p| f64::from(p.condition))
            .sum::<f64>()
            / 11.0,
    )
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

/// Sets every attribute of the user's staff to `level`, keeping the assistant manager at a
/// constant 50 so the club's administration is the same in every arm and only the coaching and
/// medical departments differ. Contracts are kept for the whole study: one running out in season
/// four would otherwise end the experiment on its own.
fn set_user_staff(game: &mut Game, level: u8) {
    let team = user_team(game);
    for member in game
        .staff
        .iter_mut()
        .filter(|s| s.team_id.as_deref() == Some(team.as_str()))
    {
        let rating = if member.post() == StaffPost::AssistantManager {
            50
        } else {
            level
        };
        member.attributes.coaching = rating;
        member.attributes.judging_ability = rating;
        member.attributes.judging_potential = rating;
        member.attributes.physiotherapy = rating;
        member.contract_end = Some("2099-01-01".to_string());
    }
}

/// Removes everyone from the user's staff except the assistant manager (the "no staff" arm: no
/// coaches, no physios, no analysts; the administration still has its assistant).
fn dismiss_coaching_and_medical_staff(game: &mut Game) {
    let team = user_team(game);
    for member in game
        .staff
        .iter_mut()
        .filter(|s| s.team_id.as_deref() == Some(team.as_str()))
        .filter(|s| s.post() != StaffPost::AssistantManager)
    {
        member.team_id = None;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    /// Proposals are never answered by the study (nothing is delegated under "ask me").
    None,
    /// A manager who accepts every administrative proposal (a renewal, a signing, a promotion) and
    /// answers the training-load proposals every other time, by the proposal's number, so both
    /// the approved and the declined path are measured.
    EveryOther,
}

struct Arm {
    name: &'static str,
    setup: fn(&mut Game),
    policy: Policy,
}

/// The club every arm shares: an assistant, and the whole administration handed over (renewals,
/// recruitment, the academy, offers, the lineup, match preparation; not the staff themselves) so the squad does not simply
/// decay over ten seasons of a harness that never signs anybody. Training and tactics are left to
/// the manager; each arm decides what happens to them.
fn administered(game: &mut Game) {
    ensure_assistant(game);
    game.delegation.apply_mode(DelegationMode::Full);
    game.delegation.set(Duty::Training, false, false);
    game.delegation.set(Duty::Tactics, false, false);
    // The staff are the variable under study: left to the assistant he would re-hire whoever an
    // arm has dismissed and replace whoever it has made weak, and the arms would converge.
    game.delegation.set(Duty::StaffRecruitment, false, false);
    game.delegation.set(Duty::StaffRenewals, false, false);
}

fn with_staff(game: &mut Game, level: u8) {
    administered(game);
    set_user_staff(game, level);
}

fn arms() -> Vec<Arm> {
    vec![
        Arm {
            name: "manual training, average staff",
            setup: |g| with_staff(g, 50),
            policy: Policy::EveryOther,
        },
        Arm {
            name: "manual training, no coaching or medical staff",
            setup: |g| {
                with_staff(g, 50);
                dismiss_coaching_and_medical_staff(g);
            },
            policy: Policy::EveryOther,
        },
        Arm {
            name: "manual training, weak staff (30)",
            setup: |g| with_staff(g, 30),
            policy: Policy::EveryOther,
        },
        Arm {
            name: "manual training, strong staff (90)",
            setup: |g| with_staff(g, 90),
            policy: Policy::EveryOther,
        },
        Arm {
            name: "training delegated, acts alone",
            setup: |g| {
                with_staff(g, 50);
                g.delegation.set(Duty::Training, true, false);
            },
            policy: Policy::EveryOther,
        },
        Arm {
            name: "training delegated, ask me",
            setup: |g| {
                with_staff(g, 50);
                g.delegation.set(Duty::Training, true, true);
            },
            policy: Policy::EveryOther,
        },
        // The pressing and tempo the tactics duty settled on in the first seed, held by hand: a
        // control that separates what those two dials do from what delegating does (I6 found they
        // do nothing to match-eve condition; see APFM_I6_VALIDATION.md section 6). The duty's other
        // dials differ from seed to seed, so this is a matched control for seed 1 only.
        Arm {
            name: "manual training, passive and patient plan held by hand",
            setup: |g| {
                with_staff(g, 50);
                let id = user_team(g);
                if let Some(team) = g.teams.iter_mut().find(|t| t.id == id) {
                    team.tactics_phase.pressing_intensity =
                        domain::team::PressingIntensity::Passive;
                    team.tactics_phase.tempo = domain::team::Tempo::Patient;
                }
            },
            policy: Policy::EveryOther,
        },
        Arm {
            name: "training and tactics delegated (everything handed over)",
            setup: |g| {
                with_staff(g, 50);
                g.delegation.set(Duty::Training, true, false);
                g.delegation.set(Duty::Tactics, true, false);
            },
            policy: Policy::EveryOther,
        },
    ]
}

#[derive(Default)]
struct Season {
    points: u32,
    rank: usize,
    staff_bill: i64,
    staff_budget: i64,
}

#[derive(Default)]
struct Band {
    user_sum: f64,
    user_n: usize,
    ai_sum: f64,
    ai_n: usize,
}

/// Everything one arm's run is measured by.
struct Recorder {
    team_id: String,
    starters: Vec<f64>,
    /// The starters' condition the day before each of the club's matches: what is fielded.
    eve: Vec<f64>,
    /// Match-eve diagnostics of the lineup actually named and the plan actually in force (I6).
    xi_eve: Vec<f64>,
    plans: Plans,
    previous_xi: Vec<String>,
    xi_changes: usize,
    /// What the club's matches were like, read from the season's results (I6).
    results: ResultsRead,
    ai_starters: Vec<f64>,
    squad: Vec<f64>,
    injured_before: std::collections::HashSet<String>,
    injuries: usize,
    injury_days: u64,
    senior_breaches: usize,
    floor_breaches: usize,
    min_finance: i64,
    seasons: Vec<Season>,
    bands: Vec<Band>,
    previous: Snapshot,
    user_players_at_start: std::collections::HashSet<String>,
}

impl Recorder {
    fn new(game: &Game) -> Self {
        let team_id = user_team(game);
        Self {
            user_players_at_start: seniors(game, &team_id)
                .iter()
                .map(|p| p.id.clone())
                .collect(),
            previous: snapshot_of(game),
            team_id,
            starters: Vec::new(),
            eve: Vec::new(),
            xi_eve: Vec::new(),
            plans: Plans::default(),
            previous_xi: Vec::new(),
            xi_changes: 0,
            results: ResultsRead::default(),
            ai_starters: Vec::new(),
            squad: Vec::new(),
            injured_before: Default::default(),
            injuries: 0,
            injury_days: 0,
            senior_breaches: 0,
            floor_breaches: 0,
            min_finance: i64::MAX,
            seasons: Vec::new(),
            bands: AGE_BANDS.iter().map(|_| Band::default()).collect(),
        }
    }
}

/// Totals of the club's league and cup matches: how many, how they ended and how many
/// substitutions the engine's own manager made (a starter taken off early is a starter who wears
/// less, so this is where a tactical plan could reach a player's condition).
#[derive(Default)]
struct ResultsRead {
    matches: usize,
    wins: usize,
    draws: usize,
    goals_for: usize,
    goals_against: usize,
    substitutions: usize,
    cards: usize,
    /// Minutes the eleven best seniors (by ovr) played in the season, summed over seasons.
    top_eleven_minutes: u64,
    /// Days the eleven best seniors spent injured, summed over seasons.
    top_eleven_injured_days: u64,
}

impl ResultsRead {
    /// The eleven best seniors' season: how much they played and how long they were out.
    fn read_top_eleven(&mut self, game: &Game, team_id: &str) {
        let mut squad = seniors(game, team_id);
        squad.sort_by(|a, b| b.ovr.cmp(&a.ovr).then(a.id.cmp(&b.id)));
        for player in squad.iter().take(11) {
            self.top_eleven_minutes += u64::from(player.stats.minutes_played);
            self.top_eleven_injured_days += u64::from(player.stats.injured_days);
        }
    }

    fn read(&mut self, game: &Game, team_id: &str) {
        for fixture in game
            .competitions
            .iter()
            .flat_map(|c| &c.fixtures)
            .filter(|f| f.home_team_id == team_id || f.away_team_id == team_id)
        {
            let (Some(result), true) = (
                &fixture.result,
                fixture.status == domain::league::FixtureStatus::Completed,
            ) else {
                continue;
            };
            let home = fixture.home_team_id == team_id;
            let (gf, ga) = if home {
                (result.home_goals, result.away_goals)
            } else {
                (result.away_goals, result.home_goals)
            };
            self.matches += 1;
            self.goals_for += usize::from(gf);
            self.goals_against += usize::from(ga);
            match gf.cmp(&ga) {
                std::cmp::Ordering::Greater => self.wins += 1,
                std::cmp::Ordering::Equal => self.draws += 1,
                std::cmp::Ordering::Less => {}
            }
            if let Some(report) = &result.report {
                let side = if home { "Home" } else { "Away" };
                self.substitutions += report
                    .events
                    .iter()
                    .filter(|e| e.event_type == "Substitution" && e.side == side)
                    .count();
                self.cards += report
                    .events
                    .iter()
                    .filter(|e| e.event_type.contains("Card") && e.side == side)
                    .count();
            }
        }
    }
}

/// The plan the user's club takes into a match, counted by dial.
#[derive(Default)]
struct Plans {
    formation: BTreeMap<String, usize>,
    style: BTreeMap<String, usize>,
    pressing: BTreeMap<String, usize>,
    tempo: BTreeMap<String, usize>,
    /// Matches in which the plan differs from the previous match's.
    plan_changes: usize,
    last: Option<String>,
}

impl Recorder {
    /// What the club is about to field: the lineup's condition, how many of the eleven are new
    /// since the last match, and the dials in force.
    fn record_plan(&mut self, game: &Game) {
        let Some(team) = game.teams.iter().find(|t| t.id == self.team_id) else {
            return;
        };
        let name = |d: &dyn std::fmt::Debug| format!("{d:?}");
        *self
            .plans
            .formation
            .entry(team.formation.clone())
            .or_default() += 1;
        *self.plans.style.entry(name(&team.play_style)).or_default() += 1;
        *self
            .plans
            .pressing
            .entry(name(&team.tactics_phase.pressing_intensity))
            .or_default() += 1;
        *self
            .plans
            .tempo
            .entry(name(&team.tactics_phase.tempo))
            .or_default() += 1;
        let signature = format!(
            "{}|{:?}|{:?}|{:?}",
            team.formation,
            team.play_style,
            team.tactics_phase.pressing_intensity,
            team.tactics_phase.tempo
        );
        if self
            .plans
            .last
            .as_ref()
            .is_some_and(|last| *last != signature)
        {
            self.plans.plan_changes += 1;
        }
        self.plans.last = Some(signature);

        let xi: Vec<&Player> = team
            .starting_xi_ids
            .iter()
            .filter_map(|id| game.players.iter().find(|p| &p.id == id))
            .collect();
        if xi.len() == 11 {
            self.xi_eve
                .push(xi.iter().map(|p| f64::from(p.condition)).sum::<f64>() / 11.0);
        }
        if !self.previous_xi.is_empty() {
            self.xi_changes += team
                .starting_xi_ids
                .iter()
                .filter(|id| !self.previous_xi.contains(id))
                .count();
        }
        self.previous_xi = team.starting_xi_ids.clone();
    }
}

impl SeasonObserver for Recorder {
    fn on_day_end(&mut self, game: &Game, _season: u32, day: u32) {
        let squad = seniors(game, &self.team_id);
        if squad.len() > SENIOR_LIMIT {
            self.senior_breaches += 1;
        }
        if squad.len() < SENIOR_FLOOR {
            self.floor_breaches += 1;
        }
        let now: std::collections::HashSet<String> = squad
            .iter()
            .filter(|p| p.injury.is_some())
            .map(|p| p.id.clone())
            .collect();
        for player in squad.iter().filter(|p| p.injury.is_some()) {
            if !self.injured_before.contains(&player.id) {
                self.injuries += 1;
                self.injury_days += u64::from(
                    player
                        .injury
                        .as_ref()
                        .map_or(0, |injury| injury.days_remaining),
                );
            }
        }
        self.injured_before = now;
        if let Some(team) = game.teams.iter().find(|t| t.id == self.team_id) {
            self.min_finance = self.min_finance.min(team.finance);
        }
        let tomorrow = (game.clock.current_date + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let plays_tomorrow = game.competitions.iter().any(|c| {
            c.fixtures.iter().any(|f| {
                f.date == tomorrow
                    && (f.home_team_id == self.team_id || f.away_team_id == self.team_id)
            })
        });
        if plays_tomorrow && let Some(c) = starters_condition(game, &self.team_id) {
            self.eve.push(c);
            self.record_plan(game);
        }
        if day.is_multiple_of(7) {
            if let Some(c) = starters_condition(game, &self.team_id) {
                self.starters.push(c);
            }
            if !squad.is_empty() {
                self.squad.push(
                    squad.iter().map(|p| f64::from(p.condition)).sum::<f64>() / squad.len() as f64,
                );
            }
            let ai: Vec<f64> = game
                .teams
                .iter()
                .filter(|t| t.id != self.team_id)
                .filter_map(|t| starters_condition(game, &t.id))
                .collect();
            if !ai.is_empty() {
                self.ai_starters
                    .push(ai.iter().sum::<f64>() / ai.len() as f64);
            }
        }
    }

    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        self.results.read(game, &self.team_id);
        self.results.read_top_eleven(game, &self.team_id);
        let mut row = Season {
            staff_bill: market::weekly_staff_bill(game, &self.team_id),
            staff_budget: market::staff_budget(game, &self.team_id),
            ..Default::default()
        };
        if let Some(league) = game
            .competitions
            .iter()
            .filter(|l| l.standings.iter().any(|s| s.team_id == self.team_id))
            .max_by_key(|l| l.standings.len())
        {
            let mut table: Vec<_> = league.standings.iter().collect();
            table.sort_by(|a, b| {
                b.points
                    .cmp(&a.points)
                    .then(
                        (i64::from(b.goals_for) - i64::from(b.goals_against))
                            .cmp(&(i64::from(a.goals_for) - i64::from(a.goals_against))),
                    )
                    .then(a.team_id.cmp(&b.team_id))
            });
            if let Some(position) = table.iter().position(|s| s.team_id == self.team_id) {
                row.rank = position + 1;
                row.points = table[position].points;
            }
        }
        self.seasons.push(row);
    }

    fn on_season_end(&mut self, game: &Game, _summary: &EndOfSeasonSummary, _season: u32) {
        let after = snapshot_of(game);
        for (id, (ovr_before, age)) in &self.previous {
            let Some((ovr_after, _)) = after.get(id) else {
                continue;
            };
            let Some(index) = AGE_BANDS
                .iter()
                .position(|(_, low, high)| *age >= *low && *age <= *high)
            else {
                continue;
            };
            let change = f64::from(*ovr_after) - f64::from(*ovr_before);
            let band = &mut self.bands[index];
            if self.user_players_at_start.contains(id) {
                band.user_sum += change;
                band.user_n += 1;
            } else {
                band.ai_sum += change;
                band.ai_n += 1;
            }
        }
        self.previous = after;
        self.user_players_at_start = seniors(game, &self.team_id)
            .iter()
            .map(|p| p.id.clone())
            .collect();
    }
}

/// Answers the waiting proposals by the arm's policy. Returns (accepted, declined).
fn answer_proposals(game: &mut Game, policy: Policy) -> (usize, usize) {
    if policy == Policy::None {
        return (0, 0);
    }
    let waiting: Vec<String> = game
        .delegation
        .proposals
        .iter()
        .map(|p| p.id.clone())
        .collect();
    let (mut accepted, mut declined) = (0, 0);
    for id in waiting {
        let number: u64 = id.trim_start_matches("prop-").parse().unwrap_or(0);
        let proposal_is_training = game
            .delegation
            .proposals
            .iter()
            .any(|p| p.id == id && p.duty == Duty::Training);
        let accept = !proposal_is_training || number.is_multiple_of(2);
        // A proposal the game can no longer carry out is dropped by the game; that is fine.
        let _ = resolve_proposal(game, &id, accept);
        if accept {
            accepted += 1;
        } else {
            declined += 1;
        }
    }
    (accepted, declined)
}

struct ArmResult {
    name: &'static str,
    recorder: Recorder,
    outcomes: BTreeMap<&'static str, usize>,
    training_decisions: usize,
    proposals_raised: u64,
    invariants_clean: bool,
    invariants_report: String,
}

fn outcome_name(outcome: DecisionOutcome) -> &'static str {
    match outcome {
        DecisionOutcome::Executed => "executed",
        DecisionOutcome::Approved => "approved",
        DecisionOutcome::Declined => "declined",
        DecisionOutcome::LeftToManager => "leftToManager",
    }
}

fn play_arm(arm: &Arm, seed: u64, seasons: u32) -> ArmResult {
    let mut game = WorldSpec::gate(seed).build().expect("the world builds");
    (arm.setup)(&mut game);
    let options = RunOptions::seasons(seasons);
    let mut recorder = Recorder::new(&game);
    let mut invariants = Invariants::new(&game);
    let mut outcomes: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut training_decisions = 0;
    for season in 1..=seasons {
        let mut day = 0;
        while !is_season_complete(&game) {
            day += 1;
            assert!(day < 470, "{}: season {season} did not complete", arm.name);
            play_day(
                &mut game,
                options,
                season,
                day,
                &mut (&mut recorder, &mut invariants),
            )
            .unwrap_or_else(|e| panic!("{}: {e}", arm.name));
            answer_proposals(&mut game, arm.policy);
        }
        roll_season_over(
            &mut game,
            options,
            season,
            &mut (&mut recorder, &mut invariants),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", arm.name));
    }
    // The history keeps the last 120 decisions, so the totals below read it season by season
    // would be better; the arm reports what is left at the end plus the proposal counter.
    for entry in &game.delegation.log {
        *outcomes.entry(outcome_name(entry.outcome)).or_default() += 1;
        if entry.duty == Duty::Training {
            training_decisions += 1;
        }
    }
    ArmResult {
        name: arm.name,
        recorder,
        outcomes,
        training_decisions,
        proposals_raised: game.delegation.next_proposal,
        invariants_clean: invariants.is_clean(),
        invariants_report: invariants.report(),
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn share_below(values: &[f64], limit: f64) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().filter(|v| **v < limit).count() as f64 / values.len() as f64 * 100.0
    }
}

fn print_arm(result: &ArmResult, seasons: u32) {
    let r = &result.recorder;
    let per_season = f64::from(seasons);
    println!("\n== {} ==", result.name);
    println!(
        "  starters' condition: mean {:.1}, min {:.1}, weeks below 70: {:.0}% (AI clubs mean {:.1}); squad mean {:.1}",
        mean(&r.starters),
        r.starters.iter().copied().fold(f64::MAX, f64::min),
        share_below(&r.starters, 70.0),
        mean(&r.ai_starters),
        mean(&r.squad),
    );
    println!(
        "  on the eve of a match ({} matches): mean {:.1}, min {:.1}, below 70: {:.0}%, below 60: {:.0}%",
        r.eve.len(),
        mean(&r.eve),
        r.eve.iter().copied().fold(f64::MAX, f64::min),
        share_below(&r.eve, 70.0),
        share_below(&r.eve, 60.0),
    );
    let histogram = |h: &BTreeMap<String, usize>| {
        let total: usize = h.values().sum::<usize>().max(1);
        h.iter()
            .map(|(k, v)| format!("{k} {:.0}%", *v as f64 / total as f64 * 100.0))
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!(
        "  lineup named on the eve: mean condition {:.1} (n={}), {:.2} changes a match, plan changed in {} of {} matches",
        mean(&r.xi_eve),
        r.xi_eve.len(),
        r.xi_changes as f64 / r.xi_eve.len().max(1) as f64,
        r.plans.plan_changes,
        r.eve.len(),
    );
    let n = r.results.matches.max(1) as f64;
    println!(
        "  results ({} matches): W {:.0}% D {:.0}%, goals {:.2}-{:.2}, substitutions by the engine's manager {:.2} a match, cards {:.2} a match",
        r.results.matches,
        r.results.wins as f64 / n * 100.0,
        r.results.draws as f64 / n * 100.0,
        r.results.goals_for as f64 / n,
        r.results.goals_against as f64 / n,
        r.results.substitutions as f64 / n,
        r.results.cards as f64 / n,
    );
    println!(
        "  the eleven best seniors: {:.1} minutes a match each, {:.1} injured days a season each",
        r.results.top_eleven_minutes as f64 / n / 11.0,
        r.results.top_eleven_injured_days as f64 / per_season / 11.0,
    );
    println!(
        "  plans: formation [{}]; style [{}]; pressing [{}]; tempo [{}]",
        histogram(&r.plans.formation),
        histogram(&r.plans.style),
        histogram(&r.plans.pressing),
        histogram(&r.plans.tempo),
    );
    println!(
        "  injuries/season {:.1}, days lost/season {:.0}",
        r.injuries as f64 / per_season,
        r.injury_days as f64 / per_season
    );
    println!(
        "  points by season: {:?}; finish: {:?}; mean {:.1}",
        r.seasons.iter().map(|s| s.points).collect::<Vec<_>>(),
        r.seasons.iter().map(|s| s.rank).collect::<Vec<_>>(),
        mean(
            &r.seasons
                .iter()
                .map(|s| f64::from(s.points))
                .collect::<Vec<_>>()
        ),
    );
    println!(
        "  staff bill/week at the end {} of budget {}",
        r.seasons.last().map_or(0, |s| s.staff_bill),
        r.seasons.last().map_or(0, |s| s.staff_budget)
    );
    let bands: Vec<String> = AGE_BANDS
        .iter()
        .zip(&r.bands)
        .map(|((label, _, _), b)| {
            format!(
                "{label}: club {:+.2} (n={}) / AI {:+.2}",
                if b.user_n > 0 {
                    b.user_sum / b.user_n as f64
                } else {
                    0.0
                },
                b.user_n,
                if b.ai_n > 0 {
                    b.ai_sum / b.ai_n as f64
                } else {
                    0.0
                },
            )
        })
        .collect();
    println!("  ovr change per season by age: {}", bands.join(" | "));
    println!(
        "  decisions in the history: {:?} ({} on training); proposals raised in all: {}",
        result.outcomes, result.training_decisions, result.proposals_raised
    );
    println!(
        "  breaches: seniors>30 on {} days, below the floor on {} days; lowest cash {}; invariants clean: {}",
        r.senior_breaches, r.floor_breaches, r.min_finance, result.invariants_clean
    );
}

/// Given the same world and the same delegated administration under seven arms
/// When each plays the configured seasons
/// Then no arm ever breaks the senior limit or the squad floor or runs the club's cash below what
///      it started with by more than the arms' own spending allows, no invariant fails, and a
///      club whose training is delegated fields starters within five points of the manager's own
///      condition on the eve of a match, with at most one match in ten below 60 (the safety, not
///      the margin: the numbers are printed for reading).
#[test]
#[ignore = "long: set I9_SEASONS and I9_SEEDS, run with --nocapture"]
fn i9_staff_training_study() {
    let seasons: u32 = std::env::var("I9_SEASONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let seeds: Vec<u64> = std::env::var("I9_SEEDS")
        .unwrap_or_else(|_| "1".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    for seed in seeds {
        let mut baseline_eve: Option<f64> = None;
        println!("\n######## seed {seed}, {seasons} seasons ########");
        let only: Vec<String> = std::env::var("I9_ARMS")
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();
        for arm in arms() {
            if !only.is_empty() && !only.iter().any(|name| arm.name.contains(name.as_str())) {
                continue;
            }
            let started = std::time::Instant::now();
            let result = play_arm(&arm, seed, seasons);
            print_arm(&result, seasons);
            println!("  ({:.0}s)", started.elapsed().as_secs_f64());
            assert_eq!(
                result.recorder.senior_breaches, 0,
                "{}: the 30-senior hold was broken",
                arm.name
            );
            assert_eq!(
                result.recorder.floor_breaches, 0,
                "{}: the squad floor was broken",
                arm.name
            );
            assert!(
                result.invariants_clean,
                "{}: {}",
                arm.name, result.invariants_report
            );
            if arm.name == "manual training, average staff" {
                baseline_eve = Some(mean(&result.recorder.eve));
            }
            // The training arms are held to the baseline. The arm that also hands over the
            // tactics is reported, not asserted: what it costs is a finding for the tactical
            // planner (APFM_I9_VALIDATION.md section 4).
            if arm.name.starts_with("training delegated")
                && let Some(baseline) = baseline_eve
            {
                let eve = mean(&result.recorder.eve);
                assert!(
                    eve >= baseline - 5.0,
                    "{}: starters reached matches at {eve:.1} against {baseline:.1} when the manager plays by hand",
                    arm.name
                );
                assert!(
                    share_below(&result.recorder.eve, 60.0) <= 10.0,
                    "{}: more than one match in ten was played by starters below 60",
                    arm.name
                );
            }
        }
    }
}

/// The user's first-team (or fitness) coach, kept on a contract that outlasts the test so the
/// rollover cannot be blamed for losing him. The generated club staff has one first-team coach.
fn keep_a_coach(game: &mut Game) -> String {
    let team = user_team(game);
    let member = game
        .staff
        .iter_mut()
        .filter(|s| s.team_id.as_deref() == Some(team.as_str()))
        .find(|s| {
            matches!(
                s.post(),
                StaffPost::FirstTeamCoach | StaffPost::FitnessCoach
            )
        })
        .expect("the generated club staff has a coach");
    member.contract_end = Some("2099-01-01".to_string());
    member.id.clone()
}

struct Quiet;
impl SeasonObserver for Quiet {}

/// Given a delegated career whose training is run by a hired coach under "ask me"
/// When it is saved and reloaded mid-season, and the same following days are played from the
///      reloaded game and from the game that was never saved
/// Then both end with identical delegation state (nothing was repeated or lost by the reload),
///      the named owner is still in charge after the reload and after the rollover, and no two
///      proposals share an id.
#[test]
fn a_reloaded_delegated_career_does_not_repeat_what_it_did() {
    let mut game = WorldSpec::gate(1).build().expect("the world builds");
    ensure_assistant(&mut game);
    let coach = keep_a_coach(&mut game);
    game.delegation.apply_mode(DelegationMode::Approval);
    let team = user_team(&game);
    delegation_owner::assign(&mut game, &team, Duty::Training, Some(&coach))
        .expect("a coach can run training");
    let options = RunOptions::seasons(1);
    let mut invariants = Invariants::new(&game);

    for day in 1..=60 {
        play_day(
            &mut game,
            options,
            1,
            day,
            &mut (&mut Quiet, &mut invariants),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        answer_proposals(&mut game, Policy::EveryOther);
    }
    assert_eq!(game.delegation.owner(Duty::Training), Some(coach.as_str()));
    // The comparison below only means something if the delegation has been doing things.
    assert!(
        game.delegation.next_proposal > 0 || !game.delegation.log.is_empty(),
        "sixty days of delegation left no proposal and no history"
    );
    assert!(
        game.delegation.applied(Duty::Training).is_some(),
        "the coach never wrote a training plan"
    );

    let dir = tempfile::tempdir().unwrap();
    let mut saves = SaveManager::init(dir.path()).unwrap();
    let save_id = saves
        .create_save_with_stats(&game, &StatsState::default(), "i9")
        .unwrap();
    saves
        .save_game_with_stats(&game, &StatsState::default(), &save_id)
        .unwrap();
    let mut reloaded = SaveManager::init(dir.path())
        .unwrap()
        .load_game(&save_id)
        .unwrap();
    assert_eq!(
        reloaded.delegation, game.delegation,
        "the round trip changed the state"
    );
    assert_eq!(
        reloaded.delegation.owner(Duty::Training),
        Some(coach.as_str())
    );

    let mut inv_b = Invariants::new(&reloaded);
    for day in 61..=90 {
        play_day(
            &mut game,
            options,
            1,
            day,
            &mut (&mut Quiet, &mut invariants),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        answer_proposals(&mut game, Policy::EveryOther);
        play_day(
            &mut reloaded,
            options,
            1,
            day,
            &mut (&mut Quiet, &mut inv_b),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        answer_proposals(&mut reloaded, Policy::EveryOther);
    }
    assert_eq!(
        reloaded.delegation, game.delegation,
        "a reloaded game does not follow the game that was never saved"
    );
    let ids: Vec<&str> = game
        .delegation
        .proposals
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    let distinct: std::collections::HashSet<&&str> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "two proposals share an id");

    // The season to its end and across the rollover: the owner and the mode are still set.
    let mut day = 90;
    while !is_season_complete(&game) {
        day += 1;
        assert!(day < 470, "the season did not complete");
        play_day(
            &mut game,
            options,
            1,
            day,
            &mut (&mut Quiet, &mut invariants),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        answer_proposals(&mut game, Policy::EveryOther);
    }
    roll_season_over(&mut game, options, 1, &mut (&mut Quiet, &mut invariants))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(game.delegation.mode(), DelegationMode::Approval);
    let still_employed = game
        .staff
        .iter()
        .any(|s: &Staff| s.id == coach && s.team_id.as_deref() == Some(team.as_str()));
    if still_employed {
        assert_eq!(
            game.delegation.owner(Duty::Training),
            Some(coach.as_str()),
            "the rollover dropped the owner of training"
        );
    }
    assert!(invariants.is_clean(), "{}", invariants.report());
}
