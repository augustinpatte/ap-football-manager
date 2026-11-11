use super::*;
use crate::turn::matchday_lookups::MatchdayLookups;
use domain::player::{Player, PlayerAttributes, Position as DomainPos};

/// Uniform attributes: `weighted_score` averages attributes, so setting them
/// all to `v` makes the condition-free positional fit ≈ `v` for any slot, with
/// the per-slot compatibility/foot penalty identical across players (same
/// position + footedness) — so it cancels in within-slot comparisons.
pub(super) fn attrs(v: u8) -> PlayerAttributes {
    PlayerAttributes {
        pace: v,
        stamina: v,
        strength: v,
        agility: v,
        passing: v,
        shooting: v,
        tackling: v,
        dribbling: v,
        defending: v,
        positioning: v,
        vision: v,
        decisions: v,
        composure: v,
        aggression: v,
        teamwork: v,
        leadership: v,
        handling: v,
        reflexes: v,
        aerial: v,
    }
}

pub(super) fn mk(id: &str, attr: u8, condition: u8) -> Player {
    mk_pos(id, DomainPos::CenterBack, attr, condition)
}

pub(super) fn mk_pos(id: &str, position: DomainPos, attr: u8, condition: u8) -> Player {
    let mut p = Player::new(
        id.to_string(),
        id.to_string(),
        id.to_string(),
        "1998-01-01".to_string(),
        "GB".to_string(),
        position,
        attrs(attr),
    );
    p.condition = condition;
    p
}

#[test]
fn management_quality_maps_reputation_to_unit_range() {
    assert_eq!(management_quality(300), 0.0);
    assert_eq!(management_quality(900), 1.0);
    assert!((management_quality(600) - 0.5).abs() < 1e-9);
    assert_eq!(management_quality(100), 0.0); // clamped below
    assert_eq!(management_quality(1200), 1.0); // clamped above
}

/// The discriminating test: an elite club must NOT bench a strong (but mildly
/// tired) starter for a much weaker fresh player. Better clubs field better
/// teams — the gap-aware tolerance enforces this.
#[test]
fn elite_club_keeps_strong_starters_over_fresh_scrubs() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 70)); // strong, mildly tired
    }
    for i in 0..3 {
        squad.push(mk(&format!("weak{i}"), 50, 100)); // weak, fully fresh
    }
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(&refs, "4-4-2", 1.0, 1, FixtureLoad::Normal);

    assert_eq!(xi.len(), 11);
    assert!(
        xi.iter().all(|p| p.id.starts_with("star")),
        "elite club fielded a weak fresh player over a strong starter: {:?}",
        xi.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
}

/// An elite club rotates a tired starter for a *comparable* fresh deputy
/// (within the fit tolerance), but still leaves the much-weaker scrubs benched.
#[test]
fn elite_club_rotates_tired_starter_for_comparable_fresh_player() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 65)); // strong, tired
    }
    squad.push(mk("deputy", 72, 100)); // comparable, fresh
    for i in 0..2 {
        squad.push(mk(&format!("weak{i}"), 50, 100)); // scrub, fresh
    }
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(&refs, "4-4-2", 1.0, 1, FixtureLoad::Normal);

    assert_eq!(xi.len(), 11);
    assert!(
        xi.iter().any(|p| p.id == "deputy"),
        "comparable fresh deputy should rotate in for a tired star"
    );
    assert_eq!(
        xi.iter().filter(|p| p.id.starts_with("star")).count(),
        10,
        "exactly one tired star should be rested"
    );
    assert!(
        xi.iter().all(|p| !p.id.starts_with("weak")),
        "scrubs are too far below tolerance to be rotated in"
    );
}

/// Same squad as above, but a poorly-run club: it is slow to notice fatigue and
/// rides a *mildly* tired XI. This is the reputation gradient, and it survives —
/// what it is not allowed to do is ride an exhausted one, which is the test
/// below.
#[test]
fn low_reputation_club_rides_mildly_tired_starters() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 65));
    }
    squad.push(mk("deputy", 72, 100));
    for i in 0..2 {
        squad.push(mk(&format!("weak{i}"), 50, 100));
    }
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(300),
        1,
        FixtureLoad::Normal,
    ); // q = 0

    assert_eq!(xi.len(), 11);
    assert!(
        xi.iter().all(|p| p.id.starts_with("star")),
        "a low-reputation club should ride its mildly tired first XI, not rotate"
    );
}

/// The behaviour this slice exists to change. A manager may be slow, or a poor
/// judge of who the better player is; no manager knowingly sends out a player
/// who can barely run when a rested deputy is available. Under the old
/// `fit_tolerance = 12 × quality` the worst manager accepted a quality drop of
/// exactly zero, so it could never rotate at all.
#[test]
fn even_the_worst_manager_rests_an_exhausted_starter() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 22)); // first choice, spent
    }
    squad.push(mk("deputy", 66, 95)); // clearly worse, but able to play
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(300),
        1,
        FixtureLoad::Normal,
    ); // q = 0

    assert_eq!(xi.len(), 11);
    assert!(
        xi.iter().any(|p| p.id == "deputy"),
        "an exhausted XI must be broken up even by the worst manager: {:?}",
        xi.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
}

/// A universal floor is not a licence to field anybody. Even exhausted, a club
/// does not replace its best player with someone hopeless.
#[test]
fn an_exhausted_starter_is_not_replaced_by_a_hopeless_deputy() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 22));
    }
    squad.push(mk("hopeless", 30, 100));
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(300),
        1,
        FixtureLoad::Normal,
    );

    assert_eq!(xi.len(), 11);
    assert!(
        xi.iter().all(|p| p.id != "hopeless"),
        "a 30-rated player is not an adequate deputy at any fatigue level"
    );
}

/// Given a middling manager and a first eleven at 48 % condition (tired, but not yet "exhausted"),
/// when a deputy 13 points worse and fully fresh is available, then the deputy starts: the more
/// tired a starter is, the bigger the drop the manager accepts to rest him. A flat tolerance of
/// nine points let a star at 48 % kick off whenever the nearest fresh player was ten worse, which
/// is how 1-4 % of AI starters began a match under half fit (F5.0 §8).
#[test]
fn a_deeply_tired_star_is_rested_for_a_decent_deputy() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 48));
    }
    squad.push(mk("deputy", 67, 95));
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(600),
        1,
        FixtureLoad::Normal,
    );

    assert!(
        xi.iter().any(|p| p.id == "deputy"),
        "a star at 48 % should make way for a deputy 13 points worse: {:?}",
        xi.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
}

