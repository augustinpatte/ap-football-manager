//! AI clubs renew their own players' contracts.
//!
//! Every contract in the world runs out, and until this module the only club
//! that could renew one was the player's: renewal negotiation, delegated
//! renewals and free-agent signings all start from `game.manager.team_id`. An
//! AI club that did not win players in the transfer market therefore lost its
//! squad a contract at a time and never replaced it.
//!
//! An AI club looks at the contracts running down on its weekly review day, the
//! same day it reviews its tactics, and renews a player when it needs him or
//! wants him and can pay him. The terms are the ones the contracts module says
//! the player expects, judged by the same `evaluate_renewal_offer` and applied
//! by the same `apply_agreed_renewal` as the player's own negotiations, and
//! paid under the same `wage_policy_verdict` — there is no AI-only copy of the
//! wage, contract-length or wage-policy rules.

use chrono::NaiveDate;
use domain::player::Player;
use domain::team::Team;

use crate::contracts::{
    RenewalDecision, RenewalOffer, apply_agreed_renewal, evaluate_renewal_offer,
    expected_contract_years, expected_wage, next_renewal_round, remaining_contract_days,
};
use crate::game::Game;
use crate::squad_floor::{
    PLANNING_MARGIN_PER_GROUP, PLANNING_TARGET_SENIORS, SigningContext, group_floor, group_index,
};

/// A contract with this many days or fewer left is one the club decides on.
/// Half a season: early enough to act on every review day until it is settled.
pub(crate) const RENEWAL_HORIZON_DAYS: i64 = 183;

/// Seniors beyond which a club stops signing free agents to improve its squad: the planning target
/// of 18 plus the cover a squad needs for a season's injuries.
const IMPROVEMENT_SQUAD_LIMIT: usize = 24;

/// A player this young is kept as a prospect, whatever his rating today.
const PROSPECT_AGE_LIMIT: i32 = 23;

/// Run every AI club's contract review that falls on this weekday.
pub(crate) fn apply_ai_contract_decisions(game: &mut Game, weekday_num: u32) {
    let current_date = game.clock.current_date.date_naive();
    for team_id in crate::ai_tactics::ai_clubs_reviewing_on(game, weekday_num) {
        let Some(team) = game.teams.iter().find(|team| team.id == team_id).cloned() else {
            continue;
        };
        let running_down = contracts_running_down(game, &team_id, current_date);
        if running_down.is_empty() {
            continue;
        }
        let facts = SquadFacts::of(game, &team_id);
        for player_index in running_down {
            if keeps_given(&facts, &game.players[player_index], current_date) {
                renew(game, &team, player_index, current_date);
            }
        }
    }
}

/// Run every AI club's squad planning that falls on this weekday.
///
/// Ordinary management, so the squad floor's emergency top-up has nothing to
/// do: a club graduates academy players who have outgrown the academy, then
/// keeps one senior above the minimum in each position group and
/// [`PLANNING_TARGET_SENIORS`] in all — promoting from its academy first, then
/// signing free agents the board lets it pay
/// ([`crate::squad_floor::bring_in_one`], the same source the emergency uses).
/// Runs after the day's departures so it sees the squad the club actually has.
pub(crate) fn apply_ai_squad_planning(game: &mut Game, weekday_num: u32) {
    let clubs = crate::ai_tactics::ai_clubs_reviewing_on(game, weekday_num);
    plan_squads(game, &clubs);
}

/// Every AI club plans its squad at once, whatever its review day: what clubs
/// do at a season's end, when retirements can take several players from one
/// club overnight and a week is too long to wait.
pub(crate) fn plan_every_ai_squad(game: &mut Game) {
    let clubs = crate::ai_tactics::ai_clubs(game);
    plan_squads(game, &clubs);
}

fn plan_squads(game: &mut Game, clubs: &[String]) {
    let current_date = game.clock.current_date.date_naive();
    // Where each club's players are, found once. A signing changes one club's list and is
    // recounted where it happens; nothing else here moves a player between clubs.
    let mut positions = crate::match_day_eligibility::positions_by_club(&game.players);
    // What the free-agent market looks like, learned once for every signing this pass makes.
    let mut context = SigningContext::default();
    for team_id in clubs {
        let Some(team) = game.teams.iter().find(|team| &team.id == team_id).cloned() else {
            continue;
        };
        let club = positions.entry(team_id.clone()).or_default();
        crate::match_day_eligibility::debug_assert_group_is_current(&game.players, team_id, club);
        graduate_overage_academy_players(game, club, current_date);
        plan_squad_depth(game, &team, club, &mut context);
    }
}

/// An academy player past the academy's age limit joins the senior squad. `club` is where the
/// club's players stand in `game.players`.
fn graduate_overage_academy_players(game: &mut Game, club: &[usize], current_date: NaiveDate) {
    for &position in club {
        let player = &mut game.players[position];
        if player.squad_role == domain::player::SquadRole::Youth
            && domain::age::age_on_or_unknown(current_date, &player.date_of_birth)
                > crate::roster::YOUTH_ACADEMY_MAX_AGE
        {
            player.squad_role = domain::player::SquadRole::Senior;
        }
    }
}

/// Fill each group to one above its minimum, then the squad to the planning
/// target, thinnest group first, for as long as a source has someone.
///
/// A senior whose contract runs out within the renewal horizon is not counted:
/// the club has had its chance to renew him, and if he is still on the old
/// contract he may walk. Generated contracts share an end date, so several can
/// leave a group on the same day — planning replaces them before they go, not
/// after.
fn plan_squad_depth(
    game: &mut Game,
    team: &Team,
    club: &mut Vec<usize>,
    context: &mut SigningContext,
) {
    use crate::squad_floor::{MIN_PLAYERS_PER_GROUP, bring_in_one, groups_thinnest_first};

    let seniors_at_the_start = seniors_in(game, club, None);
    for (group, floor) in MIN_PLAYERS_PER_GROUP {
        loop {
            let have = seniors_in(game, club, Some(&group));
            if have >= floor + PLANNING_MARGIN_PER_GROUP {
                break;
            }
            // Under the floor once the contracts running out are gone, the club needs him; above
            // it, the margin is a wish and costs what the club can afford.
            if bring_in_one(game, team, &group, have < floor, context).is_none() {
                break;
            }
            *club = crate::match_day_eligibility::club_positions(&game.players, &team.id);
        }
    }
    loop {
        let have = seniors_in(game, club, None);
        if have >= PLANNING_TARGET_SENIORS {
            break;
        }
        let required = have < crate::squad_floor::MIN_SENIOR_PLAYERS;
        let groups = groups_thinnest_first(game, &team.id);
        let brought_one = groups
            .iter()
            .any(|group| bring_in_one(game, team, group, required, context).is_some());
        if !brought_one {
            break;
        }
        *club = crate::match_day_eligibility::club_positions(&game.players, &team.id);
    }
    // A review that already had to bring someone in has done its work for the week.
    if seniors_in(game, club, None) == seniors_at_the_start
        && !plan_succession(game, team, club, context)
    {
        improve_squad(game, team, club, context);
    }
}

