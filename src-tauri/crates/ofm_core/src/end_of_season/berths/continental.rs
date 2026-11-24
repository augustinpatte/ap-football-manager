//! Who qualifies for a competition fed by berths, and the fields those berths produce.
//!
//! Berth rules name a source competition and a rule for reading clubs out of it; this module
//! evaluates them. It covers continental qualification and the shared field resolution that
//! [`super::domestic`] also uses.
//!
//! The tests for this module live in `crates/ofm_core/tests/end_of_season_tests.rs`, because they
//! exercise it through the public re-exports rather than against its internals.

use crate::game::Game;
use domain::league::{Berth, BerthRule, CompetitionScope, CompetitionType, League, StandingEntry};

/// Domestic league finishes that earn a continental berth — the top N of each
/// first division. Cup winners qualify on top of these.
const CONTINENTAL_LEAGUE_SLOTS: usize = 4;

/// The competition's declared regions, or — if it declares none — every region
/// present in the domestic competition set.
fn feeder_regions(game: &Game, competition: &League) -> std::collections::HashSet<String> {
    if competition.required_region_ids.is_empty() {
        game.competitions
            .iter()
            .filter_map(|source| source.region_id.clone())
            .collect()
    } else {
        competition.required_region_ids.iter().cloned().collect()
    }
}

/// The clubs that qualify for a continental competition next season, decided by
/// the domestic season just completed: the top finishers of each first division
/// in the competition's feeder regions, plus domestic cup winners. The field is
/// seeded by reputation and capped to the competition's size; a thin field is
/// topped up by reputation so the bracket keeps its shape.
///
/// This is what makes domestic results feed continental qualification — a club
/// that finishes top of its league then plays continental football, instead of
/// the field being frozen at world creation. Read from final standings, so call
/// it before regeneration resets them.
pub fn continental_qualified_entrants(game: &Game, competition: &League) -> Vec<String> {
    use std::collections::{BTreeMap, HashSet};

    let feeder_regions = feeder_regions(game, competition);
    let in_feeder = |c: &League| {
        c.region_id
            .as_deref()
            .is_some_and(|region| feeder_regions.contains(region))
    };

    let mut qualified: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // The first division of each feeder country is its lowest-priority league.
    let mut first_division: BTreeMap<&str, &League> = BTreeMap::new();
    for competition in &game.competitions {
        if competition.scope != CompetitionScope::Domestic
            || competition.kind != CompetitionType::League
            || !in_feeder(competition)
        {
            continue;
        }
        let Some(country) = competition.country_id.as_deref() else {
            continue;
        };
        first_division
            .entry(country)
            .and_modify(|best| {
                if competition.priority < best.priority {
                    *best = competition;
                }
            })
            .or_insert(competition);
    }
    for league in first_division.values() {
        for entry in standings_for_berth(game, league)
            .into_iter()
            .take(CONTINENTAL_LEAGUE_SLOTS)
        {
            if seen.insert(entry.team_id.clone()) {
                qualified.push(entry.team_id);
            }
        }
    }

    // Domestic cup winners earn a berth too.
    for competition in &game.competitions {
        if competition.scope != CompetitionScope::Domestic
            || competition.kind != CompetitionType::Cup
            || !in_feeder(competition)
        {
            continue;
        }
        if let Some(winner) = cup_champion(game, competition)
            && seen.insert(winner.clone())
        {
            qualified.push(winner);
        }
    }

    seed_cap_and_fill(game, competition, qualified, seen)
}

/// Whether any competition awards a berth into `target_id` — i.e. continental
/// qualification for that competition is data-defined rather than inferred.
pub fn competition_has_incoming_berths(game: &Game, target_id: &str) -> bool {
    game.competitions
        .iter()
        .flat_map(|source| &source.berths)
        .any(|berth| berth.target == target_id || berth.fallback_to.as_deref() == Some(target_id))
}

