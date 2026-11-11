//! The in-match manager for a side the player does not command: the opponent's, and the
//! player's own when the assistant has been handed the match.
//!
//! [`decide`] is a pure function of what a manager on the touchline can see (score, clock,
//! cards, injuries, tiredness, the bench) and of the club's tactical identity. It has no random
//! number, no clock of its own and no handle on the engine, so a fixed state gives a fixed
//! answer and it cannot know the future. It returns *orders*; the controller sends them through
//! the same command pipeline as the player's, under the side's own actor, so every legality rule
//! (allowances, windows, eligibility) is M12's, not this file's.
//!
//! It is consulted at checkpoints only: the half-time break, after a dismissal, after an
//! injury, and at a bounded review interval ([`REVIEW_EVERY_MS`]). Never every tick.

use std::collections::BTreeMap;

use apfm_match_lab::ids::PlayerId;
use apfm_match_lab::input::MatchInput;
use apfm_match_lab::model::{Attributes, Position};
use apfm_match_lab::pitch::Side;
use apfm_match_lab::sim::GameState;
use apfm_match_lab::sim::football::manager::{
    Line, ManagerConfig, SubContext, best_formation, line_of, need_v2, performance,
    substitution_utility_v2,
};
use apfm_match_lab::sim::formation::FormationPreset;
use apfm_match_lab::sim::session::{PlayerView, SessionView, TeamView};
use domain::team::{PlayStyle, PressingIntensity, Tempo};

use super::plan::{TacticalChange, TacticalPlan};

/// A side is reviewed at most this often between checkpoints (simulated milliseconds).
pub const REVIEW_EVERY_MS: u32 = 10 * 60_000;
/// After a tactical change by the AI, the next discretionary one waits this long, so a side
/// does not flap between plans. A dismissal or the break is never delayed by it.
pub const TACTICAL_COOLDOWN_MS: u32 = 6 * 60_000;
/// Condition below which a player is a candidate to be rested (1 = fresh).
const TIRED_BELOW: f32 = 0.45;
/// First minute at which tiredness alone justifies a substitution.
const REST_FROM_MINUTE: u32 = 55;
/// Substitutions kept back for injuries until this minute.
const INJURY_RESERVE_UNTIL_MINUTE: u32 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checkpoint {
    Halftime,
    Dismissal,
    Injury,
    Review,
}

/// The kinds of order, for "the assistant stands back from what the manager just did".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Tactics,
    Formation,
    Substitution,
}

impl Group {
    pub const COUNT: usize = 3;
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AiOrder {
    Tactics(TacticalChange),
    Substitute { off: PlayerId, on: PlayerId },
}

/// What the AI may know about the players: how good each is in a given role.
#[derive(Debug, Clone, Default)]
pub struct Squad {
    players: BTreeMap<PlayerId, (Attributes, Position)>,
}

impl Squad {
    pub fn of(input: &MatchInput) -> Self {
        let players = input
            .home
            .players
            .iter()
            .chain(&input.away.players)
            .map(|p| (p.id.clone(), (p.attributes.clone(), p.natural_position)))
            .collect();
        Self { players }
    }

    fn known(&self, id: &PlayerId) -> Option<&(Attributes, Position)> {
        self.players.get(id)
    }
}

/// The autonomous manager's own judgement of a swap (`manager::substitution_utility_v2`): the quality of
/// the replacement in the slot against the player leaving, each at his present level of fatigue, plus
/// the tactical value of the swap for the score and the clock (an attacker when chasing, a defender
/// when protecting), less what a change costs. Positive means worth making. Run F4.2: this replaces
/// OFM's own `rating x condition` comparison, so a bench player is valued the same way whether the match
/// is managed here or simulated at once. What stays OFM's is when a manager looks (checkpoints,
/// windows, the allowance), not how a swap is valued.
fn swap_utility(ctx: &AiContext<'_>, off: &PlayerView, on: &PlayerView, slot: Position) -> f32 {
    let (Some((off_attrs, _)), Some((on_attrs, on_natural))) =
        (ctx.squad.known(&off.id), ctx.squad.known(&on.id))
    else {
        return f32::NEG_INFINITY;
    };
    // The horizon is the part of the match being played (regulation, then extra time), and the
    // autonomous manager measures what is left of *that*, so a swap in extra time is judged against
    // the thirty minutes of extra time, not against ninety.
    let regulation_min = (ctx.regulation_ms as f32 / 60_000.0).max(1.0);
    let horizon_min = if ctx.view.clock_ms as f32 / 60_000.0 <= regulation_min {
        regulation_min
    } else {
        EXTRA_TIME_MINUTES
    };
    let left = minutes_left(ctx);
    let remaining = (left / horizon_min).clamp(0.0, 1.0);
    let own = ctx.own();
    let sub = SubContext {
        minute: 1.0 - remaining,
        remaining,
        score_diff: ctx.goal_difference(),
        used: own.substitutions_used,
        max: own.substitutions_used + own.substitutions_remaining,
    };
    // the autonomous manager gates a weakening swap on how much the score demands a change, 0..1
    let (chase, protect) = need_v2(&ManagerConfig::m10(), ctx.goal_difference(), left);
    // the view's condition is one minus the long-term fatigue, which is what `performance` reads
    substitution_utility_v2(
        &ManagerConfig::m10(),
        &sub,
        slot,
        off_attrs,
        performance(1.0 - off.condition, 0.0, 0.0),
        on_attrs,
        *on_natural,
        performance(1.0 - on.condition, 0.0, 0.0),
        chase.max(protect),
        None,
    )
}

pub struct AiContext<'a> {
    pub side: Side,
    pub view: &'a SessionView,
    /// The side's plan right now.
    pub plan: &'a TacticalPlan,
    /// The side's plan at kickoff: the club's identity, which the AI returns to when the
    /// match gives it no reason to deviate.
    pub identity: &'a TacticalPlan,
    pub squad: &'a Squad,
    pub checkpoint: Checkpoint,
    pub regulation_ms: u32,
    /// Groups of orders the AI must leave alone (the manager just did them).
    pub blocked: [bool; Group::COUNT],
    /// When the AI last changed this side's tactics.
    pub last_tactical_change_ms: Option<u32>,
}