/// A club whose first choices in a group will be clearly worse, or too few, a year from now (a
/// keeper of 35, two veterans on contracts it will not extend) brings in his successor now: its
/// own academy player who is expected to be good enough, else a free agent who is, in the group
/// where the drop is largest, one a review. Returns whether it brought someone in. Nothing here is
/// required of the club: the signing is subject to the same tests as any other.
fn plan_succession(
    game: &mut Game,
    team: &Team,
    club: &mut Vec<usize>,
    context: &mut SigningContext,
) -> bool {
    use crate::squad_floor::{
        BetterThan, MIN_PLAYERS_PER_GROUP, STARTERS_PER_GROUP, promote_successor,
        sign_free_agent_for,
    };

    if seniors_in(game, club, None) >= IMPROVEMENT_SQUAD_LIMIT {
        return false;
    }
    let current_date = game.clock.current_date.date_naive();
    let (current, planned) = crate::squad_outlook::club_outlook(
        club.iter()
            .map(|&position| &game.players[position])
            .filter(|player| crate::squad_outlook::counts_toward_squad(player)),
        current_date,
    );
    // Where the first choices will drop most comes first.
    let mut at_risk: Vec<(i32, usize)> = (0..4)
        .filter(|&group| {
            crate::squad_outlook::has_room_for_a_successor(planned[group].len(), group)
                && crate::squad_outlook::succession_risk(&current[group], &planned[group], group)
        })
        .map(|group| {
            let drop = crate::squad_outlook::starters_drop(&current[group], &planned[group], group);
            (drop, group)
        })
        .collect();
    at_risk.sort_unstable_by(|a, b| b.cmp(a));
    for (_, group) in at_risk {
        let position = MIN_PLAYERS_PER_GROUP[group].0.clone();
        // What the successor has to beat: the cover behind the starters in the squad a year on.
        let bar = planned[group]
            .get(STARTERS_PER_GROUP[group])
            .copied()
            .unwrap_or(0);
        if promote_successor(game, &team.id, &position, bar, context).is_some()
            || sign_free_agent_for(
                game,
                team,
                &position,
                false,
                Some(BetterThan::InAYear(bar)),
                context,
            )
            .is_some()
        {
            *club = crate::match_day_eligibility::club_positions(&game.players, &team.id);
            return true;
        }
    }
    false
}

/// A club that meets its planning target and has room to spare signs a free agent who would be
/// better than the cover it has behind its starters — one a review, so the market is absorbed at
/// the pace clubs review their squads, not in a burst. It stops at [`IMPROVEMENT_SQUAD_LIMIT`]
/// seniors: a lean full squad, not a hoard. Nothing here is required of the club: the signing is
/// subject to the same tests as any other (the room under its wage budget, its pay scale, the
/// board's policy) and is made only if it passes.
fn improve_squad(
    game: &mut Game,
    team: &Team,
    club: &mut Vec<usize>,
    context: &mut SigningContext,
) {
    use crate::squad_floor::{STARTERS_PER_GROUP, sign_free_agent_for};

    if seniors_in(game, club, None) >= IMPROVEMENT_SQUAD_LIMIT {
        return;
    }
    // Where the club's cover is weakest: the lowest rated first-cover player, group by group.
    let mut covers: Vec<(u8, usize)> = (0..4)
        .map(|group| {
            let mut ratings: Vec<u8> = club
                .iter()
                .map(|&position| &game.players[position])
                .filter(|player| player.squad_role == domain::player::SquadRole::Senior)
                .filter(|player| group_index(&player.position) == group)
                .map(|player| player.ovr)
                .collect();
            ratings.sort_unstable_by(|a, b| b.cmp(a));
            (
                ratings.get(STARTERS_PER_GROUP[group]).copied().unwrap_or(0),
                group,
            )
        })
        .collect();
    covers.sort_unstable();
    for (cover, group) in covers {
        let position = crate::squad_floor::MIN_PLAYERS_PER_GROUP[group].0.clone();
        if sign_free_agent_for(
            game,
            team,
            &position,
            false,
            Some(crate::squad_floor::BetterThan::Today(cover)),
            context,
        )
        .is_some()
        {
            *club = crate::match_day_eligibility::club_positions(&game.players, &team.id);
            return;
        }
    }
}

/// Seniors among the club's players (`club`: where they stand in `game.players`) who will still
/// be under contract past the renewal horizon, in one group or in all.
fn seniors_in(game: &Game, club: &[usize], group: Option<&domain::player::Position>) -> usize {
    let current_date = game.clock.current_date.date_naive();
    club.iter()
        .map(|&position| &game.players[position])
        .filter(|player| player.squad_role == domain::player::SquadRole::Senior)
        // No end date is a contract the expiry sweep never ends: he stays.
        .filter(|player| {
            crate::contracts::contract_days_remaining(player.contract_end(), current_date)
                .is_none_or(|days| days > RENEWAL_HORIZON_DAYS)
        })
        .filter(|player| group.is_none_or(|group| player.position.to_group_position() == *group))
        .count()
}

/// The club's own players (its loaned-out ones included) whose contracts end
/// within the horizon.
fn contracts_running_down(game: &Game, team_id: &str, current_date: NaiveDate) -> Vec<usize> {
    game.players
        .iter()
        .enumerate()
        .filter(|(_, player)| player.contract_end().is_some())
        .filter(|(_, player)| player.contract_club_id() == Some(team_id))
        .filter(|(_, player)| remaining_contract_days(player, current_date) <= RENEWAL_HORIZON_DAYS)
        .map(|(index, _)| index)
        .collect()
}

/// What a club's squad looks like when it decides on its players: how many seniors it has in each
/// group and in all, and the median rating of those seniors (an academy of low-rated youngsters is
/// not the standard a senior is held to). Counted once for a club's whole review.
pub(crate) struct SquadFacts {
    team_id: String,
    seniors_by_group: [usize; 4],
    seniors: usize,
    median_ovr: u8,
}

impl SquadFacts {
    pub(crate) fn of(game: &Game, team_id: &str) -> Self {
        let mut seniors_by_group = [0; 4];
        let mut ratings: Vec<u8> = Vec::new();
        for player in &game.players {
            if player.team_id.as_deref() == Some(team_id)
                && player.squad_role == domain::player::SquadRole::Senior
            {
                seniors_by_group[group_index(&player.position)] += 1;
                ratings.push(player.ovr);
            }
        }
        ratings.sort_unstable();
        Self {
            team_id: team_id.to_string(),
            seniors_by_group,
            seniors: ratings.len(),
            median_ovr: ratings.get(ratings.len() / 2).copied().unwrap_or(0),
        }
    }