/// Given a first eleven at 30 % condition (a risk of injury, not merely of a poor game) and the only
/// deputy 25 points worse and 55 % fit, which is under the usual floor for a deputy
/// When the AI picks its side
/// Then the deputy starts: a starter this spent is too likely to be hurt, and a manager accepts a
/// bigger drop and a less fresh deputy to avoid it.
#[test]
fn a_starter_at_risk_of_injury_is_rested_for_a_deputy_who_is_not_fresh_either() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 30));
    }
    squad.push(mk("deputy", 55, 55));
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(600),
        1,
        FixtureLoad::Normal,
    );

    assert!(
        xi.iter().any(|p| p.id == "deputy"),
        "a star at 30 % should make way for a 55 % deputy: {:?}",
        xi.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
}

/// The boundary pair for the test above, one point either side of the band's edge (40 %): the same
/// deputy (55 %, under the usual floor of 60) is fielded for a star at 39 % and not for one at 41 %.
/// The star at 41 is tired but not at risk: it is the condition of the starter that opens the band,
/// not a manager who rotates everyone.
#[test]
fn the_injury_risk_band_opens_exactly_below_forty_percent() {
    let deputy_starts = |star_condition: u8| {
        let mut squad = Vec::new();
        for i in 0..11 {
            squad.push(mk(&format!("star{i}"), 80, star_condition));
        }
        squad.push(mk("deputy", 55, 55));
        let refs: Vec<&Player> = squad.iter().collect();
        let xi = ai_select_starting_xi(
            &refs,
            "4-4-2",
            management_quality(600),
            1,
            FixtureLoad::Normal,
        );
        xi.iter().any(|p| p.id == "deputy")
    };
    assert!(deputy_starts(39), "a star at 39 % is at risk of injury");
    assert!(!deputy_starts(41), "a star at 41 % is tired, not at risk");
}

/// The control for the test above: the same deputy and the same manager, but the stars are only a
/// little tired (58 %, just under the 62.5 this manager rests at). The graded part of the tolerance
/// is then nearly nothing and the stars play on. Without this the previous test would pass for a
/// manager who rotates everyone.
#[test]
fn a_slightly_tired_star_still_plays_against_the_same_deputy() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 58));
    }
    squad.push(mk("deputy", 67, 95));
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(600),
        1,
        FixtureLoad::Normal,
    );

    assert!(
        xi.iter().all(|p| p.id.starts_with("star")),
        "a star at 58 % is not worth a 13-point drop: {:?}",
        xi.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
}

/// However tired, a star is not replaced by someone more than twenty points worse: the graded
/// tolerance is capped. The elite manager is the one the cap binds on (his uncapped tolerance at
/// 46 % is 12 + 0.4 x 29 = 23.6), so a deputy 22 points worse is the discriminating case: an
/// uncapped tolerance would field him, the cap leaves the star in. Without the cap a manager's
/// concern for the match would be traded away for a rest.
#[test]
fn the_graded_tolerance_is_capped() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 46));
    }
    squad.push(mk("scrub", 58, 100)); // 22 points worse
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(900),
        1,
        FixtureLoad::Normal,
    );

    assert!(
        xi.iter().all(|p| p.id != "scrub"),
        "a deputy 22 points worse must not start: {:?}",
        xi.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
}

/// Quality is judgement, not willingness. Given two deputies of nearly equal
/// standard, an elite manager always identifies the better one; a poor manager
/// gets it wrong some of the time. Fixed seeds, so this is deterministic.
#[test]
fn a_poor_manager_misjudges_which_deputy_is_better() {
    let pick_deputy = |quality: f64, seed: u64| -> String {
        let mut squad = Vec::new();
        for i in 0..11 {
            squad.push(mk(&format!("star{i}"), 80, 22));
        }
        squad.push(mk("better", 70, 95));
        squad.push(mk("worse", 66, 95));
        let refs: Vec<&Player> = squad.iter().collect();
        let xi = ai_select_starting_xi(&refs, "4-4-2", quality, seed, FixtureLoad::Normal);
        xi.iter()
            .find(|p| !p.id.starts_with("star"))
            .map(|p| p.id.clone())
            .unwrap_or_default()
    };

    let seeds: Vec<u64> = (0..24).collect();
    let elite: Vec<String> = seeds.iter().map(|s| pick_deputy(1.0, *s)).collect();
    let poor: Vec<String> = seeds.iter().map(|s| pick_deputy(0.0, *s)).collect();

    // Stated first and deliberately: without it, a manager that never rotated
    // at all would satisfy "sometimes picks the worse one" by picking nobody,
    // which is how the old behaviour would have passed this test.
    assert!(
        poor.iter().all(|d| d == "better" || d == "worse"),
        "a poor manager must still name a deputy for an exhausted XI: {poor:?}"
    );
    assert!(
        elite.iter().all(|d| d == "better"),
        "an elite manager should always spot the better deputy: {elite:?}"
    );
    assert!(
        poor.iter().any(|d| d == "worse"),
        "a poor manager should sometimes pick the worse deputy; got it right every time"
    );
}

/// The seed is the club and the date, so one fixture always produces one XI
/// however many times the builder is asked for it.
#[test]
fn the_same_seed_always_names_the_same_side() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 22));
    }
    squad.push(mk("a", 70, 95));
    squad.push(mk("b", 69, 95));
    let refs: Vec<&Player> = squad.iter().collect();

    let first = ai_select_starting_xi(&refs, "4-4-2", 0.0, 7, FixtureLoad::Normal);
    let second = ai_select_starting_xi(&refs, "4-4-2", 0.0, 7, FixtureLoad::Normal);

    let ids = |xi: &Vec<&Player>| xi.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&first), ids(&second));
}