/// Who won `cup`: its own bracket if it has been decided, otherwise, only if the cup has been
/// renewed for its next edition, the champion of the edition it last finished.
///
/// A cup is renewed before the rollover reads its winner, so by then its live bracket is the next,
/// unplayed one and the finished edition is only in the archive. An undecided bracket is not by
/// itself a renewed cup: a kept cup whose final was postponed also has no winner yet, and its
/// archived edition is last season's, already credited. So the archive is read only when the live
/// edition is ahead of the edition its country is settling:
///
/// - The cup's season is past the top division's: the cup was renewed ahead of its league (its
///   opener comes first) and the archived edition is the one being settled.
/// - The seasons are equal but the top division's own season has not ended: the country was renewed
///   early, so the live cup is the new, unplayed edition and the archive holds the settled one.
///   This is also a country running ahead of the rollover, whose latest completed edition is the
///   archived one.
///
/// A cup whose season equals an *ended* top division is the edition being settled (kept because a
/// tie is still to play). Its archived edition is the previous one, already credited, so it
/// contributes nobody this season rather than crediting the same champion twice.
pub(crate) fn cup_champion(game: &Game, cup: &League) -> Option<String> {
    crate::world_cup::world_cup_champion(cup).or_else(|| {
        let settling = top_division_season(game, cup)?;
        let edition_is_ahead = cup.season > settling
            || (cup.season == settling
                && !crate::end_of_season::is_league_season_ended(top_division(game, cup)?));
        if !edition_is_ahead {
            return None;
        }
        crate::edition_archive::champion_of_latest_edition_before(
            &game.edition_archive,
            &cup.id,
            cup.season,
        )
        .map(str::to_string)
    })
}

/// The first division of the cup's country: the competition whose season the rollover is settling.
fn top_division<'a>(game: &'a Game, cup: &League) -> Option<&'a League> {
    let country = cup.country_id.as_deref()?;
    game.competitions
        .iter()
        .filter(|competition| {
            competition.kind == CompetitionType::League
                && competition.scope == CompetitionScope::Domestic
                && competition.country_id.as_deref() == Some(country)
        })
        .min_by_key(|competition| competition.priority)
}

/// The season of the first division of the cup's country: the edition being settled.
fn top_division_season(game: &Game, cup: &League) -> Option<u32> {
    top_division(game, cup).map(|competition| competition.season)
}

/// The final table a berth reads.
///
/// A live table is the edition being settled only when its own season has ended. A country the
/// daily hook renewed early keeps a live table for a season now in progress while the finished one
/// sits in the archive: reading the live table then would hand a continental cup a partial new
/// table, whose early leader is not the club that finished high last season. So a live table still
/// playing (or not yet kicked off) defers to the most recent archived edition, and only a world
/// with no archived edition at all falls back to the live order — the old behaviour at its opening.
fn standings_for_berth(game: &Game, source: &League) -> Vec<StandingEntry> {
    if crate::end_of_season::is_league_season_ended(source) {
        return source.sorted_standings();
    }
    crate::edition_archive::latest_edition_before(&game.edition_archive, &source.id, source.season)
        .map(|edition| edition.standings.clone())
        .unwrap_or_else(|| source.sorted_standings())
}

/// Teams a single berth rule selects from a competition's finished results.
/// `PlayoffWinner` is scheduled and resolved separately (Phase C.3b).
pub(super) fn evaluate_berth_rule(game: &Game, source: &League, rule: &BerthRule) -> Vec<String> {
    match rule {
        BerthRule::PositionRange { from, to } => {
            let start = (*from as usize).saturating_sub(1);
            let count = (*to).saturating_sub(*from).saturating_add(1) as usize;
            standings_for_berth(game, source)
                .into_iter()
                .skip(start)
                .take(count)
                .map(|entry| entry.team_id)
                .collect()
        }
        BerthRule::CupWinner => cup_champion(game, source).into_iter().collect(),
        BerthRule::PlayoffWinner { .. } => Vec::new(),
        BerthRule::Incumbent => european_incumbents(game, source),
        BerthRule::Titleholder => european_titleholder(game, source).into_iter().collect(),
        BerthRule::EuropeanPerformance { position, places } => {
            european_performance_place(game, source, *position, *places)
                .into_iter()
                .collect()
        }
    }
}

