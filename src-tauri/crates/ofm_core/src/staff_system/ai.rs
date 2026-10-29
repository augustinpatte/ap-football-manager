//! The AI clubs' staff: they fill their vacant posts, renew the members whose contracts are
//! running out and shed what they cannot afford, through the same rules (`market`) as the
//! manager. Each club is looked at once a week, on a weekday its id fixes, so the work is
//! spread over the week and a replayed season decides the same.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use domain::message::{InboxMessage, MessageCategory, MessagePriority};
use domain::staff::{Staff, StaffPost};

use super::market::{self, OfferResponse};
use crate::game::Game;
use crate::stable_hash::stable_hash;

/// Free agents the market keeps for the manager: an AI club never takes the last of them.
pub const RESERVED_FREE_AGENTS: usize = 6;
/// An AI club does not keep a member it can no longer afford past this share of its budget.
const OVER_BUDGET_PCT: i64 = 115;
/// Days before a contract ends that the manager is warned.
pub const EXPIRY_WARNING_DAYS: i64 = 45;
/// Lowest overall an AI club will renew at.
const RENEWAL_FLOOR: u8 = 35;

fn club_day(team_id: &str) -> u32 {
    (stable_hash(team_id.as_bytes(), 0) % 7) as u32
}

fn parse(date: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date?, "%Y-%m-%d").ok()
}

/// A free agent's value for the post: quality first, cheaper wins ties, id last.
fn better(a: &Staff, b: &Staff) -> std::cmp::Ordering {
    a.overall()
        .cmp(&b.overall())
        .then(b.wage.cmp(&a.wage))
        .then(b.id.cmp(&a.id))
}

/// Looks after the staff of every AI club whose day it is.
pub fn apply_ai_staff_management(game: &mut Game, weekday_num: u32) {
    let user_team = game.manager.team_id.clone();
    let today = game.clock.current_date.date_naive();
    let due: Vec<String> = game
        .teams
        .iter()
        .filter(|t| user_team.as_deref() != Some(t.id.as_str()) && club_day(&t.id) == weekday_num)
        .map(|t| t.id.clone())
        .collect();
    if due.is_empty() {
        return;
    }

    // One pass over the staff gives every due club its people.
    let due_set: std::collections::HashSet<&str> = due.iter().map(String::as_str).collect();
    let mut by_team: HashMap<&str, Vec<&Staff>> = HashMap::new();
    for member in &game.staff {
        if let Some(team) = member.team_id.as_deref()
            && due_set.contains(team)
        {
            by_team.entry(team).or_default().push(member);
        }
    }

    // Decisions are made against the state at the start of the pass, then applied.
    #[derive(Debug)]
    enum Step {
        Renew(String, String),
        Let(String),
        Release(String, String),
        Fill(String, StaffPost),
    }
    let mut steps: Vec<Step> = Vec::new();
    for team_id in &due {
        let members = by_team.get(team_id.as_str()).cloned().unwrap_or_default();
        // Contracts that have run out or are about to.
        for member in &members {
            let Some(end) = parse(member.contract_end.as_deref()) else { continue };
            if (end - today).num_days() > 14 {
                continue;
            }
            if member.overall() >= RENEWAL_FLOOR {
                steps.push(Step::Renew(team_id.clone(), member.id.clone()));
            } else {
                steps.push(Step::Let(member.id.clone()));
            }
        }
        // Over budget: let the dearest member of the least valuable seat go.
        let bill: i64 = members.iter().map(|m| i64::from(m.wage)).sum();
        let budget = market::staff_budget(game, team_id);
        let finance = game.teams.iter().find(|t| &t.id == team_id).map_or(0, |t| t.finance);
        if bill * 100 > budget * OVER_BUDGET_PCT && finance < 0 {
            let counts: HashMap<StaffPost, usize> =
                members.iter().fold(HashMap::new(), |mut m, s| {
                    *m.entry(s.post()).or_default() += 1;
                    m
                });
            if let Some(worst) = members
                .iter()
                .filter(|m| counts[&m.post()] > m.post().required())
                .max_by_key(|m| (m.wage, std::cmp::Reverse(m.overall())))
            {
                steps.push(Step::Release(team_id.clone(), worst.id.clone()));
            }
        }
        // Vacant required posts.
        for status in market::structure_of(&members) {
            if status.is_vacant() {
                steps.push(Step::Fill(team_id.clone(), status.post));
            }
        }
    }
    drop(by_team);

    for step in steps {
        match step {
            Step::Renew(team_id, staff_id) => {
                let Some(member) = game.staff.iter().find(|s| s.id == staff_id) else { continue };
                let ask = market::renewal_ask(member, club_rep(game, &team_id));
                let years = if member.overall() >= 70 { 3 } else { 2 };
                // A renewal that does not fit the budget lets the member go.
                if market::negotiate_renewal(game, &team_id, &staff_id, ask, years)
                    .map_or(true, |r| r != OfferResponse::Accepted)
                {
                    let _ = market::release_expired(game, &staff_id);
                }
            }
            Step::Let(staff_id) => {
                let _ = market::release_expired(game, &staff_id);
            }
            Step::Release(team_id, staff_id) => {
                let _ = market::release(game, &team_id, &staff_id);
            }
            Step::Fill(team_id, post) => fill_post(game, &team_id, post),
        }
    }
}