/// Regression: load-management rotation must not skew the formation's
/// position distribution. A tired XI plus one fresh midfielder must still
/// field 1 GK / 4 DEF / 4 MID / 2 FWD — never a defender short (which
/// rendered only 10 players on the pitch).
#[test]
fn rotation_preserves_formation_position_distribution() {
    use DomainPos::{
        CenterBack, CentralMidfielder, Forward, Goalkeeper, LeftBack, LeftMidfielder, RightBack,
        RightMidfielder, Striker,
    };
    let squad = vec![
        mk_pos("gk", Goalkeeper, 75, 65),
        mk_pos("d1", CenterBack, 75, 65),
        mk_pos("d2", CenterBack, 75, 65),
        mk_pos("d3", LeftBack, 75, 65),
        mk_pos("d4", RightBack, 75, 65),
        mk_pos("m1", CentralMidfielder, 75, 65),
        mk_pos("m2", CentralMidfielder, 75, 65),
        mk_pos("m3", LeftMidfielder, 75, 65),
        mk_pos("m4", RightMidfielder, 75, 65),
        mk_pos("f1", Striker, 75, 65),
        mk_pos("f2", Striker, 75, 65),
        // Fresh midfielder load management will want to bring in.
        mk_pos("m_fresh", CentralMidfielder, 75, 100),
    ];
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(&refs, "4-4-2", 1.0, 1, FixtureLoad::Normal); // elite: rotates eagerly

    assert_eq!(xi.len(), 11);
    let group_count = |group: DomainPos| {
        xi.iter()
            .filter(|p| p.position.to_group_position() == group)
            .count()
    };
    assert_eq!(group_count(Goalkeeper), 1, "exactly one keeper");
    assert_eq!(
        group_count(DomainPos::Defender),
        4,
        "must field four defenders"
    );
    assert_eq!(
        group_count(DomainPos::Midfielder),
        4,
        "must field four midfielders"
    );
    assert_eq!(group_count(Forward), 2, "must field two forwards");
}

/// With fewer available players than formation slots, the slot-aligned fill
/// leaves gaps; the function must fall back to a contiguous selection rather
/// than flatten()ing those gaps (which would shift starters into wrong slots).
#[test]
fn select_starting_xi_falls_back_when_fewer_players_than_slots() {
    let squad: Vec<Player> = (0..9)
        .map(|i| mk_pos(&format!("p{i}"), DomainPos::CenterBack, 70, 100))
        .collect();
    let refs: Vec<&Player> = squad.iter().collect();
    let saved: Vec<String> = (0..9).map(|i| format!("p{i}")).collect();

    let xi = select_starting_xi(&saved, &refs, "4-4-2");

    // Every available player fielded once, with no slot-misaligning gaps.
    assert_eq!(xi.len(), 9);
    let unique: HashSet<&String> = xi.iter().map(|p| &p.id).collect();
    assert_eq!(unique.len(), 9);
}

/// When a saved starter is unavailable the user XI compacts; the surviving
/// starters must keep their real saved slot (matching deployed_position /
/// the UI), not shift up into the vacated slot. Regression for an injured
/// keeper turning an outfielder into the engine's goalkeeper.
#[test]
fn user_team_starter_keeps_saved_slot_when_xi_compacts() {
    use crate::clock::GameClock;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::team::Team;

    let mut players: Vec<Player> = (1..=11)
        .map(|i| mk_pos(&format!("p{i}"), DomainPos::CenterBack, 70, 100))
        .collect();
    // p1 is a natural Forward saved into the LEFT-BACK slot (index 1).
    players[0] = mk_pos("p1", DomainPos::Forward, 70, 100);
    for player in players.iter_mut() {
        player.team_id = Some("user".to_string());
    }

    let mut team = Team::new(
        "user".to_string(),
        "User FC".to_string(),
        "USR".to_string(),
        "England".to_string(),
        "London".to_string(),
        "Ground".to_string(),
        25_000,
    );
    team.formation = "4-4-2".to_string();
    // Slot 0 (GK) references a player that no longer exists, so
    // select_starting_xi drops it and the survivors compact.
    team.starting_xi_ids = std::iter::once("ghost-gk".to_string())
        .chain((1..=10).map(|i| format!("p{i}")))
        .collect();

    let mut manager = Manager::new(
        "mgr".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("user".to_string());

    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
    let game = Game::new(clock, manager, vec![team], players, vec![], vec![]);

    let (team_data, _bench) = build_team_with_bench(&game, "user");

    let p1 = team_data
        .players
        .iter()
        .find(|p| p.id == "p1")
        .expect("p1 should be fielded");
    // Saved at the left-back slot -> simulated as a defender, never shoved
    // into the vacated goalkeeper slot.
    assert_eq!(p1.position, Position::Defender);
    assert_ne!(p1.position, Position::Goalkeeper);

    // The vacated goalkeeper slot must be refilled, so the XI still fields
    // exactly one keeper (otherwise the engine's goalkeeper rating collapses
    // to its empty-set fallback).
    let keepers = team_data
        .players
        .iter()
        .filter(|p| p.position == Position::Goalkeeper)
        .count();
    assert_eq!(keepers, 1, "the XI must still contain a goalkeeper");
}

// ---------------------------------------------------------------------------
// Load management on a congested run
// ---------------------------------------------------------------------------

/// The mirror of `low_reputation_club_rides_mildly_tired_starters`: the same
/// side, the same manager. On a normal week he rides a first eleven at 65 and
/// is right to — it has seven days to recover. With another match three or
/// four days away it will not recover in time, and even he rests one.
#[test]
fn on_a_congested_run_even_a_low_reputation_club_rests_a_tired_starter() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 65));
    }
    squad.push(mk("deputy", 72, 100));
    let refs: Vec<&Player> = squad.iter().collect();

    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(300),
        1,
        FixtureLoad::Congested,
    ); // q = 0

    assert!(
        xi.iter().any(|p| p.id == "deputy"),
        "with another match days away, a first eleven at 65 should not all start"
    );
}

/// Congestion makes a manager rest players sooner, not field anybody. A starter
/// who is tired but far from spent stays on rather than make way for a player
/// well below his standard.
#[test]
fn a_congested_run_does_not_make_a_manager_field_a_much_weaker_player() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 70));
    }
    squad.push(mk("reserve", 60, 100)); // twenty points short
    let refs: Vec<&Player> = squad.iter().collect();

    // A poor manager, because that is where the congested tolerance floor is
    // the whole tolerance: at elite quality it grows to the same 12 on any week.
    let xi = ai_select_starting_xi(
        &refs,
        "4-4-2",
        management_quality(300),
        1,
        FixtureLoad::Congested,
    ); // q = 0

    assert!(
        xi.iter().all(|p| p.id.starts_with("star")),
        "a reserve twenty points short is not rotation, it is a weaker team"
    );
}