/// The clubs of a league-phase competition's edition whose league the world does not simulate:
/// no domestic league table carries them, so no domestic result can requalify them and the place
/// stays with the club. (Synthetic by design; see `BerthRule::Incumbent`.)
fn european_incumbents(game: &Game, source: &League) -> Vec<String> {
    let in_domestic_table: std::collections::HashSet<&str> = game
        .competitions
        .iter()
        .filter(|competition| {
            competition.scope == CompetitionScope::Domestic
                && competition.kind == CompetitionType::League
        })
        .flat_map(|competition| competition.participant_ids.iter().map(String::as_str))
        .collect();
    source
        .participant_ids
        .iter()
        .filter(|club| !in_domestic_table.contains(club.as_str()))
        .cloned()
        .collect()
}

/// Who won a league-phase competition's edition: its champion once the edition is completed, or,
/// when the competition has already been renewed (its new edition is drawn but unplayed), the
/// champion of the edition it last finished. An edition still being played has none yet.
fn european_titleholder(game: &Game, source: &League) -> Option<String> {
    use domain::league_phase::CompetitionStage;
    let state = source.league_phase.as_ref()?;
    match state.stage {
        CompetitionStage::Completed => state.champion_id.clone(),
        CompetitionStage::Created | CompetitionStage::LeaguePhaseScheduled => {
            crate::edition_archive::champion_of_latest_edition_before(
                &game.edition_archive,
                &source.id,
                source.season,
            )
            .map(str::to_string)
        }
        _ => None,
    }
}

/// The club finishing `position` in `source` when its association is one of the `places` best
/// performers of the latest European season among the associations whose league the world simulates
/// (UEFA's European performance spots, on this game's own coefficient points).
fn european_performance_place(
    game: &Game,
    source: &League,
    position: u32,
    places: u32,
) -> Option<String> {
    use crate::league_phase::coefficient;
    use std::collections::HashMap;
    let association = source.country_id.as_deref()?;
    let mut simulated: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for competition in &game.competitions {
        if competition.scope == CompetitionScope::Domestic
            && competition.kind == CompetitionType::League
            && let Some(country) = competition.country_id.as_deref()
        {
            simulated.insert(country);
        }
    }
    let nation_of: HashMap<&str, &str> = game
        .teams
        .iter()
        .map(|team| (team.id.as_str(), team.football_nation.as_str()))
        .collect();
    let association_of = |club: &str| nation_of.get(club).map(|nation| (*nation).to_string());
    let bonus_of = |competition_id: &str| {
        game.competitions
            .iter()
            .find(|competition| {
                competition.id == competition_id
                    && crate::league_phase::rules_of(competition).is_some()
            })
            .map(|competition| coefficient::stage_bonus_millis(competition.priority))
    };
    let records = coefficient::season_records(&coefficient::european_editions(game), &bonus_of);
    let best: Vec<String> = coefficient::association_ranking(&records, &association_of)
        .into_iter()
        .map(|(association, _)| association)
        .filter(|association| simulated.contains(association.as_str()))
        .take(places as usize)
        .collect();
    if !best.iter().any(|candidate| candidate == association) {
        return None;
    }
    standings_for_berth(game, source)
        .into_iter()
        .nth((position as usize).saturating_sub(1))
        .map(|entry| entry.team_id)
}

/// Teams a single competition's results award to `target` via its berths.
fn berth_winners(game: &Game, source: &League, target_id: &str) -> Vec<String> {
    source
        .berths
        .iter()
        .filter(|berth| berth.target == target_id)
        .flat_map(|berth| evaluate_berth_rule(game, source, &berth.rule))
        .collect()
}

