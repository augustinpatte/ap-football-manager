//! How the 2027/28 European fields follow from the season just played: domestic finishes, cup
//! winners, titleholders, European performance places and the places clubs of unsimulated leagues
//! keep. The berths below are the ones `fm24-adapter::europe` authors for the real world, in a
//! world small enough to read: three simulated associations (ENG, ES, FR), and external clubs whose
//! leagues are not simulated.

use std::collections::{BTreeSet, HashMap};

use chrono::{TimeZone, Utc};
use domain::edition_archive::CompletedEdition;
use domain::league::{
    Berth, BerthRule, CompetitionFormat, CompetitionScope, CompetitionType, Fixture,
    FixtureCompetition, FixtureStatus, KnockoutRoundState, League, MatchResult, StandingEntry,
};
use domain::league_phase::{CompetitionStage, LeaguePhaseState};
use domain::manager::Manager;
use domain::team::Team;
use ofm_core::clock::GameClock;
use ofm_core::end_of_season::resolve_continental_fields;
use ofm_core::game::Game;
use ofm_core::league_phase::profiles;

fn club(id: &str, nation: &str, reputation: u32) -> Team {
    let mut team = Team::new(
        id.to_string(),
        id.to_string(),
        id[..2.min(id.len())].to_string(),
        nation.to_string(),
        "City".to_string(),
        "Ground".to_string(),
        10_000,
    );
    team.football_nation = nation.to_string();
    team.reputation = reputation;
    team
}

fn standing(team: &str, rank: u32, of: u32) -> StandingEntry {
    let won = of - rank;
    StandingEntry {
        team_id: team.to_string(),
        played: of,
        won,
        drawn: 0,
        lost: of - won,
        goals_for: won * 2,
        goals_against: 0,
        points: won * 3,
    }
}

fn first_division(country: &str, key: &str, clubs: &[String]) -> League {
    let total = clubs.len() as u32;
    League {
        id: format!("{key}-1"),
        name: format!("{key}-1"),
        season: 2026,
        kind: CompetitionType::League,
        scope: CompetitionScope::Domestic,
        country_id: Some(country.to_string()),
        region_id: Some("europe".to_string()),
        participant_ids: clubs.to_vec(),
        standings: clubs
            .iter()
            .enumerate()
            .map(|(rank, club)| standing(club, rank as u32, total))
            .collect(),
        priority: 0,
        berths: access_berths(country),
        ..Default::default()
    }
}

fn berth(target: &str, rule: BerthRule) -> Berth {
    Berth {
        target: target.to_string(),
        rule,
        fallback_to: None,
    }
}

/// The adapter's modelled access list, for the three associations of this world.
fn access_berths(country: &str) -> Vec<Berth> {
    let (cl, eps, el, ecl) = match country {
        "FR" => ((1, 3), 4, (4, 5), (6, 6)),
        _ => ((1, 4), 5, (5, 6), (7, 7)),
    };
    let range = |(from, to): (u32, u32)| BerthRule::PositionRange { from, to };
    vec![
        berth("eu-ucl", range(cl)),
        berth(
            "eu-ucl",
            BerthRule::EuropeanPerformance {
                position: eps,
                places: 2,
            },
        ),
        berth("eu-uel", range(el)),
        berth("eu-uecl", range(ecl)),
    ]
}

fn cup(country: &str, winner: &str, loser: &str) -> League {
    let id = format!("{country}-cup");
    let final_id = format!("{id}-final");
    let fixture = Fixture {
        id: final_id.clone(),
        matchday: 1,
        date: "2026-05-15".to_string(),
        home_team_id: winner.to_string(),
        away_team_id: loser.to_string(),
        competition: FixtureCompetition::Cup,
        status: FixtureStatus::Completed,
        result: Some(MatchResult {
            home_goals: 2,
            away_goals: 1,
            home_scorers: vec![],
            away_scorers: vec![],
            report: None,
            home_penalties: None,
            away_penalties: None,
        }),
        ..Default::default()
    };
    let round_id = format!("{id}-round");
    League {
        id,
        name: country.to_string(),
        season: 2026,
        kind: CompetitionType::Cup,
        scope: CompetitionScope::Domestic,
        country_id: Some(country.to_string()),
        region_id: Some("europe".to_string()),
        fixtures: vec![fixture],
        knockout_rounds: vec![KnockoutRoundState {
            id: round_id,
            name: "Final".to_string(),
            fixture_ids: vec![final_id],
            bye_team_ids: vec![],
            completed: true,
        }],
        berths: vec![berth("eu-uel", BerthRule::CupWinner)],
        ..Default::default()
    }
}