impl AiContext<'_> {
    fn own(&self) -> &TeamView {
        team(self.view, self.side)
    }

    fn opponent(&self) -> &TeamView {
        team(self.view, self.side.opponent())
    }

    fn minute(&self) -> u32 {
        self.view.clock_ms / 60_000
    }

    fn goal_difference(&self) -> i32 {
        let (home, away) = (
            i32::from(self.view.score_home),
            i32::from(self.view.score_away),
        );
        if self.side == Side::Home {
            home - away
        } else {
            away - home
        }
    }

    fn at_the_break(&self) -> bool {
        self.view.phase == GameState::HalfTime
    }
}

fn team(view: &SessionView, side: Side) -> &TeamView {
    if side == Side::Home {
        &view.home
    } else {
        &view.away
    }
}

/// Everything the side's manager would do at this checkpoint, in the order to send it.
pub fn decide(ctx: &AiContext<'_>) -> Vec<AiOrder> {
    if ctx.view.finished {
        return Vec::new();
    }
    let mut orders = Vec::new();
    let mut spent = 0u8;
    if !ctx.blocked[Group::Substitution.index()] {
        for (off, on) in substitutions(ctx) {
            spent += 1;
            orders.push(AiOrder::Substitute { off, on });
            if spent >= allowance(ctx) {
                break;
            }
        }
    }
    if let Some(change) = posture(ctx) {
        orders.push(AiOrder::Tactics(change));
    }
    orders
}

/// Substitutions the team can still make: the allowance less what is already queued.
fn allowance(ctx: &AiContext<'_>) -> u8 {
    let own = ctx.own();
    let queued = own.pending_substitutions.len() as u8;
    let by_window = if ctx.at_the_break() || own.windows_remaining > 0 {
        u8::MAX
    } else {
        0
    };
    own.substitutions_remaining
        .saturating_sub(queued)
        .min(by_window)
}

fn substitutions(ctx: &AiContext<'_>) -> Vec<(PlayerId, PlayerId)> {
    let mut left = allowance(ctx);
    let mut taken_off: Vec<PlayerId> = ctx
        .own()
        .pending_substitutions
        .iter()
        .map(|p| p.off.clone())
        .collect();
    let mut taken_on: Vec<PlayerId> = ctx
        .own()
        .pending_substitutions
        .iter()
        .map(|p| p.on.clone())
        .collect();
    let mut swaps = Vec::new();
    let pick = |off: &PlayerView,
                left: &mut u8,
                taken_off: &mut Vec<PlayerId>,
                taken_on: &mut Vec<PlayerId>| {
        let role = off.role?;
        let on = best_replacement(ctx, off, role, taken_on)?;
        *left -= 1;
        taken_off.push(off.id.clone());
        taken_on.push(on.clone());
        Some((off.id.clone(), on))
    };

    // 1. an injured player goes off at once
    let mut hurt: Vec<&PlayerView> = ctx
        .own()
        .on_pitch
        .iter()
        .filter(|p| p.injured && !taken_off.contains(&p.id))
        .collect();
    hurt.sort_by(|a, b| a.id.0.cmp(&b.id.0));
    for p in hurt {
        if left == 0 {
            return swaps;
        }
        swaps.extend(pick(p, &mut left, &mut taken_off, &mut taken_on));
    }

    // 2. tired players, late enough, keeping one change back for an injury
    let minute = ctx.minute();
    let reserve = u8::from(minute < INJURY_RESERVE_UNTIL_MINUTE);
    if minute >= REST_FROM_MINUTE || ctx.at_the_break() {
        let mut tired: Vec<&PlayerView> = ctx
            .own()
            .on_pitch
            .iter()
            .filter(|p| {
                p.condition < TIRED_BELOW
                    && p.role != Some(Position::Gk)
                    && !p.injured
                    && !taken_off.contains(&p.id)
            })
            .collect();
        tired.sort_by(|a, b| {
            a.condition
                .total_cmp(&b.condition)
                .then(a.id.0.cmp(&b.id.0))
        });
        for p in tired.into_iter().take(2) {
            if left <= reserve {
                break;
            }
            swaps.extend(pick(p, &mut left, &mut taken_off, &mut taken_on));
        }
    }

    // 3. chasing the game, or protecting it
    swaps.extend(tactical_swap(ctx, &mut left, &mut taken_off, &mut taken_on));
    swaps
}

/// Trailing late: an attacker for a tired defender or midfielder. Leading late: a defender for
/// an attacker. At most one, and only with a change to spare.
fn tactical_swap(
    ctx: &AiContext<'_>,
    left: &mut u8,
    taken_off: &mut Vec<PlayerId>,
    taken_on: &mut Vec<PlayerId>,
) -> Option<(PlayerId, PlayerId)> {
    let diff = ctx.goal_difference();
    let minute = ctx.minute();
    if *left == 0 || diff == 0 || minute < 65 || ctx.checkpoint == Checkpoint::Injury {
        return None;
    }
    let (give_up, bring_in): (Line, Line) = if diff < 0 {
        (Line::Midfield, Line::Attack)
    } else if minute >= 75 {
        (Line::Attack, Line::Defence)
    } else {
        return None;
    };
    let off = ctx
        .own()
        .on_pitch
        .iter()
        .filter(|p| {
            p.role.is_some_and(|r| line_of(r) == give_up)
                && !p.injured
                && !taken_off.contains(&p.id)
        })
        .min_by(|a, b| {
            a.condition
                .total_cmp(&b.condition)
                .then(a.id.0.cmp(&b.id.0))
        })?;
    let role = off.role?;
    let on = ctx
        .own()
        .bench
        .iter()
        .filter(|b| {
            !b.injured
                && !taken_on.contains(&b.id)
                && b.natural_position != Position::Gk
                && line_of(b.natural_position) == bring_in
        })
        .max_by(|a, b| {
            swap_utility(ctx, off, a, role)
                .total_cmp(&swap_utility(ctx, off, b, role))
                .then(b.id.0.cmp(&a.id.0))
        })?;
    *left -= 1;
    taken_off.push(off.id.clone());
    taken_on.push(on.id.clone());
    Some((off.id.clone(), on.id.clone()))
}

/// The bench player the autonomous manager's utility likes best for `off`'s slot, only if the swap
/// is worth making (an injured player goes off whatever the utility says).
fn best_replacement(
    ctx: &AiContext<'_>,
    off: &PlayerView,
    role: Position,
    taken_on: &[PlayerId],
) -> Option<PlayerId> {
    let goalkeeper = role == Position::Gk;
    ctx.own()
        .bench
        .iter()
        .filter(|b| {
            !b.injured
                && !taken_on.contains(&b.id)
                && (b.natural_position == Position::Gk) == goalkeeper
        })
        .map(|b| (swap_utility(ctx, off, b, role), b))
        .filter(|(value, _)| off.injured || *value > 0.0)
        .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.id.0.cmp(&a.1.id.0)))
        .map(|(_, b)| b.id.clone())
}

