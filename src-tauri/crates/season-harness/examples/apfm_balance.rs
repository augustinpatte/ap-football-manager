//! Mission 4, phase 5: how the match engine behaves on FM24 squads.
//!
//! `apfm_balance <out.json> [matches-per-cell]`
//!
//! Plays single matches through the production path (`play_unwatched_fixture`: the kick-off gate,
//! the lineup the game would field, the engine) without applying the result, so the world is the
//! same for every match and only the seed (`game.seed`, which the day's dice derive from) changes.
//! Reports goals, shots, possession and results for: a big club against small ones, clubs of equal
//! strength at home and away, different lineups and tactics for the managed club, and the effect
//! of raising one group of attributes on the home side.

use std::collections::HashMap;

use domain::league::{Fixture, FixtureCompetition, FixtureStatus, League};
use domain::player::Position;
use domain::team::PlayStyle;
use ofm_core::game::Game;
use ofm_core::live_match_manager::play_unwatched_fixture;
use season_harness::fm24::{build_fm24_career, world_path_from_env};
use serde_json::{Value, json};

#[derive(Default, Clone)]
struct Agg {
    n: u32,
    hg: u32,
    ag: u32,
    h_shots: u32,
    a_shots: u32,
    h_sot: u32,
    a_sot: u32,
    poss: f64,
    hw: u32,
    d: u32,
    aw: u32,
    cards: u32,
}

impl Agg {
    fn json(&self) -> Value {
        let n = self.n.max(1) as f64;
        json!({
            "matches": self.n,
            "home_goals": self.hg as f64 / n, "away_goals": self.ag as f64 / n,
            "goals": (self.hg + self.ag) as f64 / n,
            "shots_home": self.h_shots as f64 / n, "shots_away": self.a_shots as f64 / n,
            "sot_home": self.h_sot as f64 / n, "sot_away": self.a_sot as f64 / n,
            "home_possession": self.poss / n,
            "home_win_pct": self.hw as f64 / n * 100.0, "draw_pct": self.d as f64 / n * 100.0, "away_win_pct": self.aw as f64 / n * 100.0,
            "home_points_per_match": (3 * self.hw + self.d) as f64 / n,
            "cards_per_match": self.cards as f64 / n,
        })
    }
}

fn setup(game: &mut Game) -> League {
    let mut league = game
        .competitions
        .iter()
        .find(|c| c.id == "fm-k-12")
        .cloned()
        .expect("Ligue 1");
    league.fixtures.clear();
    league
}

fn play_n(game: &mut Game, base: &League, home: &str, away: &str, n: u32, seed0: u64) -> Agg {
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let mut agg = Agg::default();
    for i in 0..n {
        game.seed = seed0 + u64::from(i) * 7919;
        let mut league = base.clone();
        league.fixtures = vec![Fixture {
            id: format!("bal-{i}"),
            competition_id: league.id.clone(),
            matchday: 1,
            date: today.clone(),
            home_team_id: home.to_string(),
            away_team_id: away.to_string(),
            competition: FixtureCompetition::Friendly,
            status: FixtureStatus::Scheduled,
            result: None,
            postponement: None,
        }];
        game.league = Some(league);
        let Ok(played) = play_unwatched_fixture(game, 0) else {
            continue;
        };
        let r = &played.report;
        agg.n += 1;
        agg.hg += r.home_goals as u32;
        agg.ag += r.away_goals as u32;
        agg.h_shots += r.home_stats.shots as u32;
        agg.a_shots += r.away_stats.shots as u32;
        agg.h_sot += r.home_stats.shots_on_target as u32;
        agg.a_sot += r.away_stats.shots_on_target as u32;
        agg.poss += r.home_possession;
        agg.cards += (r.home_stats.yellow_cards
            + r.away_stats.yellow_cards
            + r.home_stats.red_cards
            + r.away_stats.red_cards) as u32;
        match r.home_goals.cmp(&r.away_goals) {
            std::cmp::Ordering::Greater => agg.hw += 1,
            std::cmp::Ordering::Equal => agg.d += 1,
            std::cmp::Ordering::Less => agg.aw += 1,
        }
    }
    game.league = None;
    agg
}

fn strength(game: &Game) -> HashMap<String, f64> {
    let mut by: HashMap<&str, Vec<u8>> = HashMap::new();
    for p in &game.players {
        if let Some(t) = p.team_id.as_deref() {
            by.entry(t).or_default().push(p.ovr);
        }
    }
    by.into_iter()
        .map(|(t, mut v)| {
            v.sort_unstable_by(|a, b| b.cmp(a));
            (
                t.to_string(),
                v.iter().take(14).map(|x| *x as f64).sum::<f64>() / v.len().clamp(1, 14) as f64,
            )
        })
        .collect()
}

fn name(game: &Game, id: &str) -> String {
    game.teams
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.name.clone())
        .unwrap_or(id.into())
}