fn european(id: &str, priority: u32, participants: Vec<String>) -> League {
    let (short, titleholder_target) = match id {
        "eu-ucl" => ("ucl", "eu-ucl"),
        "eu-uel" => ("uel", "eu-ucl"),
        _ => ("uecl", "eu-uel"),
    };
    let _ = short;
    League {
        id: id.to_string(),
        name: id.to_string(),
        season: 2026,
        kind: CompetitionType::ContinentalClub,
        scope: CompetitionScope::Continental,
        required_region_ids: vec!["europe".to_string()],
        priority,
        participant_ids: participants,
        rules: domain::league::CompetitionRules {
            format: CompetitionFormat::LeaguePhaseAndKnockout,
            league_phase: Some(match id {
                "eu-ucl" => profiles::champions_league_2026(),
                "eu-uel" => profiles::europa_league_2026(),
                _ => profiles::conference_league_2026(),
            }),
            ..Default::default()
        },
        league_phase: Some(LeaguePhaseState::default()),
        berths: vec![
            berth(id, BerthRule::Incumbent),
            berth(titleholder_target, BerthRule::Titleholder),
        ],
        ..Default::default()
    }
}

struct World {
    game: Game,
    /// Clubs of each country, best first.
    clubs: HashMap<&'static str, Vec<String>>,
}

impl World {
    /// Three countries of eight clubs, the 2026 European editions' participants (domestic clubs
    /// that qualified in 2025/26 and external clubs whose league is not simulated), and the 2026
    /// cups, won by the 8th club of each country.
    fn new() -> Self {
        let mut teams = Vec::new();
        let mut clubs: HashMap<&'static str, Vec<String>> = HashMap::new();
        for (country, key) in [("ENG", "en"), ("ES", "es"), ("FR", "fr")] {
            let ids: Vec<String> = (1..=8).map(|n| format!("{key}{n}")).collect();
            for (rank, id) in ids.iter().enumerate() {
                teams.push(club(id, country, 900 - rank as u32 * 10));
            }
            clubs.insert(country, ids);
        }
        // External clubs: 25 in the Champions League, 27 in the Europa League, 33 in the
        // Conference League (what is left of 36 after the domestic places). Reputation falls with the index so the last one is the weakest.
        let external = |prefix: &str, count: usize| -> Vec<String> {
            (0..count).map(|n| format!("{prefix}{n:02}")).collect()
        };
        let (cl_x, el_x, ecl_x) = (external("xc", 25), external("xe", 27), external("xk", 33));
        for (group, base) in [(&cl_x, 400), (&el_x, 300), (&ecl_x, 200)] {
            for (n, id) in group.iter().enumerate() {
                teams.push(club(id, "PT", base - n as u32));
            }
        }
        let clock = GameClock::new(Utc.with_ymd_and_hms(2027, 6, 1, 12, 0, 0).unwrap());
        let manager = Manager::new(
            "m".into(),
            "A".into(),
            "B".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        let mut game = Game::new(clock, manager, teams, vec![], vec![], vec![]);

        let mut competitions = Vec::new();
        for (country, key) in [("ENG", "en"), ("ES", "es"), ("FR", "fr")] {
            competitions.push(first_division(country, key, &clubs[country]));
            competitions.push(cup(country, &format!("{key}8"), &format!("{key}7")));
        }
        // The 2026 editions: this season's participants. Their fields are 36 each.
        let pad = |mut field: Vec<String>, externals: &[String]| {
            field.extend(externals.iter().cloned());
            field
        };
        let names = |ids: &[&str]| ids.iter().map(|id| (*id).to_string()).collect::<Vec<_>>();
        let cl = pad(
            names(&[
                "en1", "en2", "en3", "en4", "es1", "es2", "es3", "es4", "fr1", "fr2", "fr3",
            ]),
            &cl_x,
        );
        let el = pad(
            names(&[
                "en5", "en6", "en8", "es5", "es6", "es8", "fr4", "fr5", "fr8",
            ]),
            &el_x,
        );
        let ecl = pad(names(&["en7", "es7", "fr6"]), &ecl_x);
        assert_eq!((cl.len(), el.len(), ecl.len()), (36, 36, 36));
        competitions.push(european("eu-ucl", 1, cl));
        competitions.push(european("eu-uel", 2, el));
        competitions.push(european("eu-uecl", 3, ecl));
        game.competitions = competitions;
        Self { game, clubs }
    }

    fn fields(&self) -> HashMap<String, Vec<String>> {
        resolve_continental_fields(&self.game)
    }

    fn competition_mut(&mut self, id: &str) -> &mut League {
        self.game
            .competitions
            .iter_mut()
            .find(|c| c.id == id)
            .unwrap()
    }

    fn finish(&mut self, id: &str, champion: &str) {
        let state = self.competition_mut(id).league_phase.as_mut().unwrap();
        state.stage = CompetitionStage::Completed;
        state.champion_id = Some(champion.to_string());
    }
}

fn set(field: &[String]) -> BTreeSet<&str> {
    field.iter().map(String::as_str).collect()
}

fn assert_every_field_full_and_disjoint(fields: &HashMap<String, Vec<String>>) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for id in ["eu-ucl", "eu-uel", "eu-uecl"] {
        let field = &fields[id];
        assert_eq!(field.len(), 36, "{id} keeps its 36 places");
        assert_eq!(set(field).len(), 36, "{id} lists no club twice");
        for club in field {
            assert!(seen.insert(club), "{club} holds two European places");
        }
    }
}

