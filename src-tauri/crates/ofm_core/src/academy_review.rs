//! The manager's academy review: where each academy player stands, in terms the manager can act
//! on — ready for the senior squad, in need of more development, worth a loan, or unlikely to
//! progress — with the facts behind the verdict and what each action would do.
//!
//! The verdict is advice drawn from the club's own squad model and from a player's own numbers;
//! it changes no player and no rating. Readiness is [`youth_pathway::pathway_of`], the same
//! question the AI clubs ask when they promote, so the manager's screen and an AI club's academy
//! agree about what "ready" means. The rest (loan, unlikely) are plain comparisons of the player's
//! rating and ceiling with the cover the senior squad already has in his position, and each is
//! given as a set of reasons the screen can show.
//!
//! The ceiling is the one figure the manager is not given. The development model's `potential`
//! stays exact on the player; the review reads a *believed* copy of him whose ceiling is the middle
//! of the band his youth staff assesses ([`crate::academy_assessment`]), and decides everything
//! (verdict, reasons, order) on that copy. A figure the manager cannot see therefore cannot show
//! through a label, a sort or a reason.

use crate::academy_advice::{AdviceFacts, RowAdvice, advise};
use crate::academy_assessment::{AcademyLens, YouthCoaching};
use crate::game::Game;
use crate::manager_identity::ManagerIdentity;
use crate::player_tier::A_TIER_MIN_RATING;
use crate::rating_view::RatingView;
use crate::squad_floor::group_index;
use crate::squad_intelligence::ClubSquad;
use crate::squad_intelligence::appetite::Appetite;
use crate::youth_pathway::{Pathway, as_senior, cover_levels, pathway_of};
use domain::player::{Player, Position, SquadRole};
use domain::team::TrainingFocus;
use serde::Serialize;

/// A youngster from this age has had time to show what he is; younger, the review does not call
/// him a loan case or a lost cause.
const JUDGED_FROM_AGE: i32 = 18;

/// How many rating points below the squad's cover a youngster may be and still gain from regular
/// senior-level football somewhere else rather than from another year of academy football.
const LOAN_REACH: i32 = 8;

/// Points of ceiling a loan has to be able to unlock: a player at his ceiling has nothing left
/// to develop, wherever he plays.
const LOAN_MIN_HEADROOM: i32 = 5;

/// A player is "unlikely" when even his ceiling leaves him this far under the squad's cover.
const UNLIKELY_MARGIN: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    ReadyForSenior,
    KeepDeveloping,
    LoanCandidate,
    UnlikelyToProgress,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcademyRow {
    pub player_id: String,
    pub full_name: String,
    pub age: i32,
    pub natural_position: Position,
    /// His rating, exact: he is the club's own player.
    pub ovr: u8,
    /// What the youth staff judge his ceiling to be: a band, never the number the development
    /// model holds. A ceiling is not a promise.
    pub ceiling: RatingView,
    pub verdict: Verdict,
    /// Translation-key suffixes under `academy.reason.`.
    pub reasons: Vec<String>,
    /// The rating of the senior squad's cover in his position group: the bar he is measured on.
    pub squad_cover: u8,
    pub seasons_left_in_academy: i32,
    pub injured: bool,
    pub on_loan: bool,
    pub loan_listed: bool,
    pub contract_end: Option<String>,
    pub weekly_wage: u32,
    /// His individual training focus; `None` when he follows his group's or the team's.
    pub training_focus: Option<TrainingFocus>,
    /// Whether a senior place is free for him today; when not, the key says why.
    pub promotion_blocked: Option<String>,
    /// Whether each action is recommended, available or blocked, and why.
    pub advice: RowAdvice,
}

