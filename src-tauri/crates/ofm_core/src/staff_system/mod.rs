//! The staff as a working organisation: posts and limits, hiring, delegation, the assistant's
//! recommendations and what the staff's quality changes in the simulation.

pub mod ai;
pub mod assistant;
pub mod delegation;
mod delegation_control;
mod delegation_listing;
mod delegation_offers;
pub mod delegation_policy;
mod delegation_recruitment;
mod delegation_report;
#[cfg(test)]
mod delegation_scenarios;
mod delegation_squad;
pub mod market;
pub mod profile;
