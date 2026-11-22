//! DATA2 audit: a numeric profile of the FM world the adapter builds, for comparing two builds.
//!
//! `cargo run --release -p fm24-adapter --example data2_audit -- <world.db> <scratch-dir> <out.json>`
//!
//! Runs the real pipeline into `<scratch-dir>`, then a career start on the resulting world (twelve
//! seasons of generated history, the default of the new-game screen) so the history a player is
//! shown can be counted too. Writes one JSON profile: counts, squad classification by club, value
//! distributions by league / OVR band / age band, the largest value anomalies among comparable
//! players, loan provenance and the named probe players. It only reads `world.db`; no football
//! data is embedded here and the output belongs in a scratch directory, never in the repository.

use chrono::NaiveDate;
use domain::player::{Player, SquadRole};
use fm24_adapter::{Config, pipeline};
use ofm_core::clock::GameClock;
use ofm_core::generator::ReputationBasis;
use ofm_core::world::build::{PastHistory, build_game_from_imported_world};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Source person ids of the probe players, checked against the source by hand (names are ambiguous:
/// two Mbappés, two Tielemans).
const PROBES: [(i64, &str); 6] = [
    (12275, "Marcus Rashford"),
    (40368, "Kylian Mbappé"),
    (55635, "Kobbie Mainoo"),
    (6606, "Youri Tielemans"),
    (53728, "Mika Godts"),
    (63609, "Gabriel Moscardo"),
];

const GAME_START_YEAR: i32 = 2026;
const HISTORY_DEPTH_YEARS: u32 = 12;

fn percentile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn summary(mut values: Vec<u64>) -> Value {
    values.sort_unstable();
    json!({
        "n": values.len(),
        "p10": percentile(&values, 0.10),
        "p25": percentile(&values, 0.25),
        "p50": percentile(&values, 0.50),
        "p75": percentile(&values, 0.75),
        "p90": percentile(&values, 0.90),
        "p99": percentile(&values, 0.99),
        "max": values.last().copied().unwrap_or(0),
    })
}

fn age_band(age: i32) -> &'static str {
    match age {
        ..=18 => "a<=18",
        19..=21 => "b19-21",
        22..=24 => "c22-24",
        25..=27 => "d25-27",
        28..=30 => "e28-30",
        31..=33 => "f31-33",
        _ => "g34+",
    }
}

