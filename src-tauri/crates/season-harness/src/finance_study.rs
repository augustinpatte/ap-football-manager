//! Run F5.3: where does a club's money go, by standing, over many seasons?
//!
//! Measurement only. A [`FinanceLog`] takes one [`ClubYear`] per club when the run opens and one
//! just before each rollover, read from the cash journal (the single source of truth for what a
//! club earned and spent), so no figure here is a model of the game's own books. The study that
//! prints them is `tests/f53_finance_study.rs`. Nothing is stored in a save.
//!
//! Clubs are grouped by the quartile of their *founding* reputation (so a club that rises keeps its
//! group) and by country.

use std::collections::{BTreeMap, HashMap};

use chrono::Duration;
use domain::finance::CashKind;
use ofm_core::finances::wage_bills_by_club;
use ofm_core::game::Game;

use crate::driver::SeasonObserver;
use crate::world_health::quantiles;

/// One club, one season: the books as the journal records them over the 365 days to the rollover.
#[derive(Debug, Clone, Default)]
pub struct ClubYear {
    pub club_id: String,
    pub country: String,
    /// 0 (lowest founding reputation quartile) to 3.
    pub tier: usize,
    pub reputation: u32,
    pub closing_cash: i64,
    pub opening_cash: i64,
    pub wage_budget: i64,
    /// Weekly wage commitment now (players + staff, loan split included).
    pub weekly_commitment: i64,
    /// Seniors on the books and the mean weekly wage of a player (staff excluded).
    pub squad_size: usize,
    pub mean_player_wage: i64,
    pub mean_top_ovr: f64,
    pub flows: BTreeMap<String, i64>,
    /// Change of the weekly player wage bill since the previous reading, split by what moved it:
    /// players who stayed at the club (renewals: the raise), players who joined (their whole wage)
    /// and players who left (their whole wage, as a negative).
    pub wage_stayed_delta: i64,
    pub wage_joined: i64,
    pub wage_left: i64,
    pub joined: usize,
    pub left: usize,
    /// Stayers whose wage changed (a renewal), and the wage they were on before.
    pub renewed: usize,
    pub renewed_before: i64,
    /// Joiners by the kind of their last contract (or `Youth`), as (count, summed weekly wage).
    pub joined_by: BTreeMap<String, (usize, i64)>,
}

impl ClubYear {
    fn flow(&self, kinds: &[CashKind]) -> i64 {
        kinds
            .iter()
            .map(|kind| self.flows.get(&format!("{kind:?}")).copied().unwrap_or(0))
            .sum()
    }

    pub fn income(&self) -> i64 {
        self.flow(&[
            CashKind::Matchday,
            CashKind::Sponsorship,
            CashKind::PrizeMoney,
            CashKind::CommercialCampaign,
        ])
    }

    /// Everything the books count toward a season's result: income less wages, upkeep and the rest.
    pub fn operating_result(&self) -> i64 {
        self.flows
            .iter()
            .filter(|(kind, _)| {
                !matches!(
                    kind.as_str(),
                    "OpeningBalance" | "TransferFeeIn" | "TransferFeeOut" | "LoanFee"
                )
            })
            .map(|(_, amount)| *amount)
            .sum()
    }

    pub fn transfer_result(&self) -> i64 {
        self.flow(&[
            CashKind::TransferFeeIn,
            CashKind::TransferFeeOut,
            CashKind::LoanFee,
        ])
    }

    pub fn wages_paid(&self) -> i64 {
        -self.flow(&[CashKind::PlayerWages, CashKind::StaffWages])
    }
}

/// Wage-adding events of one season, by the kind of contract, split by whether the club's bill was
/// over its wage budget once the event had happened. (count, summed weekly wage added)
#[derive(Debug, Clone, Default)]
pub struct GrowthLedger {
    pub over_budget: BTreeMap<String, (usize, i64)>,
    pub within_budget: BTreeMap<String, (usize, i64)>,
}