    /// Whether `player` is counted among the club's seniors (then "the others" leave him out).
    fn counts(&self, player: &Player) -> bool {
        player.team_id.as_deref() == Some(self.team_id.as_str())
            && player.squad_role == domain::player::SquadRole::Senior
    }
}

/// Does the club want this player for another contract? `facts` is the squad as the club's review
/// counted it, once.
pub(crate) fn keeps_given(facts: &SquadFacts, player: &Player, current_date: NaiveDate) -> bool {
    needed_for_depth(facts, player) || worth_keeping(facts, player, current_date)
}

/// Letting him go would leave the club under what its squad planning aims for
/// — in his group, or in all — counted as the floor counts. Whether the club
/// may go over its wage policy to keep him is not decided here: `renew` asks
/// the one rule every club answers to.
fn needed_for_depth(facts: &SquadFacts, player: &Player) -> bool {
    let counted = usize::from(facts.counts(player));
    let others_in_group = facts.seniors_by_group[group_index(&player.position)] - counted;
    others_in_group < group_floor(&player.position.to_group_position()) + PLANNING_MARGIN_PER_GROUP
        || facts.seniors - counted < PLANNING_TARGET_SENIORS
}

/// Good enough and young enough to want: a player at least as good as the squad's median, or a
/// prospect whose potential would take him there. A young player who will never reach the level of
/// the squad he is in is not worth another contract for being young.
fn worth_keeping(facts: &SquadFacts, player: &Player, current_date: NaiveDate) -> bool {
    let age = domain::age::age_on_or_unknown(current_date, &player.date_of_birth);
    (age <= PROSPECT_AGE_LIMIT && player.potential >= facts.median_ovr)
        || (age <= crate::squad_outlook::peak_age_limit(group_index(&player.position))
            && player.ovr >= facts.median_ovr)
}

/// A player this content (morale) re-signs for a modest raise rather than the whole of his ask.
const CONTENT_MORALE: u8 = 65;

/// The most a content player's wage rises at a renewal an AI club negotiates, in per cent.
///
/// `expected_wage` multiplies the wage he is paid by an age, an importance and a contract-running-
/// down premium, so offering what he expects at every renewal compounded +15 to +30 % a contract
/// for ever, on a revenue that does not grow. The premiums are leverage: a player at ease in his
/// club does not use all of it, an unhappy one does. A human manager's negotiations are not
/// touched; `expected_wage` is the same for them.
const CONTENT_PLAYER_MAX_RAISE_PCT: u32 = 10;

/// What this player agrees to sign for, when the club asks for a renewal: the whole of what he
/// `asked` unless he is content and the ask is a bigger raise than [`CONTENT_PLAYER_MAX_RAISE_PCT`].
///
/// A club that is already over its wage budget (`over_budget`) freezes a content player's wage:
/// the modest raise, repeated at every renewal of an over-budget squad, was what kept 70 of 80
/// clubs above their budgets for thirty seasons in the F5.1 world study. A player who is not
/// content still asks for everything and the board's wage policy decides, so a freeze is never
/// imposed on someone with a reason to leave. It is a cap, not a target: a player who asks for
/// less than he is paid is not raised to his old wage.
fn settled_renewal_wage(player: &Player, asked: u32, over_budget: bool) -> u32 {
    let paid = player.last_known_wage();
    if paid == 0 || player.morale < CONTENT_MORALE {
        return asked;
    }
    if over_budget {
        return asked.min(paid);
    }
    let modest = crate::contracts::round_up_wage(paid * (100 + CONTENT_PLAYER_MAX_RAISE_PCT) / 100);
    asked.min(modest.max(paid))
}

