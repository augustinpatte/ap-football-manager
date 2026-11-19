//! Results of the scouting tools.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::tool_results;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoutDispatched {
    pub scout_id: String,
    pub scout_name: String,
    pub player_id: String,
    pub player_name: String,
}

impl fmt::Display for ScoutDispatched {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "## Scout Dispatched\n\n**{}** will report on **{}**.",
            self.scout_name, self.player_name
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoutReport {
    pub message_id: String,
    pub player_name: String,
    pub position: String,
    pub avg_rating: Option<u32>,
    /// `None` for a free agent.
    pub team: Option<String>,
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoutingAssignment {
    pub id: String,
    pub scout_name: String,
    pub player_name: String,
    pub days_remaining: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoutReports {
    pub reports: Vec<ScoutReport>,
    pub active_assignments: Vec<ScoutingAssignment>,
}

impl fmt::Display for ScoutReports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.reports.is_empty() {
            write!(f, "## Scout Reports\n\nNo scout reports available.")?;
        } else {
            self.write_reports(f)?;
        }
        if !self.active_assignments.is_empty() {
            self.write_assignments(f)?;
        }
        Ok(())
    }
}

impl ScoutReports {
    fn write_reports(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "## Scout Reports ({} reports)\n\n| ID | Player | Pos | Rating | Team | Read |\n|----|--------|-----|--------|------|------|\n",
            self.reports.len()
        )?;
        for report in &self.reports {
            let rating = report
                .avg_rating
                .map_or_else(|| "?".to_string(), |v| format!("{v}/100"));
            writeln!(
                f,
                "| {} | {} | {} | {} | {} | {} |",
                report.message_id,
                report.player_name,
                report.position,
                rating,
                report.team.as_deref().unwrap_or("Free"),
                if report.read { "✓" } else { "●" },
            )?;
        }
        Ok(())
    }

    fn write_assignments(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\n### Active Assignments ({} pending)\n\n| ID | Scout | Player | Days Left |\n|----|-------|--------|------------|\n",
            self.active_assignments.len()
        )?;
        for a in &self.active_assignments {
            writeln!(
                f,
                "| {} | {} | {} | {} |",
                a.id, a.scout_name, a.player_name, a.days_remaining
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct YouthScoutingStarted {
    pub scout_id: String,
}

impl fmt::Display for YouthScoutingStarted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "## Youth Scouting Started\n\nAssignment created. Check `scout_get_reports` for results over time."
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct YouthScoutingCancelled {
    pub assignment_id: String,
}

impl fmt::Display for YouthScoutingCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "## Youth Scouting Cancelled\n\nAssignment has been cancelled."
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct YouthScoutingReassigned {
    pub assignment_id: String,
    pub scout_id: String,
}

impl fmt::Display for YouthScoutingReassigned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "## Youth Scouting Reassigned\n\nScout has been changed.")
    }
}

tool_results!(
    ScoutDispatched,
    ScoutReports,
    YouthScoutingStarted,
    YouthScoutingCancelled,
    YouthScoutingReassigned,
    ScoutDossier,
    ShortlistChanged,
    ScoutingCancelled,
    AcademyReviewResult,
);

/// One row of the dossier: a player and what the club knows of him. Ranges, never exact hidden
/// values; `None` means nobody has judged it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DossierRow {
    pub player_id: String,
    pub player_name: String,
    pub position: String,
    pub age: i32,
    pub team: Option<String>,
    pub public_rating: u8,
    pub ability_range: Option<String>,
    pub potential_range: Option<String>,
    pub confidence: Option<String>,
    pub outlook: String,
    pub squad_impact: Option<String>,
    pub fee_affordable: Option<bool>,
    pub wage_permitted: bool,
    pub risks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoutDossier {
    pub shortlist: Vec<DossierRow>,
    pub reports: Vec<DossierRow>,
}

impl fmt::Display for ScoutDossier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_dossier_table(f, "Shortlist", &self.shortlist)?;
        write_dossier_table(f, "Scouting Reports", &self.reports)?;
        write!(
            f,
            "\n*Ranges contain the true value but are not centred on it; a better scout or a second report narrows them. `-` means nobody has judged it.*"
        )
    }
}

fn write_dossier_table(
    f: &mut fmt::Formatter<'_>,
    title: &str,
    rows: &[DossierRow],
) -> fmt::Result {
    if rows.is_empty() {
        return write!(f, "## {title}\n\nNone.\n\n");
    }
    write!(
        f,
        "## {title} ({})\n\n| ID | Player | Pos | Age | Team | Rating | Ability | Potential | Confidence | Outlook | Squad | Risks |\n|----|--------|-----|-----|------|--------|---------|-----------|------------|---------|-------|-------|\n",
        rows.len()
    )?;
    for row in rows {
        writeln!(
            f,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            row.player_id,
            row.player_name,
            row.position,
            row.age,
            row.team.as_deref().unwrap_or("Free"),
            row.public_rating,
            row.ability_range.as_deref().unwrap_or("-"),
            row.potential_range.as_deref().unwrap_or("-"),
            row.confidence.as_deref().unwrap_or("-"),
            row.outlook,
            row.squad_impact.as_deref().unwrap_or("-"),
            row.risks.join(", "),
        )?;
    }
    writeln!(f)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortlistChanged {
    pub player_name: String,
    pub shortlisted: bool,
}

impl fmt::Display for ShortlistChanged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.shortlisted {
            write!(
                f,
                "## Shortlisted\n\n**{}** is on your shortlist.",
                self.player_name
            )
        } else {
            write!(
                f,
                "## Removed from Shortlist\n\n**{}** is off your shortlist.",
                self.player_name
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoutingCancelled {
    pub assignment_id: String,
}

impl fmt::Display for ScoutingCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "## Scouting Cancelled\n\nThe job was stopped; no report will be written for it."
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcademyRowResult {
    pub player_id: String,
    pub player_name: String,
    pub age: i32,
    pub position: String,
    pub rating: u8,
    /// The ceiling: what development may let him reach, not a promise.
    pub potential_ceiling: u8,
    pub expected_rating_next_year: u8,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub promotion_blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcademyReviewResult {
    pub seniors: usize,
    pub senior_limit: usize,
    pub next_intake: usize,
    pub rows: Vec<AcademyRowResult>,
}

impl fmt::Display for AcademyReviewResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "## Academy Review\n\nSenior squad {}/{}; next intake {}.\n\n",
            self.seniors, self.senior_limit, self.next_intake
        )?;
        if self.rows.is_empty() {
            return write!(f, "No academy players.");
        }
        write!(
            f,
            "| ID | Player | Age | Pos | Rating | Ceiling | Next year | Verdict | Reasons |\n|----|--------|-----|-----|--------|---------|-----------|---------|---------|\n"
        )?;
        for row in &self.rows {
            writeln!(
                f,
                "| {} | {} | {} | {} | {} | {} | {} | {}{} | {} |",
                row.player_id,
                row.player_name,
                row.age,
                row.position,
                row.rating,
                row.potential_ceiling,
                row.expected_rating_next_year,
                row.verdict,
                if row.promotion_blocked {
                    " (no senior place)"
                } else {
                    ""
                },
                row.reasons.join(", "),
            )?;
        }
        write!(f, "\n*Potential is a ceiling, not a promise. Promote with `squad_set_player_role` while a senior place is free.*")
    }
}
