//! Run F5.4: what the AI clubs' squads look like, season by season, by standing.
//!
//! Measurement only; nothing here is stored in a save and nothing here is game logic. The
//! instrument reads only what the game itself defines — the eleven a club last fielded
//! (`Team::starting_xi_ids`), the game's own positional fit (`positional_fit_for_assignment`), the
//! league tables — so the same file can be dropped into an earlier tree and run there, and the
//! sporting-strength proxy it reports is not the one the decisions under test are made from.
//!
//! The proxy is the mean rating of the fielded eleven *in the slots the formation puts them in*. It
//! is a proxy for "a team that can play", not a measure of results; the study also reports how well
//! it predicts a league season's points, so the reader can judge it.

use std::collections::{BTreeMap, HashMap};

use domain::finance::CashKind;
use domain::player::{Player, SquadRole};
use ofm_core::game::Game;
use ofm_core::player_rating::{formation_slots, positional_fit_for_assignment};

use crate::driver::SeasonObserver;

/// A fielded player rated under this in his slot is a hole in the side.
pub const WEAK_SLOT_FIT: f64 = 55.0;

/// One club, one season.
#[derive(Debug, Clone, Default)]
pub struct SquadYear {
    pub club_id: String,
    pub tier: usize,
    pub reputation: u32,
    /// Mean rating of the fielded eleven in their slots.
    pub xi_fit: f64,
    pub worst_slot_fit: f64,
    /// Fielded players rated under [`WEAK_SLOT_FIT`] in their slot.
    pub weak_slots: usize,
    /// Fielded players whose natural group is not the group of their slot (a striker at centre-back).
    pub out_of_group: usize,
    /// A fielded eleven with no natural keeper in goal, or a squad with fewer than two keepers.
    pub keeper_hole: bool,
    pub seniors: usize,
    /// Seniors outside the eleven who are rated within fifteen points of it.
    pub usable_bench: usize,
    /// Position groups (GK, DEF, MID, FWD) with fewer seniors than the floor plus the margin.
    pub thin_groups: usize,
    pub mean_age: f64,
    pub share_young: f64,
    pub share_old: f64,
    /// Players who joined since the last reading, and how many of them barely played.
    pub joined: usize,
    pub joined_unused: usize,
    /// Transfer fees the club paid out in the season, in euros.
    pub fees_paid: i64,
    pub points_per_game: Option<f64>,
    pub league: Option<String>,
}

fn group_of(player: &Player) -> usize {
    match player.natural_position.to_group_position() {
        domain::player::Position::Goalkeeper => 0,
        domain::player::Position::Defender => 1,
        domain::player::Position::Midfielder => 2,
        _ => 3,
    }
}

/// Seniors a group needs: the floor (2 keepers, 4 defenders, 4 midfielders, 2 forwards) plus the
/// planning margin of one.
const GROUP_NEED: [usize; 4] = [3, 5, 5, 3];

