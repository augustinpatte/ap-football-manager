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
use domain::player::{Player, PlayerMovementKind, SquadRole};
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
    /// Joiners split by how they arrived: permanent transfer, free agent, loan, other.
    pub joined_by_kind: [usize; JOINER_KINDS],
    pub unused_by_kind: [usize; JOINER_KINDS],
    /// Transfer fees the club paid out in the season, in euros.
    pub fees_paid: i64,
    pub points_per_game: Option<f64>,
    pub league: Option<String>,
}

/// How a player arrived at the club he is at now, for splitting the joiner count.
pub const JOINER_KINDS: usize = 4;
pub const JOINER_KIND_LABELS: [&str; JOINER_KINDS] = ["Transfer", "FreeAgent", "Loan", "Other"];

/// The kind of arrival that put a player at his current club. A loan is read first: the player
/// plays for the borrower, so the latest arrival at that club is a loan even if an earlier entry
/// was the permanent move to his parent.
fn joiner_kind(player: &Player) -> usize {
    if player.active_loan.is_some() {
        return 2;
    }
    let Some(club) = player.team_id.as_deref() else {
        return 3;
    };
    let latest = player.movement_history.iter().rev().find(|entry| {
        entry.to_team_id.as_deref() == Some(club)
            && matches!(
                entry.kind,
                PlayerMovementKind::PermanentTransfer
                    | PlayerMovementKind::FreeAgentSigning
                    | PlayerMovementKind::LoanToBuy
                    | PlayerMovementKind::LoanStart
                    | PlayerMovementKind::InitialContract
            )
    });
    match latest.map(|entry| entry.kind.clone()) {
        Some(PlayerMovementKind::PermanentTransfer | PlayerMovementKind::LoanToBuy) => 0,
        Some(PlayerMovementKind::FreeAgentSigning) => 1,
        Some(PlayerMovementKind::LoanStart) => 2,
        _ => 3,
    }
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

/// The depth a club plans to carry in each group (`ofm_core::transfers`'s `DESIRED_DEPTH`, which is
/// crate-private; this measurement copy is the same `[GK, DEF, MID, FWD]` plan `ai_squad_sales`
/// sells down to).
const PLANNED_DEPTH: [usize; 4] = [3, 7, 7, 5];

/// The eleven a club could field today, chosen the way the game's lineup picker chooses when a club
/// has no saved eleven: slot by slot in the formation's order, a natural keeper first in goal, the
/// best positional fit among those not yet picked. Injured players are not available. Ties go to the
/// lower player id, so the answer never depends on storage order. Not the picker's own function (it
/// is private to the game); the same rule, so that a figure here is "what the game would play".
fn fielded_eleven<'a>(
    slots: &[domain::player::Position],
    seniors: &[&'a Player],
) -> Vec<(&'a Player, f64)> {
    let mut available: Vec<&Player> = seniors
        .iter()
        .copied()
        .filter(|p| p.injury.is_none())
        .collect();
    available.sort_by(|a, b| a.id.cmp(&b.id));
    let mut taken: Vec<&str> = Vec::new();
    let mut eleven = Vec::new();
    for (index, slot) in slots.iter().enumerate().take(11) {
        let key = |p: &&Player| {
            let keeper_first =
                index == 0 && p.natural_position == domain::player::Position::Goalkeeper;
            (keeper_first, positional_fit_for_assignment(p, slot))
        };
        let best = available
            .iter()
            .filter(|p| !taken.contains(&p.id.as_str()))
            .fold(None::<&&Player>, |best, candidate| match best {
                Some(held) if key(held) >= key(candidate) => Some(held),
                _ => Some(candidate),
            });
        let Some(player) = best.copied() else {
            break;
        };
        taken.push(player.id.as_str());
        eleven.push((player, positional_fit_for_assignment(player, slot)));
    }
    eleven
}

fn read_club(
    game: &Game,
    team: &domain::team::Team,
    squad: &[&Player],
    tiers: &HashMap<String, usize>,
    before: &HashMap<String, Option<String>>,
) -> SquadYear {
    let slots = formation_slots(&team.formation);
    let seniors: Vec<&&Player> = squad
        .iter()
        .filter(|p| p.squad_role == SquadRole::Senior)
        .collect();
    let senior_refs: Vec<&Player> = seniors.iter().map(|p| **p).collect();
    let eleven = fielded_eleven(&slots, &senior_refs);
    let fits: Vec<f64> = eleven.iter().map(|(_, fit)| *fit).collect();
    let mut out_of_group = 0;
    let mut goalkeeper_in_goal = false;
    for (slot_index, (player, _)) in eleven.iter().enumerate() {
        let slot_group = match slots[slot_index].to_group_position() {
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
    let fielded: Vec<&str> = eleven.iter().map(|(p, _)| p.id.as_str()).collect();
    let mut by_group = [0_usize; 4];
    for player in &seniors {
        by_group[group_of(player)] += 1;
    }
    let xi_fit = fits.iter().sum::<f64>() / fits.len().max(1) as f64;
    let in_xi = |p: &Player| fielded.contains(&p.id.as_str());
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
    let mut joined_by_kind = [0_usize; JOINER_KINDS];
    let mut unused_by_kind = [0_usize; JOINER_KINDS];
    for player in &seniors {
        if let Some(previous) = before.get(&player.id)
            && previous.as_deref() != Some(team.id.as_str())
        {
            joined += 1;
            let kind = joiner_kind(player);
            joined_by_kind[kind] += 1;
            if player.stats.appearances <= 2 && !in_xi(player) {
                joined_unused += 1;
                unused_by_kind[kind] += 1;
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
        joined_by_kind,
        unused_by_kind,
        fees_paid: 0,
        points_per_game: None,
        league: None,
    }
}

/// One completed AI permanent transfer as the market and the season saw it: the buyer's intended
/// role at the moment of the deal, the fee, and what the player actually did afterwards.
#[derive(Debug, Clone)]
pub struct ObservedDeal {
    pub buyer_id: String,
    pub position_group: usize,
    pub role: &'static str,
    pub starts_at: Option<usize>,
    pub points: i32,
    pub gain: f32,
    pub fee: u64,
    pub age: i32,
    pub weekly_wage: u32,
    /// The buyer's available senior group depth when the deal was agreed.
    pub buyer_group_depth: usize,
    pub buyer_planned_depth: usize,
    /// The buyer's formation and its registered seniors in the target's group, best first.
    pub buyer_formation: String,
    pub buyer_levels: Vec<u8>,
    pub natural_position: String,
    pub alternate_positions: Vec<String>,
    pub potential: u8,
    /// The day the deal completed (`YYYY-MM-DD`).
    pub date: String,
    /// Season appearances at the next rollover.
    pub appearances: u32,
}

/// Every club's season, one reading per rollover.
#[derive(Debug, Clone, Default)]
pub struct SquadLog {
    /// `years[s - 1]` is season `s`.
    pub years: Vec<Vec<SquadYear>>,
    /// Completed AI permanent transfers, one list per season.
    pub deals: Vec<Vec<ObservedDeal>>,
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
        self.deals.push(
            ofm_core::transfers::take_deal_log()
                .into_iter()
                .map(|deal| ObservedDeal {
                    appearances: game
                        .players
                        .iter()
                        .find(|player| player.id == deal.player_id)
                        .map_or(0, |player| player.stats.appearances),
                    buyer_id: deal.buyer_id,
                    position_group: deal.position_group,
                    role: deal.role,
                    starts_at: deal.starts_at,
                    points: deal.points,
                    gain: deal.gain,
                    fee: deal.fee,
                    age: deal.age,
                    weekly_wage: deal.weekly_wage,
                    buyer_group_depth: deal.buyer_group_depth,
                    buyer_planned_depth: deal.buyer_planned_depth,
                    buyer_formation: deal.buyer_formation,
                    buyer_levels: deal.buyer_levels,
                    natural_position: deal.natural_position,
                    alternate_positions: deal.alternate_positions,
                    potential: deal.potential,
                    date: deal.date,
                })
                .collect(),
        );
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
    for &season in wanted {
        if let Some(deals) = season.checked_sub(1).and_then(|i| log.deals.get(i)) {
            let clubs = log
                .years
                .get(season - 1)
                .map_or(1, |year| year.len().max(1)) as f64;
            out.push_str(&deal_report(deals, clubs, season));
        }
    }
    if let Some(last) = wanted
        .iter()
        .rev()
        .find_map(|&season| season.checked_sub(1).and_then(|i| log.years.get(i)))
    {
        let n = last.len().max(1) as f64;
        out.push_str("\njoiners by arrival, last wanted season:\nkind | joined/club | unused %\n");
        for (kind, label) in JOINER_KIND_LABELS.iter().enumerate() {
            let joined: usize = last.iter().map(|c| c.joined_by_kind[kind]).sum();
            let unused: usize = last.iter().map(|c| c.unused_by_kind[kind]).sum();
            out.push_str(&format!(
                "{label} | {:.2} | {:.0}%\n",
                joined as f64 / n,
                100.0 * unused as f64 / joined.max(1) as f64,
            ));
        }
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

/// The smallest model gain worth a transfer at all (`squad_intelligence::impact::MIN_GAIN`, which
/// is crate-private; this measurement copy is the same threshold).
const MIN_GAIN: f32 = 0.12;

/// A group under the floor plus planning margin: the market waives the price test for such a
/// buyer, but still asks whether the signing adds anything (`transfers::realism::group_is_thin`).
/// Measurement copy of that threshold.
const THIN_BELOW: [usize; 4] = [3, 5, 5, 3];

/// How a deal sat against the two gates the market applies before appraising a signing: the thin
/// group, which waives the price test, and the model's own minimum gain. A signing the club's model
/// valued below [`MIN_GAIN`] was one the appraisal rule existed to refuse (a club under the legal
/// squad floor is the one exception, and buys a body regardless).
fn gate_report(deals: &[ObservedDeal]) -> String {
    let mut out = String::new();
    let unused = |group: &[&ObservedDeal]| {
        100.0 * group.iter().filter(|deal| deal.appearances <= 2).count() as f64
            / group.len().max(1) as f64
    };
    out.push_str(
        "value gate: group | n | thin group at deal | model gain < MIN_GAIN | unused % of those\n",
    );
    for (group, label) in ["GK", "DEF", "MID", "FWD"].iter().enumerate() {
        let in_group: Vec<&ObservedDeal> = deals
            .iter()
            .filter(|deal| deal.position_group == group)
            .collect();
        if in_group.is_empty() {
            continue;
        }
        let thin: Vec<&ObservedDeal> = in_group
            .iter()
            .copied()
            .filter(|deal| deal.buyer_planned_depth < THIN_BELOW[group])
            .collect();
        let worthless: Vec<&ObservedDeal> = in_group
            .iter()
            .copied()
            .filter(|deal| deal.gain < MIN_GAIN)
            .collect();
        let worthless_thin = worthless
            .iter()
            .filter(|deal| deal.buyer_planned_depth < THIN_BELOW[group])
            .count();
        out.push_str(&format!(
            "{label} | {} | {} | {} ({worthless_thin} thin) | {:.0}%\n",
            in_group.len(),
            thin.len(),
            worthless.len(),
            unused(&worthless),
        ));
    }
    let worthless: Vec<&ObservedDeal> = deals.iter().filter(|deal| deal.gain < MIN_GAIN).collect();
    out.push_str(&format!(
        "total: {} of {} deals ({:.0}%) below MIN_GAIN, {:.0}% unused, mean {:.1} apps, {:.0}k fee\n",
        worthless.len(),
        deals.len(),
        100.0 * worthless.len() as f64 / deals.len().max(1) as f64,
        unused(&worthless),
        mean(worthless.iter().map(|deal| f64::from(deal.appearances))),
        mean(worthless.iter().map(|deal| deal.fee as f64)) / 1000.0,
    ));
    out.push_str("GK deals (buyer | formation | date | fee k | levels | natural/alt | role | gain | apps):\n");
    for deal in deals
        .iter()
        .filter(|deal| deal.position_group == 0)
        .take(10)
    {
        out.push_str(&format!(
            "  {} | {} | {} | {} | {:?} | {}/{:?} | {} | {:.2} | {}\n",
            deal.buyer_id,
            deal.buyer_formation,
            deal.date,
            deal.fee / 1000,
            deal.buyer_levels,
            deal.natural_position,
            deal.alternate_positions,
            deal.role,
            deal.gain,
            deal.appearances,
        ));
    }
    out
}

/// Permanent AI transfers of one season, split by what the buyer's squad model said the signing
/// was for, and by what the player actually did. A "Starter" who never played and a "Succession"
/// who was always going to be cover are different things; the print separates them.
fn deal_report(deals: &[ObservedDeal], clubs: f64, season: usize) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "\nseason {season}: permanent AI transfers by buyer's intended role: {} deals ({:.2}/club)\nrole | n | starts now | group at or above need floor | unused % | mean points | mean fee (k) | mean apps | mean age | mean depth now/planned\n",
        deals.len(),
        deals.len() as f64 / clubs
    ));
    for role in ["Starter", "Depth", "Succession", "None"] {
        let group: Vec<&ObservedDeal> = deals.iter().filter(|deal| deal.role == role).collect();
        if group.is_empty() {
            continue;
        }
        let n = group.len() as f64;
        out.push_str(&format!(
            "{} | {} | {} | {} | {:.0}% | {:.1} | {:.0} | {:.1} | {:.1} | {:.1}/{:.1}\n",
            role,
            group.len(),
            group.iter().filter(|deal| deal.starts_at.is_some()).count(),
            group
                .iter()
                .filter(|deal| deal.buyer_group_depth >= GROUP_NEED[deal.position_group])
                .count(),
            100.0 * group.iter().filter(|deal| deal.appearances <= 2).count() as f64 / n,
            mean(group.iter().map(|deal| f64::from(deal.points))),
            mean(group.iter().map(|deal| deal.fee as f64)) / 1000.0,
            mean(group.iter().map(|deal| f64::from(deal.appearances))),
            mean(group.iter().map(|deal| f64::from(deal.age))),
            mean(group.iter().map(|deal| deal.buyer_group_depth as f64)),
            mean(group.iter().map(|deal| deal.buyer_planned_depth as f64)),
        ));
    }
    let opening: Vec<&ObservedDeal> = deals
        .iter()
        .filter(|deal| !is_mid_season(&deal.date))
        .collect();
    let mid: Vec<&ObservedDeal> = deals
        .iter()
        .filter(|deal| is_mid_season(&deal.date))
        .collect();
    let unused = |group: &[&ObservedDeal]| {
        100.0 * group.iter().filter(|deal| deal.appearances <= 2).count() as f64
            / group.len().max(1) as f64
    };
    out.push_str(&gate_report(deals));
    let full: Vec<&ObservedDeal> = deals
        .iter()
        .filter(|deal| deal.buyer_group_depth >= PLANNED_DEPTH[deal.position_group])
        .collect();
    let by_group = |group: usize| {
        deals
            .iter()
            .filter(|deal| deal.position_group == group)
            .count()
    };
    out.push_str(&format!(
        "context: groups GK {} / DEF {} / MID {} / FWD {} | proof: {} deals ({:.0}%) into a group already at DESIRED_DEPTH, {:.0}% unused\n",
        by_group(0),
        by_group(1),
        by_group(2),
        by_group(3),
        full.len(),
        100.0 * full.len() as f64 / deals.len().max(1) as f64,
        unused(&full),
    ));
    out.push_str(&format!(
        "unused by window: opening {} deals {:.0}% | mid-season {} deals {:.0}%\n",
        opening.len(),
        unused(&opening),
        mid.len(),
        unused(&mid),
    ));
    let mut by_buyer_group: HashMap<(&str, usize), usize> = HashMap::new();
    for deal in deals {
        *by_buyer_group
            .entry((deal.buyer_id.as_str(), deal.position_group))
            .or_default() += 1;
    }
    let repeated_clubs = by_buyer_group.values().filter(|count| **count >= 2).count();
    let repeated_deals: usize = by_buyer_group.values().filter(|count| **count >= 2).sum();
    out.push_str(&format!(
        "clubs with 2+ permanent transfers in one position group: {} ({:.1}%), {} deals\n",
        repeated_clubs,
        100.0 * repeated_clubs as f64 / clubs,
        repeated_deals,
    ));
    out
}

/// Whether a deal completed in the mid-season window. Dates are `YYYY-MM-DD`, which orders as it
/// reads; the opening window runs from the summer into early autumn, the second one in January.
fn is_mid_season(date: &str) -> bool {
    date.get(5..7)
        .and_then(|month| month.parse::<u32>().ok())
        .is_some_and(|month| month <= 2)
}

/// A one-line summary of the whole run, for comparing two trees at a glance: the mean over every
/// season of the numbers that matter most.
pub fn summary(log: &SquadLog) -> String {
    let all: Vec<&SquadYear> = log.years.iter().flatten().collect();
    let n = all.len().max(1) as f64;
    let joined: usize = all.iter().map(|c| c.joined).sum();
    let unused: usize = all.iter().map(|c| c.joined_unused).sum();
    let mut kinds = String::new();
    for (kind, label) in JOINER_KIND_LABELS.iter().enumerate() {
        let kind_joined: usize = all.iter().map(|c| c.joined_by_kind[kind]).sum();
        let kind_unused: usize = all.iter().map(|c| c.unused_by_kind[kind]).sum();
        kinds.push_str(&format!(
            " | {label} {:.2}/club {:.0}% unused",
            kind_joined as f64 / n,
            100.0 * kind_unused as f64 / kind_joined.max(1) as f64,
        ));
    }
    format!(
        "SUMMARY club-seasons {} | XI fit {:.2} | weak-slot clubs {:.1}% | out-of-group {:.3} | keeper hole {:.1}% | usable bench {:.2} | thin groups {:.3} | unused joiners {:.1}% | fees/club-season {:.2} M{}",
        all.len(),
        mean(all.iter().map(|c| c.xi_fit)),
        100.0 * all.iter().filter(|c| c.weak_slots > 0).count() as f64 / n,
        mean(all.iter().map(|c| c.out_of_group as f64)),
        100.0 * all.iter().filter(|c| c.keeper_hole).count() as f64 / n,
        mean(all.iter().map(|c| c.usable_bench as f64)),
        mean(all.iter().map(|c| c.thin_groups as f64)),
        100.0 * unused as f64 / joined.max(1) as f64,
        mean(all.iter().map(|c| c.fees_paid as f64)) / 1e6,
        kinds,
    )
}
