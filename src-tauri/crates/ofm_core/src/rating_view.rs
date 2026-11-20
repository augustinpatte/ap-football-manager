//! A player's overall rating as the manager's club is allowed to read it: the one projection every
//! list, profile and comparison reads, so none of them shows more than the club has earned.
//!
//! The true `ovr` stays on the player: the simulation, the AI clubs and the save need it. What the
//! *manager* sees depends on what his club knows (`player_knowledge`):
//!
//! * his own player, and a player at a club in his own division, whom the staff see every season,
//!   read exactly;
//! * a player a scout reported on reads as the scout's ability range, as the report holds it and
//!   labelled current or old, however the player has changed since;
//! * anyone else reads as a broad public standing (a band of several rating points), never a number.
//!
//! The estimate is [`crate::scouting_dossier`]'s own: this module only chooses what to show.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::academy_assessment::AcademyLens;
use crate::game::Game;
use crate::player_knowledge::{PlayerKnowledge, Viewer, knowledge_from};
use crate::player_tier::{A_TIER_MIN_RATING, B_TIER_MIN_RATING, S_TIER_MIN_RATING};
use crate::scouting_dossier::Confidence;
use domain::player::Player;

/// Lowest rating of the "average" standing: a player of a club's regular squad below the good ones.
const AVERAGE_STANDING_MIN_RATING: u8 = 62;

/// What anyone following the game can say about a player without having scouted him: how good he
/// is thought to be, in five steps. The cut-offs reuse the game's tiers where they exist
/// (`player_tier`), so "elite" means the same thing on every screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicStanding {
    Limited,
    Average,
    Good,
    Strong,
    Elite,
}

impl PublicStanding {
    pub fn of(ovr: u8) -> Self {
        if ovr >= S_TIER_MIN_RATING {
            Self::Elite
        } else if ovr >= A_TIER_MIN_RATING {
            Self::Strong
        } else if ovr >= B_TIER_MIN_RATING {
            Self::Good
        } else if ovr >= AVERAGE_STANDING_MIN_RATING {
            Self::Average
        } else {
            Self::Limited
        }
    }

    /// The ratings that read as this standing, lowest to highest.
    pub fn span(self) -> (u8, u8) {
        match self {
            Self::Limited => (1, AVERAGE_STANDING_MIN_RATING - 1),
            Self::Average => (AVERAGE_STANDING_MIN_RATING, B_TIER_MIN_RATING - 1),
            Self::Good => (B_TIER_MIN_RATING, A_TIER_MIN_RATING - 1),
            Self::Strong => (A_TIER_MIN_RATING, S_TIER_MIN_RATING - 1),
            Self::Elite => (S_TIER_MIN_RATING, 99),
        }
    }
}

/// How the manager reads a player's overall rating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RatingView {
    /// The club knows the number: its own player, or one in its division.
    Exact { ovr: u8 },
    /// A scout's range, from the report the club holds.
    Estimate {
        low: u8,
        high: u8,
        confidence: Confidence,
        /// `false` when the report predates the current season: the player may have changed.
        current: bool,
        scouted_on: String,
    },
    /// Nothing but the public standing.
    Standing { standing: PublicStanding },
}

impl RatingView {
    /// The lowest and highest rating the player could be, as far as the manager can tell.
    pub fn bounds(&self) -> (u8, u8) {
        match self {
            Self::Exact { ovr } => (*ovr, *ovr),
            Self::Estimate { low, high, .. } => (*low, *high),
            Self::Standing { standing } => standing.span(),
        }
    }

    /// The one figure a list sorts by: the exact rating, the middle of the range, or the middle of
    /// the standing's span. Two players the manager cannot tell apart sort alike, so the order
    /// never says more than the screen does.
    pub fn sort_value(&self) -> u8 {
        let (low, high) = self.bounds();
        ((u16::from(low) + u16::from(high)) / 2) as u8
    }

    /// The rating the club acts on when it must act on one number (the squad model asked about a
    /// target), or `None` when it only knows a broad standing.
    pub fn believed_ovr(&self) -> Option<u8> {
        match self {
            Self::Exact { ovr } => Some(*ovr),
            Self::Estimate { .. } => Some(self.sort_value()),
            Self::Standing { .. } => None,
        }
    }

