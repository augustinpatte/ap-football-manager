//! What a manager is told about their club's European competition: the draw, the league-phase
//! result, each knockout tie, the title, and the champion for everyone's front page.
//!
//! Every message goes through the sent-ledger (`inbox::emit_once`), keyed by the competition, the
//! edition and the event, so a day that runs twice, a reload or a competition renewed under the
//! same id never repeats one. The text is translation keys with plain parameters (names, scores,
//! dates, a position): nothing a player reads is English prose built here.

use crate::game::Game;
use crate::inbox::emit_once;
use domain::league::League;
use domain::league_phase::{CompetitionStage, KnockoutTie, LeaguePhaseState};
use domain::message::{InboxMessage, MessageCategory, MessagePriority};
use domain::news::{NewsArticle, NewsCategory};
use std::collections::HashMap;

fn message(
    key: &str,
    today: &str,
    subject: &str,
    body: &str,
    priority: MessagePriority,
    params: HashMap<String, String>,
) -> InboxMessage {
    InboxMessage::new(
        key.to_string(),
        String::new(),
        String::new(),
        String::new(),
        today.to_string(),
    )
    .with_category(MessageCategory::LeagueInfo)
    .with_priority(priority)
    .with_sender_role("")
    .with_i18n(subject, body, params)
    .with_sender_i18n("be.sender.leagueOffice", "be.role.competitionSecretary")
}

/// The parameters that name a competition: its stored name and, when it has one, its translation
/// key, which the frontend resolves so a manager reads the name in their own language.
fn competition_params(competition: &League) -> Vec<(&'static str, String)> {
    let mut entries = vec![("competition", competition.name.clone())];
    if let Some(key) = &competition.name_key {
        entries.push(("competitionKey", key.clone()));
    }
    entries
}

fn params(entries: &[(&str, String)]) -> HashMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

/// The score of a tie from `club`'s side: `3-2`, with the shootout in brackets when one decided
/// it (`2-2 (4-3)`).
fn result_for(tie: &KnockoutTie, club: &str) -> Option<String> {
    let resolution = tie.resolution.as_ref()?;
    let own_side_is_seeded = tie.seeded_team_id == club;
    let (own, other) = if own_side_is_seeded {
        (resolution.seeded_goals, resolution.unseeded_goals)
    } else {
        (resolution.unseeded_goals, resolution.seeded_goals)
    };
    Some(match resolution.penalties {
        Some((seeded, unseeded)) => {
            let (own_pens, other_pens) = if own_side_is_seeded {
                (seeded, unseeded)
            } else {
                (unseeded, seeded)
            };
            format!("{own}-{other} ({own_pens}-{other_pens})")
        }
        None => format!("{own}-{other}"),
    })
}

fn first_match_of(game: &Game, competition: &League, club: &str) -> Option<(String, String)> {
    let state = competition.league_phase.as_ref()?;
    let knockout = state.knockout_fixture_ids();
    let fixture = competition
        .fixtures
        .iter()
        .filter(|fixture| !knockout.contains(fixture.id.as_str()))
        .filter(|fixture| fixture.home_team_id == club || fixture.away_team_id == club)
        .min_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)))?;
    let opponent = if fixture.home_team_id == club {
        &fixture.away_team_id
    } else {
        &fixture.home_team_id
    };
    Some((game.team_name_or_id(opponent), fixture.date.clone()))
}

fn announce_draw(game: &mut Game, today: &str, competition: &League, club: &str) {
    let Some(state) = &competition.league_phase else {
        return;
    };
    if state.stage == CompetitionStage::Created || state.draw_error.is_some() {
        return;
    }
    let Some((opponent, date)) = first_match_of(game, competition, club) else {
        return;
    };
    let key = format!("european_draw_{}_{}", competition.id, competition.season);
    let matches = competition
        .rules
        .league_phase
        .as_ref()
        .map_or(0, |rules| rules.matches_per_club());
    let mut named = competition_params(competition);
    named.extend([
        ("matches", matches.to_string()),
        ("opponent", opponent),
        ("date", date),
    ]);
    let entries = params(&named);
    emit_once(game, &key, || {
        message(
            &key,
            today,
            "be.msg.europeanDraw.subject",
            "be.msg.europeanDraw.body",
            MessagePriority::High,
            entries,
        )
    });
}