/// Given the league tables and cups of 2025/26 and no European result yet
/// When the next fields are resolved
/// Then the domestic top finishers fill the Champions League (four from England and Spain, three
///      from France) beside the 25 clubs whose league is not simulated, the cup winner who is
///      outside every league place takes a Europa League place, and the Conference League is
///      completed by the strongest club left unplaced.
#[test]
fn domestic_finishes_and_cup_winners_decide_the_perimeter_places() {
    let world = World::new();
    let fields = world.fields();
    let eng = &world.clubs["ENG"];

    let cl = set(&fields["eu-ucl"]);
    for expected in [
        "en1", "en2", "en3", "en4", "es1", "es2", "es3", "es4", "fr1", "fr2", "fr3",
    ] {
        assert!(cl.contains(expected), "{expected} finished in a CL place");
    }
    assert!(
        !cl.contains("en5"),
        "no performance place before any European season"
    );
    assert_eq!(cl.iter().filter(|c| c.starts_with("xc")).count(), 25);

    let el = set(&fields["eu-uel"]);
    for expected in ["en5", "en6", "es5", "fr4", "fr5", "en8", "es8", "fr8"] {
        assert!(
            el.contains(expected),
            "{expected} earned a Europa League place"
        );
    }
    assert!(
        !el.contains(eng[6].as_str()),
        "the 7th goes to the Conference League"
    );
    assert_every_field_full_and_disjoint(&fields);
}

/// Given the Europa League's champion is a club of an unsimulated league, and the Conference
///       League's champion likewise
/// When the next fields are resolved
/// Then the Europa League champion is in the Champions League and no longer in the Europa League,
///      the Conference League champion moved up to the Europa League, and every field is still full
///      and disjoint (the club each displaced, and the vacated places, are settled by the machinery).
#[test]
fn a_european_titleholder_moves_up_one_competition() {
    let mut world = World::new();
    world.finish("eu-uel", "xe00");
    world.finish("eu-uecl", "xk00");
    let fields = world.fields();

    assert!(set(&fields["eu-ucl"]).contains("xe00"));
    assert!(!set(&fields["eu-uel"]).contains("xe00"));
    assert!(set(&fields["eu-uel"]).contains("xk00"));
    assert!(!set(&fields["eu-uecl"]).contains("xk00"));
    assert_every_field_full_and_disjoint(&fields);
}