#[derive(Debug, Clone, Default)]
pub struct FinanceLog {
    /// One ledger per season, filled day by day.
    pub growth: Vec<GrowthLedger>,
    day_before: HashMap<String, (String, i64)>,
    day_bills: HashMap<String, i64>,
    founding_tier: HashMap<String, usize>,
    /// Player id -> (club, weekly wage) at the previous reading.
    previous_wages: HashMap<String, (String, i64)>,
    /// `years[s]` is season `s`'s books; index 0 is the opening position (no flows).
    pub years: Vec<Vec<ClubYear>>,
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

fn read_year(
    game: &Game,
    tiers: &HashMap<String, usize>,
    previous: &HashMap<String, (String, i64)>,
    with_flows: bool,
) -> Vec<ClubYear> {
    let since = (game.clock.current_date.date_naive() - Duration::days(365))
        .format("%Y-%m-%d")
        .to_string();
    let mut flows: HashMap<&str, BTreeMap<String, i64>> = HashMap::new();
    if with_flows {
        for post in game.cash_journal.iter() {
            if post.date.as_str() > since.as_str() && post.kind != CashKind::OpeningBalance {
                *flows
                    .entry(post.club_id.as_str())
                    .or_default()
                    .entry(format!("{:?}", post.kind))
                    .or_default() += post.amount;
            }
        }
    }
    let bills = wage_bills_by_club(game);
    let mut squads: HashMap<&str, Vec<&domain::player::Player>> = HashMap::new();
    for player in game.players.iter().filter(|p| !p.retired) {
        if let Some(club) = player.team_id.as_deref() {
            squads.entry(club).or_default().push(player);
        }
    }
    game.teams
        .iter()
        .map(|team| {
            let flows = flows.remove(team.id.as_str()).unwrap_or_default();
            let net: i64 = flows.values().sum();
            let squad = squads.get(team.id.as_str()).map_or(&[][..], Vec::as_slice);
            let mut ovrs: Vec<f64> = squad.iter().map(|p| f64::from(p.ovr)).collect();
            ovrs.sort_by(|a, b| b.total_cmp(a));
            ovrs.truncate(14);
            let mut wage_stayed_delta = 0;
            let (mut renewed, mut renewed_before) = (0usize, 0i64);
            let mut joined_by: BTreeMap<String, (usize, i64)> = BTreeMap::new();
            let (mut wage_joined, mut joined) = (0, 0);
            for p in squad.iter() {
                let wage = i64::from(p.wage());
                match previous.get(&p.id) {
                    Some((club, before)) if club == &team.id => {
                        wage_stayed_delta += wage - before;
                        if wage != *before {
                            renewed += 1;
                            renewed_before += before;
                        }
                    }
                    _ => {
                        wage_joined += wage;
                        joined += 1;
                        let kind = if p.squad_role == domain::player::SquadRole::Youth {
                            "Youth".to_string()
                        } else {
                            p.movement_history
                                .iter()
                                .rev()
                                .find(|e| e.contract.is_some())
                                .map_or("None".to_string(), |e| format!("{:?}", e.kind))
                        };
                        let entry = joined_by.entry(kind).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += wage;
                    }
                }
            }
            let (mut wage_left, mut left) = (0, 0);
            for (id, (club, before)) in previous.iter() {
                if club == &team.id && !squad.iter().any(|p| &p.id == id) {
                    wage_left -= before;
                    left += 1;
                }
            }
            ClubYear {
                wage_stayed_delta,
                renewed,
                renewed_before,
                joined_by,
                wage_joined,
                wage_left,
                joined,
                left,
                squad_size: squad.len(),
                mean_player_wage: if squad.is_empty() {
                    0
                } else {
                    squad.iter().map(|p| i64::from(p.wage())).sum::<i64>() / squad.len() as i64
                },
                mean_top_ovr: ovrs.iter().sum::<f64>() / ovrs.len().max(1) as f64,
                club_id: team.id.clone(),
                country: team.country.clone(),
                tier: tiers.get(&team.id).copied().unwrap_or(0),
                reputation: team.reputation,
                closing_cash: team.finance,
                opening_cash: team.finance - net,
                wage_budget: team.wage_budget,
                weekly_commitment: bills.get(&team.id).copied().unwrap_or(0),
                flows,
            }
        })
        .collect()
}

fn wage_snapshot(game: &Game) -> HashMap<String, (String, i64)> {
    game.players
        .iter()
        .filter(|p| !p.retired)
        .filter_map(|p| Some((p.id.clone(), (p.team_id.clone()?, i64::from(p.wage())))))
        .collect()
}

impl FinanceLog {
    pub fn starting_from(game: &Game) -> Self {
        let tiers = founding_tiers(game);
        let previous_wages = wage_snapshot(game);
        let opening = read_year(game, &tiers, &previous_wages, false);
        Self {
            founding_tier: tiers,
            day_before: previous_wages.clone(),
            day_bills: wage_bills_by_club(game),
            growth: vec![GrowthLedger::default()],
            previous_wages,
            years: vec![opening],
        }
    }