fn announce_league_phase(game: &mut Game, today: &str, competition: &League, club: &str) {
    let Some(outcome) = competition
        .league_phase
        .as_ref()
        .and_then(|state| state.outcome.as_ref())
    else {
        return;
    };
    let Some(position) = outcome.ranking.iter().position(|id| id == club) else {
        return;
    };
    let variant = if outcome.direct.iter().any(|id| id == club) {
        "Direct"
    } else if outcome.playoff.iter().any(|id| id == club) {
        "Playoff"
    } else {
        "Eliminated"
    };
    let key = format!(
        "european_league_phase_{}_{}",
        competition.id, competition.season
    );
    let mut named = competition_params(competition);
    named.push(("rank", (position + 1).to_string()));
    let entries = params(&named);
    let subject = format!("be.msg.europeanLeaguePhase{variant}.subject");
    let body = format!("be.msg.europeanLeaguePhase{variant}.body");
    emit_once(game, &key, || {
        message(&key, today, &subject, &body, MessagePriority::High, entries)
    });
}

fn announce_ties(game: &mut Game, today: &str, competition: &League, club: &str) {
    let Some(state) = &competition.league_phase else {
        return;
    };
    // The final is told as the title (or its loss); every other tie by its result.
    for tie in state
        .ties
        .iter()
        .filter(|tie| tie.seeded_team_id == club || tie.unseeded_team_id == club)
        .filter(|tie| tie.stage != domain::league_phase::KnockoutStage::Final)
    {
        let Some(winner) = tie.winner_id() else {
            continue;
        };
        let Some(result) = result_for(tie, club) else {
            continue;
        };
        let opponent_id = if tie.seeded_team_id == club {
            &tie.unseeded_team_id
        } else {
            &tie.seeded_team_id
        };
        let won = winner == club;
        let variant = if won { "Won" } else { "Lost" };
        let key = format!(
            "european_tie_{}_{}_{}",
            competition.id, competition.season, tie.id
        );
        let mut named = competition_params(competition);
        named.extend([
            ("opponent", game.team_name_or_id(opponent_id)),
            ("result", result),
        ]);
        let entries = params(&named);
        let subject = format!("be.msg.europeanTie{variant}.subject");
        let body = format!("be.msg.europeanTie{variant}.body");
        emit_once(game, &key, || {
            message(&key, today, &subject, &body, MessagePriority::High, entries)
        });
    }
}

fn announce_champion(game: &mut Game, today: &str, competition: &League, state: &LeaguePhaseState) {
    let (CompetitionStage::Completed, Some(champion)) = (state.stage, state.champion_id.as_deref())
    else {
        return;
    };
    let champion_name = game.team_name_or_id(champion);
    let mut named = competition_params(competition);
    named.push(("club", champion_name));
    let entries = params(&named);
    if game.manager.team_id.as_deref() == Some(champion) {
        let key = format!("european_title_{}_{}", competition.id, competition.season);
        let entries = entries.clone();
        emit_once(game, &key, || {
            message(
                &key,
                today,
                "be.msg.europeanChampion.subject",
                "be.msg.europeanChampion.body",
                MessagePriority::High,
                entries,
            )
        });
    }
    // The front page, for everyone, once per edition.
    let news_id = format!(
        "european_champion_news_{}_{}",
        competition.id, competition.season
    );
    if !game.news.iter().any(|article| article.id == news_id) {
        game.news.push(
            NewsArticle::new(
                news_id,
                String::new(),
                String::new(),
                String::new(),
                today.to_string(),
                NewsCategory::Editorial,
            )
            .with_i18n(
                "be.news.europeanChampion.headline",
                "be.news.europeanChampion.body",
                "be.source.sportsGazette",
                entries,
            ),
        );
    }
}