fn xi_433(game: &Game, team: &str, worst: bool) -> Vec<String> {
    let slots: [&[Position]; 11] = [
        &[Position::Goalkeeper],
        &[Position::LeftBack],
        &[Position::CenterBack],
        &[Position::CenterBack],
        &[Position::RightBack],
        &[Position::DefensiveMidfielder],
        &[Position::CentralMidfielder],
        &[Position::AttackingMidfielder, Position::CentralMidfielder],
        &[Position::LeftWinger],
        &[Position::Striker],
        &[Position::RightWinger],
    ];
    let mut pool: Vec<&domain::player::Player> = game
        .players
        .iter()
        .filter(|p| {
            p.team_id.as_deref() == Some(team)
                && p.injury.is_none()
                && p.squad_role != domain::player::SquadRole::Youth
        })
        .collect();
    pool.sort_by_key(|p| if worst { p.ovr as i32 } else { -(p.ovr as i32) });
    let mut used = Vec::new();
    let mut out = Vec::new();
    for accepted in slots {
        let pick = pool
            .iter()
            .find(|p| !used.contains(&p.id) && accepted.contains(&p.position))
            .or_else(|| {
                pool.iter()
                    .find(|p| !used.contains(&p.id) && p.position != Position::Goalkeeper)
            })
            .map(|p| p.id.clone());
        if let Some(id) = pick {
            used.push(id.clone());
            out.push(id);
        }
    }
    out
}