    /// Every club except the player's (whose books the harness's idle manager does not run).
    fn ai_clubs<'a>(&'a self, game_user: Option<&'a str>, season: usize) -> Vec<&'a ClubYear> {
        self.years[season]
            .iter()
            .filter(|club| Some(club.club_id.as_str()) != game_user)
            .collect()
    }
}

impl SeasonObserver for FinanceLog {
    fn on_day_end(&mut self, game: &Game, _season: u32, _day: u32) {
        let now = wage_snapshot(game);
        let bills = wage_bills_by_club(game);
        let budget: HashMap<&str, i64> = game
            .teams
            .iter()
            .map(|t| (t.id.as_str(), t.wage_budget))
            .collect();
        let user = game.manager.team_id.as_deref();
        let ledger = self.growth.last_mut().expect("opened with one");
        for p in game.players.iter().filter(|p| !p.retired) {
            let Some((club, wage)) = now.get(&p.id) else {
                continue;
            };
            if Some(club.as_str()) == user {
                continue;
            }
            let added = match self.day_before.get(&p.id) {
                Some((c, w)) if c == club => wage - w,
                _ => *wage,
            };
            if added <= 0 {
                continue;
            }
            let kind = if p.squad_role == domain::player::SquadRole::Youth {
                "Youth".to_string()
            } else {
                p.movement_history
                    .iter()
                    .rev()
                    .find(|e| e.contract.is_some())
                    .map_or("None".to_string(), |e| format!("{:?}", e.kind))
            };
            let over = bills.get(club).copied().unwrap_or(0)
                > budget.get(club.as_str()).copied().unwrap_or(0);
            let map = if over {
                &mut ledger.over_budget
            } else {
                &mut ledger.within_budget
            };
            let entry = map.entry(kind).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += added;
        }
        self.day_before = now;
        self.day_bills = bills;
    }

    fn on_before_rollover(&mut self, game: &Game, _season: u32) {
        self.years.push(read_year(
            game,
            &self.founding_tier,
            &self.previous_wages,
            true,
        ));
        self.previous_wages = wage_snapshot(game);
        self.growth.push(GrowthLedger::default());
    }
}

fn mean(values: impl Iterator<Item = i64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0), |(s, n), v| (s + v as f64, n + 1));
    if n == 0 { 0.0 } else { sum / f64::from(n) }
}