/// A world where `club` has fixtures on the given days from the clock's today.
fn game_with_fixtures_on(days_from_today: &[i64]) -> Game {
    use crate::clock::GameClock;
    use chrono::{Duration, TimeZone, Utc};
    use domain::league::{Fixture, FixtureCompetition, FixtureStatus, League};
    use domain::manager::Manager;

    let today = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();
    let fixtures = days_from_today
        .iter()
        .map(|days| Fixture {
            id: format!("f{days}"),
            date: (today + Duration::days(*days))
                .format("%Y-%m-%d")
                .to_string(),
            home_team_id: "club".to_string(),
            away_team_id: "rival".to_string(),
            competition: FixtureCompetition::League,
            status: FixtureStatus::Scheduled,
            ..Default::default()
        })
        .collect();

    let manager = Manager::new(
        "mgr".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    let mut game = Game::new(
        GameClock::new(today),
        manager,
        vec![],
        vec![],
        vec![],
        vec![],
    );
    game.league = Some(League {
        id: "league".to_string(),
        fixtures,
        ..Default::default()
    });
    game
}

/// Midweek to weekend: another match three days after today's.
#[test]
fn a_match_three_days_after_this_one_is_a_congested_run() {
    let game = game_with_fixtures_on(&[0, 3]);
    assert_eq!(fixture_load(&game, "club"), FixtureLoad::Congested);
}

/// Weekend to weekend. Today's own match is still `Scheduled` at kick-off, which
/// is exactly why this must not count it — training's "two fixtures in the
/// coming week" would, and would call every ordinary week congested.
#[test]
fn a_match_a_week_after_this_one_is_a_normal_week() {
    let game = game_with_fixtures_on(&[0, 7]);
    assert_eq!(fixture_load(&game, "club"), FixtureLoad::Normal);
}

/// The edges of the window: tomorrow and four days on count; today's own match, yesterday's
/// and five days on do not. A fixture already played does not count wherever it falls.
#[test]
fn only_the_next_four_days_make_a_congested_run() {
    for (days, congested) in [(-1, false), (0, false), (1, true), (4, true), (5, false)] {
        let game = game_with_fixtures_on(&[days]);
        let expected = if congested {
            FixtureLoad::Congested
        } else {
            FixtureLoad::Normal
        };
        assert_eq!(
            fixture_load(&game, "club"),
            expected,
            "a match {days} days on"
        );
    }
    let mut played = game_with_fixtures_on(&[3]);
    played.league.as_mut().unwrap().fixtures[0].status = domain::league::FixtureStatus::Completed;
    assert_eq!(fixture_load(&played, "club"), FixtureLoad::Normal);
}

/// Given a world where two clubs play again in three days and a third does not, When every club's
/// load is read off the set found in one pass, Then each answer is the one the fixtures give.
#[test]
fn the_clubs_found_in_one_pass_are_the_clubs_the_fixtures_make_congested() {
    let game = game_with_fixtures_on(&[0, 3]);
    let found = MatchdayLookups::of(&game);

    for club in ["club", "rival", "bystander"] {
        assert_eq!(
            fixture_load_among(&game, club, Some(&found)),
            fixture_load(&game, club),
            "{club}"
        );
    }
    assert!(found.is_congested("club") && found.is_congested("rival"));
    assert!(!found.is_congested("bystander"));
}

/// The state a squad is actually picked in on a normal day advance: the
/// competition being played has been moved into the legacy `game.league` slot
/// for simulation, and its place in `game.competitions` holds an empty default.
/// The next round of that same competition lives only in `game.league`.
#[test]
fn a_competition_moved_out_for_simulation_still_counts() {
    let mut game = game_with_fixtures_on(&[0, 3]);
    game.competitions = vec![domain::league::League::default()];
    assert_eq!(fixture_load(&game, "club"), FixtureLoad::Congested);
}

/// A competition outside the active scope is resolved by a scoreline-only model
/// that charges nobody any condition, so its next fixture is no reason to rest
/// anyone for this one.
#[test]
fn a_fixture_in_a_dormant_competition_is_not_a_congested_run() {
    let mut game = game_with_fixtures_on(&[0]);
    let active = game.league.clone().expect("the active league");
    let mut dormant = game_with_fixtures_on(&[3])
        .league
        .expect("a league to borrow fixtures from");
    dormant.id = "dormant".to_string();
    game.active_competition_ids = vec![active.id.clone()];
    game.competitions = vec![active, dormant];

    assert_eq!(fixture_load(&game, "club"), FixtureLoad::Normal);
}

/// Ten fit outfielders and a goalkeeper in the treatment room — a club that
/// has to field a sore keeper or put a defender in goal.
pub(super) fn club_whose_only_keeper_is_injured(managed_by_the_user: bool) -> Game {
    use crate::clock::GameClock;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::Injury;
    use domain::team::Team;

    let mut keeper = mk_pos("keeper", DomainPos::Goalkeeper, 70, 100);
    keeper.injury = Some(Injury {
        name: "common.injuries.calfStrain".to_string(),
        days_remaining: 3,
    });
    let mut players = vec![keeper];
    let outfield = [
        (DomainPos::CenterBack, 4),
        (DomainPos::CentralMidfielder, 4),
        (DomainPos::Striker, 2),
    ];
    for (position, count) in outfield {
        for i in 0..count {
            players.push(mk_pos(
                &format!("{position:?}{i}"),
                position.clone(),
                70,
                100,
            ));
        }
    }
    for player in players.iter_mut() {
        player.team_id = Some("club".to_string());
    }

    let mut team = Team::new(
        "club".to_string(),
        "Club".to_string(),
        "CLB".to_string(),
        "England".to_string(),
        "London".to_string(),
        "Ground".to_string(),
        25_000,
    );
    team.formation = "4-4-2".to_string();

    let mut manager = Manager::new(
        "mgr".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire(
        if managed_by_the_user {
            "club"
        } else {
            "elsewhere"
        }
        .to_string(),
    );

    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
    Game::new(clock, manager, vec![team], players, vec![], vec![])
}

fn who_keeps_goal(game: &Game) -> Vec<String> {
    let (team_data, _bench) = build_team_with_bench(game, "club");
    team_data
        .players
        .iter()
        .filter(|p| p.position == Position::Goalkeeper)
        .map(|p| p.id.clone())
        .collect()
}

/// Called up from the treatment room to make up the numbers, the keeper goes in
/// goal — not into whichever slot happened to be left over, which in a 4-4-2 is
/// up front, with a centre-back in goal in his place.
#[test]
fn an_injured_keeper_called_up_by_an_ai_club_plays_in_goal() {
    let game = club_whose_only_keeper_is_injured(false);
    assert_eq!(who_keeps_goal(&game), vec!["keeper".to_string()]);
}

#[test]
fn an_injured_keeper_called_up_by_the_user_s_club_plays_in_goal() {
    let game = club_whose_only_keeper_is_injured(true);
    assert_eq!(who_keeps_goal(&game), vec!["keeper".to_string()]);
}

/// A role reaches the engine only where the position a player is actually
/// playing admits it — the rule the player's own role command already
/// enforces. Generated squads are coarse, and the coarse `Forward` bucket
/// admits `WideForward`; deployed at centre-forward in a 4-4-2 he would still
/// have played with the wide forward's attribute weighting.
#[test]
fn a_role_the_deployed_slot_does_not_admit_is_not_played() {
    use crate::clock::GameClock;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::team::{PlayerRole, Team};
    use engine::PlayerRole as EngineRole;

    let mut players = vec![mk_pos("keeper", DomainPos::Goalkeeper, 70, 100)];
    for (position, count) in [
        (DomainPos::CenterBack, 4),
        (DomainPos::CentralMidfielder, 4),
        (DomainPos::Forward, 2),
    ] {
        for i in 0..count {
            players.push(mk_pos(
                &format!("{position:?}{i}"),
                position.clone(),
                70,
                100,
            ));
        }
    }
    for player in players.iter_mut() {
        player.team_id = Some("club".to_string());
    }

    let mut team = Team::new(
        "club".to_string(),
        "Club".to_string(),
        "CLB".to_string(),
        "England".to_string(),
        "London".to_string(),
        "Ground".to_string(),
        25_000,
    );
    team.formation = "4-4-2".to_string();
    team.player_roles
        .insert("Forward0".to_string(), PlayerRole::WideForward);
    team.player_roles
        .insert("Forward1".to_string(), PlayerRole::Poacher);

    let mut manager = Manager::new(
        "mgr".to_string(),
        "Test".to_string(),
        "Manager".to_string(),
        "1980-01-01".to_string(),
        "England".to_string(),
    );
    manager.hire("elsewhere".to_string());
    let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
    let game = Game::new(clock, manager, vec![team], players, vec![], vec![]);

    let (team_data, _bench) = build_team_with_bench(&game, "club");
    let role_of = |id: &str| {
        team_data
            .players
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} should start"))
            .role
    };

    assert_eq!(
        role_of("Forward0"),
        EngineRole::Standard,
        "a wide forward's role was played at centre-forward"
    );
    assert_eq!(
        role_of("Forward1"),
        EngineRole::Poacher,
        "a role the striker's slot admits must still reach the engine"
    );
}

/// Given the domain's position groups and the roles each admits, When every role is bridged to
/// the engine, Then the engine files it under the same groups, and its role list misses none.
#[test]
fn the_engines_role_groups_match_the_domains() {
    use engine::Position as EnginePosition;

    let groups = [
        (DomainPos::Goalkeeper, EnginePosition::Goalkeeper),
        (DomainPos::Defender, EnginePosition::Defender),
        (DomainPos::Midfielder, EnginePosition::Midfielder),
        (DomainPos::Forward, EnginePosition::Forward),
    ];
    let mut bridged = Vec::new();
    for (domain_group, engine_group) in &groups {
        for role in domain_group.valid_roles() {
            let engine_role = domain_to_engine_role(role);
            if engine_role != EnginePlayerRole::Standard {
                assert!(
                    engine_role.position_groups().contains(engine_group),
                    "{role:?} is admitted by {domain_group:?} but not filed under it in the engine"
                );
            }
            bridged.push(engine_role);
        }
    }
    for engine_role in EnginePlayerRole::ALL {
        assert!(
            bridged.contains(&engine_role),
            "{engine_role:?} is in the engine's list but no domain position admits it"
        );
        for engine_group in engine_role.position_groups() {
            let (domain_group, _) = groups.iter().find(|(_, g)| g == engine_group).unwrap();
            let admitted = domain_group
                .valid_roles()
                .iter()
                .any(|role| domain_to_engine_role(role) == engine_role);
            assert!(
                admitted,
                "{engine_role:?} is filed under {engine_group:?} but the domain does not admit it there"
            );
        }
    }
    for engine_role in bridged {
        assert!(
            EnginePlayerRole::ALL.contains(&engine_role),
            "{engine_role:?} is missing from the engine's list of roles"
        );
    }
}

/// Given a club and the world's other players, When its team is built from the club's players
/// found once, Then team, bench and set-piece takers are those built by scanning the world.
#[test]
fn a_team_built_from_the_clubs_players_is_the_team_built_by_scanning() {
    for is_user in [true, false] {
        let game = club_whose_only_keeper_is_injured(is_user);
        let club = crate::match_day_eligibility::club_players(&game.players, "club");

        let from_club = build_team_with_bench_from(&game, "club", &club, None);
        let with_the_set =
            build_team_with_bench_from(&game, "club", &club, Some(&MatchdayLookups::of(&game)));
        assert_eq!(format!("{from_club:?}"), format!("{with_the_set:?}"));
        let by_scanning = build_team_with_bench(&game, "club");

        assert_eq!(format!("{from_club:?}"), format!("{by_scanning:?}"));
        let starters: Vec<String> = by_scanning.0.players.iter().map(|p| p.id.clone()).collect();
        let starting: Vec<&Player> = starters
            .iter()
            .filter_map(|id| club.iter().find(|p| &p.id == id).copied())
            .collect();
        assert_eq!(
            set_piece_takers_among(&starting),
            auto_select_set_pieces(&game, &starters)
        );
    }
}

// ---------------------------------------------------------------------------
// The first-choice eleven is the side that is best as a whole, not the one the slot walk reaches
// ---------------------------------------------------------------------------

mod first_choice_is_the_best_side {
    use super::*;
    use crate::player_rating::{formation_slots, positional_fit_for_assignment};
    use crate::squad_intelligence::test_squads::player as specialist;

    const FORMATION: &str = "4-2-3-1";

    /// One specialist of 70 for every slot of the formation, so that nothing else is in doubt.
    fn first_eleven() -> Vec<Player> {
        formation_slots(FORMATION)
            .into_iter()
            .enumerate()
            .map(|(slot, position)| specialist(&format!("starter{slot:02}"), position, 70, 27))
            .collect()
    }

    /// A striker of 88 who is also the best attacking midfielder in the squad, an attacking
    /// midfielder of 66, and a second striker of 62.
    fn trap() -> Vec<Player> {
        let mut squad: Vec<Player> = first_eleven()
            .into_iter()
            .filter(|player| {
                player.natural_position != DomainPos::Striker
                    && player.natural_position != DomainPos::AttackingMidfielder
            })
            .collect();
        squad.push(specialist("star", DomainPos::Striker, 88, 27));
        squad.push(specialist(
            "playmaker",
            DomainPos::AttackingMidfielder,
            66,
            27,
        ));
        squad.push(specialist("reserve", DomainPos::Striker, 62, 27));
        squad
    }

    fn side(squad: &[Player]) -> Vec<(&str, DomainPos)> {
        let refs: Vec<&Player> = squad.iter().collect();
        first_choice_eleven(&refs, FORMATION)
            .into_iter()
            .zip(formation_slots(FORMATION))
            .map(|(player, slot)| (player.id.as_str(), slot))
            .collect()
    }

    fn total(squad: &[Player], eleven: &[(&str, DomainPos)]) -> f64 {
        eleven
            .iter()
            .map(|(id, slot)| {
                let player = squad.iter().find(|p| p.id == *id).expect("a squad member");
                positional_fit_for_assignment(player, slot)
            })
            .sum()
    }

    /// Given a striker of 88 who is also the squad's best player at several earlier slots, and a
    /// weaker second striker
    /// When the first-choice eleven is picked for a 4-2-3-1 (the slot walk reaches left back long
    /// before the forward line, and spends him there)
    /// Then the striker plays up front and the attacking midfielder plays his own slot, because
    /// that is the side with the greater total.
    #[test]
    fn a_striker_is_not_spent_on_the_slot_that_is_filled_before_his_own() {
        let squad = trap();
        let eleven = side(&squad);
        let striker_slot = eleven
            .iter()
            .find(|(id, _)| *id == "star")
            .map(|(_, s)| s.clone());
        assert_eq!(striker_slot, Some(DomainPos::Striker), "{eleven:?}");
        assert!(
            eleven.iter().any(|(id, _)| *id == "playmaker"),
            "{eleven:?}"
        );
        assert!(!eleven.iter().any(|(id, _)| *id == "reserve"), "{eleven:?}");
    }

    /// Given the same squad
    /// When it is handed over in any storage order
    /// Then the same eleven is named in the same slots (ties never depend on the order stored).
    #[test]
    fn the_side_does_not_depend_on_the_order_the_squad_is_stored_in() {
        let mut squad = trap();
        // Two players rated identically in every slot, so the only thing left to decide between
        // them is their ids.
        let mut twin = squad
            .iter()
            .find(|player| player.id == "starter07")
            .expect("a starter")
            .clone();
        twin.id = "starter07b".to_string();
        squad.push(twin);
        let forward = side(&squad);
        let mut reversed = squad.clone();
        reversed.reverse();
        assert_eq!(forward, side(&reversed));
    }

    /// Given a keeper who rates far below every outfielder in goal, and a spare outfielder
    /// When the eleven is picked
    /// Then the natural keeper still plays in goal: a keeper is not an optimisation variable.
    #[test]
    fn a_natural_keeper_plays_in_goal_however_the_outfielders_rate_there() {
        let mut squad = first_eleven();
        let keeper = squad
            .iter_mut()
            .find(|player| player.natural_position == DomainPos::Goalkeeper)
            .expect("a keeper");
        keeper.attributes = attrs(5);
        let keeper_id = keeper.id.clone();
        squad.push(specialist("spare", DomainPos::DefensiveMidfielder, 70, 27));
        let eleven = side(&squad);
        assert_eq!(eleven[0].0, keeper_id);
    }

    /// Given the trap squad
    /// When the side is compared with every exchange of two slots' occupants
    /// Then none gives a greater total (a local-optimality check; the trap test is what fails on
    /// the slot walk).
    #[test]
    fn no_exchange_of_two_slots_improves_the_side() {
        let squad = trap();
        let eleven = side(&squad);
        let base = total(&squad, &eleven);
        for first in 0..eleven.len() {
            for second in first + 1..eleven.len() {
                let mut swapped = eleven.clone();
                let (a, b) = (swapped[first].0, swapped[second].0);
                swapped[first].0 = b;
                swapped[second].0 = a;
                assert!(
                    total(&squad, &swapped) <= base + 1e-9,
                    "swapping slots {first} and {second} improves the side"
                );
            }
        }
    }

    /// Given fewer healthy players than the formation has slots
    /// When the eleven is picked
    /// Then the players there are, in slot order, are all named: the short side is still a prefix
    /// the kick-off completes from the treatment room.
    #[test]
    fn a_short_squad_is_named_in_slot_order() {
        let squad: Vec<Player> = first_eleven().into_iter().take(9).collect();
        let refs: Vec<&Player> = squad.iter().collect();
        let eleven = first_choice_eleven(&refs, FORMATION);
        assert_eq!(eleven.len(), 9);
        // The slot walk names the best remaining player for slot 0, then slot 1, and so on: the
        // keeper first, then the left back.
        assert_eq!(eleven[0].id, "starter00");
        assert_eq!(eleven[1].id, "starter01");
    }
}

// ---------------------------------------------------------------------------
// Managers rotate differently
// ---------------------------------------------------------------------------

fn mildly_tired_stars_with_a_fresh_deputy() -> Vec<Player> {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 58));
    }
    squad.push(mk("deputy", 72, 100));
    squad
}

