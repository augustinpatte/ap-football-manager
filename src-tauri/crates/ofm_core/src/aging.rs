use crate::contracts::{iso, record_movement};
use crate::game::Game;
use chrono::NaiveDate;
use domain::player::{Player, PlayerMovementEntry, PlayerMovementKind};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn seeded_value(player_id: &str, season: u32, salt: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    player_id.hash(&mut hasher);
    season.hash(&mut hasher);
    salt.hash(&mut hasher);
    (hasher.finish() % u32::MAX as u64) as u32
}

fn has_expired_contract(player: &Player, current_date: NaiveDate) -> bool {
    player
        .contract_end()
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .is_some_and(|contract_end| contract_end < current_date)
}

fn retirement_chance(player: &Player, age: i32, current_date: NaiveDate) -> u32 {
    if age < 33 {
        return 0;
    }

    let mut chance: u32 = match age {
        33 => 12,
        34 => 24,
        35 => 42,
        36 => 60,
        37 => 78,
        _ => 100,
    };

    if player.contract_end().is_none() || has_expired_contract(player, current_date) {
        chance += 18;
    }
    if player.team_id.is_none() {
        chance += 10;
    }
    if player.stats.appearances < 10 {
        chance += 8;
    }
    if crate::match_rating::is_rated(player.stats.avg_rating) {
        if player.stats.avg_rating <= 6.4 {
            chance += 8;
        }
        if player.stats.avg_rating >= 7.4 {
            chance = chance.saturating_sub(10);
        }
    }

    chance.min(100)
}

fn should_retire(player: &Player, age: i32, current_date: NaiveDate, season: u32) -> bool {
    let chance = retirement_chance(player, age, current_date);
    if chance == 0 {
        return false;
    }

    let roll = seeded_value(&player.id, season, "retirement-roll") % 100;
    roll < chance
}