fn read_club(
    game: &Game,
    team: &domain::team::Team,
    squad: &[&Player],
    tiers: &HashMap<String, usize>,
    before: &HashMap<String, Option<String>>,
) -> SquadYear {
    let slots = formation_slots(&team.formation);
    let by_id: HashMap<&str, &Player> = squad.iter().map(|p| (p.id.as_str(), *p)).collect();
    let mut fits = Vec::new();
    let (mut out_of_group, mut goalkeeper_in_goal) = (0, false);
    for (slot_index, id) in team.starting_xi_ids.iter().enumerate() {
        let (Some(slot), Some(player)) = (slots.get(slot_index), by_id.get(id.as_str())) else {
            continue;
        };
        fits.push(positional_fit_for_assignment(player, slot));
        let slot_group = match slot.to_group_position() {
            domain::player::Position::Goalkeeper => 0,
            domain::player::Position::Defender => 1,
            domain::player::Position::Midfielder => 2,
            _ => 3,
        };
        if group_of(player) != slot_group {
            out_of_group += 1;
        }
        if slot_index == 0 && player.natural_position == domain::player::Position::Goalkeeper {
            goalkeeper_in_goal = true;
        }
    }
    let seniors: Vec<&&Player> = squad
        .iter()
        .filter(|p| p.squad_role == SquadRole::Senior)
        .collect();
    let mut by_group = [0_usize; 4];
    for player in &seniors {
        by_group[group_of(player)] += 1;
    }
    let xi_fit = fits.iter().sum::<f64>() / fits.len().max(1) as f64;
    let in_xi = |p: &Player| team.starting_xi_ids.contains(&p.id);
    let usable_bench = seniors
        .iter()
        .filter(|p| !in_xi(p) && f64::from(p.ovr) >= xi_fit - 15.0)
        .count();
    let ages: Vec<i32> = seniors
        .iter()
        .map(|p| {
            domain::age::age_on_or_unknown(game.clock.current_date.date_naive(), &p.date_of_birth)
        })
        .collect();
    let n = ages.len().max(1) as f64;
    let (mut joined, mut joined_unused) = (0, 0);
    for player in &seniors {
        if let Some(previous) = before.get(&player.id)
            && previous.as_deref() != Some(team.id.as_str())
        {
            joined += 1;
            if player.stats.appearances <= 2 && !in_xi(player) {
                joined_unused += 1;
            }
        }
    }
    SquadYear {
        club_id: team.id.clone(),
        tier: tiers.get(&team.id).copied().unwrap_or(0),
        reputation: team.reputation,
        xi_fit,
        worst_slot_fit: fits.iter().copied().fold(f64::MAX, f64::min),
        weak_slots: fits.iter().filter(|fit| **fit < WEAK_SLOT_FIT).count(),
        out_of_group,
        keeper_hole: !goalkeeper_in_goal || by_group[0] < 2,
        seniors: seniors.len(),
        usable_bench,
        thin_groups: (0..4).filter(|&g| by_group[g] < GROUP_NEED[g]).count(),
        mean_age: ages.iter().map(|a| f64::from(*a)).sum::<f64>() / n,
        share_young: ages.iter().filter(|a| **a <= 21).count() as f64 / n,
        share_old: ages.iter().filter(|a| **a >= 32).count() as f64 / n,
        joined,
        joined_unused,
        fees_paid: 0,
        points_per_game: None,
        league: None,
    }
}

/// Every club's season, one reading per rollover.
#[derive(Debug, Clone, Default)]
pub struct SquadLog {
    /// `years[s - 1]` is season `s`.
    pub years: Vec<Vec<SquadYear>>,
    /// The league champion's founding tier, per season.
    pub champions: Vec<Vec<usize>>,
    tiers: HashMap<String, usize>,
    before: HashMap<String, Option<String>>,
    user: Option<String>,
}

fn founding_tiers(game: &Game) -> HashMap<String, usize> {
    let mut by_rep: Vec<(&str, u32)> = game
        .teams
        .iter()
        .map(|team| (team.id.as_str(), team.reputation))
        .collect();
    by_rep.sort_by_key(|(id, rep)| (*rep, id.to_string()));
    let n = by_rep.len().max(1);
    by_rep
        .iter()
        .enumerate()
        .map(|(rank, (id, _))| (id.to_string(), (rank * 4 / n).min(3)))
        .collect()
}

fn who_is_where(game: &Game) -> HashMap<String, Option<String>> {
    game.players
        .iter()
        .filter(|p| !p.retired)
        .map(|p| (p.id.clone(), p.team_id.clone()))
        .collect()
}

impl SquadLog {
    pub fn starting_from(game: &Game) -> Self {
        Self {
            tiers: founding_tiers(game),
            before: who_is_where(game),
            user: game.manager.team_id.clone(),
            ..Self::default()
        }
    }