fn deputies_in(xi: &[&Player]) -> usize {
    xi.iter().filter(|player| player.id == "deputy").count()
}

/// Given the same club, the same mildly tired eleven and the same fresh deputy
/// When a manager who rotates readily and one who rarely does pick the side
/// Then the first rests a starter and the second rides the tired eleven.
#[test]
fn a_manager_who_rotates_readily_rests_a_player_that_one_who_rarely_does_keeps() {
    let squad = mildly_tired_stars_with_a_fresh_deputy();
    let refs: Vec<&Player> = squad.iter().collect();
    let pick = |rotation: f64| {
        deputies_in(&ai_select_starting_xi_by(
            &refs,
            "4-4-2",
            0.5,
            1,
            FixtureLoad::Normal,
            rotation,
            0.5,
        ))
    };
    assert_eq!(pick(1.0), 1, "the rotating manager rode a tired eleven");
    assert_eq!(
        pick(0.0),
        0,
        "the manager who rarely rotates rested a player"
    );
}

/// Given a mildly tired eleven either side of the neutral manager's rest line, 50 + 25 x quality
/// When the neutral manager picks the side at quality one half (a line of 62.5)
/// Then he rests a starter at 62 and rides one at 63: the line the club had before managers had an
/// identity, pinned to its number rather than compared with a function that delegates to it.
#[test]
fn the_neutral_manager_rests_a_player_at_exactly_the_old_line() {
    let at = |condition: u8| {
        let mut squad = Vec::new();
        for i in 0..11 {
            squad.push(mk(&format!("star{i}"), 80, condition));
        }
        squad.push(mk("deputy", 72, 100));
        squad
    };
    for neutral in [true, false] {
        let rested = |condition: u8| {
            let squad = at(condition);
            let refs: Vec<&Player> = squad.iter().collect();
            let xi = if neutral {
                ai_select_starting_xi(&refs, "4-4-2", 0.5, 1, FixtureLoad::Normal)
            } else {
                ai_select_starting_xi_by(&refs, "4-4-2", 0.5, 1, FixtureLoad::Normal, 0.5, 0.5)
            };
            deputies_in(&xi) == 1
        };
        assert!(
            rested(62),
            "a starter at 62 should be rested (neutral {neutral})"
        );
        assert!(
            !rested(63),
            "a starter at 63 should be ridden (neutral {neutral})"
        );
    }
}