/// Minutes left in the horizon being played: the rest of regulation, or of extra time (two
/// fifteen-minute halves) once regulation is over.
/// Extra time is two fifteen-minute halves.
const EXTRA_TIME_MINUTES: f32 = 30.0;

fn minutes_left(ctx: &AiContext<'_>) -> f32 {
    let regulation = ctx.regulation_ms as f32 / 60_000.0;
    let now = ctx.view.clock_ms as f32 / 60_000.0;
    if now <= regulation {
        regulation - now
    } else {
        regulation + EXTRA_TIME_MINUTES - now
    }
}

/// How much more attacking (positive) or more cautious (negative) the situation asks the side
/// to be than its club identity does, on a scale of about -2 to +2.
///
/// The urgency of the score and the clock is the autonomous manager's own judgement
/// (`manager::need_v2`: goals behind against minutes left, a lead only needing protection once the
/// end is in sight), so a club behaves the same whether a match is managed here or simulated at once.
/// Only what follows from it, the OFM vocabulary the plan is expressed in, is this module's.
pub fn need(ctx: &AiContext<'_>) -> f32 {
    let (chase, protect) = need_v2(
        &ManagerConfig::m10(),
        ctx.goal_difference(),
        minutes_left(ctx),
    );
    let mut need = 2.0 * (chase - protect);
    let men = ctx.own().on_pitch.len() as i32 - ctx.opponent().on_pitch.len() as i32;
    if men < 0 {
        // short-handed: shore up, but a side that is behind still has to chase a little
        need -=
            f32::from(men.unsigned_abs() as u8) * if ctx.goal_difference() < 0 { 0.3 } else { 0.7 };
    } else if men > 0 {
        need += men as f32 * 0.5;
    }
    need
}

fn lean(style: &PlayStyle) -> f32 {
    match style {
        PlayStyle::Defensive => -1.0,
        PlayStyle::Counter => -0.5,
        PlayStyle::Balanced => 0.0,
        PlayStyle::Possession => 0.5,
        PlayStyle::HighPress => 1.0,
        PlayStyle::Attacking => 1.0,
    }
}

fn average_condition(team: &TeamView) -> f32 {
    let n = team.on_pitch.len().max(1) as f32;
    team.on_pitch.iter().map(|p| p.condition).sum::<f32>() / n
}

/// The tactical change the situation asks for, if any.
fn posture(ctx: &AiContext<'_>) -> Option<TacticalChange> {
    let immediate = matches!(ctx.checkpoint, Checkpoint::Halftime | Checkpoint::Dismissal);
    if !immediate
        && ctx
            .last_tactical_change_ms
            .is_some_and(|t| ctx.view.clock_ms.saturating_sub(t) < TACTICAL_COOLDOWN_MS)
    {
        return None;
    }
    let need = need(ctx);
    let target = lean(&ctx.identity.style) * 0.5 + need;
    let fresh = average_condition(ctx.own());
    let mut change = TacticalChange::default();

    if !ctx.blocked[Group::Tactics.index()] {
        let style = if target >= 1.2 {
            PlayStyle::Attacking
        } else if target >= 0.6 {
            if ctx.identity.style == PlayStyle::HighPress {
                PlayStyle::HighPress
            } else {
                PlayStyle::Possession
            }
        } else if target >= -0.3 {
            ctx.identity.style.clone()
        } else if target >= -0.9 {
            PlayStyle::Counter
        } else {
            PlayStyle::Defensive
        };
        let pressing = if need >= 0.9 && fresh >= 0.55 {
            PressingIntensity::Aggressive
        } else if need <= -0.8 || fresh < 0.4 {
            PressingIntensity::Passive
        } else {
            ctx.identity.phase.pressing_intensity.clone()
        };
        let tempo = if need >= 0.7 {
            Tempo::Direct
        } else if need <= -0.7 {
            Tempo::Patient
        } else {
            ctx.identity.phase.tempo.clone()
        };
        if style != ctx.plan.style {
            change.style = Some(style);
        }
        if pressing != ctx.plan.phase.pressing_intensity {
            change.pressing = Some(pressing);
        }
        if tempo != ctx.plan.phase.tempo {
            change.tempo = Some(tempo);
        }
    }

    if !ctx.blocked[Group::Formation.index()] && immediate {
        change.formation = formation_for(ctx);
    }
    (change != TacticalChange::default()).then_some(change)
}

/// The M12 preset a club's current formation is judged as. OFM supports shapes M12 has no preset for
/// (3-4-3, 4-1-4-1); such a club is read as the preset with the closest line counts (defenders,
/// midfielders, forwards; the earlier preset in `FormationPreset::ALL` wins a tie), so a dismissal still
/// asks the autonomous manager's question of it instead of being ignored.
fn nearest_preset(label: &str) -> Option<FormationPreset> {
    if let Some(preset) = FormationPreset::from_label(label) {
        return Some(preset);
    }
    let counts = |l: &str| -> Option<(i32, i32, i32)> {
        // every segment must be a small number: "4-x-2", an absurd label or stray text reads as no shape
        let n: Vec<i32> = l
            .split('-')
            .map(|p| p.parse::<u8>().ok().map(i32::from))
            .collect::<Option<_>>()?;
        // a middle line of several numbers (4-2-3-1) is one midfield
        (n.len() >= 3).then(|| (n[0], n[1..n.len() - 1].iter().sum(), n[n.len() - 1]))
    };
    let target = counts(label)?;
    FormationPreset::ALL
        .iter()
        .copied()
        .filter_map(|p| counts(p.label()).map(|c| (p, c)))
        .min_by_key(|(_, c)| {
            (c.0 - target.0).abs() + (c.1 - target.1).abs() + (c.2 - target.2).abs()
        })
        .map(|(p, _)| p)
}