fn club_rep(game: &Game, team_id: &str) -> u32 {
    game.teams.iter().find(|t| t.id == team_id).map_or(0, |t| t.reputation)
}

/// Signs the best free agent for `post` that the club can afford.
fn fill_post(game: &mut Game, team_id: &str, post: StaffPost) {
    let free = game.staff.iter().filter(|s| s.team_id.is_none()).count();
    if free <= RESERVED_FREE_AGENTS {
        return;
    }
    let reputation = club_rep(game, team_id);
    let mut candidates: Vec<&Staff> = game
        .staff
        .iter()
        .filter(|s| s.team_id.is_none() && s.post() == post)
        .filter(|s| market::hire_blocker(game, team_id, &s.id, market::asking_wage(s, reputation)).is_none())
        .collect();
    candidates.sort_by(|a, b| better(b, a));
    let Some(choice) = candidates.first() else { return };
    let (id, wage) = (choice.id.clone(), market::asking_wage(choice, reputation));
    let _ = market::hire(game, team_id, &id, wage, 2);
}

/// Staff of the user's club whose contract is ending soon are announced once; those whose
/// contract has ended leave. Runs daily.
pub fn process_user_staff_expiry(game: &mut Game) {
    let Some(team_id) = game.manager.team_id.clone() else { return };
    let today = game.clock.current_date.date_naive();
    let stamp = today.format("%Y-%m-%d").to_string();
    let mut warnings = Vec::new();
    let mut leaving = Vec::new();
    for member in game.staff.iter().filter(|s| s.team_id.as_deref() == Some(team_id.as_str())) {
        let Some(end) = parse(member.contract_end.as_deref()) else { continue };
        let left = (end - today).num_days();
        if left <= 0 {
            leaving.push((member.id.clone(), member.display_name.clone()));
        } else if left <= EXPIRY_WARNING_DAYS {
            warnings.push((member.id.clone(), member.display_name.clone(), left, end.year()));
        }
    }
    for (id, name, days, year) in warnings {
        let key = format!("staff_contract_ending_{id}_{year}");
        crate::inbox::emit_once(game, &key, || {
            staff_message(&key, "be.msg.staffContractEnding", &[("name", &name), ("days", &days.to_string())], &stamp)
        });
    }
    for (id, name) in leaving {
        if market::release_expired(game, &id).is_ok() {
            let key = format!("staff_contract_ended_{id}_{stamp}");
            crate::inbox::emit_once(game, &key, || {
                staff_message(&key, "be.msg.staffContractEnded", &[("name", &name)], &stamp)
            });
        }
    }
}