/// Given a congested run, quality zero, and starters at 87 or 85 against a deputy at 100
/// When a manager who rarely rotates and the neutral manager pick the side
/// Then at 87 the first rides his starters (his line is 86, half the 8-point range under 90) and the
/// neutral manager rests them (90), and at 85 the first rests too: congestion halves the
/// disposition, it neither doubles it nor drops it.
#[test]
fn on_a_congested_run_a_disposition_moves_the_line_by_half_as_much() {
    let pick = |condition: u8, rotation: f64| {
        let mut squad = Vec::new();
        for i in 0..11 {
            squad.push(mk(&format!("star{i}"), 80, condition));
        }
        squad.push(mk("deputy", 72, 100));
        let refs: Vec<&Player> = squad.iter().collect();
        deputies_in(&ai_select_starting_xi_by(
            &refs,
            "4-4-2",
            0.0,
            1,
            FixtureLoad::Congested,
            rotation,
            0.5,
        ))
    };
    assert_eq!(pick(87, 0.0), 0);
    assert_eq!(pick(87, 0.5), 1);
    assert_eq!(pick(85, 0.0), 1);
}

/// Given one tired starter and two adequate fresh deputies, a veteran a little better and a
/// prospect a little worse with room to grow
/// When a manager who trusts youth, a sceptic and the neutral manager rest the starter
/// Then the believer gives the shirt to the prospect and the other two to the veteran, so trust
/// chooses among adequate deputies and never fields one who is not.
#[test]
fn a_manager_who_trusts_youth_gives_the_rested_starters_shirt_to_the_prospect() {
    let mut squad = Vec::new();
    for i in 0..10 {
        squad.push(mk(&format!("star{i}"), 80, 100));
    }
    squad.push(mk("tired-star", 80, 58));
    let mut veteran = mk("veteran", 74, 100);
    veteran.ovr = 74;
    veteran.potential = 74;
    let mut prospect = mk("prospect", 71, 100);
    prospect.ovr = 71;
    prospect.potential = 86;
    squad.push(veteran);
    squad.push(prospect);
    let refs: Vec<&Player> = squad.iter().collect();
    let deputy = |trust: f64| {
        // Elite quality and a manager who rotates readily: the choice is the only variable.
        let xi = ai_select_starting_xi_by(&refs, "4-4-2", 1.0, 1, FixtureLoad::Normal, 1.0, trust);
        xi.iter()
            .find(|player| player.id == "veteran" || player.id == "prospect")
            .map(|player| player.id.clone())
            .expect("the tired starter was rested")
    };
    assert_eq!(deputy(1.0), "prospect", "the believer chose the veteran");
    assert_eq!(
        deputy(0.5),
        "veteran",
        "the neutral manager chose the prospect"
    );
    assert_eq!(deputy(0.0), "veteran", "the sceptic chose the prospect");
}