fn ovr_band(ovr: u8) -> String {
    let low = (ovr / 5) * 5;
    format!("{low:02}-{:02}", low + 4)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [world_db, scratch, out] = args.as_slice() else {
        return Err("usage: data2_audit <world.db> <scratch-dir> <out.json>".to_string());
    };
    if Path::new(out) == Path::new(world_db) || Path::new(scratch) == Path::new(world_db) {
        return Err("the output and scratch paths must not be the source database".to_string());
    }
    let mut cfg = Config::default_config();
    // Exploration only: `DATA2_TRUST_SAVE=1.0` re-runs the audit with another trust band.
    if let Some(trust) = std::env::var("DATA2_TRUST_SAVE")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        cfg.valuation.trust_save = trust;
    }
    let skip_career = std::env::var_os("DATA2_SKIP_CAREER").is_some();
    let run = pipeline::run(
        Path::new(world_db),
        &Path::new(scratch).join("package"),
        &cfg,
    )?;
    let on = NaiveDate::parse_from_str(&cfg.start.date, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let age_of = |p: &Player| domain::age::age_on_or_unknown(on, &p.date_of_birth);

    let league_of: BTreeMap<String, String> = run
        .clubs
        .iter()
        .map(|club| {
            let league = run
                .competitions
                .iter()
                .find(|c| c.id == club.competition_id)
                .map_or_else(|| "?".to_string(), |c| c.name.clone());
            (fm24_adapter::ids::club(club.id), league)
        })
        .collect();
    let club_name: BTreeMap<String, String> = run
        .clubs
        .iter()
        .map(|c| (fm24_adapter::ids::club(c.id), c.name.clone()))
        .collect();
    let imported = |p: &Player| p.id.starts_with("fm-p-");
    let plan_of: BTreeMap<&str, &pipeline::PlayerPlan> =
        run.plans.iter().map(|p| (p.def.id.as_str(), p)).collect();

    // --- counts ---------------------------------------------------------------------------------
    let club_players: Vec<&Player> = run
        .world
        .players
        .iter()
        .filter(|p| p.team_id.is_some())
        .collect();
    let free_agents_imported = run
        .world
        .players
        .iter()
        .filter(|p| p.team_id.is_none() && imported(p))
        .count();
    let free_agents_generated = run
        .world
        .players
        .iter()
        .filter(|p| p.team_id.is_none() && !imported(p))
        .count();
    let fillers_at_clubs = club_players.iter().filter(|p| !imported(p)).count();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for plan in &run.plans {
        *kinds.entry(plan.placement.kind.label()).or_default() += 1;
    }
    let mut value_sources: BTreeMap<&str, usize> = BTreeMap::new();
    let mut end_sources: BTreeMap<&str, usize> = BTreeMap::new();
    for plan in &run.plans {
        *value_sources.entry(plan.value_source).or_default() += 1;
        *end_sources.entry(plan.contract_end_source).or_default() += 1;
    }
    let missing_value = run
        .world
        .players
        .iter()
        .filter(|p| imported(p) && p.market_value == 0)
        .count();

    // --- squad classification by club ----------------------------------------------------------
    let mut clubs = Vec::new();
    for club in &run.clubs {
        let id = fm24_adapter::ids::club(club.id);
        let here: Vec<&Player> = run
            .world
            .players
            .iter()
            .filter(|p| p.team_id.as_deref() == Some(id.as_str()))
            .collect();
        let seniors = here
            .iter()
            .filter(|p| p.squad_role == SquadRole::Senior)
            .count();
        let youth = here.len() - seniors;
        let loaned_in = here.iter().filter(|p| p.active_loan.is_some()).count();
        let loaned_out = run
            .world
            .players
            .iter()
            .filter(|p| {
                p.active_loan
                    .as_ref()
                    .is_some_and(|l| l.parent_team_id == id)
            })
            .count();
        let ovrs: Vec<u64> = here
            .iter()
            .filter(|p| p.squad_role == SquadRole::Senior)
            .map(|p| u64::from(p.ovr))
            .collect();
        clubs.push(json!({
            "club": id, "name": club.name, "league": league_of[&id],
            "total": here.len(), "senior": seniors, "youth": youth,
            "imported": here.iter().filter(|p| imported(p)).count(),
            "generated": here.iter().filter(|p| !imported(p)).count(),
            "loaned_in": loaned_in, "loaned_out": loaned_out,
            "senior_ovr": summary(ovrs),
        }));
    }
    let mut by_role_age: BTreeMap<String, usize> = BTreeMap::new();
    for p in &club_players {
        let role = if p.squad_role == SquadRole::Senior {
            "senior"
        } else {
            "youth"
        };
        *by_role_age
            .entry(format!("{}/{role}", age_band(age_of(p))))
            .or_default() += 1;
    }

    // --- distributions --------------------------------------------------------------------------
    let imported_club: Vec<&Player> = club_players
        .iter()
        .copied()
        .filter(|p| imported(p))
        .collect();
    let mut value_by_league: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut value_by_ovr: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut value_by_age: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for p in &imported_club {
        let league = p
            .team_id
            .as_ref()
            .and_then(|t| league_of.get(t))
            .cloned()
            .unwrap_or_default();
        value_by_league
            .entry(league)
            .or_default()
            .push(p.market_value);
        value_by_ovr
            .entry(ovr_band(p.ovr))
            .or_default()
            .push(p.market_value);
        value_by_age
            .entry(age_band(age_of(p)).to_string())
            .or_default()
            .push(p.market_value);
    }
    let all_values: Vec<u64> = imported_club.iter().map(|p| p.market_value).collect();

    // --- anomalies: value against comparable players ---------------------------------------------
    // Comparable = same OVR band (5 points), same age band, same position group. The reference is the
    // cohort median; the ratio is value / median. Ranked by distance from 1 on a log scale.
    let mut cohorts: BTreeMap<(String, &'static str, String), Vec<u64>> = BTreeMap::new();
    let cohort_key = |p: &Player| {
        (
            ovr_band(p.ovr),
            age_band(age_of(p)),
            format!("{:?}", p.position.to_group_position()),
        )
    };
    for p in &imported_club {
        cohorts
            .entry(cohort_key(p))
            .or_default()
            .push(p.market_value);
    }
    let mut anomalies: Vec<(f64, Value)> = Vec::new();
    for p in &imported_club {
        let cohort = &cohorts[&cohort_key(p)];
        if cohort.len() < 8 {
            continue;
        }
        let mut sorted = cohort.clone();
        sorted.sort_unstable();
        let median = percentile(&sorted, 0.5).max(1);
        let ratio = p.market_value.max(1) as f64 / median as f64;
        anomalies.push((
            ratio.ln().abs(),
            json!({
                "id": p.id, "name": p.full_name, "age": age_of(p), "ovr": p.ovr, "pot": p.potential,
                "value_eur": p.market_value, "cohort_median_eur": median, "ratio": (ratio * 100.0).round() / 100.0,
                "cohort_size": cohort.len(),
                "club": p.team_id.as_ref().and_then(|t| club_name.get(t)),
                "contract_end": p.contract_end(),
            }),
        ));
    }
    anomalies.sort_by(|a, b| b.0.total_cmp(&a.0));
    let anomalies: Vec<Value> = anomalies.into_iter().take(40).map(|(_, v)| v).collect();

    // Rank inversions among comparable players: of two players of the same group and age band, the
    // one with a clearly higher OVR (>= 4) is worth less. Sampled deterministically, per cohort.
    let mut inversions = 0usize;
    let mut pairs = 0usize;
    let mut groups: BTreeMap<(&'static str, String), Vec<&Player>> = BTreeMap::new();
    for p in &imported_club {
        groups
            .entry((
                age_band(age_of(p)),
                format!("{:?}", p.position.to_group_position()),
            ))
            .or_default()
            .push(p);
    }
    for members in groups.values_mut() {
        members.sort_by(|a, b| a.id.cmp(&b.id));
        for (i, a) in members.iter().enumerate() {
            // Fixed stride keeps the pair count bounded (about 200 comparisons per player).
            for b in members.iter().skip(i + 1).step_by(7).take(200) {
                let (hi, lo) = if a.ovr >= b.ovr { (a, b) } else { (b, a) };
                if hi.ovr >= lo.ovr + 4 {
                    pairs += 1;
                    if hi.market_value < lo.market_value {
                        inversions += 1;
                    }
                }
            }
        }
    }

    // Strict inversions: clearly-ordered comparable pairs (see `valuation::strict_inversions`).
    let source_of: BTreeMap<String, &fm24_adapter::source::PlayerRow> = run
        .players
        .iter()
        .map(|r| (fm24_adapter::ids::player(r.person_id), r))
        .collect();
    let mut comparable: Vec<fm24_adapter::valuation::Comparable> = imported_club
        .iter()
        .filter_map(|p| {
            let row = source_of.get(p.id.as_str())?;
            let natural_code = plan_of.get(p.id.as_str())?.natural_code;
            Some(fm24_adapter::valuation::Comparable {
                id: p.id.clone(),
                inputs: fm24_adapter::valuation::Inputs::from_row(
                    row,
                    natural_code,
                    p.contract_end(),
                    on,
                    &cfg,
                ),
                value_eur: p.market_value,
            })
        })
        .collect();
    let (strict_pairs, strict_inversions) =
        fm24_adapter::valuation::strict_inversions(&mut comparable);

    // --- loans ----------------------------------------------------------------------------------
    let mut loans = Vec::new();
    let mut loan_wage_pct: BTreeMap<u8, usize> = BTreeMap::new();
    for p in run.world.players.iter().filter(|p| p.active_loan.is_some()) {
        let Some(loan) = &p.active_loan else { continue };
        *loan_wage_pct.entry(loan.wage_contribution_pct).or_default() += 1;
        loans.push(json!({
            "id": p.id,
            "kind": plan_of.get(p.id.as_str()).map(|pl| pl.placement.kind.label()),
            "owner": loan.parent_team_id, "borrower": loan.loan_team_id,
            "borrower_name": loan.loan_team_name, "end": loan.end_date,
            "wage_pct": loan.wage_contribution_pct, "buy_option": loan.buy_option_fee,
        }));
    }

    // --- probes ---------------------------------------------------------------------------------
    let team_name = |id: &str| club_name.get(id).cloned().unwrap_or_else(|| id.to_string());
    let mut probes = Vec::new();
    for (person, label) in PROBES {
        let id = fm24_adapter::ids::player(person);
        let source = run.players.iter().find(|r| r.person_id == person);
        let world = run.world.players.iter().find(|p| p.id == id);
        let plan = plan_of.get(id.as_str());
        probes.push(json!({
            "label": label, "person_id": person,
            "source": source.map(|r| json!({
                "fm_uid": r.fm_uid, "fm_uid_status": r.fm_uid_status, "birth": r.birth_date,
                "ca": r.ca, "pa": r.pa, "value_gbp": r.value_gbp, "value_source": r.value_source,
                "owner_club": r.club_name, "owner_club_id": r.club_id,
                "registered_club": r.registered_club_name, "registered_club_id": r.registered_club_id,
                "registered_slot": r.registered_slot, "registered_is_youth": r.registered_is_youth,
                "contract_start": r.start_date, "contract_end": r.end_date,
                "wage_gbp_week": r.wage_gbp_week, "wage_source": r.wage_source,
                "squad_status_code": r.squad_status_code, "contract_type_code": r.contract_type_code,
            })),
            "plan": plan.map(|p| json!({
                "kind": p.placement.kind.label(), "role": format!("{:?}", p.placement.role),
                "playing_club": p.placement.playing_club, "owner_club": p.placement.owner_club,
                "value_source": p.value_source, "def_value_eur": p.def.value,
                "value_in_source_eur": p.value_in_source_eur, "value_estimate_eur": p.value_estimate_eur,
            })),
            "world": world.map(|p| json!({
                "ovr": p.ovr, "potential": p.potential, "age": age_of(p),
                "market_value_eur": p.market_value, "wage_eur_week": p.wage(),
                "squad_role": format!("{:?}", p.squad_role),
                "team": p.team_id.as_deref().map(team_name),
                "loan": p.active_loan.as_ref().map(|l| json!({
                    "owner": team_name(&l.parent_team_id), "borrower": team_name(&l.loan_team_id),
                    "borrower_name": l.loan_team_name, "wage_pct": l.wage_contribution_pct,
                    "end": l.end_date,
                })),
                "career_entries_in_world_file": p.career.len(),
            })),
        }));
    }

    // --- a career start: the history a player is shown ---------------------------------------------
    let started = std::time::Instant::now();
    let clock = GameClock::new(ofm_core::world::start_date_for_year(GAME_START_YEAR)?);
    let manager = domain::manager::Manager::new(
        "audit-manager".to_string(),
        "Audit".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    let (game, _) = build_game_from_imported_world(
        clock,
        manager,
        PastHistory {
            start_year: GAME_START_YEAR,
            depth_years: if skip_career { 0 } else { HISTORY_DEPTH_YEARS },
        },
        run.world.clone(),
        ReputationBasis::Stored,
    );
    let build_seconds = started.elapsed().as_secs_f64();
    let imported_game: Vec<&Player> = game.players.iter().filter(|p| imported(p)).collect();
    let with_history = imported_game
        .iter()
        .filter(|p| !p.career.is_empty())
        .count();
    let synthetic_entries: usize = imported_game
        .iter()
        .map(|p| p.career.iter().filter(|e| e.synthetic).count())
        .sum();
    let unflagged_entries: usize = imported_game
        .iter()
        .map(|p| p.career.iter().filter(|e| !e.synthetic).count())
        .sum();
    let probe_history: Vec<Value> = PROBES
        .iter()
        .map(|(person, label)| {
            let id = fm24_adapter::ids::player(*person);
            let entries: Vec<Value> = game
                .players
                .iter()
                .find(|p| p.id == id)
                .map(|p| {
                    p.career
                        .iter()
                        .map(|e| json!([e.season, e.team_name, e.synthetic]))
                        .collect()
                })
                .unwrap_or_default();
            json!({"label": label, "entries": entries})
        })
        .collect();
    let mut game_roles: BTreeMap<String, usize> = BTreeMap::new();
    for p in game.players.iter().filter(|p| p.team_id.is_some()) {
        *game_roles.entry(format!("{:?}", p.squad_role)).or_default() += 1;
    }

    let profile = json!({
        "source_sha256_before": run.source_sha256_before,
        "source_sha256_after": run.source_sha256_after,
        "world_revision_config": cfg.world.id,
        "counts": {
            "clubs": run.clubs.len(),
            "imported_players_planned": run.plans.len(),
            "imported_players_in_world": run.world.players.iter().filter(|p| imported(p)).count(),
            "generated_fillers_at_clubs": fillers_at_clubs,
            "world_players_total": run.world.players.len(),
            "club_players": club_players.len(),
            "free_agents_imported": free_agents_imported,
            "free_agents_generated_by_ofm": free_agents_generated,
            "generated_removed_by_trim": run.removed_generated_players,
            "academy_promoted_to_reach_floor": run.promoted_academy_players.len(),
            "loans_applied": run.loans_applied.len(),
            "players_in_source": [run.total_players_in_source.0, run.total_players_in_source.1],
        },
        "placement_kinds": kinds,
        "senior_on_status": run.plans.iter().filter(|p| p.placement.senior_on_status).map(|p| {
            let row = source_of.get(p.def.id.as_str());
            json!({"id": p.def.id, "name": p.def.last_name, "club": club_name.get(&fm24_adapter::ids::club(p.placement.playing_club)),
                   "ca": row.map(|r| r.ca), "status": row.and_then(|r| r.squad_status_code)})
        }).collect::<Vec<_>>(),
        "value_sources": value_sources,
        "contract_end_sources": end_sources,
        "imported_with_zero_value": missing_value,
        "players_by_age_band_and_role": by_role_age,
        "game_squad_roles": game_roles,
        "clubs": clubs,
        "ovr": summary(imported_club.iter().map(|p| u64::from(p.ovr)).collect()),
        "potential": summary(imported_club.iter().map(|p| u64::from(p.potential)).collect()),
        "value_eur": summary(all_values),
        "value_eur_by_league": value_by_league.into_iter().map(|(k, v)| (k, summary(v))).collect::<BTreeMap<_, _>>(),
        "value_eur_by_ovr_band": value_by_ovr.into_iter().map(|(k, v)| (k, summary(v))).collect::<BTreeMap<_, _>>(),
        "value_eur_by_age_band": value_by_age.into_iter().map(|(k, v)| (k, summary(v))).collect::<BTreeMap<_, _>>(),
        "comparable_pair_inversions": {"pairs": pairs, "inversions": inversions,
            "rate": if pairs == 0 { 0.0 } else { inversions as f64 / pairs as f64 }},
        "strict_pair_inversions": {"pairs": strict_pairs, "inversions": strict_inversions,
            "rate": if strict_pairs == 0 { 0.0 } else { strict_inversions as f64 / strict_pairs as f64 }},
        "largest_value_anomalies": anomalies,
        "loans": {"count": loans.len(), "wage_contribution_pct": loan_wage_pct, "rows": loans},
        "probes": probes,
        "career_start": {
            "history_depth_years": HISTORY_DEPTH_YEARS,
            "build_seconds": build_seconds,
            "imported_players_in_game": imported_game.len(),
            "imported_with_any_career_entry": with_history,
            "synthetic_entries_on_imported_players": synthetic_entries,
            "unflagged_entries_on_imported_players": unflagged_entries,
            "probe_history": probe_history,
        },
    });
    std::fs::write(
        out,
        serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}