    fn read(&mut self, game: &Game) {
        let mut squads: HashMap<&str, Vec<&Player>> = HashMap::new();
        for player in game.players.iter().filter(|p| !p.retired) {
            if let Some(club) = player.team_id.as_deref() {
                squads.entry(club).or_default().push(player);
            }
        }
        let since = (game.clock.current_date.date_naive() - chrono::Duration::days(365))
            .format("%Y-%m-%d")
            .to_string();
        let mut fees: HashMap<&str, i64> = HashMap::new();
        for post in game.cash_journal.iter() {
            if post.kind == CashKind::TransferFeeOut && post.date.as_str() > since.as_str() {
                *fees.entry(post.club_id.as_str()).or_default() += -post.amount;
            }
        }
        let mut year: Vec<SquadYear> = game
            .teams
            .iter()
            .filter(|team| Some(&team.id) != self.user.as_ref())
            .map(|team| {
                let squad = squads.get(team.id.as_str()).map_or(&[][..], Vec::as_slice);
                let mut read = read_club(game, team, squad, &self.tiers, &self.before);
                read.fees_paid = fees.get(team.id.as_str()).copied().unwrap_or(0);
                read
            })
            .collect();
        let mut champions = Vec::new();
        for league in &game.competitions {
            if league.standings.len() < 8 {
                continue;
            }
            let mut best: Option<(&str, u32)> = None;
            for entry in &league.standings {
                if entry.played == 0 {
                    continue;
                }
                if best.is_none_or(|(_, points)| entry.points > points) {
                    best = Some((entry.team_id.as_str(), entry.points));
                }
                if let Some(club) = year.iter_mut().find(|c| c.club_id == entry.team_id) {
                    club.points_per_game = Some(f64::from(entry.points) / f64::from(entry.played));
                    club.league = Some(league.id.clone());
                }
            }
            if let Some((team, _)) = best {
                champions.push(self.tiers.get(team).copied().unwrap_or(0));
            }
        }
        self.years.push(year);
        self.champions.push(champions);
        self.before = who_is_where(game);
    }
}

impl SeasonObserver for SquadLog {
    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        self.read(game);
    }
}

fn mean<I: IntoIterator<Item = f64>>(values: I) -> f64 {
    let (sum, count) = values
        .into_iter()
        .fold((0.0, 0_usize), |(s, n), v| (s + v, n + 1));
    sum / count.max(1) as f64
}

fn std_dev(values: &[f64]) -> f64 {
    let m = mean(values.iter().copied());
    mean(values.iter().map(|v| (v - m).powi(2))).sqrt()
}

fn pearson(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 4 {
        return None;
    }
    let (mx, my) = (
        mean(pairs.iter().map(|p| p.0)),
        mean(pairs.iter().map(|p| p.1)),
    );
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (x, y) in pairs {
        sxy += (x - mx) * (y - my);
        sxx += (x - mx).powi(2);
        syy += (y - my).powi(2);
    }
    (sxx > 0.0 && syy > 0.0).then(|| sxy / (sxx * syy).sqrt())
}

/// Mean within-league correlation of the proxy with points per game: does a stronger fielded eleven
/// win more? Leagues of eight or more clubs only.
fn proxy_predicts_points(year: &[SquadYear]) -> Option<f64> {
    let mut by_league: BTreeMap<&str, Vec<(f64, f64)>> = BTreeMap::new();
    for club in year {
        if let (Some(league), Some(ppg)) = (club.league.as_deref(), club.points_per_game) {
            by_league
                .entry(league)
                .or_default()
                .push((club.xi_fit, ppg));
        }
    }
    let rs: Vec<f64> = by_league.values().filter_map(|p| pearson(p)).collect();
    (!rs.is_empty()).then(|| mean(rs))
}