mod wired_to_the_clubs_own_manager {
    use super::*;
    use crate::clock::GameClock;
    use crate::manager_identity::ManagerIdentity;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::team::Team;

    fn manager_like(wanted: impl Fn(&ManagerIdentity) -> bool) -> String {
        (0..)
            .map(|index| format!("manager-{index}"))
            .find(|id| wanted(&ManagerIdentity::of(id)))
            .expect("some manager fits")
    }

    /// An AI club of tired starters and fresh deputies, run by `manager_id`.
    fn tired_club(manager_id: Option<&str>) -> Game {
        let positions = [
            DomainPos::Goalkeeper,
            DomainPos::LeftBack,
            DomainPos::CenterBack,
            DomainPos::CenterBack,
            DomainPos::RightBack,
            DomainPos::LeftMidfielder,
            DomainPos::CentralMidfielder,
            DomainPos::CentralMidfielder,
            DomainPos::RightMidfielder,
            DomainPos::Striker,
            DomainPos::Striker,
        ];
        let mut players: Vec<Player> = positions
            .into_iter()
            .enumerate()
            .map(|(i, position)| mk_pos(&format!("star{i}"), position, 80, 56))
            .collect();
        for (id, position) in [
            ("dep-keeper", DomainPos::Goalkeeper),
            ("dep-defender", DomainPos::CenterBack),
            ("dep-midfielder", DomainPos::CentralMidfielder),
            ("dep-striker", DomainPos::Striker),
        ] {
            players.push(mk_pos(id, position, 72, 100));
        }
        for player in &mut players {
            player.team_id = Some("club".to_string());
        }
        let mut team = Team::new(
            "club".into(),
            "Club".into(),
            "CLB".into(),
            "England".into(),
            "London".into(),
            "Ground".into(),
            25_000,
        );
        team.formation = "4-4-2".to_string();
        team.manager_id = manager_id.map(str::to_string);
        let mut user = Manager::new(
            "user".into(),
            "Test".into(),
            "Manager".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        user.hire("elsewhere".to_string());
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap());
        Game::new(clock, user, vec![team], players, vec![], vec![])
    }