/// The shape the situation asks for, if any: the autonomous manager's own judgement
/// (`manager::best_formation`, which weighs the forward presence and defensive cover of each preset
/// against what the score and the numbers need, and charges for a change and for going back to the
/// shape just left) applied to the presets M12 can build. OFM used to hard-code 4-5-1 for ten men and
/// 4-3-3 for a man up; the same function now picks, so a club reorganises the same way whether the
/// match is managed here or simulated at once.
fn formation_for(ctx: &AiContext<'_>) -> Option<String> {
    let men = ctx.own().on_pitch.len() as i32 - ctx.opponent().on_pitch.len() as i32;
    let current = nearest_preset(&ctx.plan.formation)?;
    let (chase, protect) = need_v2(
        &ManagerConfig::m10(),
        ctx.goal_difference(),
        minutes_left(ctx),
    );
    // the club's own shape is the one "just left" once the plan has moved away from it
    let previous = (ctx.plan.formation != ctx.identity.formation)
        .then(|| nearest_preset(&ctx.identity.formation))
        .flatten();
    best_formation(
        &ManagerConfig::m10(),
        current,
        previous,
        chase,
        protect,
        men,
    )
    .map(|preset| preset.label().to_string())
}

#[cfg(test)]
mod tests {
    use super::super::plan::supported_formations;
    use super::*;
    use apfm_match_lab::fixtures::identical_teams_input;
    use apfm_match_lab::model::Team;
    use apfm_match_lab::output::Period;
    use apfm_match_lab::sim::football::FootballStats;
    use apfm_match_lab::sim::football::manager::{formation_profile, formation_utility};
    use apfm_match_lab::sim::session::PendingSubstitution;

    fn player_view(team: &Team, id: &PlayerId, role: Option<Position>) -> PlayerView {
        let p = team.player(id).expect("squad member");
        PlayerView {
            id: id.clone(),
            name: p.name.clone(),
            shirt_number: p.shirt_number,
            natural_position: p.natural_position,
            role,
            condition: 1.0,
            minutes_played: 0.0,
            yellow_cards: 0,
            injured: false,
        }
    }

    fn team_view(side: Side, team: &Team) -> TeamView {
        TeamView {
            side,
            formation: "4-4-2".into(),
            tactics: team.tactics,
            on_pitch: team
                .lineup
                .iter()
                .map(|l| player_view(team, &l.player, Some(l.role)))
                .collect(),
            bench: team
                .bench
                .iter()
                .map(|id| player_view(team, id, None))
                .collect(),
            used: Vec::new(),
            sent_off: Vec::new(),
            substitutions_used: 0,
            substitutions_remaining: 5,
            windows_remaining: 3,
            pending_substitutions: Vec::new(),
        }
    }

    struct Fixture {
        view: SessionView,
        squad: Squad,
        plan: TacticalPlan,
        identity: TacticalPlan,
    }

    fn fixture(minute: u32, home: u8, away: u8) -> Fixture {
        let input = identical_teams_input(1, 65);
        let view = SessionView {
            session_id: "s".into(),
            revision: 1,
            clock_ms: minute * 60_000,
            period: if minute < 45 {
                Period::FirstHalf
            } else {
                Period::SecondHalf
            },
            phase: GameState::InPlay,
            paused: false,
            finished: false,
            score_home: home,
            score_away: away,
            home: team_view(Side::Home, &input.home),
            away: team_view(Side::Away, &input.away),
            event_count: 0,
            stats: FootballStats::default(),
        };
        let plan = TacticalPlan {
            style: PlayStyle::Balanced,
            phase: Default::default(),
            formation: "4-4-2".into(),
        };
        Fixture {
            squad: Squad::of(&input),
            view,
            identity: plan.clone(),
            plan,
        }
    }

    impl Fixture {
        fn ctx(&self, checkpoint: Checkpoint) -> AiContext<'_> {
            AiContext {
                side: Side::Home,
                view: &self.view,
                plan: &self.plan,
                identity: &self.identity,
                squad: &self.squad,
                checkpoint,
                regulation_ms: 90 * 60_000,
                blocked: [false; Group::COUNT],
                last_tactical_change_ms: None,
            }
        }

        fn tactics(&self, checkpoint: Checkpoint) -> Option<TacticalChange> {
            decide(&self.ctx(checkpoint))
                .into_iter()
                .find_map(|o| match o {
                    AiOrder::Tactics(c) => Some(c),
                    _ => None,
                })
        }

