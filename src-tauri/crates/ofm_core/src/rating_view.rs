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

use serde::Serialize;

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

    pub fn rating_of(&self, player: &'a Player) -> RatingView {
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
