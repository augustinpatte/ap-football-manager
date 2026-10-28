//! FM24 -> Openfoot Manager adapter.
//!
//! Reads `world.db` (the importer's output, schema v3) **read-only**, selects the 240-club perimeter
//! by internal ids (never by name), converts it into OFM's own package structures
//! (`ofm_core::generator::package`), lets OFM build the world with a fixed seed, applies the few
//! post-build overrides the package format cannot carry, checks the result against the source and
//! writes one deterministic world file.
//!
//! Identifiers are `fm-p-<person.id>` / `fm-s-<person.id>` / `fm-c-<club.id>` / `fm-k-<competition.id>`:
//! the FM Unique ID is not unique (see `person.fm_uid_status`) and is only ever kept as metadata.

pub mod attributes;
pub mod checks;
pub mod competitions;
pub mod config;
pub mod finance;
pub mod free_agents;
pub mod ids;
pub mod install;
pub mod journal;
pub mod pipeline;
pub mod placement;
pub mod positions;
pub mod refs;
pub mod report;
pub mod rules;
pub mod source;
pub mod staff;

pub use config::Config;
pub use journal::Journal;

/// Error type of the crate: a message that names what failed.
pub type Result<T> = std::result::Result<T, String>;