    /// Whether the player could be rated between `min` and `max`, as far as the manager can tell.
    /// A filter asks this, not the true rating: a bound that cut through a range would let the
    /// manager find the rating by narrowing it.
    pub fn may_be_rated_within(&self, min: Option<u8>, max: Option<u8>) -> bool {
        let (low, high) = self.bounds();
        min.is_none_or(|min| high >= min) && max.is_none_or(|max| low <= max)
    }
}

/// Everything a list needs to read many players' ratings: found once, not per player.
pub struct RatingLens<'a> {
    game: &'a Game,
    viewer: Viewer<'a>,
}

impl<'a> RatingLens<'a> {
    pub fn of(game: &'a Game) -> Self {
        Self {
            game,
            viewer: Viewer::of(game),
        }
    }

    pub fn rating_of(&self, player: &Player) -> RatingView {
        match knowledge_from(self.game, &self.viewer, player) {
            PlayerKnowledge::Own | PlayerKnowledge::Known => RatingView::Exact { ovr: player.ovr },
            PlayerKnowledge::Scouted { .. } | PlayerKnowledge::Unknown => {
                self.reported_or_public(player)
            }
        }
    }

    /// The dossier's report if the club holds one, read with the one freshness rule the search and
    /// the dossier screen share; otherwise the public standing.
    fn reported_or_public(&self, player: &Player) -> RatingView {
        let dossier = &self.game.scouting_dossier;
        let Some(report) = dossier.report_on(&player.id) else {
            return RatingView::Standing {
                standing: PublicStanding::of(player.ovr),
            };
        };
        let current = dossier
            .current_report_on(&player.id, self.game.season_context.season_start.as_deref())
            .is_some();
        RatingView::Estimate {
            low: report.ability.low,
            high: report.ability.high,
            confidence: report.confidence(),
            current,
            scouted_on: report.scouted_on.clone(),
        }
    }
}

/// How the manager reads each requested player's rating. An id no player has is left out.
pub fn ratings_view(game: &Game, player_ids: &[String]) -> HashMap<String, RatingView> {
    let wanted: HashSet<&str> = player_ids.iter().map(String::as_str).collect();
    let lens = RatingLens::of(game);
    game.players
        .iter()
        .filter(|player| wanted.contains(player.id.as_str()))
        .map(|player| (player.id.clone(), lens.rating_of(player)))
        .collect()
}

/// How the manager reads one youth prospect a scout proposed: his current rating and his ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProspectReading {
    pub player_id: String,
    pub current: RatingView,
    /// What he could become. A prospect is not a player of the club until it signs him, so a scout
    /// proposing him gives a broad standing of each, not the two numbers the generator drew.
    pub ceiling: RatingView,
}

/// How the manager reads each prospect in a scout's youth recruitment message. Once the club has
/// signed one, he is its own player: his rating reads exactly, and his ceiling as the club's youth
/// staff assess it ([`crate::academy_assessment`]), a band and never the figure the development
/// model holds. `None` when no message has that id or it proposes nobody.
pub fn youth_prospect_readings(game: &Game, message_id: &str) -> Option<Vec<ProspectReading>> {
    let message = game
        .messages
        .iter()
        .find(|message| message.id == message_id)?;
    let prospects = message.context.youth_prospects.as_ref()?;
    let club = game.manager.team_id.as_deref();
    let academy = AcademyLens::of(game);
    Some(
        prospects
            .iter()
            .map(|prospect| {
                let signed = club.is_some() && prospect.team_id.as_deref() == club;
                if signed {
                    // The signed youngster in the game is the one that has been developing since;
                    // the message only holds the player as the scout proposed him. If he is no
                    // longer the club's (sold since), the message is out of date and he reads as
                    // any stranger does.
                    let own = game
                        .players
                        .iter()
                        .find(|player| player.id == prospect.id)
                        .unwrap_or(prospect);
                    if let Some(ceiling) = academy.as_ref().and_then(|lens| lens.ceiling_of(own)) {
                        return ProspectReading {
                            player_id: prospect.id.clone(),
                            current: RatingView::Exact { ovr: own.ovr },
                            ceiling,
                        };
                    }
                }
                ProspectReading {
                    player_id: prospect.id.clone(),
                    current: standing_of(prospect.ovr),
                    ceiling: standing_of(prospect.potential.max(prospect.ovr)),
                }
            })
            .collect(),
    )
}