/// The academy as a whole, in what the manager is allowed to know about it.
#[derive(Debug, Clone, Serialize)]
pub struct AcademySummary {
    pub coaching: YouthCoaching,
    /// The middle of the assessed ceilings, averaged; `None` with nobody in the academy.
    pub average_ceiling: Option<u8>,
    /// Youngsters whose ceiling is assessed to reach at least the game's A tier even on the low
    /// reading: the ones the staff are sure about.
    pub high_ceiling_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcademyReview {
    pub rows: Vec<AcademyRow>,
    pub summary: Option<AcademySummary>,
    pub seniors: usize,
    pub senior_limit: usize,
    /// Academy players the club holds per position group, `[GK, DEF, MID, FWD]`, against the
    /// academy it aims for.
    pub by_group: [usize; 4],
    pub target_by_group: [usize; 4],
    /// Youngsters the next season-end intake will bring in, by the same plan the intake uses.
    pub next_intake: usize,
}

/// The refusal (a translation key) when `team_id` has no senior place for a promoted youngster:
/// the same rule every signing route answers to, including the places promised to deals waiting
/// to register and to seniors out on loan. `None` when he can be registered.
pub fn promotion_place_refusal(game: &Game, team_id: &str) -> Option<String> {
    crate::transfers::signing_place_refusal(game, team_id, "")
}

/// What the review needs of the club once, for every row.
struct ReviewContext<'a> {
    today: chrono::NaiveDate,
    covers: [u8; 4],
    squad: Option<ClubSquad>,
    youth_trust: f64,
    place_refusal: Option<String>,
    lens: &'a AcademyLens<'a>,
}

/// The review of the manager's own academy; `None` when the manager has no club.
pub fn academy_review(game: &Game) -> Option<AcademyReview> {
    let team_id = game.manager.team_id.as_deref()?;
    let team = game.teams.iter().find(|t| t.id == team_id)?;
    let lens = AcademyLens::of(game)?;
    let club: Vec<usize> = game
        .players
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.team_id.as_deref() == Some(team_id) || p.contract_club_id() == Some(team_id)
        })
        .map(|(i, _)| i)
        .collect();
    let context = ReviewContext {
        today: game.clock.current_date.date_naive(),
        covers: cover_levels(game, &club),
        squad: ClubSquad::of_for_planning(game, team_id, Appetite::NEUTRAL),
        youth_trust: ManagerIdentity::for_club(game, Some(team)).youth_trust,
        place_refusal: promotion_place_refusal(game, team_id),
        lens: &lens,
    };
    let capacity = crate::contracts::squad_capacity(game, team_id);

    let academy: Vec<&Player> = club
        .iter()
        .map(|&i| &game.players[i])
        .filter(|p| {
            p.squad_role == SquadRole::Youth && !p.retired && p.contract_club_id() == Some(team_id)
        })
        .collect();
    let mut assessed: Vec<(AcademyRow, u8)> = academy.iter().map(|p| row_of(p, &context)).collect();
    // Best first by verdict, then by the ceiling the staff believe in; a tie falls to the id, which
    // says nothing about the ceiling. The sort key is the band's middle, never the true figure.
    assessed.sort_by(|(a, a_ceiling), (b, b_ceiling)| {
        verdict_rank(a.verdict)
            .cmp(&verdict_rank(b.verdict))
            .then(b_ceiling.cmp(a_ceiling))
            .then(a.player_id.cmp(&b.player_id))
    });
    let summary = summary_of(&lens, &assessed);
    let rows = assessed.into_iter().map(|(row, _)| row).collect();

    let mut by_group = [0usize; 4];
    for p in &academy {
        by_group[group_index(&p.position)] += 1;
    }
    Some(AcademyReview {
        rows,
        summary: Some(summary),
        seniors: capacity.registered_seniors,
        senior_limit: capacity.senior_limit,
        by_group,
        target_by_group: crate::youth_intake::ACADEMY_TARGET_PER_GROUP,
        next_intake: crate::youth_intake::plan_for(academy.iter().copied())
            .groups
            .len(),
    })
}

