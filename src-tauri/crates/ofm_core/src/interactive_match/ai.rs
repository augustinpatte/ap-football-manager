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
use apfm_match_lab::sim::football::manager::{Line, fit, line_of, quality};
use apfm_match_lab::sim::session::{PlayerView, SessionView, TeamView};
use domain::team::{PlayStyle, PressingIntensity, Tempo};

use super::plan::{TacticalChange, TacticalPlan, supported_formations};

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

    /// How well `id` plays in `role`: his quality for the role times how natural it is to him.
    fn rating(&self, id: &PlayerId, role: Position) -> f32 {
        self.players
            .get(id)
            .map_or(0.0, |(a, natural)| quality(a, role) * fit(*natural, role))
    }
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
            ctx.squad
                .rating(&a.id, role)
                .total_cmp(&ctx.squad.rating(&b.id, role))
                .then(b.id.0.cmp(&a.id.0))
        })?;
    *left -= 1;
    taken_off.push(off.id.clone());
    taken_on.push(on.id.clone());
    Some((off.id.clone(), on.id.clone()))
}

/// The best fresh bench player for `role`, only if he is a real improvement on `off`.
fn best_replacement(
    ctx: &AiContext<'_>,
    off: &PlayerView,
    role: Position,
    taken_on: &[PlayerId],
) -> Option<PlayerId> {
    let goalkeeper = role == Position::Gk;
    let effective =
        |id: &PlayerId, condition: f32| ctx.squad.rating(id, role) * (0.5 + 0.5 * condition);
    let current = if off.injured {
        0.0
    } else {
        effective(&off.id, off.condition)
    };
    ctx.own()
        .bench
        .iter()
        .filter(|b| {
            !b.injured
                && !taken_on.contains(&b.id)
                && (b.natural_position == Position::Gk) == goalkeeper
        })
        .map(|b| (effective(&b.id, b.condition), b))
        .filter(|(value, _)| *value > current + 0.03 || off.injured)
        .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.id.0.cmp(&a.1.id.0)))
        .map(|(_, b)| b.id.clone())
}

/// How much more attacking (positive) or more cautious (negative) the situation asks the side
/// to be than its club identity does.
fn need(ctx: &AiContext<'_>) -> f32 {
    let urgency = ((ctx.minute() as f32 - 45.0) / 40.0).clamp(0.0, 1.0);
    let diff = ctx.goal_difference().clamp(-2, 2) as f32;
    let mut need = -diff * urgency;
    let men = ctx.own().on_pitch.len() as i32 - ctx.opponent().on_pitch.len() as i32;
    if men < 0 {
        // short-handed: shore up, but a side that is behind still has to chase a little
        need -= f32::from(men.unsigned_abs() as u8) * if diff < 0.0 { 0.3 } else { 0.7 };
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
        change.formation = formation_for(ctx, need);
    }
    (change != TacticalChange::default()).then_some(change)
}

/// A shape change only when a man has gone, and only to one M12 can build.
fn formation_for(ctx: &AiContext<'_>, need: f32) -> Option<String> {
    let men = ctx.own().on_pitch.len() as i32 - ctx.opponent().on_pitch.len() as i32;
    let wanted = if men < 0 && need <= 0.5 {
        "4-5-1"
    } else if men > 0 && ctx.goal_difference() <= 0 {
        "4-3-3"
    } else {
        return None;
    };
    (ctx.plan.formation != wanted && supported_formations().iter().any(|f| f == wanted))
        .then(|| wanted.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use apfm_match_lab::fixtures::identical_teams_input;
    use apfm_match_lab::model::Team;
    use apfm_match_lab::output::Period;
    use apfm_match_lab::sim::football::FootballStats;
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
        input: MatchInput,
        view: SessionView,
        squad: Squad,
        plan: TacticalPlan,
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
            input,
            view,
            plan,
        }
    }

    impl Fixture {
        fn ctx(&self, checkpoint: Checkpoint) -> AiContext<'_> {
            AiContext {
                side: Side::Home,
                view: &self.view,
                plan: &self.plan,
                identity: &self.plan,
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

    /// Given a side that has just lost a player while level,
    /// When the dismissal is the checkpoint,
    /// Then it reorganises at once into a compact shape M12 can build, and turns cautious.
    #[test]
    fn a_dismissal_makes_the_side_reorganise() {
        let mut f = fixture(40, 0, 0);
        let gone = f.view.home.on_pitch.remove(9);
        f.view.home.sent_off.push(gone);
        let c = f.tactics(Checkpoint::Dismissal).expect("a reaction");
        assert_eq!(c.formation.as_deref(), Some("4-5-1"));
        assert!(supported_formations().contains(&"4-5-1".to_string()));
        assert_eq!(c.style, Some(PlayStyle::Counter));
    }

    /// Given the opponent down to ten, Then a side that is not winning pushes up.
    #[test]
    fn a_numerical_advantage_is_pressed_when_not_winning() {
        let mut f = fixture(50, 0, 0);
        f.view.away.on_pitch.pop();
        let c = f.tactics(Checkpoint::Dismissal).expect("a reaction");
        assert_eq!(c.formation.as_deref(), Some("4-3-3"));
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
        for order in decide(&f.ctx(Checkpoint::Injury)) {
            if let AiOrder::Substitute { on, .. } = order {
                let b = f.view.home.bench.iter().find(|b| b.id == on).unwrap();
                assert_eq!(b.natural_position, Position::Gk);
            }
        }
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
        let _ = &f.input;
    }

    /// Given a finished match, Then there is nothing to decide.
    #[test]
    fn a_finished_match_needs_no_manager() {
        let mut f = fixture(90, 0, 3);
        f.view.finished = true;
        assert_eq!(decide(&f.ctx(Checkpoint::Review)), Vec::new());
    }
}