fn standing_of(rating: u8) -> RatingView {
    RatingView::Standing {
        standing: PublicStanding::of(rating),
    }
}

/// How the manager reads one player's rating.
pub fn rating_view(game: &Game, player: &Player) -> RatingView {
    RatingLens::of(game).rating_of(player)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::GameClock;
    use crate::game::ScoutedPlayer;
    use crate::scouting_dossier::{Range, ScoutReport};
    use crate::test_support::uniform_attributes;
    use chrono::{TimeZone, Utc};
    use domain::league::League;
    use domain::manager::Manager;
    use domain::player::Position;

    const MY_CLUB: &str = "mine";

    fn player(id: &str, team_id: Option<&str>, ovr: u8) -> Player {
        let mut player = Player::new(
            id.to_string(),
            id.to_string(),
            id.to_string(),
            "2000-01-01".to_string(),
            "GB".to_string(),
            Position::Midfielder,
            uniform_attributes(ovr),
        );
        player.team_id = team_id.map(str::to_string);
        player.ovr = ovr;
        player.potential = ovr.saturating_add(9).min(99);
        player
    }

    fn division(id: &str, clubs: &[&str]) -> League {
        let clubs: Vec<String> = clubs.iter().map(|club| club.to_string()).collect();
        League::new(id.to_string(), id.to_string(), 2026, &clubs)
    }

    /// My club and "rival" play in division one; "faraway" plays in division two. The season began
    /// on 2026-08-01 and today is 2026-09-15.
    fn world(players: Vec<Player>) -> Game {
        let mut manager = Manager::new(
            "m1".to_string(),
            "A".to_string(),
            "B".to_string(),
            "1980-01-01".to_string(),
            "GB".to_string(),
        );
        manager.team_id = Some(MY_CLUB.to_string());
        let clock = GameClock::new(Utc.with_ymd_and_hms(2026, 9, 15, 12, 0, 0).unwrap());
        let mut game = Game::new(clock, manager, vec![], players, vec![], vec![]);
        game.competitions = vec![
            division("division-one", &[MY_CLUB, "rival"]),
            division("division-two", &["faraway", "other"]),
        ];
        game.season_context.season_start = Some("2026-08-01".to_string());
        game
    }

    fn report(player_id: &str, scouted_on: &str, ability: Range) -> ScoutReport {
        ScoutReport {
            player_id: player_id.to_string(),
            scout_id: "scout".to_string(),
            scout_name: "Scout".to_string(),
            scouted_on: scouted_on.to_string(),
            judging_ability: 70,
            judging_potential: 70,
            reports: 1,
            ability,
            potential: Some(Range { low: 70, high: 84 }),
            age: 26,
            strengths: vec![],
            weaknesses: vec![],
        }
    }

    fn view_of(game: &Game, player_id: &str) -> RatingView {
        let player = game.players.iter().find(|p| p.id == player_id).unwrap();
        rating_view(game, player)
    }

    /// Given a player at the manager's own club
    /// When his rating is read
    /// Then it is his exact rating
    #[test]
    fn the_clubs_own_player_reads_exactly() {
        let game = world(vec![player("own", Some(MY_CLUB), 71)]);

        assert_eq!(view_of(&game, "own"), RatingView::Exact { ovr: 71 });
    }

    /// Given a player at a rival in the manager's own division
    /// When his rating is read
    /// Then it is exact: the staff see him every season
    #[test]
    fn a_rival_in_the_same_division_reads_exactly() {
        let game = world(vec![player("rival-player", Some("rival"), 66)]);

        assert_eq!(
            view_of(&game, "rival-player"),
            RatingView::Exact { ovr: 66 }
        );
    }

    /// Given an unscouted player at a club in another division
    /// When his rating is read
    /// Then only his public standing comes back, and two players in one standing read identically
    #[test]
    fn an_unscouted_player_elsewhere_reads_as_a_standing_not_a_number() {
        let game = world(vec![
            player("a", Some("faraway"), 71),
            player("b", Some("faraway"), 76),
            player("c", Some("faraway"), 77),
        ]);

        assert_eq!(
            view_of(&game, "a"),
            RatingView::Standing {
                standing: PublicStanding::Good
            }
        );
        assert_eq!(view_of(&game, "a"), view_of(&game, "b"));
        assert_ne!(view_of(&game, "b"), view_of(&game, "c"));
        let wire = serde_json::to_string(&view_of(&game, "a")).unwrap();
        assert!(!wire.contains("71") && !wire.contains("ovr"), "{wire}");
    }

    /// Given a free agent nobody has scouted
    /// When his rating is read
    /// Then he is a standing too: no club puts him in the manager's division
    #[test]
    fn an_unscouted_free_agent_reads_as_a_standing() {
        let game = world(vec![player("free", None, 58)]);

        assert_eq!(
            view_of(&game, "free"),
            RatingView::Standing {
                standing: PublicStanding::Limited
            }
        );
    }

    /// Given a player a scout reported on this season, with a narrow range
    /// When his rating is read
    /// Then it is the report's range, current, with the report's confidence and date
    #[test]
    fn a_player_with_a_current_report_reads_as_the_scouts_range() {
        let mut game = world(vec![player("p", Some("faraway"), 74)]);
        game.scouting_dossier
            .reports
            .push(report("p", "2026-09-01", Range { low: 71, high: 76 }));

        assert_eq!(
            view_of(&game, "p"),
            RatingView::Estimate {
                low: 71,
                high: 76,
                confidence: Confidence::High,
                current: true,
                scouted_on: "2026-09-01".to_string(),
            }
        );
    }

    /// Given a report with a wide range from a weak scout
    /// When the rating is read
    /// Then the low confidence travels with it: the screen must not draw it as a firm figure
    #[test]
    fn a_wide_report_carries_low_confidence() {
        let mut game = world(vec![player("p", Some("faraway"), 74)]);
        game.scouting_dossier
            .reports
            .push(report("p", "2026-09-01", Range { low: 64, high: 82 }));

        assert!(matches!(
            view_of(&game, "p"),
            RatingView::Estimate {
                confidence: Confidence::Low,
                ..
            }
        ));
    }

    /// Given a report from before this season that says 60 to 66, and a player who has since grown
    /// to 75
    /// When the rating is read
    /// Then it still says 60 to 66, flagged as old: knowledge is not refreshed silently
    #[test]
    fn an_old_report_stays_as_written_and_is_marked_old() {
        let mut game = world(vec![player("p", Some("faraway"), 60)]);
        game.scouting_dossier
            .reports
            .push(report("p", "2026-03-10", Range { low: 60, high: 66 }));
        game.players[0].ovr = 75;

        let RatingView::Estimate {
            low, high, current, ..
        } = view_of(&game, "p")
        else {
            panic!("a reported player reads as an estimate");
        };
        assert_eq!((low, high, current), (60, 66, false));
    }

    /// Given a player the club scouted in an earlier build (a snapshot, but no dossier report)
    /// When his rating is read
    /// Then he reads as a standing: the club holds no range to show
    #[test]
    fn a_legacy_snapshot_without_a_report_reads_as_a_standing() {
        let mut game = world(vec![player("p", Some("faraway"), 72)]);
        game.scouted_players.push(ScoutedPlayer {
            player_id: "p".to_string(),
            scouted_on: "2026-09-01".to_string(),
            attributes: uniform_attributes(72),
        });

        assert!(matches!(
            view_of(&game, "p"),
            RatingView::Standing {
                standing: PublicStanding::Good
            }
        ));
    }

    /// Given a report on a player who later joins the manager's club
    /// When his rating is read
    /// Then it is exact: he is the club's own now
    #[test]
    fn a_reported_player_who_joins_the_club_reads_exactly() {
        let mut game = world(vec![player("p", Some("faraway"), 74)]);
        game.scouting_dossier
            .reports
            .push(report("p", "2026-09-01", Range { low: 71, high: 76 }));
        game.players[0].team_id = Some(MY_CLUB.to_string());

        assert_eq!(view_of(&game, "p"), RatingView::Exact { ovr: 74 });
    }

    /// Given the standing cut-offs
    /// When ratings either side of each are read
    /// Then the standing changes exactly there, and each standing's span matches its cut-offs
    #[test]
    fn standings_change_at_the_cut_offs_and_spans_agree_with_them() {
        for (ovr, standing) in [
            (61, PublicStanding::Limited),
            (62, PublicStanding::Average),
            (69, PublicStanding::Average),
            (70, PublicStanding::Good),
            (76, PublicStanding::Good),
            (77, PublicStanding::Strong),
            (80, PublicStanding::Strong),
            (81, PublicStanding::Elite),
        ] {
            assert_eq!(PublicStanding::of(ovr), standing, "ovr {ovr}");
            let (low, high) = standing.span();
            assert!(
                (low..=high).contains(&ovr),
                "ovr {ovr} outside {standing:?}"
            );
        }
    }

    /// Given an exact rating, a range and a standing
    /// When they are sorted and filtered
    /// Then a standing sorts at the middle of its span, and a filter keeps whoever *could* be in
    ///      the bounds, so a bound never cuts through what the manager cannot tell apart
    #[test]
    fn sorting_and_filtering_never_see_inside_a_standing() {
        let good = RatingView::Standing {
            standing: PublicStanding::Good,
        };
        assert_eq!(good.sort_value(), 73);
        // 70..=76 overlaps a minimum of 75 although a player at 71 would not pass if it were exact.
        assert!(good.may_be_rated_within(Some(75), None));
        assert!(!good.may_be_rated_within(Some(77), None));
        assert!(!good.may_be_rated_within(None, Some(69)));
        let exact = RatingView::Exact { ovr: 71 };
        assert!(!exact.may_be_rated_within(Some(75), None));
        let estimate = RatingView::Estimate {
            low: 64,
            high: 70,
            confidence: Confidence::Moderate,
            current: true,
            scouted_on: "2026-09-01".to_string(),
        };
        assert_eq!(estimate.sort_value(), 67);
        assert_eq!(estimate.believed_ovr(), Some(67));
        assert_eq!(good.believed_ovr(), None);
    }

    /// Given a request for several players, one of them an id no player has
    /// When their ratings are read in one go
    /// Then each real player is answered as `rating_view` would, and the stray id is left out
    #[test]
    fn a_bulk_request_answers_each_player_and_skips_unknown_ids() {
        let game = world(vec![
            player("own", Some(MY_CLUB), 71),
            player("stranger", Some("faraway"), 74),
        ]);
        let ids = ["own", "stranger", "nobody"].map(str::to_string);

        let answer = ratings_view(&game, &ids);

        assert_eq!(answer.len(), 2);
        assert_eq!(answer["own"], RatingView::Exact { ovr: 71 });
        assert_eq!(answer["stranger"], view_of(&game, "stranger"));
    }

    /// Given a scout's youth message proposing a prospect rated 52 with potential 78, one the club
    ///       has since signed, and an id no message has
    /// When the readings are asked
    /// Then the unsigned prospect reads as standings of his rating and his ceiling; the signed one
    ///      reads his rating exactly and his ceiling as the youth staff's band, never the exact
    ///      figure; and an unknown message has none
    #[test]
    fn a_proposed_prospect_reads_as_standings_until_the_club_signs_him() {
        let mut game = world(vec![]);
        let mut offered = player("offered", None, 52);
        offered.potential = 78;
        let mut signed = player("signed", Some(MY_CLUB), 55);
        signed.potential = 84;
        let mut message = domain::message::InboxMessage::new(
            "youth-scout-1".to_string(),
            String::new(),
            String::new(),
            "Scout".to_string(),
            "2026-09-01".to_string(),
        );
        message.context.youth_prospects = Some(vec![offered, signed]);
        game.messages.push(message);

        let readings = youth_prospect_readings(&game, "youth-scout-1").unwrap();

        assert_eq!(
            readings[0].current,
            RatingView::Standing {
                standing: PublicStanding::Limited
            }
        );
        assert_eq!(
            readings[0].ceiling,
            RatingView::Standing {
                standing: PublicStanding::Strong
            }
        );
        assert_eq!(readings[1].current, RatingView::Exact { ovr: 55 });
        // With no youth staff to narrow it the band is 24 wide: 73 to 96 holds 84.
        let RatingView::Estimate { low, high, .. } = readings[1].ceiling else {
            panic!("a signed prospect's ceiling reads as an assessed band");
        };
        assert!(low <= 84 && 84 <= high && low < high, "{low}-{high}");
        let signed_wire = serde_json::to_string(&readings[1]).unwrap();
        assert!(
            !signed_wire.contains("\"exact\",\"ovr\":84"),
            "{signed_wire}"
        );
        assert!(youth_prospect_readings(&game, "nope").is_none());
        let wire = serde_json::to_string(&readings[0]).unwrap();
        assert!(!wire.contains("52") && !wire.contains("78"), "{wire}");
    }

    /// Given a prospect the club signed, who has developed since the scout proposed him
    /// When the readings are asked
    /// Then the ceiling is the band of the player in the game, not of the message's old snapshot,
    ///      and his rating is the current one.
    #[test]
    fn a_signed_prospect_reads_the_player_he_has_become() {
        let mut game = world(vec![]);
        let mut proposed = player("kid", Some(MY_CLUB), 52);
        proposed.potential = 60;
        let mut grown = player("kid", Some(MY_CLUB), 70);
        grown.potential = 95;
        game.players.push(grown);
        let mut message = domain::message::InboxMessage::new(
            "youth-scout-2".to_string(),
            String::new(),
            String::new(),
            "Scout".to_string(),
            "2026-09-01".to_string(),
        );
        message.context.youth_prospects = Some(vec![proposed]);
        game.messages.push(message);

        let reading = &youth_prospect_readings(&game, "youth-scout-2").unwrap()[0];

        assert_eq!(reading.current, RatingView::Exact { ovr: 70 });
        let RatingView::Estimate { low, high, .. } = reading.ceiling else {
            panic!("an assessed band");
        };
        assert!(low <= 95 && 95 <= high, "{low}-{high}");
    }

    /// Given a prospect the club signed and has since sold to a rival, in a message that still says
    ///       he is the club's
    /// When the readings are asked
    /// Then he reads as a stranger does, standings of his rating and ceiling: neither his current
    ///      rating nor his potential is read exactly because an old message calls him signed.
    #[test]
    fn a_prospect_sold_since_signing_reads_as_a_stranger() {
        let mut game = world(vec![]);
        let mut snapshot = player("sold", Some(MY_CLUB), 52);
        snapshot.potential = 78;
        let mut now_at_rival = player("sold", Some("faraway"), 74);
        now_at_rival.potential = 90;
        game.players.push(now_at_rival);
        let mut message = domain::message::InboxMessage::new(
            "youth-scout-3".to_string(),
            String::new(),
            String::new(),
            "Scout".to_string(),
            "2026-09-01".to_string(),
        );
        message.context.youth_prospects = Some(vec![snapshot]);
        game.messages.push(message);

        let reading = &youth_prospect_readings(&game, "youth-scout-3").unwrap()[0];

        assert!(
            matches!(reading.current, RatingView::Standing { .. }),
            "{:?}",
            reading.current
        );
        assert!(
            matches!(reading.ceiling, RatingView::Standing { .. }),
            "{:?}",
            reading.ceiling
        );
    }

    /// Given a game with a report and a standing, saved to JSON and loaded again
    /// When the ratings are read from the reloaded game
    /// Then they are the same as before the save
    #[test]
    fn the_views_survive_a_save_and_reload() {
        let mut game = world(vec![
            player("reported", Some("faraway"), 74),
            player("unknown", Some("other"), 79),
            player("own", Some(MY_CLUB), 68),
        ]);
        game.scouting_dossier.reports.push(report(
            "reported",
            "2026-03-10",
            Range { low: 70, high: 78 },
        ));
        let before: Vec<RatingView> = ["reported", "unknown", "own"]
            .iter()
            .map(|id| view_of(&game, id))
            .collect();

        let reloaded: Game = serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();

        let after: Vec<RatingView> = ["reported", "unknown", "own"]
            .iter()
            .map(|id| view_of(&reloaded, id))
            .collect();
        assert_eq!(before, after);
    }
}