fn main() {
    let out = std::env::args().nth(1).expect("output path");
    let n: u32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let path = world_path_from_env().expect("APFM_WORLD_JSON");
    let psg = "fm-c-498";
    let mut game = build_fm24_career(&path, psg, 7).expect("career");
    let base = setup(&mut game);
    let st = strength(&game);
    let s = |id: &str| (st[id] * 10.0).round() / 10.0;
    let mut report = json!({"matches_per_cell": n});

    // 1. big club against small ones, both venues
    let opponents = [
        ("fm-c-488", "Ligue 1, dernier (Le Mans)"),
        ("fm-c-501", "Ligue 1, milieu de tableau (Lens)"),
        ("fm-c-33725", "Ligue 2 (Annecy)"),
        ("fm-c-39927", "National / Ligue 3 (Thionville)"),
    ];
    let mut big = Vec::new();
    for (opp, label) in opponents {
        let home = play_n(&mut game, &base, psg, opp, n, 1000);
        let away = play_n(&mut game, &base, opp, psg, n, 5000);
        big.push(json!({"opponent": label, "strength_gap": s(psg) - s(opp), "psg_home": home.json(), "psg_away": away.json()}));
    }
    report["big_vs_small"] = json!(big);

    // 2. equal strength: the closest pairs in Ligue 1 and in the Premier League
    let mut pairs = Vec::new();
    for comp in ["fm-k-12", "fm-k-8", "fm-k-17"] {
        let c = game.competitions.iter().find(|c| c.id == comp).unwrap();
        let mut ids: Vec<&String> = c.participant_ids.iter().collect();
        ids.sort_by(|a, b| st[*a].partial_cmp(&st[*b]).unwrap());
        let mid = &ids[ids.len() / 3..ids.len() * 2 / 3];
        let mut best = (f64::MAX, "", "");
        for i in 0..mid.len() {
            for j in i + 1..mid.len() {
                let d = (st[mid[i]] - st[mid[j]]).abs();
                if d < best.0 {
                    best = (d, mid[i], mid[j]);
                }
            }
        }
        pairs.push((best.1.to_string(), best.2.to_string(), best.0));
    }
    let mut equal = Vec::new();
    let mut tot_home = Agg::default();
    for (a, b, gap) in &pairs {
        let ab = play_n(&mut game, &base, a, b, n, 9000);
        let ba = play_n(&mut game, &base, b, a, n, 12000);
        for x in [&ab, &ba] {
            tot_home.n += x.n;
            tot_home.hg += x.hg;
            tot_home.ag += x.ag;
            tot_home.hw += x.hw;
            tot_home.d += x.d;
            tot_home.aw += x.aw;
            tot_home.h_shots += x.h_shots;
            tot_home.a_shots += x.a_shots;
            tot_home.poss += x.poss;
        }
        equal.push(json!({"a": name(&game, a), "b": name(&game, b), "strength_gap": gap, "a_home": ab.json(), "b_home": ba.json()}));
    }
    report["equal_strength"] = json!(equal);
    report["home_advantage_equal_strength_total"] = tot_home.json();

    // 3. managed club: lineups and tactics, against a mid-table opponent
    let opp = "fm-c-501";
    let mut lineups = Vec::new();
    let original_xi = game
        .teams
        .iter()
        .find(|t| t.id == psg)
        .unwrap()
        .starting_xi_ids
        .clone();
    let original_formation = game
        .teams
        .iter()
        .find(|t| t.id == psg)
        .unwrap()
        .formation
        .clone();
    let original_style = game
        .teams
        .iter()
        .find(|t| t.id == psg)
        .unwrap()
        .play_style
        .clone();
    for (label, worst) in [
        ("meilleur 11 (4-3-3)", false),
        ("11 le plus faible (4-3-3)", true),
    ] {
        let xi = xi_433(&game, psg, worst);
        let team = game.teams.iter_mut().find(|t| t.id == psg).unwrap();
        team.formation = "4-3-3".into();
        team.starting_xi_ids = xi.clone();
        let strength_xi: f64 = xi
            .iter()
            .filter_map(|id| game.players.iter().find(|p| &p.id == id))
            .map(|p| p.ovr as f64)
            .sum::<f64>()
            / xi.len().max(1) as f64;
        let r = play_n(&mut game, &base, psg, opp, n, 20000);
        lineups.push(json!({"lineup": label, "xi_mean_ovr": strength_xi, "players": xi.len(), "result": r.json()}));
    }
    report["lineups_psg_home_vs_lens"] = json!(lineups);

    let best = xi_433(&game, psg, false);
    let mut tactics = Vec::new();
    for style in [
        PlayStyle::Balanced,
        PlayStyle::Attacking,
        PlayStyle::Defensive,
        PlayStyle::Possession,
        PlayStyle::Counter,
        PlayStyle::HighPress,
    ] {
        let team = game.teams.iter_mut().find(|t| t.id == psg).unwrap();
        team.formation = "4-3-3".into();
        team.starting_xi_ids = best.clone();
        team.play_style = style.clone();
        let r = play_n(&mut game, &base, psg, opp, n, 30000);
        tactics.push(json!({"style": format!("{style:?}"), "result": r.json()}));
    }
    report["play_styles_psg_home_vs_lens"] = json!(tactics);
    let mut formations = Vec::new();
    for f in ["4-3-3", "4-4-2", "3-5-2", "5-3-2", "4-1-4-1"] {
        let team = game.teams.iter_mut().find(|t| t.id == psg).unwrap();
        team.formation = f.into();
        team.play_style = PlayStyle::Balanced;
        // the game fields its own XI for the formation when the stored one does not fit
        let r = play_n(&mut game, &base, psg, opp, n, 40000);
        formations.push(json!({"formation": f, "result": r.json()}));
    }
    report["formations_psg_home_vs_lens"] = json!(formations);
    {
        let team = game.teams.iter_mut().find(|t| t.id == psg).unwrap();
        team.starting_xi_ids = original_xi;
        team.formation = original_formation;
        team.play_style = original_style;
    }

    // 4. attribute influence: raise one group of attributes by 10 on every player of Lens, equal opponent Toulouse
    let (a, b, _) = pairs[0].clone();
    let baseline = play_n(&mut game, &base, &a, &b, n, 50000);
    let saved: Vec<(usize, domain::player::PlayerAttributes)> = game
        .players
        .iter()
        .enumerate()
        .filter(|(_, p)| p.team_id.as_deref() == Some(a.as_str()))
        .map(|(i, p)| (i, p.attributes.clone()))
        .collect();
    type Boost = fn(&mut domain::player::PlayerAttributes);
    let groups: [(&str, Boost); 6] = [
        ("tout +10", |at| {
            for v in [
                &mut at.pace,
                &mut at.stamina,
                &mut at.strength,
                &mut at.agility,
                &mut at.passing,
                &mut at.shooting,
                &mut at.tackling,
                &mut at.dribbling,
                &mut at.defending,
                &mut at.positioning,
                &mut at.vision,
                &mut at.decisions,
                &mut at.composure,
                &mut at.aggression,
                &mut at.teamwork,
                &mut at.leadership,
                &mut at.handling,
                &mut at.reflexes,
                &mut at.aerial,
            ] {
                *v = v.saturating_add(10).min(99);
            }
        }),
        ("tir +10", |at| {
            at.shooting = at.shooting.saturating_add(10).min(99)
        }),
        ("passes +10", |at| {
            at.passing = at.passing.saturating_add(10).min(99)
        }),
        ("défense +10 (tacle, défense)", |at| {
            at.tackling = at.tackling.saturating_add(10).min(99);
            at.defending = at.defending.saturating_add(10).min(99);
        }),
        ("vitesse +10", |at| {
            at.pace = at.pace.saturating_add(10).min(99)
        }),
        ("gardien +10 (réflexes, prise de balle)", |at| {
            at.reflexes = at.reflexes.saturating_add(10).min(99);
            at.handling = at.handling.saturating_add(10).min(99);
        }),
    ];
    let mut attrs = vec![json!({"change": "aucun", "result": baseline.json()})];
    for (label, boost) in groups {
        for (i, _) in &saved {
            boost(&mut game.players[*i].attributes);
        }
        let r = play_n(&mut game, &base, &a, &b, n, 50000);
        attrs.push(json!({"change": label, "result": r.json(), "goal_difference_per_match": (r.hg as f64 - r.ag as f64) / r.n.max(1) as f64}));
        for (i, at) in &saved {
            game.players[*i].attributes = at.clone();
        }
    }
    report["attribute_influence"] = json!({"team": name(&game, &a), "opponent": name(&game, &b), "baseline_goal_difference": (baseline.hg as f64 - baseline.ag as f64) / baseline.n.max(1) as f64, "runs": attrs});

    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("ok");
}