    fn deputies_fielded(game: &Game) -> usize {
        let (team_data, _) = build_team_with_bench(game, "club");
        team_data
            .players
            .iter()
            .filter(|player| player.id.starts_with("dep-"))
            .count()
    }

    /// The tired club with one tired striker and two adequate fresh strikers to replace him: a
    /// veteran, and a prospect with room to grow.
    fn club_with_a_prospect(manager_id: Option<&str>) -> Game {
        let mut game = tired_club(manager_id);
        // A club of the highest standing, so its manager reads his players as they are and the
        // only difference between two clubs is the manager's trust.
        game.teams[0].reputation = 900;
        game.players.retain(|player| !player.id.starts_with("dep-"));
        for player in &mut game.players {
            player.condition = if player.id == "star10" { 56 } else { 100 };
            player.ovr = 80;
            player.potential = 80;
        }
        let mut veteran = mk_pos("dep-veteran", DomainPos::Striker, 76, 100);
        veteran.ovr = 76;
        veteran.potential = 76;
        let mut prospect = mk_pos("dep-prospect", DomainPos::Striker, 72, 100);
        prospect.ovr = 72;
        prospect.potential = 88;
        for mut player in [veteran, prospect] {
            player.team_id = Some("club".to_string());
            game.players.push(player);
        }
        for (id, position) in [
            ("dep-keeper", DomainPos::Goalkeeper),
            ("dep-defender", DomainPos::CenterBack),
            ("dep-midfielder", DomainPos::CentralMidfielder),
        ] {
            let mut spare = mk_pos(id, position, 40, 100);
            spare.ovr = 40;
            spare.potential = 40;
            spare.team_id = Some("club".to_string());
            game.players.push(spare);
        }
        game
    }

    /// Given the same club and the same two fresh strikers behind a tired one, run by a manager
    /// who trusts youth, one who does not, and none
    /// When the AI picks its side
    /// Then only the believer fields the prospect: trust reaches the real selection path.
    #[test]
    fn the_ai_selection_reads_its_own_managers_trust_in_youth() {
        let believer = manager_like(|m| m.youth_trust > 0.9 && m.rotation > 0.5);
        let sceptic = manager_like(|m| m.youth_trust < 0.1 && m.rotation > 0.5);
        let fields_the_prospect = |game: &Game| {
            let (team_data, _) = build_team_with_bench(game, "club");
            team_data.players.iter().any(|p| p.id == "dep-prospect")
        };
        assert!(fields_the_prospect(&club_with_a_prospect(Some(&believer))));
        assert!(!fields_the_prospect(&club_with_a_prospect(Some(&sceptic))));
        assert!(!fields_the_prospect(&club_with_a_prospect(None)));
    }

    /// Given the same tired club run by a manager who rotates readily, one who rarely does, and
    /// none at all
    /// When the AI picks its side for a match
    /// Then the first rests players the second rides, and the club with no manager does what the
    /// neutral manager does: the disposition reaches the real selection path.
    #[test]
    fn the_ai_selection_reads_its_own_managers_rotation() {
        let eager = manager_like(|m| m.rotation > 0.9);
        let reluctant = manager_like(|m| m.rotation < 0.1);
        assert!(deputies_fielded(&tired_club(Some(&eager))) > 0);
        assert_eq!(deputies_fielded(&tired_club(Some(&reluctant))), 0);
        assert!(
            deputies_fielded(&tired_club(None)) > 0,
            "neutral rests a starter at 56"
        );
    }
}
/// Given a spent starter (the exhausted case that no manager rides)
/// When the least rotating manager picks the side
/// Then he rests him too: temperament shifts when a manager notices, never whether he does.
#[test]
fn even_a_manager_who_hates_rotating_rests_an_exhausted_starter() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 22));
    }
    squad.push(mk("deputy", 66, 95));
    let refs: Vec<&Player> = squad.iter().collect();
    let xi = ai_select_starting_xi_by(&refs, "4-4-2", 0.0, 1, FixtureLoad::Normal, 0.0, 0.5);
    assert_eq!(deputies_in(&xi), 1);
}

/// Given starters at 43 condition, below the exhausted line (45), and a fresh deputy
/// When the lowest-quality, least-rotating manager picks the side
/// Then he rests them: dispositions move when a manager notices fatigue, never past the point at
/// which a player is not fit to start (his threshold would otherwise be 42).
#[test]
fn a_manager_who_hates_rotating_still_rests_a_player_just_under_the_exhausted_line() {
    let mut squad = Vec::new();
    for i in 0..11 {
        squad.push(mk(&format!("star{i}"), 80, 43));
    }
    squad.push(mk("deputy", 72, 100));
    let refs: Vec<&Player> = squad.iter().collect();
    let xi = ai_select_starting_xi_by(&refs, "4-4-2", 0.0, 1, FixtureLoad::Normal, 0.0, 0.5);
    assert_eq!(deputies_in(&xi), 1);
}

/// Given a second goalkeeper rated above every outfielder in the squad
/// When the first-choice eleven is picked
/// Then he sits on the bench: away from goal a natural keeper is an emergency body, as the squad
/// model rates him, so the club is valued for the side it fields.
#[test]
fn a_spare_goalkeeper_is_not_fielded_outfield_over_an_outfielder() {
    let mut squad = Vec::new();
    squad.push(mk_pos("first-keeper", DomainPos::Goalkeeper, 91, 100));
    squad.push(mk_pos("spare-keeper", DomainPos::Goalkeeper, 90, 100));
    for (i, position) in [
        DomainPos::LeftBack,
        DomainPos::CenterBack,
        DomainPos::CenterBack,
        DomainPos::RightBack,
        DomainPos::LeftMidfielder,
        DomainPos::CentralMidfielder,
        DomainPos::CentralMidfielder,
        DomainPos::RightMidfielder,
        DomainPos::Striker,
        DomainPos::Striker,
    ]
    .into_iter()
    .enumerate()
    {
        squad.push(mk_pos(&format!("outfield{i}"), position, 40, 100));
    }
    let refs: Vec<&Player> = squad.iter().collect();
    let eleven = first_choice_eleven(&refs, "4-4-2");
    assert_eq!(eleven[0].id, "first-keeper");
    assert!(
        !eleven.iter().any(|p| p.id == "spare-keeper"),
        "the spare keeper was fielded outfield"
    );
}
