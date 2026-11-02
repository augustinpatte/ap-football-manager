//! Tuning numbers and error keys for free-agent talks, kept together so the shape of a
//! negotiation can be read and adjusted in one place.

/// Weeks in a contract year, for spreading a signing bonus over the contract.
pub(crate) const WEEKS_PER_YEAR: u64 = 52;

// ── time ──
/// A player answers an offer between this many days after it and [`MAX_RESPONSE_DAYS`]: long
/// enough that signing is not instant, short enough never to stall a plan.
pub(crate) const MIN_RESPONSE_DAYS: u64 = 1;
pub(crate) const MAX_RESPONSE_DAYS: u64 = 3;
/// How long the terms of a counter-offer stand before the player moves on.
pub(crate) const COUNTER_STANDS_DAYS: u64 = 3;
/// Offers the manager may make in one talk before the player breaks it off.
pub(crate) const MAX_ROUNDS: u8 = 4;
/// How long a club that insulted a player, or exhausted his patience, is not heard.
pub(crate) const BROKEN_OFF_COOLDOWN_DAYS: i64 = 14;
/// How long an ended talk stays on the player's record: for the interface's history, and for the
/// cool-off above to be read off it.
pub(crate) const CLOSED_TALK_RETENTION_DAYS: i64 = 45;
const _: () = assert!(CLOSED_TALK_RETENTION_DAYS > BROKEN_OFF_COOLDOWN_DAYS);

// ── limits ──
/// Talks the manager may have open at once, so a click cannot spread across the whole market.
pub(crate) const MAX_OPEN_TALKS_PER_MANAGER: usize = 5;
/// Seniors a club may register. A free agent is not signed into a full squad list.
pub(crate) const MAX_SENIOR_SQUAD: usize = 30;

// ── the score ──
pub(crate) const ACCEPT_AT: i32 = 0;
pub(crate) const COUNTER_AT: i32 = -18;
pub(crate) const WAGE_TERM_MIN: i32 = -25;
pub(crate) const WAGE_TERM_MAX: i32 = 20;
pub(crate) const STANDING_FLOOR_OVR: i32 = 45;
pub(crate) const STANDING_PER_OVR: i32 = 20;
/// Weight of reputation against the level a squad plays at, in a club's standing.
pub(crate) const CLUB_REPUTATION_WEIGHT_PCT: i32 = 60;
/// Seniors that make up a squad's level.
pub(crate) const SQUAD_LEVEL_SAMPLE: usize = 14;
/// Standing a player gives up without noticing.
pub(crate) const STEP_DOWN_FREE_GAP: i32 = 60;
pub(crate) const STEP_DOWN_POINTS_PER_STANDING: i32 = 8;
pub(crate) const STEP_UP_POINTS_PER_STANDING: i32 = 25;
pub(crate) const STEP_UP_BONUS_MAX: i32 = 12;
pub(crate) const ROLE_KEY: i32 = 12;
pub(crate) const ROLE_STARTER: i32 = 6;
pub(crate) const ROLE_BACKUP: i32 = -12;
/// Rating points over the next best that make a player the key man of his position.
pub(crate) const KEY_PLAYER_LEAD: u8 = 3;
/// Places below the starters that still mean a share of the matches.
pub(crate) const ROTATION_PLACES: usize = 2;
pub(crate) const DURATION_POINTS_PER_YEAR: i32 = 6;
pub(crate) const OLD_PLAYER_LONG_DEAL_POINTS: i32 = 4;
/// A free agent starts to lower his sights after this long without a club, a point every few days.
pub(crate) const DESPERATION_STARTS_AFTER_DAYS: i64 = 21;
pub(crate) const DESPERATION_DAYS_PER_POINT: i64 = 3;
pub(crate) const DESPERATION_MAX: i32 = 20;
pub(crate) const PROMISE_POINTS: i32 = 8;

// ── rival clubs ──
/// A free agent this good, or better, draws other clubs' attention.
pub(crate) const ATTRACTIVE_FREE_AGENT_OVR: u8 = 62;
/// Free agents the market considers for new interest in a day, best first.
pub(crate) const ATTRACTIVE_FREE_AGENTS_PER_DAY: usize = 25;
/// Clubs looked at for each, picked by a rotation that changes daily.
pub(crate) const CLUBS_SAMPLED_PER_FREE_AGENT: usize = 12;
pub(crate) const MAX_NEW_RIVAL_TALKS_PER_DAY: usize = 6;
pub(crate) const MAX_OPEN_RIVAL_TALKS_PER_PLAYER: usize = 3;

// ── error keys ──
pub(crate) const ERR_TALK_ALREADY_OPEN: &str = "be.error.freeAgents.talkAlreadyOpen";
pub(crate) const ERR_TALKS_BROKEN_OFF: &str = "be.error.freeAgents.talksBrokenOff";
pub(crate) const ERR_TOO_MANY_OPEN_TALKS: &str = "be.error.freeAgents.tooManyOpenTalks";
pub(crate) const ERR_SQUAD_FULL: &str = "be.error.freeAgents.squadFull";
pub(crate) const ERR_NO_COUNTER_TO_ANSWER: &str = "be.error.freeAgents.noCounterToAnswer";
pub(crate) const ERR_COUNTER_EXPIRED: &str = "be.error.freeAgents.counterExpired";
pub(crate) const ERR_NO_OPEN_TALK: &str = "be.error.freeAgents.noOpenTalk";
pub(crate) const ERR_INVALID_OFFER: &str = "be.error.freeAgents.invalidOffer";