        fn outfield_starter(&self, n: usize) -> PlayerId {
            self.view
                .home
                .on_pitch
                .iter()
                .filter(|p| p.role != Some(Position::Gk))
                .nth(n)
                .unwrap()
                .id
                .clone()
        }
    }

    /// Given a side trailing by two goals late on,
    /// When its manager reviews,
    /// Then it goes more attacking: an attacking style, aggressive pressing, a direct tempo.
    #[test]
    fn a_side_losing_late_chases_the_game() {
        let mut f = fixture(78, 0, 2);
        f.plan.phase.tempo = Tempo::Patient; // a patient club: the chase asks for a faster one
        let c = f.tactics(Checkpoint::Review).expect("a reaction");
        assert_eq!(c.style, Some(PlayStyle::Attacking));
        assert_eq!(c.pressing, Some(PressingIntensity::Aggressive));
        assert_eq!(c.tempo, Some(Tempo::Direct));
    }

    /// Given a side leading by two goals late on,
    /// Then it protects the lead: a defensive style, passive pressing, a patient tempo.
    #[test]
    fn a_side_winning_late_protects_the_lead() {
        let f = fixture(80, 2, 0);
        let c = f.tactics(Checkpoint::Review).expect("a reaction");
        assert_eq!(c.style, Some(PlayStyle::Defensive));
        assert_eq!(c.pressing, Some(PressingIntensity::Passive));
        assert_eq!(c.tempo, Some(Tempo::Patient));
    }

    /// Given a level game early on, Then nothing changes: no reaction without a reason.
    #[test]
    fn a_level_game_early_changes_nothing() {
        let f = fixture(20, 0, 0);
        assert_eq!(decide(&f.ctx(Checkpoint::Review)), Vec::new());
    }

    /// The shape the autonomous manager's own utility ranks first for `current` in the given state,
    /// computed here from the raw presets and not through the adapter under test.
    fn autonomous_choice(
        current: &str,
        identity: &str,
        chase: f32,
        protect: f32,
        men: i32,
    ) -> Option<String> {
        let cur = FormationPreset::from_label(current).expect("a preset");
        FormationPreset::ALL
            .iter()
            .copied()
            .filter(|c| *c != cur)
            .map(|c| {
                let u = formation_utility(
                    &ManagerConfig::m10(),
                    &cur.slots(),
                    &c.slots(),
                    chase,
                    protect,
                    men,
                    current != identity && FormationPreset::from_label(identity) == Some(c),
                );
                (u, c)
            })
            .filter(|(u, _)| *u > 0.0)
            // the earlier preset wins a tie, as the autonomous manager's own loop does
            .fold(None::<(f32, FormationPreset)>, |best, x| match best {
                Some(b) if b.0 >= x.0 => Some(b),
                _ => Some(x),
            })
            .map(|(_, c)| c.label().to_string())
    }

    /// Given a side that has just lost a player while level,
    /// When the dismissal is the checkpoint,
    /// Then it reorganises at once into the shape the autonomous manager's own utility ranks first
    /// (one with more defensive cover than 4-4-2), and turns cautious.
    #[test]
    fn a_dismissal_makes_the_side_reorganise() {
        let mut f = fixture(40, 0, 0);
        let gone = f.view.home.on_pitch.remove(9);
        f.view.home.sent_off.push(gone);
        let c = f.tactics(Checkpoint::Dismissal).expect("a reaction");
        let (chase, protect) = need_v2(&ManagerConfig::m10(), 0, 50.0);
        let expected = autonomous_choice("4-4-2", "4-4-2", chase, protect, -1);
        assert_eq!(c.formation, expected);
        let picked = FormationPreset::from_label(c.formation.as_deref().expect("a shape change"))
            .expect("a preset");
        assert!(
            formation_profile(&picked.slots()).1
                > formation_profile(&FormationPreset::F442.slots()).1,
            "ten men get a shape with more cover"
        );
        assert!(supported_formations().contains(&picked.label().to_string()));
        assert_eq!(c.style, Some(PlayStyle::Counter));
    }

    /// Given a side that set up with a back five and is two goals behind late in the match,
    /// When the manager reviews at a checkpoint that allows a change of shape,
    /// Then it goes to 4-3-3, the preset with the most forward presence that the autonomous manager's
    /// utility prefers over its current shape: a fixed "ten men play 4-5-1" could never produce this.
    #[test]
    fn a_side_that_sat_back_pushes_up_when_it_must_chase() {
        let mut f = fixture(75, 0, 2);
        f.plan.formation = "5-3-2".into();
        let c = f.tactics(Checkpoint::Dismissal).expect("a reaction");
        assert_eq!(c.formation.as_deref(), Some("4-3-3"));
    }

    /// Given a side that is two goals ahead at half-time with all eleven on the pitch,
    /// When the manager reviews at the break,
    /// Then it protects the lead with a back five (the autonomous manager's utility ranks 5-3-2 first):
    /// the break is a checkpoint for a change of shape even with the numbers level.
    #[test]
    fn a_two_goal_lead_at_the_break_is_protected_with_a_back_five() {
        let f = fixture(45, 2, 0);
        let c = f.tactics(Checkpoint::Halftime).expect("a reaction");
        assert_eq!(c.formation.as_deref(), Some("5-3-2"));
    }

    /// Given the same two-goal deficit and a side now in a back five,
    /// When the club's own shape (the one it started with) is 4-3-3 and when it is 4-2-3-1,
    /// Then going back to the shape just left costs extra: it is chosen when it is not the club's own
    /// shape, and not when it is.
    #[test]
    fn going_back_to_the_shape_just_left_costs_extra() {
        let choose = |identity: &str| {
            let mut f = fixture(75, 0, 2);
            f.plan.formation = "5-3-2".into();
            f.identity.formation = identity.into();
            f.tactics(Checkpoint::Dismissal).and_then(|c| c.formation)
        };
        assert_eq!(choose("4-2-3-1").as_deref(), Some("4-3-3"));
        assert_ne!(choose("4-3-3").as_deref(), Some("4-3-3"));
    }

    /// Given the opponent down to ten and a level score,
    /// When the dismissal is the checkpoint,
    /// Then the shape stays: the autonomous manager sees no gain worth the cost of a change.
    #[test]
    fn a_level_game_a_man_up_keeps_its_shape() {
        let mut f = fixture(50, 0, 0);
        f.view.away.on_pitch.pop();
        let formation = f.tactics(Checkpoint::Dismissal).and_then(|c| c.formation);
        assert_eq!(formation, None);
    }

    /// Given a level game with all twenty-two on the pitch at the break,
    /// When the manager reviews at half-time,
    /// Then the shape stays: a change of formation has a cost, and nothing here earns it.
    #[test]
    fn a_level_half_time_keeps_the_shape() {
        let f = fixture(45, 0, 0);
        let formation = f.tactics(Checkpoint::Halftime).and_then(|c| c.formation);
        assert_eq!(formation, None);
    }

    /// Given a side three goals down with twenty-five minutes left and all eleven on the pitch,
    /// When the manager reviews at a checkpoint that allows a change of shape,
    /// Then the shape stays and the tactics sliders do the chasing: the autonomous manager's
    /// utility (`formation_utility`) puts a price on a change of shape that a deficit alone does not
    /// pay, and the assistant now judges it the same way.
    #[test]
    fn a_deficit_alone_is_chased_with_the_sliders_not_the_shape() {
        let f = fixture(65, 0, 3);
        let c = f.tactics(Checkpoint::Halftime).expect("a reaction");
        let (chase, protect) = need_v2(&ManagerConfig::m10(), -3, 25.0);
        assert_eq!(
            c.formation,
            autonomous_choice("4-4-2", "4-4-2", chase, protect, 0)
        );
        assert_eq!(c.formation, None);
        assert_eq!(
            c.style,
            Some(PlayStyle::Attacking),
            "it chases with the style"
        );
    }

    /// Given a side set up in 3-4-3, which M12 has no preset for, that has just lost a player,
    /// When the dismissal is the checkpoint,
    /// Then it is judged as the preset with the closest line counts and still reorganises: a club on a shape the engine cannot name is not left alone when it is a man down.
    #[test]
    fn a_club_on_a_shape_without_a_preset_is_still_reorganised() {
        assert_eq!(
            nearest_preset("3-4-3"),
            Some(FormationPreset::F442),
            "4-4-2, 4-3-3 and 3-5-2 are all two lines away; the earliest preset wins"
        );
        assert_eq!(
            nearest_preset("4-1-4-1"),
            Some(FormationPreset::F4231),
            "4-2-3-1 and 4-5-1 have the same line counts; the earlier preset wins"
        );
        assert_eq!(
            nearest_preset("5-3-2"),
            Some(FormationPreset::F532),
            "a preset is itself"
        );
        assert_eq!(nearest_preset("nonsense"), None);
        assert_eq!(
            nearest_preset("4-x-2"),
            None,
            "a segment that is not a number is no shape"
        );
        assert_eq!(
            nearest_preset("2000000000-2000000000-2000000000"),
            None,
            "an absurd label does not overflow"
        );
        let mut f = fixture(40, 0, 0);
        f.plan.formation = "3-4-3".into();
        f.identity.formation = "3-4-3".into();
        let gone = f.view.home.on_pitch.remove(9);
        f.view.home.sent_off.push(gone);
        let c = f.tactics(Checkpoint::Dismissal).expect("a reaction");
        assert!(
            c.formation.is_some(),
            "a man down, the shape is reconsidered"
        );
    }

    /// Given the same ten men, once with the score level and once a goal down late,
    /// When the manager chooses a shape,
    /// Then the choice follows the score: chasing, it keeps more forward presence than when level.
    /// A fixed "4-5-1 for ten men" would give the same shape in both.
    #[test]
    fn the_shape_chosen_for_ten_men_follows_the_score() {
        let shape_for = |home: u8, away: u8| {
            let mut f = fixture(75, home, away);
            let gone = f.view.home.on_pitch.remove(9);
            f.view.home.sent_off.push(gone);
            f.tactics(Checkpoint::Dismissal)
                .and_then(|c| c.formation)
                .and_then(|l| FormationPreset::from_label(&l))
                .map_or_else(
                    || formation_profile(&FormationPreset::F442.slots()),
                    |p| formation_profile(&p.slots()),
                )
        };
        let level = shape_for(0, 0);
        let chasing = shape_for(0, 1);
        assert!(
            chasing.0 > level.0,
            "chasing keeps more forward presence: {chasing:?} vs {level:?}"
        );
    }

    /// Given the manager has just ordered tactics himself,
    /// When the assistant is consulted in the stand-back window,
    /// Then it gives no tactical order (authority boundary); an unblocked group is untouched.
    #[test]
    fn the_assistant_leaves_blocked_orders_alone() {
        let f = fixture(78, 0, 2);
        let mut ctx = f.ctx(Checkpoint::Review);
        ctx.blocked[Group::Tactics.index()] = true;
        assert!(
            decide(&ctx)
                .iter()
                .all(|o| !matches!(o, AiOrder::Tactics(_)))
        );
        ctx.blocked = [true; Group::COUNT];
        assert_eq!(decide(&ctx), Vec::new());
    }

    /// Given a tactical change made two minutes ago,
    /// Then a discretionary review waits (no flapping); a dismissal does not.
    #[test]
    fn a_review_respects_the_cooldown_but_a_dismissal_does_not() {
        let f = fixture(78, 0, 2);
        let mut ctx = f.ctx(Checkpoint::Review);
        ctx.last_tactical_change_ms = Some(76 * 60_000);
        assert!(
            !decide(&ctx)
                .iter()
                .any(|o| matches!(o, AiOrder::Tactics(_)))
        );
        ctx.checkpoint = Checkpoint::Dismissal;
        assert!(
            decide(&ctx)
                .iter()
                .any(|o| matches!(o, AiOrder::Tactics(_)))
        );
    }

    /// Given an injured outfield player and a goalkeeper on the bench among others,
    /// Then the replacement is an outfield player, never the keeper, and the injured man is off.
    #[test]
    fn an_injured_outfield_player_is_replaced_by_an_outfield_player() {
        let mut f = fixture(30, 0, 0);
        let hurt = f.outfield_starter(2);
        f.view
            .home
            .on_pitch
            .iter_mut()
            .find(|p| p.id == hurt)
            .unwrap()
            .injured = true;
        let orders = decide(&f.ctx(Checkpoint::Injury));
        let Some(AiOrder::Substitute { off, on }) = orders.first() else {
            panic!("a substitution is expected: {orders:?}");
        };
        assert_eq!(*off, hurt);
        let on_view = f.view.home.bench.iter().find(|b| b.id == *on).unwrap();
        assert_ne!(on_view.natural_position, Position::Gk);
    }

    /// Given an injured goalkeeper, Then only a goalkeeper can replace him.
    #[test]
    fn an_injured_goalkeeper_is_replaced_by_a_goalkeeper_or_not_at_all() {
        let mut f = fixture(30, 0, 0);
        f.view
            .home
            .on_pitch
            .iter_mut()
            .find(|p| p.role == Some(Position::Gk))
            .unwrap()
            .injured = true;
        assert!(
            f.view
                .home
                .bench
                .iter()
                .any(|b| b.natural_position == Position::Gk),
            "the bench has a keeper to bring on"
        );
        let swaps: Vec<_> = decide(&f.ctx(Checkpoint::Injury))
            .into_iter()
            .filter_map(|o| match o {
                AiOrder::Substitute { on, .. } => Some(on),
                _ => None,
            })
            .collect();
        assert_eq!(swaps.len(), 1, "the injured keeper is replaced");
        let b = f.view.home.bench.iter().find(|b| b.id == swaps[0]).unwrap();
        assert_eq!(b.natural_position, Position::Gk);
    }

    /// Given a tired player, Then he is rested after the 55th minute but not before it.
    #[test]
    fn a_tired_player_is_rested_late_not_early() {
        for (minute, rested) in [(30u32, false), (70, true)] {
            let mut f = fixture(minute, 0, 0);
            let tired = f.outfield_starter(3);
            f.view
                .home
                .on_pitch
                .iter_mut()
                .find(|p| p.id == tired)
                .unwrap()
                .condition = 0.2;
            let subs = decide(&f.ctx(Checkpoint::Review))
                .into_iter()
                .filter(|o| matches!(o, AiOrder::Substitute { .. }))
                .count();
            assert_eq!(subs > 0, rested, "minute {minute}");
        }
    }

    /// Given no substitutions left, or every one already queued, Then none is proposed.
    #[test]
    fn nothing_is_proposed_without_an_allowance() {
        let mut f = fixture(75, 0, 2);
        let tired = f.outfield_starter(3);
        f.view
            .home
            .on_pitch
            .iter_mut()
            .find(|p| p.id == tired)
            .unwrap()
            .condition = 0.1;
        f.view.home.substitutions_remaining = 0;
        assert!(
            decide(&f.ctx(Checkpoint::Review))
                .iter()
                .all(|o| !matches!(o, AiOrder::Substitute { .. }))
        );

        f.view.home.substitutions_remaining = 1;
        f.view.home.pending_substitutions.push(PendingSubstitution {
            off: f.outfield_starter(5),
            on: f.view.home.bench[1].id.clone(),
        });
        assert!(
            decide(&f.ctx(Checkpoint::Review))
                .iter()
                .all(|o| !matches!(o, AiOrder::Substitute { .. }))
        );
    }

    /// Given the same state, Then the same orders: no randomness, no hidden state.
    #[test]
    fn the_policy_is_deterministic() {
        let mut f = fixture(72, 0, 1);
        let tired = f.outfield_starter(4);
        f.view
            .home
            .on_pitch
            .iter_mut()
            .find(|p| p.id == tired)
            .unwrap()
            .condition = 0.3;
        let first = decide(&f.ctx(Checkpoint::Review));
        assert!(!first.is_empty());
        for _ in 0..5 {
            assert_eq!(decide(&f.ctx(Checkpoint::Review)), first);
        }
    }

    /// Given a finished match, Then there is nothing to decide.
    #[test]
    fn a_finished_match_needs_no_manager() {
        let mut f = fixture(90, 0, 3);
        f.view.finished = true;
        assert_eq!(decide(&f.ctx(Checkpoint::Review)), Vec::new());
    }

    /// Given a side whose plan has drifted to Attacking while its club identity is Balanced,
    /// When the game gives no reason to stay aggressive (level, early),
    /// Then it returns to the club's own style: identity and current plan are not the same thing.
    #[test]
    fn a_side_returns_to_its_club_identity_when_the_match_asks_nothing_more() {
        let mut f = fixture(20, 0, 0);
        f.plan.style = PlayStyle::Attacking;
        assert_eq!(f.identity.style, PlayStyle::Balanced);
        let c = f.tactics(Checkpoint::Review).expect("a reaction");
        assert_eq!(c.style, Some(PlayStyle::Balanced));
    }

    /// Given a tired player and a side that is also trailing late (so both a substitution and a
    /// tactical change are on offer),
    /// When each group is blocked in turn,
    /// Then blocking substitutions removes only the substitution and blocking tactics removes
    /// only the tactical order: the groups are independent, and both were there to block.
    #[test]
    fn blocking_one_group_leaves_the_other_untouched() {
        let mut f = fixture(75, 0, 2);
        let tired = f.outfield_starter(3);
        f.view
            .home
            .on_pitch
            .iter_mut()
            .find(|p| p.id == tired)
            .unwrap()
            .condition = 0.1;
        let kinds = |ctx: &AiContext<'_>| {
            let orders = decide(ctx);
            (
                orders
                    .iter()
                    .any(|o| matches!(o, AiOrder::Substitute { .. })),
                orders.iter().any(|o| matches!(o, AiOrder::Tactics(_))),
            )
        };
        let mut ctx = f.ctx(Checkpoint::Review);
        assert_eq!(kinds(&ctx), (true, true), "both are on offer");
        ctx.blocked[Group::Substitution.index()] = true;
        assert_eq!(kinds(&ctx), (false, true));
        ctx.blocked = [false; Group::COUNT];
        ctx.blocked[Group::Tactics.index()] = true;
        assert_eq!(kinds(&ctx), (true, false));
    }

    /// Given a table of scores, clocks and men,
    /// When this manager's need is compared with the autonomous manager's (`need_v2`),
    /// Then it is that judgement on the same scale: one policy for urgency, two ways to execute it.
    #[test]
    fn urgency_is_the_autonomous_managers_judgement() {
        for (minute, home, away) in [
            (30, 0, 1),
            (50, 0, 1),
            (70, 0, 1),
            (80, 0, 2),
            (60, 1, 0),
            (85, 1, 0),
            (88, 2, 0),
            (20, 0, 0),
        ] {
            let f = fixture(minute, home, away);
            let diff = i32::from(home) - i32::from(away);
            let left = 90.0 - minute as f32;
            let (chase, protect) = need_v2(&ManagerConfig::m10(), diff, left);
            assert!(
                (need(&f.ctx(Checkpoint::Review)) - 2.0 * (chase - protect)).abs() < 1e-5,
                "{minute}' {home}-{away}"
            );
        }
    }

    /// Given one goal down with an hour left and one goal down with a quarter of an hour left,
    /// Then the second asks for much more: urgency is minutes against goals, not the clock alone.
    #[test]
    fn a_goal_down_is_urgent_only_when_time_is_short() {
        let early = need(&fixture(30, 0, 1).ctx(Checkpoint::Review));
        let late = need(&fixture(75, 0, 1).ctx(Checkpoint::Review));
        assert!(late > early + 0.8, "early {early}, late {late}");
    }

    /// Given the same state played in extra time,
    /// Then the horizon is the rest of extra time, so a goal down with 5 minutes left is urgent.
    #[test]
    fn extra_time_uses_its_own_horizon() {
        let mut f = fixture(115, 0, 1);
        f.view.clock_ms = 115 * 60_000;
        let ctx = f.ctx(Checkpoint::Review);
        assert!((minutes_left(&ctx) - 5.0).abs() < 1e-4);
        assert!(need(&ctx) > 1.0);
    }

    /// The bench player OFM's policy proposes for `off`, and the one the autonomous manager's own
    /// utility ranks first among the same candidates.
    fn proposed_and_ranked(f: &Fixture, off: &PlayerView) -> (Option<PlayerId>, Option<PlayerId>) {
        let ctx = f.ctx(Checkpoint::Review);
        let role = off.role.expect("a starter");
        let ranked = ctx
            .own()
            .bench
            .iter()
            .filter(|b| (b.natural_position == Position::Gk) == (role == Position::Gk))
            .map(|b| {
                // the autonomous manager's own call, built here from the raw view, not through the
                // adapter under test
                let (off_a, _) = ctx.squad.known(&off.id).expect("known");
                let (on_a, on_nat) = ctx.squad.known(&b.id).expect("known");
                // the autonomous manager's horizon: what is left of regulation, or of the thirty
                // minutes of extra time once regulation is over
                let regulation = ctx.regulation_ms as f32 / 60_000.0;
                let now = ctx.view.clock_ms as f32 / 60_000.0;
                let (total, left) = if now <= regulation {
                    (regulation, regulation - now)
                } else {
                    (30.0, regulation + 30.0 - now)
                };
                let remaining = (left / total).clamp(0.0, 1.0);
                let sub = SubContext {
                    minute: 1.0 - remaining,
                    remaining,
                    score_diff: ctx.goal_difference(),
                    used: ctx.own().substitutions_used,
                    max: ctx.own().substitutions_used + ctx.own().substitutions_remaining,
                };
                let (chase, protect) = need_v2(&ManagerConfig::m10(), ctx.goal_difference(), left);
                let u = substitution_utility_v2(
                    &ManagerConfig::m10(),
                    &sub,
                    role,
                    off_a,
                    performance(1.0 - off.condition, 0.0, 0.0),
                    on_a,
                    *on_nat,
                    performance(1.0 - b.condition, 0.0, 0.0),
                    chase.max(protect),
                    None,
                );
                (u, b.id.clone())
            })
            .filter(|(u, _)| *u > 0.0)
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.0.cmp(&a.1.0)))
            .map(|(_, id)| id);
        (best_replacement(&ctx, off, role, &[]), ranked)
    }

    /// Given comparable states of one match (a level game, a goal down, a goal up) with a tired
    /// midfielder,
    /// When OFM's policy picks his replacement and the autonomous manager's utility ranks the same
    /// bench,
    /// Then they choose the same player: a bench player is valued by one judgement, whichever mode
    /// the match is played in. (This guards the wiring only: its bench has one natural midfielder, so
    /// it cannot tell the score-aware policy from a score-blind one; that discrimination is
    /// `the_score_decides_which_bench_player_comes_on`.)
    #[test]
    fn a_replacement_is_the_one_the_autonomous_manager_would_choose() {
        for (minute, home, away) in [
            (70, 0, 0),
            (75, 0, 1),
            (78, 2, 1),
            (82, 0, 2),
            (100, 1, 1),
            (110, 0, 1),
        ] {
            let mut f = fixture(minute, home, away);
            let off_id = f
                .view
                .home
                .on_pitch
                .iter()
                .find(|p| p.role == Some(Position::Cm))
                .expect("a central midfielder")
                .id
                .clone();
            for p in &mut f.view.home.on_pitch {
                if p.id == off_id {
                    p.condition = 0.35;
                }
            }
            let off = f
                .view
                .home
                .on_pitch
                .iter()
                .find(|p| p.id == off_id)
                .expect("starter")
                .clone();
            let (proposed, ranked) = proposed_and_ranked(&f, &off);
            assert_eq!(proposed, ranked, "minute {minute}, {home}-{away}");
        }
    }

    /// Two bench midfielders who play the slot equally well (the same passing, vision, stamina,
    /// dribbling, work rate and decisions) and differ only in what the score might want of them: one
    /// is a finisher (finishing and pace up, tackling, marking and positioning down), the other a
    /// stopper (the reverse). Returns (finisher, stopper).
    fn finisher_and_stopper(f: &mut Fixture, off: &PlayerView) -> (PlayerId, PlayerId) {
        let ids: Vec<PlayerId> = f
            .view
            .home
            .bench
            .iter()
            .filter(|b| b.natural_position != Position::Gk)
            .take(2)
            .map(|b| b.id.clone())
            .collect();
        let (base, _) = f.squad.known(&off.id).expect("known").clone();
        let set = |f: &mut Fixture, id: &PlayerId, attack: bool| {
            let mut a = base.clone();
            let (up, down) = if attack { (85, 25) } else { (25, 85) };
            a.finishing = up;
            a.pace = up;
            a.tackling = down;
            a.marking = down;
            a.positioning = down;
            f.squad.players.insert(id.clone(), (a, Position::Cm));
        };
        set(f, &ids[0], true);
        set(f, &ids[1], false);
        (ids[0].clone(), ids[1].clone())
    }

    /// Given a tired central midfielder and two bench midfielders of identical quality in the slot,
    /// one a finisher and one a stopper,
    /// When the manager's team is a goal down late, and when it is a goal up late,
    /// Then it brings on the finisher when chasing and the stopper when protecting: the score decides
    /// which bench player is worth bringing on. A policy that ignored the score (rating times
    /// condition) would pick the same player in both.
    #[test]
    fn the_score_decides_which_bench_player_comes_on() {
        let choose = |home: u8, away: u8| {
            let mut f = fixture(78, home, away);
            let off_id = f
                .view
                .home
                .on_pitch
                .iter()
                .find(|p| p.role == Some(Position::Cm))
                .expect("a central midfielder")
                .id
                .clone();
            for p in &mut f.view.home.on_pitch {
                if p.id == off_id {
                    p.condition = 0.35;
                }
            }
            let off = f
                .view
                .home
                .on_pitch
                .iter()
                .find(|p| p.id == off_id)
                .expect("starter")
                .clone();
            let (finisher, stopper) = finisher_and_stopper(&mut f, &off);
            let ctx = f.ctx(Checkpoint::Review);
            let picked = best_replacement(&ctx, &off, Position::Cm, &[]).expect("a replacement");
            (picked, finisher, stopper)
        };
        let (chasing, finisher, stopper) = choose(0, 1);
        assert_eq!(chasing, finisher, "a goal down with twelve minutes left");
        let (protecting, finisher2, stopper2) = choose(1, 0);
        assert_eq!(
            (&finisher, &stopper),
            (&finisher2, &stopper2),
            "the same bench in both states"
        );
        assert_eq!(protecting, stopper, "a goal up with twelve minutes left");
        assert_ne!(chasing, protecting);
    }

    /// Given a tired starter and a bench player who is fresh but much weaker,
    /// When the policy looks for a replacement,
    /// Then nobody is proposed: freshness alone is not a reason to swap.
    #[test]
    fn freshness_alone_does_not_make_a_swap() {
        let mut f = fixture(70, 0, 0);
        let off_id = f.outfield_starter(5);
        for p in &mut f.view.home.on_pitch {
            if p.id == off_id {
                p.condition = 0.8; // a little tired
            }
        }
        // every bench player is hopeless: strip their attributes in the squad's knowledge
        for b in f.view.home.bench.clone() {
            if let Some((a, _)) = f.squad.players.get_mut(&b.id) {
                // every numeric attribute at 20, whatever the field list is
                let mut v = serde_json::to_value(&*a).expect("attributes serialise");
                if let Some(o) = v.as_object_mut() {
                    for x in o.values_mut().filter(|x| x.is_number()) {
                        *x = serde_json::json!(20);
                    }
                }
                *a = serde_json::from_value(v).expect("attributes parse");
            }
        }
        let off = f
            .view
            .home
            .on_pitch
            .iter()
            .find(|p| p.id == off_id)
            .expect("starter")
            .clone();
        let ctx = f.ctx(Checkpoint::Review);
        assert_eq!(
            best_replacement(&ctx, &off, off.role.expect("role"), &[]),
            None
        );
    }
}