/// Continental field from data-defined berths: collect every competition's berth
/// winners for this target, then apply the same reputation seeding, field cap,
/// and top-up as the inferred path so a thin field still fills its bracket.
pub fn berth_qualified_entrants(game: &Game, target: &League) -> Vec<String> {
    use std::collections::HashSet;

    let mut qualified: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for source in &game.competitions {
        for team_id in berth_winners(game, source, &target.id) {
            if seen.insert(team_id.clone()) {
                qualified.push(team_id);
            }
        }
    }
    seed_cap_and_fill(game, target, qualified, seen)
}

pub(super) struct BerthFieldOptions<S, E> {
    /// Which competitions may receive a field.
    pub(super) scope_match: S,
    /// Which of a source's berths award places on this path.
    pub(super) berth_eligible: E,
    /// Seed by reputation, cap to the field size, and top up a thin field.
    pub(super) fill: bool,
    /// Whether a berth's `fallbackTo` also offers the club a place. Continental
    /// qualification cascades this way; domestic promotion does not, because a
    /// club that misses its target simply stays in its own league.
    pub(super) follow_fallback: bool,
}

/// Resolve every berth-fed field whose target matches `options.scope_match`,
/// honouring cross-target exclusivity: a club ends in the single most
/// prestigious matching target (lowest priority) it earns. When
/// `options.follow_fallback` is set, a berth's `fallbackTo` offers the club a
/// further target on the same terms. Each field is emitted in evaluation order
/// (source list, then that source's berths, then standings) so an
/// oversubscribed domestic cap keeps a stable prefix rather than HashMap
/// iteration. Returns `target_id -> field`; targets without incoming berths
/// are absent.
pub(super) fn resolve_berth_fields<S, E>(
    game: &Game,
    options: BerthFieldOptions<S, E>,
) -> std::collections::HashMap<String, Vec<String>>
where
    S: Fn(&League) -> bool,
    E: Fn(&League, &Berth) -> bool,
{
    use std::collections::{HashMap, HashSet};

    // Matching berth-fed targets, most prestigious (lowest priority) first.
    let mut targets: Vec<&League> = game
        .competitions
        .iter()
        .filter(|competition| {
            (options.scope_match)(competition)
                && competition_has_incoming_berths(game, &competition.id)
        })
        .collect();
    targets.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    let priority_of: HashMap<&str, u32> = targets
        .iter()
        .map(|c| (c.id.as_str(), c.priority))
        .collect();

    // Each club keeps the most prestigious target any of its berths award it,
    // whether it reached that target as a primary or as a fallback.
    let mut best: HashMap<String, (u32, String)> = HashMap::new();
    let mut consider = |club: &str, target: &str| {
        if let Some(&prio) = priority_of.get(target) {
            let slot = best
                .entry(club.to_string())
                .or_insert((u32::MAX, String::new()));
            if prio < slot.0 {
                *slot = (prio, target.to_string());
            }
        }
    };
    for source in &game.competitions {
        for berth in &source.berths {
            if !(options.berth_eligible)(source, berth) {
                continue;
            }
            for winner in evaluate_berth_rule(game, source, &berth.rule) {
                consider(&winner, &berth.target);
                if options.follow_fallback
                    && let Some(fallback) = &berth.fallback_to
                {
                    consider(&winner, fallback);
                }
            }
        }
    }

    // Every club placed in any target — excluded from all targets' reputation
    // top-up so a thin field never pulls in a club already qualified elsewhere.
    let all_placed: HashSet<String> = best.keys().cloned().collect();
    // Replay evaluation order so each field's vec is stable: competitions,
    // then authored berths, then standings. HashMap iteration is not.
    let mut raw: HashMap<String, Vec<String>> = HashMap::new();
    let mut emitted: HashSet<String> = HashSet::new();
    for source in &game.competitions {
        for berth in &source.berths {
            if !(options.berth_eligible)(source, berth) {
                continue;
            }
            for winner in evaluate_berth_rule(game, source, &berth.rule) {
                if !emitted.insert(winner.clone()) {
                    continue;
                }
                if let Some((_, target)) = best.get(&winner) {
                    raw.entry(target.clone()).or_default().push(winner);
                }
            }
        }
    }

    // A titleholder holds its place by right: it is never the club an oversubscribed field cuts.
    let titleholders: HashSet<String> = game
        .competitions
        .iter()
        .flat_map(|source| {
            source
                .berths
                .iter()
                .filter(|berth| berth.rule == BerthRule::Titleholder)
                .flat_map(move |berth| evaluate_berth_rule(game, source, &berth.rule))
        })
        .collect();

    let mut fields = HashMap::new();
    let mut cut_from_above: Vec<String> = Vec::new();
    for target in &targets {
        let mut qualified = raw.remove(&target.id).unwrap_or_default();
        // The clubs the competition above had no room for are offered this one.
        for club in std::mem::take(&mut cut_from_above) {
            if !qualified.contains(&club) {
                qualified.push(club);
            }
        }
        let field = if options.fill {
            let (field, cut) = seed_cap_and_fill_keeping(
                game,
                target,
                qualified,
                all_placed.clone(),
                &titleholders,
            );
            cut_from_above = cut;
            field
        } else {
            qualified
        };
        fields.insert(target.id.clone(), field);
    }
    fields
}