/// The day he last lost his club, from the ledger; `None` for a player the world has always had
/// without one (an FM24 free agent, a generated one).
fn unemployed_since(player: &Player) -> Option<NaiveDate> {
    player
        .movement_history
        .iter()
        .rev()
        .find(|entry| entry.kind == PlayerMovementKind::Released)
        .and_then(|entry| NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").ok())
}

/// The chance, in percent, that a player with no club for a year gives up the professional game at
/// this season's end.
///
/// The simulated clubs are a few hundred; the players their contracts release are thousands, and
/// the real world has far more clubs to take them in than this one does. A free agent who has
/// found nobody for a whole year, and is past the age at which a club takes a chance on a
/// prospect, drifts out: surely if he is below the standard of the clubs' squads, less so if he is
/// above it. `market` is the pool as it stands.
fn drifting_out_chance(
    player: &Player,
    age: i32,
    current_date: NaiveDate,
    market: &FreeAgentMarket,
) -> u32 {
    if player.team_id.is_some() || player.retired || age < FREE_AGENT_PROSPECT_AGE {
        return 0;
    }
    if unemployed_since(player).is_some_and(|since| (current_date - since).num_days() < 365) {
        return 0;
    }
    let base = if player.ovr >= market.median_rating {
        15
    } else {
        45
    };
    let older = (age - FREE_AGENT_PROSPECT_AGE).clamp(0, 12) as u32;
    let chance = (base + 5 * older).min(90);
    // Only what the market holds beyond its reserve drifts out: the share of the pool above it.
    chance * market.excess_percent() / 100
}

/// What the free-agent market looks like at a season's end, read once.
struct FreeAgentMarket {
    /// Median rating of the players clubs hold.
    median_rating: u8,
    /// Free agents in the world.
    free_agents: usize,
    /// Free agents the world keeps whatever happens: a pool for the squad floor to draw on.
    reserve: usize,
}

impl FreeAgentMarket {
    fn of(game: &Game) -> Self {
        let mut ratings: Vec<u8> = game
            .players
            .iter()
            .filter(|player| player.team_id.is_some() && !player.retired)
            .map(|player| player.ovr)
            .collect();
        ratings.sort_unstable();
        Self {
            median_rating: ratings.get(ratings.len() / 2).copied().unwrap_or(0),
            free_agents: game
                .players
                .iter()
                .filter(|player| player.team_id.is_none() && !player.retired)
                .count(),
            reserve: game.teams.len() * FREE_AGENT_RESERVE_PER_CLUB,
        }
    }

    /// Percent of the pool that is beyond the reserve: 0 at or under it, approaching 100 as the
    /// pool outgrows it.
    fn excess_percent(&self) -> u32 {
        if self.free_agents <= self.reserve {
            return 0;
        }
        (100 * (self.free_agents - self.reserve) / self.free_agents) as u32
    }
}

/// Free agents per club the world keeps before any drifts out.
const FREE_AGENT_RESERVE_PER_CLUB: usize = 6;

/// A free agent younger than this is still a prospect: he does not give up.
const FREE_AGENT_PROSPECT_AGE: i32 = 21;

fn retire_player(player: &mut Player, current_date: NaiveDate) {
    // Recorded first, while his club is still known. It ends the contract: the dates
    // and the wage follow it.
    record_movement(
        player,
        PlayerMovementEntry {
            from_team_id: player.contract_club_id().map(str::to_string),
            ..PlayerMovementEntry::new(iso(current_date), PlayerMovementKind::Retired)
        },
    );
    player.retired = true;
    player.team_id = None;
    player.transfer_listed = false;
    player.loan_listed = false;
    player.transfer_offers.clear();
}

pub fn apply_seasonal_aging(game: &mut Game, current_date: NaiveDate, season: u32) {
    let world_seed = game.seed;
    let market = FreeAgentMarket::of(game);
    for player in game.players.iter_mut() {
        let age = domain::age::age_on_or_unknown(current_date, &player.date_of_birth);
        let mut stream = crate::seed::rng_for_seed(
            world_seed,
            &format!("development/{}", player.id),
            &iso(current_date),
        );
        crate::development::develop_one_season(player, age, world_seed, &mut stream);
        crate::player_rating::refresh_player_derived(player, current_date);

        let drifts_out = {
            let chance = drifting_out_chance(player, age, current_date, &market);
            chance > 0 && seeded_value(&player.id, season, "free-agent-drift") % 100 < chance
        };
        if drifts_out || should_retire(player, age, current_date, season) {
            if let Some(team_id) = player.team_id.clone()
                && let Some(team) = game.teams.iter_mut().find(|team| team.id == team_id)
            {
                team.remove_player_references(&player.id);
            }
            retire_player(player, current_date);
        }
    }
    crate::retirees::archive_retired_players(game, &iso(current_date), season);
}

#[cfg(test)]
mod tests {
    use super::{apply_seasonal_aging, retirement_chance, should_retire};
    use crate::clock::GameClock;
    use crate::game::Game;
    use chrono::{NaiveDate, TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::{Player, PlayerAttributes, PlayerSeasonStats, Position};
    use domain::team::Team;

    fn make_player(id: &str, dob: &str) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            format!("Player {id}"),
            dob.to_string(),
            "England".to_string(),
            Position::Forward,
            PlayerAttributes {
                pace: 70,
                stamina: 70,
                strength: 70,
                agility: 70,
                passing: 70,
                shooting: 70,
                tackling: 70,
                dribbling: 70,
                defending: 70,
                positioning: 70,
                vision: 70,
                decisions: 70,
                composure: 70,
                aggression: 70,
                teamwork: 70,
                leadership: 70,
                handling: 20,
                reflexes: 20,
                aerial: 70,
            },
        );
        player.team_id = Some("team1".to_string());
        player
    }

    fn make_game(players: Vec<Player>) -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Alex".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team1".to_string());

        let team = Team::new(
            "team1".to_string(),
            "Test FC".to_string(),
            "TFC".to_string(),
            "England".to_string(),
            "London".to_string(),
            "Ground".to_string(),
            20_000,
        );

        Game::new(clock, manager, vec![team], players, vec![], vec![])
    }

    /// Given two players of 36 in the same situation, one rated 85 and one rated 60,
    /// When their chance of retiring this summer is read,
    /// Then it is the same: how good a player is does not keep him playing.
    #[test]
    fn a_star_retires_no_later_than_anyone_else_of_his_age() {
        let current_date = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let mut star = make_player("star", "1990-01-01");
        star.ovr = 85;
        let mut journeyman = make_player("journeyman", "1990-01-01");
        journeyman.ovr = 60;

        assert_eq!(
            retirement_chance(&star, 36, current_date),
            retirement_chance(&journeyman, 36, current_date)
        );
    }

    #[test]
    fn deterministic_retirement_favors_older_out_of_contract_players() {
        let current_date = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let mut player = make_player("older-pro", "1988-01-01");
        player.stage_contract_end(Some("2026-05-01".to_string()));
        player.stats = PlayerSeasonStats {
            appearances: 6,
            avg_rating: 6.1,
            ..PlayerSeasonStats::default()
        };

        let age = domain::age::age_on_or_unknown(current_date, &player.date_of_birth);
        assert!(should_retire(&player, age, current_date, 1));
    }

    /// Given a 27-year-old free agent rated below the clubs' median who has had no club for years
    /// When the chance that he gives up is worked out
    /// Then it is high; for one rated above the median it is low; for a prospect, a player under
    ///      contract or one released only months ago, it is none.
    #[test]
    fn a_free_agent_unemployed_for_a_year_may_give_up_the_game() {
        use super::{FreeAgentMarket, drifting_out_chance};
        let today = NaiveDate::from_ymd_opt(2027, 6, 30).unwrap();
        let market = FreeAgentMarket {
            median_rating: 58,
            free_agents: 1_000,
            reserve: 0,
        };
        let mut free = make_player("free", "2000-01-01");
        free.team_id = None;
        free.ovr = 50;

        assert_eq!(drifting_out_chance(&free, 27, today, &market), 75);
        free.ovr = 65;
        assert_eq!(drifting_out_chance(&free, 27, today, &market), 45);
        assert_eq!(drifting_out_chance(&free, 19, today, &market), 0);

        let mut employed = free.clone();
        employed.team_id = Some("club".to_string());
        assert_eq!(drifting_out_chance(&employed, 27, today, &market), 0);
    }

    /// Given a market whose free agents number the reserve, and one with twice as many
    /// When the chance that a free agent gives up is worked out
    /// Then none give up at the reserve, and half the chance applies at double it: only the
    ///      excess drifts out.
    #[test]
    fn only_the_free_agents_beyond_the_reserve_drift_out() {
        use super::{FreeAgentMarket, drifting_out_chance};
        let today = NaiveDate::from_ymd_opt(2027, 6, 30).unwrap();
        let mut free = make_player("free", "2000-01-01");
        free.team_id = None;
        free.ovr = 50;
        let at = |free_agents| FreeAgentMarket {
            median_rating: 58,
            free_agents,
            reserve: 100,
        };

        assert_eq!(drifting_out_chance(&free, 27, today, &at(100)), 0);
        assert_eq!(drifting_out_chance(&free, 27, today, &at(200)), 75 / 2);
    }

    /// Given a free agent let go three months ago
    /// When the chance that he gives up is worked out
    /// Then it is none: he has not had a year to find a club.
    #[test]
    fn a_player_released_this_year_has_not_given_up() {
        use super::{FreeAgentMarket, drifting_out_chance};
        use domain::player::{PlayerMovementEntry, PlayerMovementKind};
        let today = NaiveDate::from_ymd_opt(2027, 6, 30).unwrap();
        let mut free = make_player("free", "2000-01-01");
        free.ovr = 40;
        crate::contracts::record_movement(
            &mut free,
            PlayerMovementEntry::new("2027-03-30".to_string(), PlayerMovementKind::Released),
        );
        free.team_id = None;

        let market = FreeAgentMarket {
            median_rating: 58,
            free_agents: 1_000,
            reserve: 0,
        };
        assert_eq!(drifting_out_chance(&free, 27, today, &market), 0);
    }

    #[test]
    fn apply_seasonal_aging_retires_veteran_and_reduces_pace() {
        let mut veteran = make_player("older-pro", "1988-01-01");
        veteran.stage_contract_start(Some("2023-07-01".to_string()));
        veteran.stage_contract_end(Some("2026-05-01".to_string()));
        veteran.attributes.pace = 20;
        veteran.transfer_listed = true;
        veteran.stats = PlayerSeasonStats {
            appearances: 6,
            avg_rating: 6.1,
            ..PlayerSeasonStats::default()
        };

        let mut game = make_game(vec![veteran]);
        game.teams[0].starting_xi_ids = vec!["older-pro".to_string()];
        game.teams[0].training_groups = vec![domain::team::TrainingGroup {
            id: "group-1".to_string(),
            name: "Core".to_string(),
            focus: domain::team::TrainingFocus::Tactical,
            player_ids: vec!["older-pro".to_string()],
        }];
        game.teams[0].match_roles.captain = Some("older-pro".to_string());
        let current_date = game.clock.current_date.date_naive();

        apply_seasonal_aging(&mut game, current_date, 1);

        assert!(
            game.players.is_empty(),
            "he leaves the roster for the archive"
        );
        assert_eq!(game.retired_players.len(), 1);
        assert_eq!(game.retired_players[0].id, "older-pro");
        assert_eq!(game.retired_players[0].retired_on, current_date.to_string());
        let veteran = game.retired_players[0].to_player();
        assert!(veteran.retired);
        assert_eq!(veteran.team_id, None);
        assert_eq!(
            veteran.contract_start(),
            None,
            "a retired player has no agreement, so neither date survives"
        );
        assert_eq!(veteran.contract_end(), None);
        assert!(!veteran.transfer_listed);
        assert!(veteran.attributes.pace < 20);
        assert!(game.teams[0].starting_xi_ids.is_empty());
        assert!(game.teams[0].training_groups[0].player_ids.is_empty());
        assert_eq!(game.teams[0].match_roles.captain, None);
    }

    /// Retirement ends the contract like any other exit: it is in the history, and
    /// the wage goes with it instead of being left on a player who will never play.
    #[test]
    fn retirement_appends_a_retired_entry_and_zeroes_the_wage() {
        use domain::player::PlayerMovementKind;
        let mut veteran = make_player("older-pro", "1988-01-01");
        veteran.stage_contract_start(Some("2023-07-01".to_string()));
        veteran.stage_contract_end(Some("2026-05-01".to_string()));
        veteran.stage_wage(9_000);
        veteran.stats = PlayerSeasonStats {
            appearances: 6,
            avg_rating: 6.1,
            ..PlayerSeasonStats::default()
        };
        let mut game = make_game(vec![veteran]);
        let current_date = game.clock.current_date.date_naive();

        apply_seasonal_aging(&mut game, current_date, 1);

        assert!(game.players.is_empty());
        let veteran = game.retired_players[0].to_player();
        assert!(veteran.retired);
        let entry = veteran
            .movement_history
            .last()
            .expect("retirement is recorded");
        assert_eq!(entry.kind, PlayerMovementKind::Retired);
        assert_eq!(entry.from_team_id.as_deref(), Some("team1"));
        assert_eq!(veteran.wage(), 0, "no wage left on a retired player");
    }
    /// Given a contracted 34-year-old with enough appearances and an unrated season,
    /// when retirement is considered, then only the age term applies.
    #[test]
    fn an_unrated_veteran_has_no_retirement_rating_modifier() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let mut player = make_player("unrated-veteran", "1992-01-01");
        player.stage_contract_end(Some("2027-06-30".to_string()));
        player.stats.appearances = 20;
        player.stats.avg_rating = 0.0;
        assert_eq!(retirement_chance(&player, 34, date), 24);
        player.stats.avg_rating = 6.8;
        assert_eq!(retirement_chance(&player, 34, date), 24);
    }

    /// Given a contracted veteran at a club, with enough appearances, an unrated season and an
    /// ordinary overall,
    /// when his retirement chance is read at each age from 32 to 40,
    /// then it is 0 before 33, then 12, 24, 42, 60 and 78, and certain from 38.
    #[test]
    fn the_retirement_chance_rises_with_each_year_from_33() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let mut player = make_player("ageing-pro", "1990-01-01");
        player.stage_contract_end(Some("2027-06-30".to_string()));
        player.stats.appearances = 20;
        assert!(player.ovr < 80, "an 80+ overall lowers the chance");

        let by_age: Vec<(i32, u32)> = (32..=40)
            .map(|age| (age, retirement_chance(&player, age, date)))
            .collect();

        assert_eq!(
            by_age,
            vec![
                (32, 0),
                (33, 12),
                (34, 24),
                (35, 42),
                (36, 60),
                (37, 78),
                (38, 100),
                (39, 100),
                (40, 100),
            ]
        );
    }

    /// Given an old save storing the numeric zero sentinel, when the player is loaded,
    /// then his unrated season has no retirement penalty.
    #[test]
    fn a_loaded_unrated_veteran_keeps_the_neutral_retirement_chance() {
        let mut player = make_player("loaded-unrated", "1992-01-01");
        player.stage_contract_end(Some("2027-06-30".to_string()));
        player.stats.appearances = 20;
        let json = serde_json::to_string(&player).unwrap();
        let loaded: Player = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.stats.avg_rating, 0.0);
        assert_eq!(
            retirement_chance(&loaded, 34, NaiveDate::from_ymd_opt(2026, 5, 20).unwrap()),
            24
        );
    }

    /// Given a rated veteran, when retirement is considered, then established poor/good modifiers survive.
    #[test]
    fn a_rated_veteran_keeps_the_existing_retirement_modifiers() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let mut player = make_player("rated-veteran", "1992-01-01");
        player.stage_contract_end(Some("2027-06-30".to_string()));
        player.stats.appearances = 20;
        player.stats.avg_rating = 6.4;
        assert_eq!(retirement_chance(&player, 34, date), 32);
        player.stats.avg_rating = 7.4;
        assert_eq!(retirement_chance(&player, 34, date), 14);
    }

    /// A cohort of `count` players of `age` rated like `make_player`, with the given ceiling, in a
    /// game that has played one season of ageing: the mean change in rating of the players still on
    /// the roster afterwards (the retirees leave it, and are not part of the average).
    fn mean_change_after_a_season(age: i32, potential: u8, count: usize) -> f64 {
        let born = format!("{}-01-01", 2026 - age);
        let players: Vec<Player> = (0..count)
            .map(|n| {
                let mut player = make_player(&format!("cohort-{age}-{n}"), &born);
                player.stage_contract_end(Some("2030-06-30".to_string()));
                crate::player_rating::refresh_player_derived(
                    &mut player,
                    chrono::NaiveDate::from_ymd_opt(2026, 5, 20).unwrap(),
                );
                player.potential = potential;
                player
            })
            .collect();
        let before: std::collections::HashMap<String, f64> = players
            .iter()
            .map(|p| (p.id.clone(), f64::from(p.ovr)))
            .collect();
        let mut game = make_game(players);
        let date = game.clock.current_date.date_naive();

        apply_seasonal_aging(&mut game, date, 1);

        let changes: Vec<f64> = game
            .players
            .iter()
            .map(|after| f64::from(after.ovr) - before[&after.id])
            .collect();
        assert!(!changes.is_empty(), "the whole cohort retired");
        changes.iter().sum::<f64>() / changes.len() as f64
    }

    /// Given a cohort of 17-year-olds with room to grow and a cohort of 35-year-olds,
    /// When a season of ageing is applied,
    /// Then the young gain rating on average and the old lose it.
    #[test]
    fn a_season_develops_the_young_and_wears_the_old() {
        assert!(mean_change_after_a_season(17, 90, 80) > 2.0);
        // Flat-70 attributes, a ceiling at the rating: the mean change of the players who stay.
        assert!(mean_change_after_a_season(35, 70, 200) < -1.0);
    }

    /// Given a cohort of 26-year-olds rated 70 with a ceiling of 85,
    /// When a season of ageing is applied,
    /// Then they are still growing: development tapers, it does not stop at 26.
    #[test]
    fn development_tapers_through_the_late_twenties_and_does_not_stop_dead() {
        let at_26 = mean_change_after_a_season(26, 85, 120);
        let at_30 = mean_change_after_a_season(30, 85, 120);
        assert!(at_26 > 0.0, "26-year-olds changed by {at_26}");
        assert!(at_26 > at_30, "26: {at_26}, 30: {at_30}");
    }
}