/// Offer what the player expects, if the board allows the wage, and if he
/// takes it put it into effect.
fn renew(game: &mut Game, team: &Team, player_index: usize, current_date: NaiveDate) {
    let player = &game.players[player_index];
    let asked = expected_wage(player, team, current_date);
    // The bill is worked out once: the verdict and the grace check below both need it.
    let bill = crate::finances::calc_wages(game, &team.id);
    let over_budget = team.wage_budget > 0 && bill > team.wage_budget;
    let wage = settled_renewal_wage(player, asked, over_budget);
    let offer = RenewalOffer {
        weekly_wage: wage,
        contract_years: expected_contract_years(player, current_date),
    };
    let verdict = crate::contract_wage_policy::wage_policy_verdict_at_bill(
        game,
        team,
        player,
        offer.weekly_wage,
        bill,
    );
    if !verdict.permits() {
        return;
    }
    // The board's policy allows a club already over budget a further 25 000 a week at every
    // decision, a margin sized for clubs paying hundreds of thousands: at a club whose whole
    // budget is 16 000 it let every renewal pass at any raise. In a world with a simulation scope
    // (every career; the single-league test worlds have none and keep OFM's legacy margin) an AI
    // club's raise is held to the proportional grace as well, unless the squad floor is what lets
    // it through.
    if verdict == crate::contract_wage_policy::WagePolicyVerdict::WithinPolicy
        && !game.active_competition_ids.is_empty()
    {
        let extra = crate::finances::weekly_commitment_at_wage(
            player,
            &team.id,
            i64::from(offer.weekly_wage),
        ) - crate::finances::player_weekly_wage_for_team(player, &team.id);
        let books = crate::ai_finance_guard::SigningBooks {
            cash: team.finance,
            weekly_bill: bill,
            weekly_income: 0,
            wage_budget: team.wage_budget,
        };
        if !books.can_add_wage(extra) {
            return;
        }
    }
    // A player who settled for less than he asked has agreed already; one who asked and got it
    // is judged as any renewal is.
    if wage >= asked {
        let outcome = evaluate_renewal_offer(player, team, current_date, &offer);
        if outcome.decision != RenewalDecision::Accepted {
            return;
        }
    }
    let round = next_renewal_round(player, None);
    if let Err(error) = apply_agreed_renewal(
        &mut game.players[player_index],
        team,
        offer.weekly_wage,
        offer.contract_years,
        current_date,
        crate::contracts::RenewalAgreedBy::Manager { round },
    ) {
        log::error!(
            "[ai_contracts] {} could not renew {}: {error}",
            team.id,
            game.players[player_index].id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::contract_wage_policy::wage_policy_verdict;
    use chrono::{Datelike, Duration, TimeZone, Utc};
    use domain::manager::Manager;
    use domain::player::{PlayerAttributes, Position};

    const TODAY: (i32, u32, u32) = (2026, 8, 3);

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(TODAY.0, TODAY.1, TODAY.2).unwrap()
    }

    fn attrs(level: u8) -> PlayerAttributes {
        PlayerAttributes {
            pace: level,
            stamina: level,
            strength: level,
            agility: level,
            passing: level,
            shooting: level,
            tackling: level,
            dribbling: level,
            defending: level,
            positioning: level,
            vision: level,
            decisions: level,
            composure: level,
            aggression: level,
            teamwork: level,
            leadership: level,
            handling: level,
            reflexes: level,
            aerial: level,
        }
    }

    fn player(
        id: &str,
        team_id: &str,
        position: Position,
        ovr: u8,
        age: i32,
        days_left: i64,
    ) -> Player {
        let born = format!("{}-01-01", TODAY.0 - age);
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            born,
            "England".to_string(),
            position,
            attrs(ovr),
        );
        player.team_id = Some(team_id.to_string());
        player.ovr = ovr;
        player.stage_wage(1_000);
        player.stage_contract_end(Some(
            (today() + Duration::days(days_left))
                .format("%Y-%m-%d")
                .to_string(),
        ));
        player
    }

    fn team(id: &str) -> Team {
        let mut team = Team::new(
            id.to_string(),
            id.to_string(),
            id.to_uppercase(),
            "England".to_string(),
            "London".to_string(),
            "Ground".to_string(),
            20_000,
        );
        team.wage_budget = 10_000_000;
        // Cash enough to carry any raise these tests ask for: the books are not what they are about.
        team.finance = 100_000_000;
        team
    }

    /// Two clubs of nineteen — one over the planning target, and a senior over
    /// the planning margin in every group, so nobody is needed for depth unless
    /// a test says so — every contract long, rated 60, aged 26.
    fn world() -> Game {
        let clock = GameClock::new(
            Utc.with_ymd_and_hms(TODAY.0, TODAY.1, TODAY.2, 12, 0, 0)
                .unwrap(),
        );
        let mut manager = Manager::new(
            "mgr".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("user".to_string());
        let mut players = Vec::new();
        for club in ["user", "ai"] {
            for (group, count) in [
                (Position::Goalkeeper, 3),
                (Position::Defender, 6),
                (Position::Midfielder, 6),
                (Position::Forward, 4),
            ] {
                for i in 0..count {
                    players.push(player(
                        &format!("{club}_{group:?}{i}"),
                        club,
                        group.clone(),
                        60,
                        26,
                        1_000,
                    ));
                }
            }
        }
        Game::new(
            clock,
            manager,
            vec![team("user"), team("ai")],
            players,
            vec![],
            vec![],
        )
    }

    fn review_day() -> u32 {
        crate::ai_tactics::review_weekday("ai")
    }

    fn contract_end(game: &Game, id: &str) -> String {
        game.players
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .contract_end()
            .unwrap()
            .to_string()
    }

    #[test]
    fn a_good_player_running_down_his_contract_is_renewed_on_the_review_day() {
        let mut game = world();
        game.players
            .push(player("star", "ai", Position::Midfielder, 75, 27, 90));
        let before = contract_end(&game, "star");

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(
            contract_end(&game, "star") > before,
            "the club let a player it wants run down"
        );
        // An AI renewal goes through the one renewal writer, so it is in his history.
        let star = game.players.iter().find(|p| p.id == "star").unwrap();
        let entry = star
            .movement_history
            .last()
            .expect("the renewal is recorded");
        assert_eq!(entry.kind, domain::player::PlayerMovementKind::Renewal);
        assert_eq!(entry.to_team_id.as_deref(), Some("ai"));
        assert_eq!(
            entry.contract.as_ref().map(|c| c.weekly_wage),
            Some(star.wage())
        );
    }

    fn wage_of(game: &Game, id: &str) -> u32 {
        game.players.iter().find(|p| p.id == id).unwrap().wage()
    }

    /// Given a content star whose contract is running down and an AI club inside its wage budget,
    /// When the club reviews its contracts,
    /// Then he is renewed with a raise (the control for the next test, which needs a raise to exist).
    #[test]
    fn a_club_inside_its_budget_renews_a_content_star_with_a_raise() {
        let mut game = world();
        let mut star = player("star", "ai", Position::Midfielder, 75, 27, 90);
        star.morale = 80;
        game.players.push(star);

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(wage_of(&game, "star") > 1_000, "{}", wage_of(&game, "star"));
    }

    /// Given the same star and a club whose wage bill is far over its budget,
    /// When the club reviews its contracts,
    /// Then his wage is never raised: the freeze is wired into the real renewal, not only into the
    /// pure function. (Whether the board renews him at all is the wage policy's business.)
    #[test]
    fn a_club_over_its_budget_does_not_raise_a_content_stars_wage_at_renewal() {
        let mut game = world();
        game.teams[1].wage_budget = 1_000;
        let mut star = player("star", "ai", Position::Midfielder, 75, 27, 90);
        star.morale = 80;
        game.players.push(star);

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(
            wage_of(&game, "star") <= 1_000,
            "{}",
            wage_of(&game, "star")
        );
    }

    #[test]
    fn nothing_is_decided_on_another_day() {
        let mut game = world();
        game.players
            .push(player("star", "ai", Position::Midfielder, 75, 27, 90));
        let before = contract_end(&game, "star");

        apply_ai_contract_decisions(&mut game, (review_day() + 1) % 7);

        assert_eq!(contract_end(&game, "star"), before);
    }

    #[test]
    fn the_players_own_club_is_left_to_the_player() {
        let mut game = world();
        game.players.push(player(
            "user_star",
            "user",
            Position::Midfielder,
            75,
            27,
            90,
        ));
        let before = contract_end(&game, "user_star");

        for weekday in 0..7 {
            apply_ai_contract_decisions(&mut game, weekday);
        }

        assert_eq!(contract_end(&game, "user_star"), before);
    }

    /// Past his best, below the squad's standard, and not needed for depth:
    /// the club lets the contract run out.
    #[test]
    fn a_fading_player_the_club_can_spare_is_let_go() {
        let mut game = world();
        game.players
            .push(player("veteran", "ai", Position::Midfielder, 50, 34, 90));
        let before = contract_end(&game, "veteran");

        apply_ai_contract_decisions(&mut game, review_day());

        assert_eq!(contract_end(&game, "veteran"), before);
    }

    /// Given a keeper of 34 and a midfielder of 34, both rated above the squad's median and both
    /// on contracts that end in 90 days
    /// When the club reviews its contracts
    /// Then it renews the keeper, who ages later, and lets the midfielder run out.
    #[test]
    fn a_good_keeper_of_thirty_four_is_renewed_where_an_outfield_player_of_that_age_is_not() {
        let mut game = world();
        game.players
            .push(player("old_keeper", "ai", Position::Goalkeeper, 75, 34, 90));
        game.players
            .push(player("old_mid", "ai", Position::Midfielder, 75, 34, 90));
        let keeper_before = contract_end(&game, "old_keeper");
        let mid_before = contract_end(&game, "old_mid");

        apply_ai_contract_decisions(&mut game, review_day());

        assert_ne!(contract_end(&game, "old_keeper"), keeper_before);
        assert_eq!(contract_end(&game, "old_mid"), mid_before);
    }

    /// An AI club of nineteen seniors rated 60, an academy of thirty youngsters
    /// rated 30, and a 29-year-old midfielder rated `rating` whose contract ends
    /// in 90 days. The academy is big enough to pull a median over everyone
    /// down to 55, while the seniors' median stays at 60.
    fn club_with_a_weak_academy_and_a_journeyman(rating: u8) -> Game {
        let mut game = world();
        for i in 0..30 {
            let mut youngster = player(
                &format!("academy{i}"),
                "ai",
                Position::Midfielder,
                30,
                17,
                1_000,
            );
            youngster.squad_role = domain::player::SquadRole::Youth;
            game.players.push(youngster);
        }
        game.players.push(player(
            "journeyman",
            "ai",
            Position::Midfielder,
            rating,
            29,
            90,
        ));
        game
    }

    /// Given an AI club whose academy is full of low-rated youngsters, when a
    /// senior below the seniors' standard runs down his contract, he is judged
    /// against the seniors and let go. Counting the academy would have dragged
    /// the standard down to him and kept him.
    #[test]
    fn the_squad_median_is_the_seniors_median() {
        let mut game = club_with_a_weak_academy_and_a_journeyman(55);
        // The fixture splits the two medians around him — worked out here from
        // the fixture, not by the function under test.
        let median = |seniors_only: bool| {
            let mut ratings: Vec<u8> = game
                .players
                .iter()
                .filter(|p| p.team_id.as_deref() == Some("ai"))
                .filter(|p| !seniors_only || p.squad_role == domain::player::SquadRole::Senior)
                .map(|p| p.ovr)
                .collect();
            ratings.sort_unstable();
            ratings[ratings.len() / 2]
        };
        assert!(55 >= median(false), "a median over everyone would keep him");
        assert!(55 < median(true), "the seniors' median lets him go");
        let before = contract_end(&game, "journeyman");

        apply_ai_contract_decisions(&mut game, review_day());

        assert_eq!(contract_end(&game, "journeyman"), before);
    }

    /// The same club, the same player at the seniors' standard: he is renewed,
    /// so nothing but the standard he is measured against lets him go above.
    #[test]
    fn a_player_at_the_seniors_standard_is_renewed_at_a_club_with_a_weak_academy() {
        let mut game = club_with_a_weak_academy_and_a_journeyman(60);
        let before = contract_end(&game, "journeyman");

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(contract_end(&game, "journeyman") > before);
    }

    /// The same player, but the club's only other keepers are the floor itself:
    /// letting him go would leave it one injury from a crisis.
    #[test]
    fn a_player_the_club_cannot_do_without_is_renewed_whoever_he_is() {
        let mut game = world();
        game.players.retain(|p| p.id != "ai_Goalkeeper2");
        game.players
            .push(player("old_keeper", "ai", Position::Goalkeeper, 50, 34, 90));
        let before = contract_end(&game, "old_keeper");

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(contract_end(&game, "old_keeper") > before);
    }

    /// An expensive forward running down his contract, at a club whose wage
    /// budget is exactly its current bill — so renewing him on the terms he
    /// expects is over the board's policy — keeping `forwards_besides_him`
    /// other forwards.
    fn over_policy_forward(forwards_besides_him: usize) -> Game {
        let mut game = world();
        game.players.retain(|p| {
            !(p.team_id.as_deref() == Some("ai")
                && p.position == Position::Forward
                && p.id.as_str() >= format!("ai_Forward{forwards_besides_him}").as_str())
        });
        // Everyone else on no wage, so his raise alone decides the verdict.
        for other in game.players.iter_mut() {
            if other.team_id.as_deref() == Some("ai") {
                other.stage_wage(0);
            }
        }
        let mut star = player("costly_fwd", "ai", Position::Forward, 80, 27, 90);
        star.market_value = 40_000_000;
        game.players.push(star);
        let bill = crate::finances::calc_wages(&game, "ai");
        game.teams[1].wage_budget = bill;
        game
    }

    fn verdict_for(game: &Game, id: &str) -> crate::contract_wage_policy::WagePolicyVerdict {
        let team = &game.teams[1];
        let player = game.players.iter().find(|p| p.id == id).unwrap();
        wage_policy_verdict(game, team, player, expected_wage(player, team, today()))
    }

    /// One other forward: without him the club is below the floor, so the board
    /// lets the wage policy go — the same rule the player's own renewals use.
    #[test]
    fn a_club_goes_over_its_wage_policy_to_keep_a_player_it_needs_for_the_floor() {
        use crate::contract_wage_policy::WagePolicyVerdict;
        let mut game = over_policy_forward(1);
        assert_eq!(
            verdict_for(&game, "costly_fwd"),
            WagePolicyVerdict::OverPolicyToKeepSquadFloor
        );
        let before = contract_end(&game, "costly_fwd");

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(contract_end(&game, "costly_fwd") > before);
    }

    /// Two other forwards: the club wants him for depth, but letting him go
    /// leaves it at the floor, not below it. The wage policy holds.
    #[test]
    fn a_club_at_the_floor_without_him_keeps_to_its_wage_policy() {
        use crate::contract_wage_policy::WagePolicyVerdict;
        let mut game = over_policy_forward(2);
        // An unhappy player asks for everything he can; a content one would settle for a modest
        // raise that fits the policy (see `a_content_player_settles_for_a_modest_raise`).
        game.players
            .iter_mut()
            .find(|player| player.id == "costly_fwd")
            .unwrap()
            .morale = 40;
        assert!(needed_for_depth(
            &SquadFacts::of(&game, "ai"),
            game.players.last().unwrap()
        ));
        assert_eq!(
            verdict_for(&game, "costly_fwd"),
            WagePolicyVerdict::OverPolicy
        );
        let before = contract_end(&game, "costly_fwd");

        apply_ai_contract_decisions(&mut game, review_day());

        assert_eq!(contract_end(&game, "costly_fwd"), before);
    }

    /// Given an AI club with three keepers, one of them out on loan, when its
    /// fading fourth keeper's contract runs down, then he is kept: the keeper
    /// on loan plays for his borrower this week, so the club counts him as the
    /// squad floor does — not at all.
    #[test]
    fn a_keeper_out_on_loan_does_not_count_toward_the_depth_a_club_needs() {
        let mut game = world();
        let loaned = game
            .players
            .iter_mut()
            .find(|p| p.id == "ai_Goalkeeper2")
            .unwrap();
        loaned.team_id = Some("user".to_string());
        loaned.active_loan = Some(domain::player::ActiveLoan {
            parent_team_id: "ai".to_string(),
            loan_team_id: "user".to_string(),
            loan_team_name: None,
            start_date: "2026-07-01".to_string(),
            end_date: "2027-06-30".to_string(),
            wage_contribution_pct: 100,
            buy_option_fee: None,
            loan_start_minutes: 0,
            loan_start_appearances: 0,
            development_reported_minutes: 0,
            development_reported_appearances: 0,
        });
        game.players
            .push(player("old_keeper", "ai", Position::Goalkeeper, 50, 34, 90));
        let before = contract_end(&game, "old_keeper");

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(contract_end(&game, "old_keeper") > before);
    }

    /// Given an AI club whose best player's contract ends today, and today is
    /// the club's review day, when the day is played, then he is renewed and
    /// not released: the club decides before the expiry sweep runs, or the
    /// sweep would have made him a free agent first.
    #[test]
    fn a_day_renews_a_key_player_whose_contract_ends_that_day_before_releasing_him() {
        let mut game = world();
        let mut day = game.clock.current_date;
        while day.weekday().num_days_from_monday() != review_day() {
            day += Duration::days(1);
        }
        game.clock.current_date = day;
        let mut star = player("star", "ai", Position::Midfielder, 80, 27, 0);
        star.stage_contract_end(Some(day.format("%Y-%m-%d").to_string()));
        game.players.push(star);

        crate::turn::process_day(&mut game);

        let star = game.players.iter().find(|p| p.id == "star").unwrap();
        assert_eq!(star.team_id.as_deref(), Some("ai"), "he was released");
        assert!(
            star.contract_end() > Some(day.format("%Y-%m-%d").to_string().as_str()),
            "his contract was not renewed"
        );
    }

    // --- Ordinary squad planning ------------------------------------------------

    fn academy(id: &str, team_id: &str, position: Position, age: i32) -> Player {
        let mut youngster = player(id, team_id, position, 55, age, 1_000);
        youngster.squad_role = domain::player::SquadRole::Youth;
        youngster
    }

    fn free_agent(id: &str, position: Position, ovr: u8) -> Player {
        let mut agent = player(id, "ai", position, ovr, 27, 1_000);
        agent.team_id = None;
        agent.stage_contract_end(None);
        agent.stage_wage(0);
        agent
    }

    fn role(game: &Game, id: &str) -> domain::player::SquadRole {
        game.players.iter().find(|p| p.id == id).unwrap().squad_role
    }

    fn club_of<'a>(game: &'a Game, id: &str) -> Option<&'a str> {
        game.players
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.team_id.as_deref())
    }

    /// Given an AI club with a 22-year-old academy player, on its review day he
    /// graduates to the senior squad — the same age limit that stops a manager
    /// putting a player over 21 into the academy.
    #[test]
    fn an_ai_club_graduates_academy_players_past_the_academy_age() {
        let mut game = world();
        game.players
            .push(academy("graduate", "ai", Position::Midfielder, 22));
        game.players
            .push(academy("prospect", "ai", Position::Midfielder, 19));

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(role(&game, "graduate"), domain::player::SquadRole::Senior);
        assert_eq!(role(&game, "prospect"), domain::player::SquadRole::Youth);
    }

    /// Given an AI club with exactly two senior keepers and an academy keeper,
    /// on its review day the academy keeper is promoted: one above the floor.
    #[test]
    fn an_ai_club_at_the_floor_promotes_youth_to_keep_a_margin() {
        let mut game = world();
        game.players.retain(|p| p.id != "ai_Goalkeeper2");
        game.players
            .push(academy("young_keeper", "ai", Position::Goalkeeper, 18));
        game.players
            .push(free_agent("free_keeper", Position::Goalkeeper, 80));

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(
            role(&game, "young_keeper"),
            domain::player::SquadRole::Senior
        );
        assert_eq!(club_of(&game, "free_keeper"), None, "youth comes first");
    }

    /// Given an AI club with exactly two senior keepers, no academy keeper, and
    /// a free-agent keeper within its wage policy, he is signed.
    #[test]
    fn an_ai_club_at_the_floor_signs_a_free_agent_it_can_afford() {
        let mut game = world();
        game.players.retain(|p| p.id != "ai_Goalkeeper2");
        game.players
            .push(free_agent("free_keeper", Position::Goalkeeper, 70));

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(club_of(&game, "free_keeper"), Some("ai"));
        let keeper = game.players.iter().find(|p| p.id == "free_keeper").unwrap();
        let entry = keeper
            .movement_history
            .last()
            .expect("the signing is recorded");
        assert_eq!(
            entry.kind,
            domain::player::PlayerMovementKind::FreeAgentSigning
        );
        assert_eq!(entry.to_team_id.as_deref(), Some("ai"));
    }

    fn signed_by_ai(game: &Game, ids: &[&str]) -> usize {
        ids.iter()
            .filter(|id| club_of(game, id) == Some("ai"))
            .count()
    }

    /// Given an AI club one keeper below its margin and three affordable free-agent keepers, when
    /// it plans its squad, then it signs exactly one: the signing is counted before it asks again.
    #[test]
    fn a_club_one_keeper_below_its_margin_signs_exactly_one_of_three_free_keepers() {
        let mut game = world();
        game.players.retain(|p| p.id != "ai_Goalkeeper2");
        let keepers = ["free_keeper_a", "free_keeper_b", "free_keeper_c"];
        for id in keepers {
            game.players.push(free_agent(id, Position::Goalkeeper, 70));
        }

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(signed_by_ai(&game, &keepers), 1);
    }

    /// Given an AI club whose groups all meet their margin but whose squad is two short of the
    /// planning target, and five affordable free-agent midfielders, when it plans its squad, then
    /// it signs exactly two: each signing is counted toward the squad before it asks again.
    #[test]
    fn a_club_two_below_its_squad_target_signs_exactly_two_of_five_free_players() {
        let mut game = world();
        game.players.retain(|p| {
            !["ai_Defender5", "ai_Midfielder5", "ai_Forward3"].contains(&p.id.as_str())
        });
        let agents = ["free_a", "free_b", "free_c", "free_d", "free_e"];
        for id in agents {
            game.players.push(free_agent(id, Position::Midfielder, 70));
        }

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(signed_by_ai(&game, &agents), 2);
    }

    /// Given a club that meets its target and has room under its wage budget, a free agent
    /// better than its cover and one worse
    /// When it plans its squad
    /// Then it signs the better one, only him, and only one a review.
    #[test]
    fn a_club_with_room_signs_a_free_agent_better_than_its_cover() {
        let mut game = world();
        for other in game.players.iter_mut() {
            if other.team_id.as_deref() == Some("ai") {
                other.stage_wage(500);
            }
        }
        game.players
            .push(free_agent("better", Position::Midfielder, 70));
        game.players
            .push(free_agent("also_better", Position::Midfielder, 68));
        game.players
            .push(free_agent("worse", Position::Midfielder, 50));

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(club_of(&game, "better"), Some("ai"));
        assert_eq!(club_of(&game, "also_better"), None, "one a review");
        assert_eq!(club_of(&game, "worse"), None);
    }

    fn set_age(game: &mut Game, id: &str, age: i32) {
        let player = game.players.iter_mut().find(|p| p.id == id).unwrap();
        player.date_of_birth = format!("{}-01-01", TODAY.0 - age);
    }

    fn set_rating(game: &mut Game, id: &str, ovr: u8, potential: u8) {
        let player = game.players.iter_mut().find(|p| p.id == id).unwrap();
        player.ovr = ovr;
        player.potential = potential;
    }

    /// A club whose first-choice keeper is 35 (rated 80) with two keepers of 60 and 58 behind him,
    /// and a 19-year-old free-agent keeper rated 56 who is expected to be 65 next year: no better
    /// than the cover today, clearly better than it a year on.
    fn club_with_an_ageing_keeper(starter_age: i32) -> Game {
        let mut game = world();
        for other in game.players.iter_mut() {
            if other.team_id.as_deref() == Some("ai") {
                other.stage_wage(500);
            }
        }
        set_rating(&mut game, "ai_Goalkeeper0", 80, 80);
        set_age(&mut game, "ai_Goalkeeper0", starter_age);
        set_rating(&mut game, "ai_Goalkeeper1", 60, 60);
        set_rating(&mut game, "ai_Goalkeeper2", 58, 58);
        let mut prospect = free_agent("prospect", Position::Goalkeeper, 56);
        prospect.potential = 78;
        prospect.date_of_birth = format!("{}-01-01", TODAY.0 - 19);
        game.players.push(prospect);
        game
    }

    /// Given a club whose 35-year-old keeper is its only good one, and a young free-agent keeper
    /// who is not better than the cover today but will be next year
    /// When it plans its squad
    /// Then it signs him: the succession is planned before the keeper retires, not after.
    #[test]
    fn a_club_with_an_ageing_keeper_signs_his_successor_before_he_retires() {
        let mut game = club_with_an_ageing_keeper(35);

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(club_of(&game, "prospect"), Some("ai"));
    }

    /// Given the same club, with a keeper of 27
    /// When it plans its squad
    /// Then it does not sign the prospect: nothing is about to leave, and he is no better than
    /// the cover it has. The control that makes the case above about the keeper's age.
    #[test]
    fn a_club_with_a_keeper_in_his_prime_signs_no_successor() {
        let mut game = club_with_an_ageing_keeper(27);

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(club_of(&game, "prospect"), None);
    }

    /// Given a club with an ageing keeper, an academy keeper who will be good enough next year, and
    /// a free agent who will be too
    /// When it plans its squad
    /// Then it promotes its own: the succession is built at home first, and costs no wage.
    #[test]
    fn a_club_promotes_its_own_academy_keeper_as_the_successor() {
        let mut game = club_with_an_ageing_keeper(35);
        let mut youth = academy("home_keeper", "ai", Position::Goalkeeper, 18);
        youth.ovr = 54;
        youth.potential = 80;
        game.players.push(youth);

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(
            role(&game, "home_keeper"),
            domain::player::SquadRole::Senior
        );
        assert_eq!(club_of(&game, "prospect"), None);
    }

    /// Given a club with a keeper of 35 and three young free-agent keepers, each expected to be better
    /// than its cover a year on
    /// When it plans its squad five reviews running
    /// Then it signs two and stops: a succession replaces a departure, it does not stack the group
    /// (a keeper group plans for three; the transition year carries one more).
    #[test]
    fn a_club_signs_successors_up_to_one_above_its_depth_and_no_more() {
        let mut game = club_with_an_ageing_keeper(35);
        for (id, ovr) in [("prospect_b", 57), ("prospect_c", 58)] {
            let mut keeper = free_agent(id, Position::Goalkeeper, ovr);
            keeper.potential = 78;
            keeper.date_of_birth = format!("{}-01-01", TODAY.0 - 19);
            game.players.push(keeper);
        }
        for _ in 0..5 {
            apply_ai_squad_planning(&mut game, review_day());
        }
        assert_eq!(
            signed_by_ai(&game, &["prospect", "prospect_b", "prospect_c"]),
            2
        );
    }

    /// Given a keeper of 35 who is the best the club has, a keeper of 36 likewise (a boundary pair)
    /// When contracts running down are reviewed
    /// Then the 35-year-old keeper is renewed (a keeper's renewal age is 35) and the 36-year-old is not.
    #[test]
    fn a_keeper_is_renewed_at_thirty_five_and_not_at_thirty_six() {
        let renewed = |age| {
            let mut game = world();
            game.players.push(player(
                "old_keeper",
                "ai",
                Position::Goalkeeper,
                75,
                age,
                90,
            ));
            let before = contract_end(&game, "old_keeper");
            apply_ai_contract_decisions(&mut game, review_day());
            contract_end(&game, "old_keeper") != before
        };
        assert!(renewed(35));
        assert!(!renewed(36));
    }

    /// Given a club of twenty-four seniors, and a free agent better than any of them
    /// When it plans its squad
    /// Then it signs nobody: it is full.
    #[test]
    fn a_full_squad_signs_no_one_to_improve_itself() {
        let mut game = world();
        for index in 0..6 {
            game.players.push(player(
                &format!("extra{index}"),
                "ai",
                Position::Midfielder,
                60,
                26,
                900,
            ));
        }
        game.players
            .push(free_agent("star", Position::Midfielder, 90));

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(club_of(&game, "star"), None);
    }

    /// The same, but paying him would break the wage policy: he is not signed,
    /// because the club is at its keeper minimum, not below it.
    #[test]
    fn an_ai_club_does_not_break_its_wage_policy_for_a_margin() {
        let mut game = world();
        game.players.retain(|p| p.id != "ai_Goalkeeper2");
        for other in game.players.iter_mut() {
            if other.team_id.as_deref() == Some("ai") {
                other.stage_wage(0);
            }
        }
        let mut costly = free_agent("costly_keeper", Position::Goalkeeper, 85);
        costly.market_value = 40_000_000;
        game.players.push(costly);
        game.teams[1].wage_budget = crate::finances::calc_wages(&game, "ai");

        apply_ai_squad_planning(&mut game, review_day());

        assert_eq!(club_of(&game, "costly_keeper"), None);
    }

    /// Given the player's club at its keeper minimum with an academy keeper,
    /// its review day brings nobody in: the manager decides.
    #[test]
    fn the_players_club_is_never_planned_for() {
        let mut game = world();
        game.players.retain(|p| p.id != "user_Goalkeeper2");
        game.players.push(academy(
            "user_young_keeper",
            "user",
            Position::Goalkeeper,
            18,
        ));
        let user_day = crate::ai_tactics::review_weekday("user");

        apply_ai_squad_planning(&mut game, user_day);

        assert_eq!(
            role(&game, "user_young_keeper"),
            domain::player::SquadRole::Youth
        );
    }

    #[test]
    fn a_contract_with_years_to_run_is_not_touched() {
        let mut game = world();
        game.players
            .push(player("star", "ai", Position::Midfielder, 75, 27, 800));
        let before = contract_end(&game, "star");

        apply_ai_contract_decisions(&mut game, review_day());

        assert_eq!(contract_end(&game, "star"), before);
    }

    /// A young player is kept for what he may become, not what he is: rated 45 today, but with a
    /// potential well above the squad's median, he is renewed.
    #[test]
    fn a_prospect_is_kept_whatever_his_rating() {
        let mut game = world();
        let mut prospect = player("prospect", "ai", Position::Forward, 45, 20, 90);
        prospect.potential = 85;
        game.players.push(prospect);
        let before = contract_end(&game, "prospect");

        apply_ai_contract_decisions(&mut game, review_day());

        assert!(contract_end(&game, "prospect") > before);
    }

    /// Given a young player who is rated 45 and whose potential is 55, in a squad whose median
    /// is 60
    /// When his contract runs down
    /// Then he is not renewed for being young: he will never be good enough for this squad.
    #[test]
    fn a_young_player_who_will_never_reach_the_squad_is_not_renewed() {
        let mut game = world();
        let mut hopeless = player("hopeless", "ai", Position::Forward, 45, 20, 90);
        hopeless.potential = 55;
        game.players.push(hopeless);
        let before = contract_end(&game, "hopeless");

        apply_ai_contract_decisions(&mut game, review_day());

        assert_eq!(contract_end(&game, "hopeless"), before);
    }

    /// Four years of contracts running out, one day at a time. Both clubs start
    /// alike — contracts ending across the next four years, ages spread from
    /// prospect to veteran — and only the AI club has anyone to renew them:
    /// it must still be at the squad floor at the end, while the player's club,
    /// which nobody renews for here, runs below it. No top-up is involved.
    #[test]
    fn an_ai_club_keeps_its_squad_across_seasons_by_renewing_it() {
        use crate::squad_floor::squad_shortfall;

        let mut game = world();
        for (index, player) in game.players.iter_mut().enumerate() {
            let days_left = 60 + (index as i64 * 97) % 1_400;
            player.stage_contract_end(Some(
                (today() + Duration::days(days_left))
                    .format("%Y-%m-%d")
                    .to_string(),
            ));
            let age = 19 + (index as i32 * 5) % 15;
            player.date_of_birth = format!("{}-01-01", TODAY.0 - age);
        }

        for _ in 0..4 * 365 {
            let weekday = game.clock.current_date.weekday().num_days_from_monday();
            apply_ai_contract_decisions(&mut game, weekday);
            crate::contracts::process_contract_expiries(&mut game);
            game.clock.advance_days(1);
        }

        assert!(
            squad_shortfall(&game, "ai").is_empty(),
            "the AI club fell below the floor: {:?}",
            squad_shortfall(&game, "ai")
        );
        assert!(
            !squad_shortfall(&game, "user").is_empty(),
            "the club nobody renewed for kept its squad too, so this test proves nothing"
        );
    }

    #[test]
    fn a_content_player_settles_for_a_modest_raise() {
        let mut content = player("c", "ai", Position::Midfielder, 70, 26, 100);
        content.morale = 80;
        content.stage_wage(1_000);
        // He could ask for 1 400; he settles for +10 %.
        assert_eq!(settled_renewal_wage(&content, 1_400, false), 1_100);
        // An ask below the cap is not raised to it.
        assert_eq!(settled_renewal_wage(&content, 1_050, false), 1_050);
        // A player who is not content asks for all of it.
        content.morale = 40;
        assert_eq!(settled_renewal_wage(&content, 1_400, false), 1_400);
    }

    /// Given a club over its wage budget and a content player on 1 000 who asks 1 400, when his
    /// renewal is settled, then his wage is frozen: a club that cannot pay what it pays does not
    /// pay more.
    #[test]
    fn a_club_over_its_wage_budget_freezes_a_content_players_wage() {
        let mut content = player("c", "ai", Position::Midfielder, 70, 26, 100);
        content.morale = 80;
        content.stage_wage(1_000);
        assert_eq!(settled_renewal_wage(&content, 1_400, true), 1_000);
        // The control: the same player at a club inside its budget takes the modest raise.
        assert_eq!(settled_renewal_wage(&content, 1_400, false), 1_100);
    }

    /// Given the same club over budget and a player who is not content, when his renewal is
    /// settled, then he still asks for everything, so the board's wage policy decides and he
    /// leaves if it refuses: a freeze is never imposed on an unhappy player.
    #[test]
    fn an_unhappy_player_is_not_frozen_by_an_over_budget_club() {
        let mut unhappy = player("u", "ai", Position::Midfielder, 70, 26, 100);
        unhappy.morale = 40;
        unhappy.stage_wage(1_000);
        assert_eq!(settled_renewal_wage(&unhappy, 1_400, true), 1_400);
    }

    /// Given a content player whose ask is below his current wage, when the club is over budget,
    /// then the freeze never raises him: it is a cap, not a target.
    #[test]
    fn a_freeze_never_raises_a_wage() {
        let mut content = player("c", "ai", Position::Midfielder, 70, 26, 100);
        content.morale = 80;
        content.stage_wage(1_000);
        assert_eq!(settled_renewal_wage(&content, 900, true), 900);
    }

    #[test]
    fn a_player_with_no_wage_on_record_asks_what_he_asks() {
        let mut free = player("f", "ai", Position::Midfielder, 70, 26, 100);
        free.morale = 90;
        free.stage_wage(0);
        assert_eq!(settled_renewal_wage(&free, 1_400, true), 1_400);
    }
}