/// A printable report for the seasons asked for: one block per season, one line per tier and a
/// total, then one line per country at the last season.
pub fn report(log: &FinanceLog, user_team: Option<&str>, seasons: &[usize]) -> String {
    let mut out = String::new();
    for &s in seasons.iter().filter(|s| **s < log.years.len()) {
        let clubs = log.ai_clubs(user_team, s);
        let squad_line = |tier: Option<usize>| {
            let group: Vec<&&ClubYear> = clubs
                .iter()
                .filter(|club| tier.is_none_or(|t| club.tier == t))
                .collect();
            format!(
                "squad {:.1} meanWage/wk {:.0} top14 ovr {:.1} playerWages {:.1}M staffWages {:.2}M | weekly bill moved: renewals {:.1} at {:+.1}% stayers {:+.0} joiners {:+.0} ({:.1}) leavers {:+.0} ({:.1})",
                mean(group.iter().map(|c| c.squad_size as i64)),
                mean(group.iter().map(|c| c.mean_player_wage)),
                group.iter().map(|c| c.mean_top_ovr).sum::<f64>() / group.len().max(1) as f64,
                -mean(group.iter().map(|c| c.flow(&[CashKind::PlayerWages]))) / 1e6,
                -mean(group.iter().map(|c| c.flow(&[CashKind::StaffWages]))) / 1e6,
                mean(group.iter().map(|c| c.renewed as i64)),
                100.0 * group.iter().map(|c| c.wage_stayed_delta).sum::<i64>() as f64
                    / group.iter().map(|c| c.renewed_before).sum::<i64>().max(1) as f64,
                mean(group.iter().map(|c| c.wage_stayed_delta)),
                mean(group.iter().map(|c| c.wage_joined)),
                mean(group.iter().map(|c| c.joined as i64)),
                mean(group.iter().map(|c| c.wage_left)),
                mean(group.iter().map(|c| c.left as i64)),
            )
        };
        out.push_str(&format!(
            "-- season {s}: tier | n | debt | deep | cashP10/P50/P90 M | income | wages | upkeep | other | op.result | transfers | wage/budget P50 | overBudget(>1.1) | commit*52/income P50 | repMean\n"
        ));
        for tier in (0..4).map(Some).chain(std::iter::once(None)) {
            let group: Vec<&&ClubYear> = clubs
                .iter()
                .filter(|club| tier.is_none_or(|t| club.tier == t))
                .collect();
            if group.is_empty() {
                continue;
            }
            let cash: Vec<f64> = group.iter().map(|c| c.closing_cash as f64 / 1e6).collect();
            let q = quantiles(&cash);
            let ratio: Vec<f64> = group
                .iter()
                .filter(|c| c.wage_budget > 0)
                .map(|c| c.weekly_commitment as f64 / c.wage_budget as f64)
                .collect();
            let cover: Vec<f64> = group
                .iter()
                .filter(|c| c.income() > 0)
                .map(|c| c.weekly_commitment as f64 * 52.0 / c.income() as f64)
                .collect();
            let upkeep = |c: &&&ClubYear| -c.flow(&[CashKind::Upkeep]);
            let other =
                |c: &&&ClubYear| -(c.operating_result() - c.income() + c.wages_paid() + upkeep(c));
            out.push_str(&format!(
                "   {:>5} | {:>3} | {:>3} | {:>3} | {:>6.1}/{:>6.1}/{:>6.1} | {:>6.1} | {:>6.1} | {:>5.1} | {:>5.1} | {:>6.1} | {:>6.1} | {:>4.2} | {:>3} | {:>4.2} | {:>4.0}\n",
                tier.map_or("ALL".to_string(), |t| format!("Q{}", t + 1)),
                group.len(),
                group.iter().filter(|c| c.closing_cash < 0).count(),
                group.iter().filter(|c| c.closing_cash < -50_000_000).count(),
                q.p10,
                q.p50,
                q.p90,
                mean(group.iter().map(|c| c.income())) / 1e6,
                mean(group.iter().map(|c| c.wages_paid())) / 1e6,
                mean(group.iter().map(upkeep)) / 1e6,
                mean(group.iter().map(other)) / 1e6,
                mean(group.iter().map(|c| c.operating_result())) / 1e6,
                mean(group.iter().map(|c| c.transfer_result())) / 1e6,
                quantiles(&ratio).p50,
                group
                    .iter()
                    .filter(|c| c.wage_budget > 0
                        && c.weekly_commitment as f64 > c.wage_budget as f64 * 1.1)
                    .count(),
                quantiles(&cover).p50,
                mean(group.iter().map(|c| i64::from(c.reputation))),
            ));
            out.push_str(&format!("          {}\n", squad_line(tier)));
            let mut kinds: BTreeMap<&str, (usize, i64)> = BTreeMap::new();
            for c in group.iter() {
                for (k, (n, w)) in &c.joined_by {
                    let e = kinds.entry(k.as_str()).or_insert((0, 0));
                    e.0 += n;
                    e.1 += w;
                }
            }
            let clubs_n = group.len().max(1) as f64;
            let line: Vec<String> = kinds
                .iter()
                .map(|(k, (n, w))| {
                    format!(
                        "{k} {:.2}/club @{}",
                        *n as f64 / clubs_n,
                        w / (*n as i64).max(1)
                    )
                })
                .collect();
            out.push_str(&format!("          joiners: {}\n", line.join(" | ")));
        }
    }
    for &s in seasons
        .iter()
        .filter(|s| **s >= 1 && **s <= log.growth.len())
    {
        // growth[s-1] holds the days of season s
        let g = &log.growth[s - 1];
        let fmt = |m: &BTreeMap<String, (usize, i64)>| {
            m.iter()
                .map(|(k, (n, w))| format!("{k} {n} (+{}/wk)", w))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        out.push_str(&format!(
            "-- wage-adding events, season {s}\n     over budget after the event: {}\n     within budget:               {}\n",
            fmt(&g.over_budget),
            fmt(&g.within_budget)
        ));
    }
    if let Some(last) = log.years.len().checked_sub(1) {
        let mut by_country: BTreeMap<&str, Vec<&ClubYear>> = BTreeMap::new();
        for club in log.ai_clubs(user_team, last) {
            by_country
                .entry(club.country.as_str())
                .or_default()
                .push(club);
        }
        out.push_str(&format!("-- by country at season {last}: country | n | debt | cashP50 M | op.result mean M | overBudget\n"));
        for (country, group) in by_country {
            let cash: Vec<f64> = group.iter().map(|c| c.closing_cash as f64 / 1e6).collect();
            out.push_str(&format!(
                "   {:<14} | {:>3} | {:>3} | {:>6.1} | {:>6.1} | {:>3}\n",
                country,
                group.len(),
                group.iter().filter(|c| c.closing_cash < 0).count(),
                quantiles(&cash).p50,
                mean(group.iter().map(|c| c.operating_result())) / 1e6,
                group
                    .iter()
                    .filter(|c| c.wage_budget > 0
                        && c.weekly_commitment as f64 > c.wage_budget as f64 * 1.1)
                    .count(),
            ));
        }
    }
    out
}