/// Resolve every berth-fed continental field at once, honouring cross-target
/// exclusivity and the `fallbackTo` cascade: a club ends in the single most
/// prestigious target (lowest priority) it earns, and a berth's `fallbackTo`
/// is a lower-preference target used when the club doesn't earn the primary.
/// Returns `target_id -> field`; targets without incoming berths are absent
/// (the caller keeps the inferred path for those).
pub fn resolve_continental_fields(game: &Game) -> std::collections::HashMap<String, Vec<String>> {
    resolve_berth_fields(
        game,
        BerthFieldOptions {
            scope_match: |competition: &League| competition.scope == CompetitionScope::Continental,
            berth_eligible: |_: &League, _: &Berth| true,
            fill: true,
            follow_fallback: true,
        },
    )
}

/// Shared tail for both qualification paths: seed by reputation, cap to the
/// target's field size, and top up a thin field from the feeder regions.
fn seed_cap_and_fill(
    game: &Game,
    competition: &League,
    qualified: Vec<String>,
    seen: std::collections::HashSet<String>,
) -> Vec<String> {
    seed_cap_and_fill_keeping(
        game,
        competition,
        qualified,
        seen,
        &std::collections::HashSet::new(),
    )
    .0
}

/// [`seed_cap_and_fill`] for a field that may be oversubscribed: the clubs in `protected` (a
/// titleholder, who holds its place by right) are never the ones cut, and the clubs that were cut
/// are returned beside the field so the caller can offer them the next competition down.
fn seed_cap_and_fill_keeping(
    game: &Game,
    competition: &League,
    mut qualified: Vec<String>,
    seen: std::collections::HashSet<String>,
    protected: &std::collections::HashSet<String>,
) -> (Vec<String>, Vec<String>) {
    let field_size = competition.participant_ids.len().max(4);
    let feeder_regions = feeder_regions(game, competition);

    let reputation = |id: &str| {
        game.teams
            .iter()
            .find(|team| team.id == id)
            .map(|team| team.reputation)
            .unwrap_or(0)
    };
    qualified.sort_by(|a, b| {
        protected
            .contains(b)
            .cmp(&protected.contains(a))
            .then_with(|| reputation(b).cmp(&reputation(a)))
            .then_with(|| a.cmp(b))
    });
    let cut = qualified.split_off(qualified.len().min(field_size));

    if qualified.len() < field_size {
        let mut fillers: Vec<_> = game
            .teams
            .iter()
            .filter(|team| !seen.contains(&team.id))
            .filter(|team| {
                feeder_regions.contains(game.region_for_country(&team.football_nation).as_str())
            })
            .collect();
        fillers.sort_by(|a, b| {
            b.reputation
                .cmp(&a.reputation)
                .then_with(|| a.id.cmp(&b.id))
        });
        for team in fillers {
            if qualified.len() >= field_size {
                break;
            }
            qualified.push(team.id.clone());
        }
    }

    (qualified, cut)
}
