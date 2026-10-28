//! Output files: the world, the id mapping, the extension (FM originals), the report.
//!
//! Everything is written in a fixed order with no timestamps, so a run on the same inputs writes
//! the same bytes.

use crate::Result;
use crate::checks::Check;
use crate::ids;
use crate::pipeline::Run;
use ofm_core::generator::export_world_to_json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub struct Written {
    pub world_path: PathBuf,
    pub world_sha256: String,
    pub world_bytes: u64,
    pub files: Vec<(String, String, u64)>,
}

/// OFM's own export (which normalises the world), re-written compactly with every object's keys
/// sorted. OFM keeps some `HashMap`s in the world (tactical roles, translations): their iteration
/// order changes from one process to the next, so the raw export is not byte-stable. Sorting the
/// keys makes the file -- and its SHA-256 -- depend on the data only.
pub fn canonical_world_json(world: &ofm_core::generator::WorldData) -> Result<String> {
    let exported = export_world_to_json(world)?;
    let value: serde_json::Value = serde_json::from_str(&exported).map_err(|e| e.to_string())?;
    let mut out = String::with_capacity(exported.len() / 2);
    write_sorted(&value, &mut out);
    Ok(out)
}

fn write_sorted(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(key.clone()).to_string());
                out.push(':');
                write_sorted(&map[key], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_sorted(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Writes every output into `out_dir`. Refuses to write the world if a blocking check failed.
pub fn write_all(run: &Run, checks: &[Check], out_dir: &Path) -> Result<Written> {
    if let Some(failed) = checks.iter().find(|c| c.blocking && !c.ok) {
        return Err(format!(
            "blocking check `{}` failed: {}",
            failed.name, failed.detail
        ));
    }
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut files: Vec<(String, String, u64)> = Vec::new();
    let mut put = |name: &str, content: Vec<u8>| -> Result<()> {
        let path = out_dir.join(name);
        std::fs::write(&path, &content).map_err(|e| format!("write {}: {e}", path.display()))?;
        files.push((name.to_string(), sha256_hex(&content), content.len() as u64));
        Ok(())
    };

    let world_json = canonical_world_json(&run.world)?;
    let world_name = format!("{}.json", run.cfg.world.id);
    let world_sha256 = sha256_hex(world_json.as_bytes());
    let world_bytes = world_json.len() as u64;
    put(&world_name, world_json.into_bytes())?;
    put("mapping.csv", mapping_csv(run).into_bytes())?;
    put(
        &format!("{}.extension.jsonl", run.cfg.world.id),
        extension_jsonl(run).into_bytes(),
    )?;
    put("finance.csv", finance_csv(run).into_bytes())?;
    put(
        "generated_players.csv",
        generated_players_csv(run).into_bytes(),
    )?;
    put("generated_staff.csv", generated_staff_csv(run).into_bytes())?;
    put("journal.csv", journal_csv(run).into_bytes())?;
    put("checks.csv", checks_csv(checks).into_bytes())?;
    put(
        "ADAPTER_REPORT.md",
        adapter_report(run, checks, &world_name, &world_sha256, world_bytes).into_bytes(),
    )?;

    let manifest: String = files
        .iter()
        .map(|(name, sha, bytes)| format!("{sha}  {bytes:>12}  {name}\n"))
        .collect();
    std::fs::write(out_dir.join("SHA256SUMS"), manifest).map_err(|e| e.to_string())?;
    Ok(Written {
        world_path: out_dir.join(&world_name),
        world_sha256,
        world_bytes,
        files,
    })
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn mapping_csv(run: &Run) -> String {
    let mut out =
        String::from("ofm_id,entity,world_db_id,fm_uid,fm_uid_status,name,ofm_club_id,note\n");
    for row in &run.mapping {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{}",
            row.ofm_id,
            row.entity,
            row.world_db_id,
            row.fm_uid.map(|u| u.to_string()).unwrap_or_default(),
            row.fm_uid_status,
            csv_field(&row.name),
            row.ofm_club_id,
            csv_field(&row.note)
        );
    }
    out
}

fn finance_csv(run: &Run) -> String {
    let mut out = String::from("club_id,ofm_club_id,field,value_eur,provenance,origin,note\n");
    let mut records = run.finance.clone();
    records.sort_by(|a, b| a.club_id.cmp(&b.club_id).then(a.field.cmp(b.field)));
    for r in records {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{}",
            r.club_id,
            ids::club(r.club_id),
            r.field,
            r.value,
            r.provenance.label(),
            csv_field(&r.origin),
            csv_field(&r.note)
        );
    }
    out
}

fn generated_players_csv(run: &Run) -> String {
    let mut out = String::from("club_id,ofm_club_id,player_id,position,ovr,origin\n");
    for g in &run.generated_players {
        let _ = writeln!(
            out,
            "{},{},{},{:?},{},généré par OFM (pas FM24)",
            g.club_id,
            ids::club(g.club_id),
            g.id,
            g.position,
            g.ovr
        );
    }
    out
}

fn generated_staff_csv(run: &Run) -> String {
    let mut out = String::from("club_id,ofm_club_id,staff_id,role,origin\n");
    for g in &run.generated_staff {
        let _ = writeln!(
            out,
            "{},{},{},{},généré par OFM (pas FM24)",
            g.club_id,
            ids::club(g.club_id),
            g.id,
            g.role
        );
    }
    out
}

fn journal_csv(run: &Run) -> String {
    let mut out = String::from("severity,code,entity,message\n");
    for n in run.journal.notes() {
        let _ = writeln!(
            out,
            "{:?},{},{},{}",
            n.severity,
            n.code,
            csv_field(&n.entity),
            csv_field(&n.message)
        );
    }
    out
}

fn checks_csv(checks: &[Check]) -> String {
    let mut out = String::from("name,blocking,ok,detail\n");
    for c in checks {
        let _ = writeln!(
            out,
            "{},{},{},{}",
            c.name,
            c.blocking,
            c.ok,
            csv_field(&c.detail)
        );
    }
    out
}

/// One JSON object per imported player and per free agent of the pool: what OFM cannot hold, plus
/// the provenance of derived values.
fn extension_jsonl(run: &Run) -> String {
    let rows: BTreeMap<i64, &crate::source::PlayerRow> =
        run.players.iter().map(|p| (p.person_id, p)).collect();
    let mut out = String::new();
    for plan in &run.plans {
        let row = rows[&plan.person_id];
        let value = serde_json::json!({
            "id": plan.def.id,
            "fm_person_id": row.person_id,
            "fm_uid": row.fm_uid,
            "fm_uid_status": row.fm_uid_status,
            "ca": row.ca,
            "pa": row.pa,
            "reputation": { "current": row.rep_current, "home": row.rep_home, "world": row.rep_world },
            "height_cm": row.height_cm,
            "feet": { "left": row.foot_left, "right": row.foot_right },
            "fm_natural_position": plan.natural_code,
            "fm_positions": row.positions,
            "fm_attributes": row.attrs.0,
            "contract": { "type_code": row.contract_type_code, "squad_status_code": row.squad_status_code },
            "placement": {
                "kind": plan.placement.kind.label(),
                "contract_club_id": row.club_id,
                "registered_club_id": row.registered_club_id,
                "registered_team_slot": row.registered_slot,
                "playing_club_id": plan.placement.playing_club,
                "owner_club_id": plan.placement.owner_club,
                "role": format!("{:?}", plan.placement.role),
                "promoted_from_academy": run.promoted_academy_players.contains(&plan.def.id),
            },
            "wage_gbp_week": { "value": row.wage_gbp_week, "original": row.wage_original, "source": row.wage_source },
            "provenance": {
                "nationality": plan.nationality_source,
                "contract_end": plan.contract_end_source,
                "value": plan.value_source,
                "attributes": "dérivés (52 -> 19, voir attributes.rs)",
                "potential": "dérivé (OVR OFM + 0,31 x (PA - CA))",
            },
        });
        out.push_str(&value.to_string());
        out.push('\n');
    }
    for fa in &run.free_agent_plans {
        let row = &fa.source;
        let value = serde_json::json!({
            "id": fa.def.id,
            "fm_person_id": row.person_id,
            "fm_uid": row.fm_uid,
            "fm_uid_status": row.fm_uid_status,
            "free_agent": true,
            "ca": row.ca,
            "pa": row.pa,
            "reputation": { "current": row.rep_current, "home": row.rep_home, "world": row.rep_world },
            "height_cm": row.height_cm,
            "feet": { "left": row.foot_left, "right": row.foot_right },
            "fm_natural_position": fa.natural_code,
            "fm_positions": row.positions,
            "fm_attributes": row.attrs.0,
            "asked_wage_eur_week": fa.asked_wage,
            "provenance": {
                "pool": "agent libre FM24 (joueur sans contrat dans la save, tiré dans le pool actif)",
                "wage": fa.wage_source,
                "value": fa.value_source,
                "attributes": "dérivés (52 -> 19, voir attributes.rs)",
                "potential": "dérivé (OVR OFM + 0,31 x (PA - CA))",
            },
        });
        out.push_str(&value.to_string());
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------------------------
// statistics
// ---------------------------------------------------------------------------------------------

pub fn percentile(sorted: &[u8], q: f64) -> u8 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[index]
}

/// Least-squares slope and intercept of OFM rating on FM CA (used to calibrate the potential step).
pub fn ovr_on_ca_fit(run: &Run) -> (f64, f64) {
    let ca: BTreeMap<i64, i64> = run.players.iter().map(|p| (p.person_id, p.ca)).collect();
    let points: Vec<(f64, f64)> = run
        .plans
        .iter()
        .map(|p| (ca[&p.person_id] as f64, f64::from(p.ovr_expected)))
        .collect();
    let n = points.len() as f64;
    let (sx, sy): (f64, f64) = points
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mx, my) = (sx / n, sy / n);
    let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let slope = sxy / sxx;
    (slope, my - slope * mx)
}

/// Where the save's players play and in what role: the registered team decides, not the contract.
fn placement_section(r: &mut String, run: &Run) {
    use crate::placement::Kind;
    let _ = writeln!(r, "## Effectifs : équipe d'inscription, prêts, académie\n");
    let _ = writeln!(
        r,
        "La save enregistre, pour chaque joueur, le club de son **contrat** et l'**équipe** avec laquelle il est inscrit (première équipe, réserve, jeunes). Les deux diffèrent pour les prêts et les équipes B affiliées. Règles : `placement.rs`, vérifiées indépendamment par `tools/roster_audit/oracle.py`.\n"
    );
    let mut by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    for plan in &run.plans {
        *by_kind.entry(plan.placement.kind.label()).or_default() += 1;
    }
    let _ = writeln!(r, "| Cas | Joueurs |\n|---|---|");
    for (kind, n) in &by_kind {
        let _ = writeln!(r, "| {kind} | {n} |");
    }
    let senior = run
        .plans
        .iter()
        .filter(|p| p.placement.role == domain::player::SquadRole::Senior)
        .count();
    let _ = writeln!(
        r,
        "\n- Rôle d'après la save : **{senior}** seniors (première équipe, ou au-delà de l'âge d'académie d'OFM), **{}** académie.",
        run.plans.len() - senior
    );
    let _ = writeln!(
        r,
        "- **{}** joueurs importés comme prêts (`team_id` = club où ils jouent, `active_loan` = propriétaire) ; fin `{}`, part du salaire payée par l'emprunteur {} % : **valeurs configurées**, la save n'en contient pas.",
        run.loans_applied.len(),
        run.cfg.loans.end_date,
        run.cfg.loans.wage_contribution_pct
    );
    let outside = run
        .plans
        .iter()
        .filter(|p| {
            matches!(
                p.placement.kind,
                Kind::LoanToOutsideClub | Kind::AffiliateLoan
            )
        })
        .count();
    let _ = writeln!(
        r,
        "- Dont **{outside}** prêtés à un club que le monde ne contient pas (leur nom est gardé dans le prêt ; ils reviennent à leur propriétaire à la fin du prêt).",
    );
    let _ = writeln!(
        r,
        "- **{}** joueurs de l'académie promus en équipe première pour atteindre le plancher d'OFM (15 seniors, 2 G / 4 D / 4 M / 2 A) au lieu de générer un joueur ; **{}** joueurs générés par OFM subsistent.\n",
        run.promoted_academy_players.len(),
        run.generated_players.len()
    );
}

/// The pool of FM24 free agents: why each unattached player is or is not in it, and what the pool
/// asks against what contracted players earn.
fn free_agents_section(r: &mut String, run: &Run) {
    let stats = &run.free_agent_stats;
    let _ = writeln!(r, "## Agents libres FM24 (pool actif)\n");
    if run.free_agent_plans.is_empty() {
        let _ = writeln!(
            r,
            "Aucun agent libre FM24 importé (`free_agents.enabled = false` ou aucun candidat).\n"
        );
        return;
    }
    let _ = writeln!(
        r,
        "La save contient **{}** joueurs sans contrat : de vrais agents libres, des joueurs de clubs de ligues non chargées et des retraités. Un pool borné de **{}** entre dans le monde (tirage par empreinte de l'identifiant et de la graine) ; les autres restent dans `world.db`, intacts.\n",
        stats.candidates, stats.chosen
    );
    let _ = writeln!(r, "| Raison | Joueurs |\n|---|---|");
    for (label, n) in [
        ("Sans date de naissance", stats.no_birth_date),
        ("Hors de la tranche d'âge", stats.outside_age_range),
        ("Niveau insuffisant", stats.below_level),
        ("Nation inconnue d'OFM", stats.no_known_nation),
        ("Aucun poste exploitable", stats.no_position),
        ("**Éligibles**", stats.eligible),
        ("dont **dans le pool**", stats.chosen),
        (
            "dont laissés dans world.db (pool plein)",
            stats.eligible - stats.chosen,
        ),
    ] {
        let _ = writeln!(r, "| {label} | {n} |");
    }
    let mut ovr: Vec<u8> = run
        .free_agent_plans
        .iter()
        .map(|p| p.ovr_expected)
        .collect();
    ovr.sort_unstable();
    let mut asked: Vec<u32> = run
        .free_agent_plans
        .iter()
        .filter_map(|p| p.asked_wage)
        .collect();
    asked.sort_unstable();
    let pick = |v: &[u32], q: f64| {
        v.get(((v.len().max(1) - 1) as f64 * q).round() as usize)
            .copied()
            .unwrap_or(0)
    };
    let _ = writeln!(
        r,
        "\nNote OFM du pool : p10 {} · médiane {} · p90 {}. Salaire demandé (€/semaine) : p10 {} · médiane {} · p90 {} (estimé sur les joueurs sous contrat de même niveau et de même âge, quantile {}). Valeur : estimée de la même façon (`estimée (adaptateur)`), sauf {} valeurs lues dans la save.\n",
        percentile(&ovr, 0.1),
        percentile(&ovr, 0.5),
        percentile(&ovr, 0.9),
        pick(&asked, 0.1),
        pick(&asked, 0.5),
        pick(&asked, 0.9),
        run.cfg.free_agents.wage_quantile,
        run.free_agent_plans
            .iter()
            .filter(|p| p.value_source == "save")
            .count()
    );
}

fn adapter_report(
    run: &Run,
    checks: &[Check],
    world_name: &str,
    world_sha256: &str,
    world_bytes: u64,
) -> String {
    let mut r = String::new();
    let world = &run.world;
    let pool_ids: BTreeSet<&str> = run
        .free_agent_plans
        .iter()
        .map(|p| p.def.id.as_str())
        .collect();
    let imported_players = world
        .players
        .iter()
        .filter(|p| p.id.starts_with("fm-p-") && !pool_ids.contains(p.id.as_str()))
        .count();
    let generated_players = world.players.len() - imported_players - pool_ids.len();
    let imported_staff = world
        .staff
        .iter()
        .filter(|s| s.id.starts_with("fm-s-"))
        .count();
    let generated_staff = world.staff.len() - imported_staff;
    let _ = writeln!(r, "# Rapport de l'adaptateur FM24 -> OpenFootManager\n");
    let _ = writeln!(
        r,
        "Généré par `apfm-import` (aucun horodatage : le rapport est reproductible).\n"
    );
    let _ = writeln!(r, "## Entrées\n");
    let _ = writeln!(
        r,
        "- Source : `world.db` (SHA-256 `{}`), schéma importeur `{}`, save FM `{}` (SHA-256 `{}`), saison {}.",
        run.source_sha256_before,
        run.schema.importer_version,
        run.schema.game_version,
        run.schema.source_sha256,
        run.schema.start_season
    );
    let _ = writeln!(
        r,
        "- Source modifiée par l'adaptateur : **{}** (SHA-256 avant/après identique).",
        if run.source_sha256_before == run.source_sha256_after {
            "non"
        } else {
            "OUI"
        }
    );
    let _ = writeln!(
        r,
        "- Graine de construction : {} ; taux £→€ : {} ; version de configuration : {}.\n",
        run.cfg.world.seed, run.cfg.money.gbp_to_eur, run.cfg.world.version
    );
    let _ = writeln!(r, "## Sortie\n");
    let _ = writeln!(
        r,
        "- Monde : `{world_name}` — {world_bytes} octets — SHA-256 `{world_sha256}`."
    );
    let _ = writeln!(
        r,
        "- Clubs : **{}** ; compétitions : **{}** ; joueurs sous contrat importés : **{imported_players}** ; agents libres FM24 importés : **{}** ; joueurs générés par OFM : **{generated_players}** ; staff importé : **{imported_staff}** ; staff généré par OFM : **{generated_staff}** ; managers réels : **{}**.\n",
        world.teams.len(),
        run.competitions.len(),
        run.free_agent_plans.len(),
        world.managers.len()
    );
    placement_section(&mut r, run);
    free_agents_section(&mut r, run);

    let _ = writeln!(
        r,
        "## Contrôles ({} / {} réussis)\n",
        checks.iter().filter(|c| c.ok).count(),
        checks.len()
    );
    let _ = writeln!(
        r,
        "| Contrôle | Bloquant | Résultat | Détail |\n|---|---|---|---|"
    );
    for c in checks {
        let _ = writeln!(
            r,
            "| `{}` | {} | {} | {} |",
            c.name,
            if c.blocking { "oui" } else { "non" },
            if c.ok { "OK" } else { "ÉCHEC" },
            c.detail.replace('|', "/")
        );
    }

    let _ = writeln!(r, "\n## Journal (par code)\n");
    let _ = writeln!(r, "| Gravité | Code | Occurrences |\n|---|---|---|");
    for ((code, severity), count) in run.journal.counts() {
        let _ = writeln!(r, "| {severity:?} | `{code}` | {count} |");
    }

    let _ = writeln!(
        r,
        "\n## Joueurs générés par OFM (jamais présentés comme importés)\n"
    );
    let mut per_club: BTreeMap<i64, usize> = BTreeMap::new();
    for g in &run.generated_players {
        *per_club.entry(g.club_id).or_default() += 1;
    }
    let _ = writeln!(
        r,
        "{} joueurs générés dans {} clubs (liste complète : `generated_players.csv`). {} joueurs générés par OFM ont été retirés car non nécessaires au plancher de jeu.\n",
        generated_players,
        per_club.len(),
        run.removed_generated_players
    );
    let clubs_by_id: BTreeMap<i64, &crate::source::ClubRow> =
        run.clubs.iter().map(|c| (c.id, c)).collect();
    let mut top: Vec<(&i64, &usize)> = per_club.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let _ = writeln!(r, "| Club | Joueurs générés |\n|---|---|");
    for (club, n) in top.into_iter().take(12) {
        let _ = writeln!(
            r,
            "| {} (`{}`) | {n} |",
            clubs_by_id
                .get(club)
                .map(|c| c.name.as_str())
                .unwrap_or("?"),
            ids::club(*club)
        );
    }

    let _ = writeln!(r, "\n## Staff généré par OFM\n");
    let mut by_role: BTreeMap<&str, usize> = BTreeMap::new();
    for g in &run.generated_staff {
        *by_role.entry(g.role.as_str()).or_default() += 1;
    }
    let _ = writeln!(
        r,
        "{} membres générés (liste : `generated_staff.csv`) : {}.",
        generated_staff,
        by_role
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(r, "\n## Managers\n");
    let _ = writeln!(
        r,
        "{} clubs ont leur entraîneur principal FM comme manager. Clubs sans entraîneur principal en équipe première : {} ({}).",
        world.managers.len(),
        run.manager_clubs_missing.len(),
        run.manager_clubs_missing
            .iter()
            .map(|c| ids::club(*c))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let _ = writeln!(r, "\n## Finances (provenance)\n");
    let mut by_field: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for f in &run.finance {
        *by_field.entry((f.field, f.provenance.label())).or_default() += 1;
    }
    let _ = writeln!(r, "| Champ | Provenance | Clubs |\n|---|---|---|");
    for ((field, provenance), n) in by_field {
        let _ = writeln!(r, "| {field} | {provenance} | {n} |");
    }
    let _ = writeln!(
        r,
        "\nLes valeurs « importée » viennent de `club_finance_start`, elle-même **générée par l'importeur** (`source = générée`, ordres de grandeur à vérifier) ; la capacité du stade est tirée par OFM (absente de world.db)."
    );

    let _ = writeln!(r, "\n## Distribution des notes OFM (joueurs importés)\n");
    let _ = writeln!(
        r,
        "| Division | Joueurs | Médiane | p90 | Max | Moy. des 14 meilleurs/club |\n|---|---|---|---|---|---|"
    );
    let ovr_of: BTreeMap<&str, u8> = world
        .players
        .iter()
        .map(|p| (p.id.as_str(), p.ovr))
        .collect();
    let comp_of_club: BTreeMap<i64, i64> =
        run.clubs.iter().map(|c| (c.id, c.competition_id)).collect();
    for comp in &run.competitions {
        let mut ratings: Vec<u8> = Vec::new();
        let mut firsts: Vec<f64> = Vec::new();
        for club in run.clubs.iter().filter(|c| comp_of_club[&c.id] == comp.id) {
            let mut club_ratings: Vec<u8> = run
                .plans
                .iter()
                .filter(|p| p.def.club == ids::club(club.id))
                .filter_map(|p| ovr_of.get(p.def.id.as_str()).copied())
                .collect();
            club_ratings.sort_unstable_by(|a, b| b.cmp(a));
            let top14: Vec<u8> = club_ratings.iter().copied().take(14).collect();
            if !top14.is_empty() {
                firsts.push(top14.iter().map(|v| f64::from(*v)).sum::<f64>() / top14.len() as f64);
            }
            ratings.extend(club_ratings);
        }
        ratings.sort_unstable();
        let strength = if firsts.is_empty() {
            0.0
        } else {
            firsts.iter().sum::<f64>() / firsts.len() as f64
        };
        let _ = writeln!(
            r,
            "| {} | {} | {} | {} | {} | {:.1} |",
            comp.name,
            ratings.len(),
            percentile(&ratings, 0.5),
            percentile(&ratings, 0.9),
            percentile(&ratings, 1.0),
            strength
        );
    }
    let (slope, intercept) = ovr_on_ca_fit(run);
    let _ = writeln!(
        r,
        "\nPente de la note OFM sur le CA FM : {slope:.3} point OFM par point de CA (ordonnée {intercept:.1}) ; coefficient de potentiel configuré : {}.",
        run.cfg.potential.ovr_points_per_ca
    );
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_of_a_sorted_slice() {
        let v: Vec<u8> = (1..=100).collect();
        assert_eq!(percentile(&v, 0.5), 51);
        assert_eq!(percentile(&v, 1.0), 100);
        assert_eq!(percentile(&[], 0.5), 0);
    }

    #[test]
    fn csv_fields_with_commas_are_quoted() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn canonical_json_sorts_keys_at_every_depth() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"b":1,"a":{"d":[{"z":1,"y":2}],"c":"x"}}"#).unwrap();
        let mut out = String::new();
        write_sorted(&value, &mut out);
        assert_eq!(out, r#"{"a":{"c":"x","d":[{"y":2,"z":1}]},"b":1}"#);
    }

    #[test]
    fn sha256_of_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