/// Given a club that finishes in a Champions League place, wins its cup and wins the Europa League
/// When the next fields are resolved
/// Then it holds one place, in the Champions League, and every other competition is still full.
#[test]
fn a_club_with_three_routes_in_holds_one_place_in_the_top_competition() {
    let mut world = World::new();
    // en8 (the cup winner) also wins the Europa League; en1 is Champions League by league finish.
    world.finish("eu-uel", "en1");
    world.competition_mut("ENG-cup").fixtures[0].home_team_id = "en1".to_string();
    let fields = world.fields();

    let holders: Vec<&str> = ["eu-ucl", "eu-uel", "eu-uecl"]
        .into_iter()
        .filter(|id| set(&fields[*id]).contains("en1"))
        .collect();
    assert_eq!(holders, vec!["eu-ucl"]);
    assert_every_field_full_and_disjoint(&fields);
}

/// Given clubs whose league the world does not simulate
/// When the next fields are resolved with no European result moving anyone
/// Then each keeps its place in its own competition: nothing demotes a club for a league that was
///      never played.
#[test]
fn clubs_of_unsimulated_leagues_keep_their_places() {
    let world = World::new();
    let fields = world.fields();

    for (competition, prefix, count) in [
        ("eu-ucl", "xc", 25),
        ("eu-uel", "xe", 27),
        ("eu-uecl", "xk", 33),
    ] {
        let kept = set(&fields[competition])
            .iter()
            .filter(|club| club.starts_with(prefix))
            .count();
        assert_eq!(kept, count, "{competition} keeps its incumbents");
    }
}

/// Given a club promoted last season, with no European history, that finishes third
/// When the next fields are resolved
/// Then it qualifies like any other finisher: results, not history, decide.
#[test]
fn a_newly_promoted_club_qualifies_by_finishing_in_a_place() {
    let mut world = World::new();
    world.game.teams.push(club("newbie", "FR", 100));
    let league = world.competition_mut("fr-1");
    league.participant_ids.push("newbie".to_string());
    league.standings.insert(2, standing("newbie", 0, 9));
    // It is third in the table, ahead of the club listed third before.
    league.standings[2].points = 1_000;
    league
        .standings
        .sort_by_key(|entry| std::cmp::Reverse(entry.points));
    let fields = world.fields();

    assert!(set(&fields["eu-ucl"]).contains("newbie"));
    assert_every_field_full_and_disjoint(&fields);
}

fn edition_where(winners: &[(&str, &str)], season: u32) -> CompletedEdition {
    let fixtures: Vec<Fixture> = winners
        .iter()
        .enumerate()
        .map(|(n, (winner, loser))| Fixture {
            id: format!("m{season}-{n}"),
            matchday: 1,
            date: format!("{season}-10-01"),
            home_team_id: (*winner).to_string(),
            away_team_id: (*loser).to_string(),
            competition: FixtureCompetition::ContinentalClub,
            status: FixtureStatus::Completed,
            result: Some(MatchResult {
                home_goals: 2,
                away_goals: 0,
                home_scorers: vec![],
                away_scorers: vec![],
                report: None,
                home_penalties: None,
                away_penalties: None,
            }),
            ..Default::default()
        })
        .collect();
    CompletedEdition {
        competition_id: "eu-ucl".to_string(),
        season,
        completed_on: format!("{season}-06-01"),
        champion_id: winners[0].0.to_string(),
        participant_ids: winners
            .iter()
            .flat_map(|(a, b)| [(*a).to_string(), (*b).to_string()])
            .collect(),
        standings: vec![],
        groups: vec![],
        knockout_rounds: vec![],
        fixtures,
        movements: None,
        league_phase: Some(LeaguePhaseState::default()),
    }
}

/// Given a European season in which English and Spanish clubs won all their matches and the
///       French clubs lost theirs
/// When the next fields are resolved
/// Then the two best-performing associations (England and Spain) get a fifth Champions League
///      place and France, the worst, does not get its fourth.
#[test]
fn european_performance_places_go_to_the_best_performing_associations() {
    let mut world = World::new();
    world.game.edition_archive.push(edition_where(
        &[
            ("en1", "fr1"),
            ("en2", "fr2"),
            ("es1", "fr3"),
            ("es2", "fr1"),
        ],
        2026,
    ));
    let fields = world.fields();

    let cl = set(&fields["eu-ucl"]);
    assert!(cl.contains("en5"), "England's performance place");
    assert!(cl.contains("es5"), "Spain's performance place");
    assert!(!cl.contains("fr4"), "France did not perform");
    assert_every_field_full_and_disjoint(&fields);
}