/// An inbox message from the assistant about the staff.
pub fn staff_message(id: &str, key: &str, params: &[(&str, &str)], date: &str) -> InboxMessage {
    InboxMessage::new(id.to_string(), String::new(), String::new(), String::new(), date.to_string())
        .with_category(MessageCategory::Contract)
        .with_priority(MessagePriority::Normal)
        .with_i18n(
            &format!("{key}.subject"),
            &format!("{key}.body"),
            params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<std::collections::HashMap<_, _>>(),
        )
        .with_sender_i18n("be.sender.assistantManager", "be.role.assistantManager")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use chrono::{TimeZone, Utc};
    use domain::manager::Manager;
    use domain::staff::StaffAttributes;
    use domain::team::Team;

    fn member(id: &str, post: StaffPost, level: u8, wage: u32, team: Option<&str>) -> Staff {
        let mut s = Staff::new(
            id.into(), "A".into(), id.into(), "1980-01-01".into(), post.role(),
            StaffAttributes { coaching: level, judging_ability: level, judging_potential: level, physiotherapy: level },
        );
        s.specialization = post.specialization();
        s.post = Some(post);
        s.wage = wage;
        s.team_id = team.map(str::to_string);
        s
    }

    /// A user club "user" and an AI club "ai" whose day is today.
    fn world(staff: Vec<Staff>) -> (Game, u32) {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap());
        let mut manager = Manager::new("m".into(), "T".into(), "M".into(), "1980-01-01".into(), "England".into());
        manager.hire("user".into());
        let mk = |id: &str| {
            let mut t = Team::new(id.into(), id.into(), id.into(), "England".into(), "L".into(), "S".into(), 20_000);
            t.finance = 5_000_000;
            t.wage_budget = 200_000;
            t.reputation = 600;
            t
        };
        let game = Game::new(clock, manager, vec![mk("user"), mk("ai")], vec![], staff, vec![]);
        (game, club_day("ai"))
    }

    fn all_posts_but(post: StaffPost) -> Vec<Staff> {
        StaffPost::ALL
            .iter()
            .filter(|p| p.required() > 0 && **p != post)
            .map(|p| member(&format!("have-{p:?}"), *p, 60, 1000, Some("ai")))
            .collect()
    }

    #[test]
    fn an_ai_club_fills_a_vacant_post_with_the_best_member_it_can_afford() {
        let mut staff = all_posts_but(StaffPost::Physio);
        staff.push(member("cheap", StaffPost::Physio, 55, 1000, None));
        staff.push(member("best", StaffPost::Physio, 80, 2000, None));
        staff.push(member("too-dear", StaffPost::Physio, 95, 500_000, None));
        for i in 0..8 { staff.push(member(&format!("pad{i}"), StaffPost::Scout, 40, 500, None)); }
        let (mut game, day) = world(staff);
        apply_ai_staff_management(&mut game, day);
        let physio: Vec<_> = game.staff.iter().filter(|s| s.team_id.as_deref() == Some("ai") && s.post() == StaffPost::Physio).collect();
        assert_eq!(physio.len(), 1);
        assert_eq!(physio[0].id, "best");
    }

    #[test]
    fn an_ai_club_never_hires_without_budget() {
        let mut staff = all_posts_but(StaffPost::Physio);
        staff.push(member("only", StaffPost::Physio, 80, 90_000, None));
        for i in 0..8 { staff.push(member(&format!("pad{i}"), StaffPost::Scout, 40, 500, None)); }
        let (mut game, day) = world(staff);
        apply_ai_staff_management(&mut game, day);
        assert!(game.staff.iter().find(|s| s.id == "only").unwrap().team_id.is_none());
    }

    #[test]
    fn an_ai_club_leaves_the_last_free_agents_for_the_manager() {
        let mut staff = all_posts_but(StaffPost::Physio);
        staff.push(member("only", StaffPost::Physio, 80, 2000, None));
        let (mut game, day) = world(staff);
        apply_ai_staff_management(&mut game, day);
        assert!(game.staff.iter().find(|s| s.id == "only").unwrap().team_id.is_none());
    }

    #[test]
    fn a_good_member_with_an_ending_contract_is_renewed_and_a_poor_one_leaves() {
        let mut staff = all_posts_but(StaffPost::Scout);
        let mut good = member("good", StaffPost::Scout, 70, 1000, Some("ai"));
        good.contract_end = Some("2026-08-05".into());
        let mut poor = member("poor", StaffPost::Scout, 20, 1000, Some("ai"));
        poor.contract_end = Some("2026-08-05".into());
        staff.push(good);
        staff.push(poor);
        for i in 0..8 { staff.push(member(&format!("pad{i}"), StaffPost::Physio, 40, 500, None)); }
        let (mut game, day) = world(staff);
        apply_ai_staff_management(&mut game, day);
        let good = game.staff.iter().find(|s| s.id == "good").unwrap();
        assert_eq!(good.team_id.as_deref(), Some("ai"));
        assert!(good.contract_end.as_deref().unwrap() > "2027-01-01");
        assert!(game.staff.iter().find(|s| s.id == "poor").unwrap().team_id.is_none());
    }

    #[test]
    fn the_users_club_is_left_alone_by_the_ai() {
        let mut staff = vec![member("ex", StaffPost::Scout, 70, 1000, Some("user"))];
        staff[0].contract_end = Some("2026-08-01".into());
        let (mut game, _) = world(staff);
        for day in 0..7 { apply_ai_staff_management(&mut game, day); }
        assert_eq!(game.staff[0].team_id.as_deref(), Some("user"));
    }

    #[test]
    fn the_manager_is_warned_once_and_the_member_leaves_when_the_contract_ends() {
        let mut m = member("ex", StaffPost::Scout, 70, 1000, Some("user"));
        m.contract_end = Some("2026-08-20".into());
        let (mut game, _) = world(vec![m]);
        process_user_staff_expiry(&mut game);
        process_user_staff_expiry(&mut game);
        assert_eq!(game.messages.len(), 1);
        game.clock.current_date = Utc.with_ymd_and_hms(2026, 8, 21, 12, 0, 0).unwrap();
        process_user_staff_expiry(&mut game);
        assert!(game.staff[0].team_id.is_none());
    }
}