/// Tells the manager what their club's European competition did today, and the world who won.
pub fn announce(game: &mut Game, today: &str) {
    let club = game.manager.team_id.clone();
    for index in 0..game.competitions.len() {
        let competition = &game.competitions[index];
        let Some(state) = competition
            .league_phase
            .as_ref()
            .filter(|state| state.stage != CompetitionStage::Created)
        else {
            continue;
        };
        let managed = club
            .as_deref()
            .filter(|club| competition.participant_ids.iter().any(|id| id == club))
            .map(str::to_string);
        let news_id = format!(
            "european_champion_news_{}_{}",
            competition.id, competition.season
        );
        let champion_to_tell = state.stage == CompetitionStage::Completed
            && !game.news.iter().any(|article| article.id == news_id);
        // Only a competition with something to say is copied: the borrow of the game for
        // `emit_once` forbids reading it in place, and a league phase carries its fixtures.
        if managed.is_none() && !champion_to_tell {
            continue;
        }
        let competition = competition.clone();
        let state = state.clone();
        if let Some(club) = managed {
            announce_draw(game, today, &competition, &club);
            announce_league_phase(game, today, &competition, &club);
            announce_ties(game, today, &competition, &club);
        }
        announce_champion(game, today, &competition, &state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::league_phase::profiles;
    use crate::league_phase::tests::{clubs, drawn, play_everything};
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::team::Team;

    fn game_with(league: League, manager_club: &str) -> Game {
        let teams: Vec<Team> = clubs()
            .iter()
            .map(|id| {
                Team::new(
                    id.clone(),
                    format!("Name of {id}"),
                    id.clone(),
                    "ENG".into(),
                    "City".into(),
                    "Ground".into(),
                    1,
                )
            })
            .collect();
        let mut manager = Manager::new(
            "m".into(),
            "A".into(),
            "B".into(),
            "1980-01-01".into(),
            "ENG".into(),
        );
        manager.hire(manager_club.to_string());
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap());
        let mut game = Game::new(clock, manager, teams, vec![], vec![], vec![]);
        game.competitions = vec![league];
        game
    }

    fn ids(game: &Game, prefix: &str) -> Vec<String> {
        game.messages
            .iter()
            .filter(|m| m.id.starts_with(prefix))
            .map(|m| m.id.clone())
            .collect()
    }

    /// Given a drawn edition and the manager's club in it
    /// When the day is announced, twice
    /// Then one draw message names the club's first opponent and date, once.
    #[test]
    fn the_draw_is_announced_once_with_the_first_match() {
        let league = drawn(profiles::champions_league_2026(), 1);
        let first = league
            .fixtures
            .iter()
            .filter(|f| f.home_team_id == "club-05" || f.away_team_id == "club-05")
            .min_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)))
            .unwrap()
            .clone();
        let mut game = game_with(league, "club-05");

        announce(&mut game, "2026-06-02");
        announce(&mut game, "2026-06-03");

        let draw = ids(&game, "european_draw_");
        assert_eq!(draw.len(), 1, "{draw:?}");
        let message = game.messages.iter().find(|m| m.id == draw[0]).unwrap();
        assert_eq!(
            message.subject_key.as_deref(),
            Some("be.msg.europeanDraw.subject")
        );
        assert_eq!(message.i18n_params["date"], first.date);
        let opponent = if first.home_team_id == "club-05" {
            &first.away_team_id
        } else {
            &first.home_team_id
        };
        assert_eq!(
            message.i18n_params["opponent"],
            format!("Name of {opponent}")
        );
        assert!(
            ids(&game, "european_league_phase_").is_empty(),
            "nothing is said about a league phase not yet played"
        );
    }

    /// Given a played edition and the champion as the manager
    /// When the days are announced, twice
    /// Then the manager hears of the league-phase result, each tie and the title exactly once, and
    ///      the world gets one front-page story naming the champion.
    #[test]
    fn a_played_edition_is_told_once_per_event() {
        let mut league = drawn(profiles::champions_league_2026(), 1);
        play_everything(&mut league);
        let state = league.league_phase.clone().unwrap();
        let champion = state.champion_id.clone().unwrap();
        let ties: usize = state
            .ties
            .iter()
            .filter(|tie| tie.seeded_team_id == champion || tie.unseeded_team_id == champion)
            // The final is told as the title, not as a tie.
            .filter(|tie| tie.stage != domain::league_phase::KnockoutStage::Final)
            .count();
        let mut game = game_with(league, &champion);

        announce(&mut game, "2027-06-06");
        announce(&mut game, "2027-06-07");

        assert_eq!(ids(&game, "european_league_phase_").len(), 1);
        assert_eq!(ids(&game, "european_tie_").len(), ties);
        assert!(
            ties >= 2,
            "a champion plays at least two ties before the final: {ties}"
        );
        assert_eq!(ids(&game, "european_title_").len(), 1);
        let won: Vec<_> = game
            .messages
            .iter()
            .filter(|m| m.id.starts_with("european_tie_"))
            .map(|m| m.subject_key.clone().unwrap())
            .collect();
        assert!(
            won.iter().all(|key| key == "be.msg.europeanTieWon.subject"),
            "{won:?}"
        );
        let news: Vec<_> = game
            .news
            .iter()
            .filter(|article| article.id.starts_with("european_champion_news_"))
            .collect();
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].i18n_params["club"], format!("Name of {champion}"));
    }

    /// Given a played edition and a manager whose club was knocked out
    /// When the days are announced
    /// Then the lost tie is told as a loss with the opponent and the result from the club's side,
    ///      and no title message is sent to them.
    #[test]
    fn a_knocked_out_club_hears_of_the_loss_and_not_of_a_title() {
        let mut league = drawn(profiles::champions_league_2026(), 1);
        play_everything(&mut league);
        let state = league.league_phase.clone().unwrap();
        let loser_tie = state
            .ties
            .iter()
            .find(|tie| tie.stage == domain::league_phase::KnockoutStage::QuarterFinals)
            .unwrap();
        let loser = if loser_tie.winner_id() == Some(loser_tie.seeded_team_id.as_str()) {
            loser_tie.unseeded_team_id.clone()
        } else {
            loser_tie.seeded_team_id.clone()
        };
        let mut game = game_with(league, &loser);

        announce(&mut game, "2027-06-06");

        let lost: Vec<_> = game
            .messages
            .iter()
            .filter(|m| m.subject_key.as_deref() == Some("be.msg.europeanTieLost.subject"))
            .collect();
        assert_eq!(lost.len(), 1);
        assert!(lost[0].i18n_params["result"].contains('-'));
        assert!(ids(&game, "european_title_").is_empty());
    }

    fn tie_result(seeded: u16, unseeded: u16, penalties: Option<(u8, u8)>) -> KnockoutTie {
        KnockoutTie {
            id: "t".into(),
            stage: domain::league_phase::KnockoutStage::QuarterFinals,
            seeded_team_id: "s".into(),
            unseeded_team_id: "u".into(),
            resolution: Some(domain::league_phase::TieResolution {
                winner_id: "s".into(),
                seeded_goals: seeded,
                unseeded_goals: unseeded,
                penalties,
                decision: domain::league_phase::TieDecision::Aggregate,
            }),
            ..Default::default()
        }
    }

    /// Given a tie won 3-1 by the seeded club, and one level 2-2 and won by the seeded club on
    ///       penalties 5-4
    /// When the result is written from each club's side
    /// Then the club's own goals come first, and a shootout is written in brackets, also from its
    ///      own side.
    #[test]
    fn a_result_is_written_from_the_clubs_own_side() {
        assert_eq!(
            result_for(&tie_result(3, 1, None), "s").as_deref(),
            Some("3-1")
        );
        assert_eq!(
            result_for(&tie_result(3, 1, None), "u").as_deref(),
            Some("1-3")
        );
        let level = tie_result(2, 2, Some((5, 4)));
        assert_eq!(result_for(&level, "s").as_deref(), Some("2-2 (5-4)"));
        assert_eq!(result_for(&level, "u").as_deref(), Some("2-2 (4-5)"));
    }

    /// Given a played edition and a manager in each of the three league-phase zones in turn
    /// When the league-phase result is announced
    /// Then each hears their own variant (direct, play-off, eliminated) with their own rank.
    #[test]
    fn each_zone_hears_its_own_league_phase_message() {
        let mut league = drawn(profiles::champions_league_2026(), 1);
        play_everything(&mut league);
        let outcome = league.league_phase.clone().unwrap().outcome.unwrap();
        for (club, rank, variant) in [
            (outcome.direct[0].clone(), 1, "Direct"),
            (outcome.playoff[0].clone(), 9, "Playoff"),
            (outcome.eliminated[0].clone(), 25, "Eliminated"),
        ] {
            let mut game = game_with(league.clone(), &club);
            announce(&mut game, "2027-06-06");
            let message = game
                .messages
                .iter()
                .find(|m| m.id.starts_with("european_league_phase_"))
                .expect("a league-phase message");
            assert_eq!(
                message.subject_key.as_deref(),
                Some(format!("be.msg.europeanLeaguePhase{variant}.subject").as_str())
            );
            assert_eq!(message.i18n_params["rank"], rank.to_string());
        }
    }

    /// Given a champion who is the manager, and a manager of a club outside the competition
    /// When the days are announced
    /// Then the champion hears of the title through the tie messages of the rounds before the final
    ///      only (the final is told as the title, never also as "through"), and the outsider hears
    ///      nothing but sees the champion on the front page.
    #[test]
    fn the_final_is_told_as_the_title_and_outsiders_only_read_the_news() {
        let mut league = drawn(profiles::champions_league_2026(), 1);
        league.name_key = Some("tournaments.competitions.championsLeague".into());
        play_everything(&mut league);
        let champion = league.league_phase.clone().unwrap().champion_id.unwrap();
        let mut winner = game_with(league.clone(), &champion);
        announce(&mut winner, "2027-06-06");
        let final_tie = winner.competitions[0]
            .league_phase
            .as_ref()
            .unwrap()
            .ties_of(domain::league_phase::KnockoutStage::Final)
            .next()
            .unwrap()
            .id
            .clone();
        assert!(
            !winner.messages.iter().any(|m| m.id.ends_with(&final_tie)),
            "no \"through\" message for the final"
        );
        let title = winner
            .messages
            .iter()
            .find(|m| m.id.starts_with("european_title_"))
            .unwrap();
        assert_eq!(
            title.i18n_params["competitionKey"], "tournaments.competitions.championsLeague",
            "the competition is named by its translation key"
        );

        // A club that is not in the competition.
        let mut outsider = game_with(league, "club-01");
        outsider.manager.team_id = Some("someone-else".into());
        announce(&mut outsider, "2027-06-06");
        assert!(outsider.messages.is_empty());
        assert_eq!(outsider.news.len(), 1);
    }

    /// Given an announced edition, saved and loaded again
    /// When the day is announced on the loaded game
    /// Then nothing is sent twice: the ledger is saved with the game.
    #[test]
    fn a_reload_never_repeats_an_announcement() {
        let mut league = drawn(profiles::champions_league_2026(), 1);
        play_everything(&mut league);
        let champion = league.league_phase.clone().unwrap().champion_id.unwrap();
        let mut game = game_with(league, &champion);
        announce(&mut game, "2027-06-06");
        let sent = game.messages.len();
        assert!(sent > 0);

        let mut loaded: Game =
            serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();
        announce(&mut loaded, "2027-06-07");

        assert_eq!(loaded.messages.len(), sent);
        assert_eq!(loaded.news.len(), game.news.len());
    }
}
