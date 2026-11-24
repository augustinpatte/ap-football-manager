//! What the season rollover does with a league-phase competition.
//!
//! Kept beside the competition rather than in `end_of_season`, which only asks: *offer this
//! competition its next field*, and *renew it*. An edition is renewed only once it is on record in
//! the archive (or was never played); one that is still being played is kept, with the field the
//! next edition will be drawn from set aside for the daily renewal.

use chrono::{DateTime, Utc};
use domain::edition_archive::CompletedEdition;
use domain::league::League;
use domain::team::Team;

/// Offers `entrants` as the next edition's field. A field of the wrong size is refused (and
/// logged), never squeezed in. `renewing_now` says the rollover is about to renew the competition:
/// the field becomes its participants. Otherwise (still being played) it is set aside.
pub(crate) fn offer_field(competition: &mut League, entrants: &[String], renewing_now: bool) {
    let Some(rules) = super::rules_of(competition) else {
        return;
    };
    let field_size = rules.field_size();
    if entrants.len() != field_size {
        log::warn!(
            "[rollover] {} was offered {} clubs for its {field_size}-club league phase; keeping its field",
            competition.id,
            entrants.len()
        );
    } else if renewing_now {
        competition.participant_ids = entrants.to_vec();
    } else if let Some(state) = competition.league_phase.as_mut() {
        state.next_entrants = entrants.to_vec();
    }
}

/// Renews the competition for its next edition at the rollover. An edition that was played but is
/// not in the archive is left as it is: renewing it would lose it. A failed draw is recorded on the
/// competition (stage `Created`, `draw_error`) and retried by the daily renewal.
pub(crate) fn renew(
    competition: &mut League,
    teams: &[Team],
    archive: &[CompletedEdition],
    coefficients: &std::collections::BTreeMap<String, u32>,
    salt: u64,
    (season, start): (u32, DateTime<Utc>),
) {
    let on_record = archive
        .iter()
        .any(|edition| edition.is_edition_of(&competition.id, competition.season));
    if super::edition_was_played(competition) && !on_record {
        log::error!(
            "[rollover] {} season {} is not in the archive and is kept, not renewed",
            competition.id,
            competition.season
        );
        return;
    }
    if let Err(error) =
        super::renew_with_coefficients(competition, teams, coefficients, salt, season, start)
    {
        log::error!(
            "[rollover] {} has no next edition: {}",
            competition.id,
            error.key
        );
    }
}