fn summary_of(lens: &AcademyLens, assessed: &[(AcademyRow, u8)]) -> AcademySummary {
    let average_ceiling = (!assessed.is_empty()).then(|| {
        (assessed.iter().map(|(_, mid)| u32::from(*mid)).sum::<u32>() / assessed.len() as u32) as u8
    });
    let high_ceiling_count = assessed
        .iter()
        .filter(|(row, _)| {
            matches!(&row.ceiling, RatingView::Estimate { low, .. } if *low >= A_TIER_MIN_RATING)
        })
        .count();
    AcademySummary {
        coaching: lens.coaching(),
        average_ceiling,
        high_ceiling_count,
    }
}

fn verdict_rank(verdict: Verdict) -> u8 {
    match verdict {
        Verdict::ReadyForSenior => 0,
        Verdict::LoanCandidate => 1,
        Verdict::KeepDeveloping => 2,
        Verdict::UnlikelyToProgress => 3,
    }
}

/// The row and the middle of his assessed ceiling, which the order of the list sorts on.
fn row_of(player: &Player, context: &ReviewContext) -> (AcademyRow, u8) {
    let believed = context.lens.believed(player);
    let ceiling = context
        .lens
        .ceiling_of(player)
        .unwrap_or(RatingView::Exact { ovr: player.ovr });
    let (_, optimistic) = ceiling.bounds();
    let age = domain::age::age_on_or_unknown(context.today, &player.date_of_birth);
    let cover = context.covers[group_index(&player.position)];
    let expected = crate::squad_outlook::expected_ovr_for(&believed, context.today);
    let pathway = context.squad.as_ref().and_then(|squad| {
        squad
            .signing(&as_senior(&believed), 0.0)
            .map(|marginal| pathway_of(&marginal, age, context.youth_trust))
    });
    let (verdict, reasons) = judge(&believed, optimistic, age, cover, expected, pathway);
    let contract_days_left =
        crate::contracts::contract_days_remaining(player.contract_end(), context.today);
    let advice = advise(&AdviceFacts {
        verdict,
        age,
        injured: player.injury.is_some(),
        on_loan: player.active_loan.is_some(),
        loan_listed: player.loan_listed,
        place_refusal: context.place_refusal.as_deref(),
        contract_days_left,
    });
    let row = AcademyRow {
        player_id: player.id.clone(),
        full_name: player.full_name.clone(),
        age,
        natural_position: player.natural_position.clone(),
        ovr: player.ovr,
        ceiling,
        verdict,
        reasons,
        squad_cover: cover,
        seasons_left_in_academy: (crate::roster::YOUTH_ACADEMY_MAX_AGE - age + 1).max(0),
        injured: player.injury.is_some(),
        on_loan: player.active_loan.is_some(),
        loan_listed: player.loan_listed,
        contract_end: player.contract_end().map(str::to_string),
        weekly_wage: player.wage(),
        training_focus: player.training_focus.clone(),
        promotion_blocked: context.place_refusal.clone(),
        advice,
    };
    (row, believed.potential)
}