/// One printed row per wanted season, then the same by standing at the last wanted season.
pub fn report(log: &SquadLog, wanted: &[usize]) -> String {
    let mut out = String::new();
    out.push_str(
        "season | clubs | XI fit mean (sd) | worst slot | clubs w/ weak slot | weak slots/club | out-of-group starters/club | keeper hole % | seniors | usable bench | thin groups/club | age | <=21 % | >=32 % | joined/club | unused joiners % | fees/club (M) | r(XI,ppg) | titles by tier Q1..Q4\n",
    );
    for &season in wanted {
        let Some(year) = season.checked_sub(1).and_then(|i| log.years.get(i)) else {
            continue;
        };
        let n = year.len().max(1) as f64;
        let xi: Vec<f64> = year.iter().map(|c| c.xi_fit).collect();
        let joined: usize = year.iter().map(|c| c.joined).sum();
        let unused: usize = year.iter().map(|c| c.joined_unused).sum();
        let titles = (0..4)
            .map(|tier| {
                log.champions[season - 1]
                    .iter()
                    .filter(|t| **t == tier)
                    .count()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("/");
        out.push_str(&format!(
            "{season} | {} | {:.1} ({:.1}) | {:.1} | {:.0}% | {:.2} | {:.2} | {:.1}% | {:.1} | {:.1} | {:.2} | {:.1} | {:.0}% | {:.0}% | {:.1} | {:.0}% | {:.2} | {} | {titles}\n",
            year.len(),
            mean(xi.iter().copied()),
            std_dev(&xi),
            mean(year.iter().map(|c| c.worst_slot_fit)),
            100.0 * year.iter().filter(|c| c.weak_slots > 0).count() as f64 / n,
            mean(year.iter().map(|c| c.weak_slots as f64)),
            mean(year.iter().map(|c| c.out_of_group as f64)),
            100.0 * year.iter().filter(|c| c.keeper_hole).count() as f64 / n,
            mean(year.iter().map(|c| c.seniors as f64)),
            mean(year.iter().map(|c| c.usable_bench as f64)),
            mean(year.iter().map(|c| c.thin_groups as f64)),
            mean(year.iter().map(|c| c.mean_age)),
            100.0 * mean(year.iter().map(|c| c.share_young)),
            100.0 * mean(year.iter().map(|c| c.share_old)),
            joined as f64 / n,
            100.0 * unused as f64 / joined.max(1) as f64,
            mean(year.iter().map(|c| c.fees_paid as f64)) / 1e6,
            proxy_predicts_points(year).map_or("n/a".to_string(), |r| format!("{r:.2}")),
        ));
    }
    if let Some(last) = wanted
        .iter()
        .rev()
        .find_map(|s| s.checked_sub(1).and_then(|i| log.years.get(i)))
    {
        out.push_str("\nby founding reputation quartile, last wanted season:\ntier | XI fit | weak slots/club | usable bench | seniors | fees/club (M)\n");
        for tier in 0..4 {
            let group: Vec<&SquadYear> = last.iter().filter(|c| c.tier == tier).collect();
            out.push_str(&format!(
                "Q{} | {:.1} | {:.2} | {:.1} | {:.1} | {:.2}\n",
                tier + 1,
                mean(group.iter().map(|c| c.xi_fit)),
                mean(group.iter().map(|c| c.weak_slots as f64)),
                mean(group.iter().map(|c| c.usable_bench as f64)),
                mean(group.iter().map(|c| c.seniors as f64)),
                mean(group.iter().map(|c| c.fees_paid as f64)) / 1e6,
            ));
        }
    }
    out
}

/// A one-line summary of the whole run, for comparing two trees at a glance: the mean over every
/// season of the numbers that matter most.
pub fn summary(log: &SquadLog) -> String {
    let all: Vec<&SquadYear> = log.years.iter().flatten().collect();
    let n = all.len().max(1) as f64;
    let joined: usize = all.iter().map(|c| c.joined).sum();
    let unused: usize = all.iter().map(|c| c.joined_unused).sum();
    format!(
        "SUMMARY club-seasons {} | XI fit {:.2} | weak-slot clubs {:.1}% | out-of-group {:.3} | keeper hole {:.1}% | usable bench {:.2} | thin groups {:.3} | unused joiners {:.1}% | fees/club-season {:.2} M",
        all.len(),
        mean(all.iter().map(|c| c.xi_fit)),
        100.0 * all.iter().filter(|c| c.weak_slots > 0).count() as f64 / n,
        mean(all.iter().map(|c| c.out_of_group as f64)),
        100.0 * all.iter().filter(|c| c.keeper_hole).count() as f64 / n,
        mean(all.iter().map(|c| c.usable_bench as f64)),
        mean(all.iter().map(|c| c.thin_groups as f64)),
        100.0 * unused as f64 / joined.max(1) as f64,
        mean(all.iter().map(|c| c.fees_paid as f64)) / 1e6,
    )
}