/// The verdict and the reasons behind it. Ordered: a player the squad can use now is ready
/// whatever his age; then the cases that need a judgement of his future, which only a player old
/// enough to have shown it gets.
///
/// `player` is the believed copy (his ceiling is the middle of the assessed band) and
/// `optimistic_ceiling` the top of the band: a youngster is written off only when even the staff's
/// most hopeful reading leaves him under the cover.
fn judge(
    player: &Player,
    optimistic_ceiling: u8,
    age: i32,
    cover: u8,
    expected: u8,
    pathway: Option<Pathway>,
) -> (Verdict, Vec<String>) {
    let ovr = i32::from(player.ovr);
    let ceiling = i32::from(player.potential.max(player.ovr));
    let cover = i32::from(cover);
    let mut reasons = Vec::new();

    match pathway {
        Some(Pathway::Starter) => {
            reasons.push("wouldStart".to_string());
            return (Verdict::ReadyForSenior, reasons);
        }
        Some(Pathway::Cover) => {
            reasons.push("addsCover".to_string());
            return (Verdict::ReadyForSenior, reasons);
        }
        Some(Pathway::Succession) => reasons.push("succession".to_string()),
        Some(Pathway::NotYet) | None => {}
    }
    if player.injury.is_some() {
        reasons.push("injured".to_string());
    }
    if age < JUDGED_FROM_AGE {
        reasons.push("tooYoungToJudge".to_string());
        return (Verdict::KeepDeveloping, reasons);
    }
    if i32::from(optimistic_ceiling) + UNLIKELY_MARGIN < cover {
        reasons.push("ceilingBelowCover".to_string());
        return (Verdict::UnlikelyToProgress, reasons);
    }
    let headroom = ceiling - ovr;
    if player.active_loan.is_none()
        && player.injury.is_none()
        && headroom >= LOAN_MIN_HEADROOM
        && ovr + LOAN_REACH >= cover
    {
        reasons.push("closeToSeniorLevel".to_string());
        reasons.push("needsMatchMinutes".to_string());
        return (Verdict::LoanCandidate, reasons);
    }
    if i32::from(expected) > ovr {
        reasons.push("stillImproving".to_string());
    } else {
        reasons.push("nearCeiling".to_string());
    }
    (Verdict::KeepDeveloping, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::squad_intelligence::test_squads::{
        Squad, player as senior_of, team, today, with_potential,
    };
    use domain::player::PlayerAttributes;
    use domain::staff::{Staff, StaffAttributes, StaffPost};

    /// `judge` with the optimistic end of the band set to the player's own ceiling: the verdict
    /// rules, apart from how the band is read.
    fn judge_exact(
        player: &Player,
        age: i32,
        cover: u8,
        expected: u8,
        pathway: Option<Pathway>,
    ) -> (Verdict, Vec<String>) {
        judge(player, player.potential, age, cover, expected, pathway)
    }

    fn youth(ovr: u8, potential: u8) -> Player {
        let attrs: PlayerAttributes = serde_json::from_value(serde_json::json!({
            "pace": 50, "stamina": 50, "strength": 50, "agility": 50, "passing": 50,
            "shooting": 50, "tackling": 50, "dribbling": 50, "defending": 50,
            "positioning": 50, "vision": 50, "decisions": 50, "composure": 50,
            "aggression": 50, "teamwork": 50, "leadership": 50,
            "handling": 50, "reflexes": 50, "aerial": 50
        }))
        .unwrap();
        let mut p = Player::new(
            "y".into(),
            "Y".into(),
            "Y Y".into(),
            "2006-01-01".into(),
            "GB".into(),
            Position::Midfielder,
            attrs,
        );
        p.squad_role = SquadRole::Youth;
        p.ovr = ovr;
        p.potential = potential;
        p
    }

    /// Given the squad model says a youngster would start, or add cover
    /// When he is judged
    /// Then he is ready, whatever his age, and the reason says which.
    #[test]
    fn a_player_the_squad_can_use_is_ready_at_any_age() {
        let (verdict, reasons) = judge_exact(&youth(70, 75), 16, 65, 71, Some(Pathway::Starter));
        assert_eq!(verdict, Verdict::ReadyForSenior);
        assert_eq!(reasons, vec!["wouldStart"]);
        assert_eq!(
            judge_exact(&youth(60, 70), 17, 62, 62, Some(Pathway::Cover)).0,
            Verdict::ReadyForSenior
        );
    }

    /// Given a sixteen-year-old far below the cover
    /// When he is judged
    /// Then the review does not write him off: too young to judge.
    #[test]
    fn a_child_is_never_called_a_lost_cause() {
        let (verdict, reasons) = judge_exact(&youth(35, 45), 16, 70, 38, Some(Pathway::NotYet));
        assert_eq!(verdict, Verdict::KeepDeveloping);
        assert!(reasons.contains(&"tooYoungToJudge".to_string()));
    }

    /// Given a nineteen-year-old whose ceiling is below the cover he would have to beat
    /// When he is judged
    /// Then he is unlikely to progress, with the reason; and the same player with a ceiling at the
    ///      cover is not.
    #[test]
    fn a_ceiling_below_the_cover_is_unlikely_to_progress() {
        let (verdict, reasons) = judge_exact(&youth(52, 60), 19, 68, 55, Some(Pathway::NotYet));
        assert_eq!(verdict, Verdict::UnlikelyToProgress);
        assert!(reasons.contains(&"ceilingBelowCover".to_string()));
        assert_ne!(
            judge_exact(&youth(52, 68), 19, 68, 55, Some(Pathway::NotYet)).0,
            Verdict::UnlikelyToProgress
        );
    }

    /// Given a nineteen-year-old close to the cover with room to grow
    /// When he is judged
    /// Then he is a loan candidate; injured, or already out on loan, or at his ceiling, he is not.
    #[test]
    fn a_player_close_to_senior_level_with_room_to_grow_is_a_loan_candidate() {
        assert_eq!(
            judge_exact(&youth(62, 74), 19, 66, 65, Some(Pathway::NotYet)).0,
            Verdict::LoanCandidate
        );
        let mut hurt = youth(62, 74);
        hurt.injury = Some(domain::player::Injury {
            name: "x".into(),
            days_remaining: 10,
        });
        assert_eq!(
            judge_exact(&hurt, 19, 66, 65, None).0,
            Verdict::KeepDeveloping
        );
        assert_eq!(
            judge_exact(&youth(62, 64), 19, 66, 63, None).0,
            Verdict::KeepDeveloping,
            "no headroom: a loan unlocks nothing"
        );
    }

    /// Given a youngster far below the cover but with a high ceiling
    /// When he is judged
    /// Then he keeps developing in the academy: potential alone does not make a loan case.
    #[test]
    fn a_distant_prospect_stays_in_the_academy() {
        let (verdict, reasons) = judge_exact(&youth(45, 80), 18, 70, 52, Some(Pathway::NotYet));
        assert_eq!(verdict, Verdict::KeepDeveloping);
        assert!(reasons.contains(&"stillImproving".to_string()));
    }

    // ── the manager's view never carries the true ceiling ──────────────────────────────────────

    /// The manager's club: a balanced senior squad, and these academy players (id, position, rating,
    /// true ceiling, age), the staff given.
    fn club_with(
        academy: &[(&str, domain::player::Position, u8, u8, i32)],
        staff: Vec<Staff>,
    ) -> Game {
        use chrono::TimeZone;
        let mut manager = domain::manager::Manager::new(
            "m".into(),
            "A".into(),
            "B".into(),
            "1980-01-01".into(),
            "England".into(),
        );
        manager.hire("club".into());
        let mut players = Squad::balanced().players;
        for (id, position, ovr, ceiling, age) in academy {
            let mut youngster =
                with_potential(senior_of(id, position.clone(), *ovr, *age), *ceiling);
            youngster.squad_role = SquadRole::Youth;
            youngster.ovr = *ovr;
            players.push(youngster);
        }
        let clock = crate::clock::GameClock::new(
            chrono::Utc.from_utc_datetime(&today().and_hms_opt(12, 0, 0).unwrap()),
        );
        let mut game = Game::new(
            clock,
            manager,
            vec![team("4-4-2", 5_000_000, 600_000)],
            players,
            staff,
            vec![],
        );
        for player in &mut game.players {
            player.team_id = Some("club".into());
        }
        game
    }

    fn youth_staff(level: u8) -> Staff {
        let mut staff = Staff::new(
            format!("yc-{level}"),
            "Y".into(),
            "C".into(),
            "1975-01-01".into(),
            StaffPost::HeadOfYouth.role(),
            StaffAttributes {
                coaching: level,
                judging_ability: level,
                judging_potential: level,
                physiotherapy: level,
            },
        );
        staff.specialization = StaffPost::HeadOfYouth.specialization();
        staff.post = Some(StaffPost::HeadOfYouth);
        staff.team_id = Some("club".into());
        staff
    }

    fn row_json(game: &Game, id: &str) -> serde_json::Value {
        let review = academy_review(game).expect("the manager has a club");
        let row = review.rows.iter().find(|r| r.player_id == id).expect("row");
        serde_json::to_value(row).unwrap()
    }

    /// Without the identity fields, so two different youngsters can be compared.
    fn anonymous(mut row: serde_json::Value) -> serde_json::Value {
        let object = row.as_object_mut().unwrap();
        for key in ["player_id", "full_name"] {
            object.remove(key);
        }
        row
    }

    /// Given two academy players identical but for a true ceiling that falls in one assessment band,
    /// and ceilings that would have given them different verdicts if the review had read them
    /// When the review reads them
    /// Then every field of their rows matches: verdict, reasons, band and advice give nothing away.
    #[test]
    fn two_players_whose_ceilings_share_a_band_read_identically() {
        use domain::player::Position::Midfielder;
        let cover = academy_review(&club_with(&[("probe", Midfielder, 40, 45, 19)], vec![]))
            .unwrap()
            .rows[0]
            .squad_cover;
        // The ovr at which a ceiling at the band's foot leaves no room for a loan (under the five
        // points a loan has to unlock) and one at its top leaves plenty, inside one 12-point band.
        let (ovr, low, high) = (cover.saturating_sub(6)..=cover)
            .find_map(|ovr| {
                let start = (ovr + 4 - 1) / 12 * 12 + 1;
                let (low, high) = (start.max(ovr), start + 11);
                (high >= ovr + LOAN_MIN_HEADROOM as u8 && low < ovr + LOAN_MIN_HEADROOM as u8)
                    .then_some((ovr, low, high))
            })
            .expect("a rating near the cover that straddles a band");
        let honest = |ceiling: u8| judge_exact(&youth(ovr, ceiling), 19, cover, ovr, None).0;
        assert_ne!(
            honest(low),
            honest(high),
            "the fixture must discriminate: read directly, the two ceilings give different verdicts"
        );
        let game = club_with(
            &[
                ("low", Midfielder, ovr, low, 19),
                ("high", Midfielder, ovr, high, 19),
            ],
            vec![],
        );

        assert_eq!(
            anonymous(row_json(&game, "low")),
            anonymous(row_json(&game, "high"))
        );
    }

    /// Given an academy and the whole review serialized
    /// When the wire is searched
    /// Then neither the potential nor the development model's next-year rating is a field of any row.
    #[test]
    fn the_review_wire_has_no_potential_and_no_expected_rating() {
        use domain::player::Position::Midfielder;
        let game = club_with(&[("y", Midfielder, 50, 68, 19)], vec![]);
        let wire = serde_json::to_string(&academy_review(&game).unwrap()).unwrap();

        for key in ["\"potential\"", "expected_ovr", "next_year"] {
            assert!(!wire.contains(key), "{key} leaked in {wire}");
        }
    }

    /// Given two players of one verdict whose true ceilings order one way and then the other
    /// When the review lists them
    /// Then the list does not follow the true ceiling: inside one band the id decides.
    #[test]
    fn the_default_order_does_not_follow_the_true_ceiling_inside_a_band() {
        use domain::player::Position::Midfielder;
        let order = |a_ceiling: u8, b_ceiling: u8| -> Vec<String> {
            let game = club_with(
                &[
                    ("a", Midfielder, 50, a_ceiling, 19),
                    ("b", Midfielder, 50, b_ceiling, 19),
                ],
                vec![],
            );
            academy_review(&game)
                .unwrap()
                .rows
                .into_iter()
                .map(|r| r.player_id)
                .collect()
        };

        assert_eq!(order(62, 71), order(71, 62));
        assert_eq!(order(62, 71), vec!["a", "b"]);
    }

    /// Given a ceiling far above another's, in different bands
    /// When the review lists them (the same verdict)
    /// Then the higher assessed band comes first: the order still means something.
    #[test]
    fn a_clearly_higher_assessed_ceiling_is_listed_first() {
        use domain::player::Position::Midfielder;
        let game = club_with(
            &[("a", Midfielder, 50, 60, 19), ("b", Midfielder, 50, 90, 19)],
            vec![],
        );

        let rows = academy_review(&game).unwrap().rows;
        assert_eq!(rows[0].player_id, "b");
    }

    /// Given a club with no youth staff and one with a strong head of youth
    /// When the same player is reviewed
    /// Then the strong department reads a narrower band inside the wide one, and the summary
    /// says why.
    #[test]
    fn a_strong_youth_department_narrows_the_band_it_gives() {
        use domain::player::Position::Midfielder;
        let academy = [("y", Midfielder, 50, 68, 19)];
        let band =
            |game: &Game| match serde_json::from_value::<serde_json::Value>(row_json(game, "y"))
                .unwrap()["ceiling"]
                .clone()
            {
                serde_json::Value::Object(o) => (
                    o["low"].as_u64().unwrap(),
                    o["high"].as_u64().unwrap(),
                    o["confidence"].as_str().unwrap().to_string(),
                ),
                other => panic!("{other}"),
            };
        let bare = club_with(&academy, vec![]);
        let strong = club_with(&academy, vec![youth_staff(90)]);

        let (bare_low, bare_high, bare_trust) = band(&bare);
        let (low, high, trust) = band(&strong);
        assert!(high - low < bare_high - bare_low);
        assert!(bare_low <= low && high <= bare_high);
        assert_eq!((bare_trust.as_str(), trust.as_str()), ("low", "high"));
        let summary = academy_review(&strong).unwrap().summary.unwrap();
        assert!(summary.coaching.has_staff && summary.coaching.progression_bonus_pct > 0);
        assert!(
            !academy_review(&bare)
                .unwrap()
                .summary
                .unwrap()
                .coaching
                .has_staff
        );
    }

    /// Given a game saved to JSON and loaded again
    /// When the academy is reviewed before and after
    /// Then the assessments, verdicts and order are identical: nothing is drawn or stored.
    #[test]
    fn the_assessment_is_the_same_after_a_save_and_reload() {
        use domain::player::Position::{Midfielder, Striker};
        let game = club_with(
            &[
                ("a", Midfielder, 50, 68, 19),
                ("b", Striker, 61, 84, 17),
                ("c", Midfielder, 44, 52, 20),
            ],
            vec![youth_staff(60)],
        );
        let reloaded: Game = serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();

        let before = serde_json::to_value(academy_review(&game).unwrap()).unwrap();
        let after = serde_json::to_value(academy_review(&reloaded).unwrap()).unwrap();
        assert_eq!(before, after);
        assert_eq!(
            before,
            serde_json::to_value(academy_review(&game).unwrap()).unwrap()
        );
    }

    /// Given the review reading a believed copy of each youngster
    /// When the true ceiling is changed inside the band
    /// Then the stored player still holds the exact value: the simulation's number is untouched.
    #[test]
    fn the_review_does_not_change_the_players_true_ceiling() {
        use domain::player::Position::Midfielder;
        let game = club_with(&[("y", Midfielder, 50, 68, 19)], vec![]);
        let _ = academy_review(&game);

        let stored = game.players.iter().find(|p| p.id == "y").unwrap();
        assert_eq!((stored.ovr, stored.potential), (50, 68));
    }
}
