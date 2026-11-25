import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { formatDate } from "../../../lib/helpers";
import type { ClubReport, FastForwardResponse } from "../../../services/fastForwardService";
import { resolveBackendError } from "../../../utils/backendI18n";
import { userErrorText } from "../../../utils/errorMessage";
import { Button, TabPanel, Tabs } from "../../ui";
import type { ReportNavigation } from "./ReportLines";
import {
  DelegationTab,
  FinancesTab,
  MatchesTab,
  SquadTab,
  SummaryTab,
  TimelineTab,
  TransfersTab,
} from "./ReportSections";
import { isPartialRun, reportClubs } from "./simulationReport.helpers";

const SECTIONS = [
  "summary",
  "matches",
  "transfers",
  "finances",
  "squad",
  "delegation",
  "timeline",
] as const;
type Section = (typeof SECTIONS)[number];

const SECTION_VIEWS: Record<
  Section,
  (props: { club: ClubReport; nav?: ReportNavigation }) => React.JSX.Element
> = {
  summary: SummaryTab,
  matches: MatchesTab,
  transfers: TransfersTab,
  finances: FinancesTab,
  squad: SquadTab,
  delegation: DelegationTab,
  timeline: TimelineTab,
};

interface SimulationReportViewProps {
  response: FastForwardResponse;
  /** Leaves the report: back to the result it was opened from, or closed. */
  onBack: () => void;
  backLabel: string;
  /** Where a name in the report can lead; without it names are plain text. */
  nav?: ReportNavigation;
}

function ClubStretchHeader({ club }: { club: ClubReport }) {
  const { t } = useTranslation();
  return (
    <div className="mt-3">
      <p className="text-lead font-bold text-fg">{club.club_name}</p>
      <p className="text-caption text-fg-muted">
        {t("fastForward.report.period", {
          from: formatDate(club.from),
          until: formatDate(club.until),
          days: club.days_played,
        })}
      </p>
      <p className="text-caption text-fg-muted">
        {club.ended === "dismissed"
          ? t("fastForward.report.ended.dismissed")
          : club.ended === "moved"
            ? t("fastForward.report.ended.moved")
            : t("fastForward.report.ended.continues")}
      </p>
    </div>
  );
}

interface ClubBodyProps {
  club: ClubReport;
  section: Section;
  onSection: (section: Section) => void;
  prefix: string;
  nav?: ReportNavigation;
}

/** One club's stretch: who it was, and the report's sections for it. */
function ClubBody({ club, section, onSection, prefix, nav }: ClubBodyProps) {
  const { t } = useTranslation();
  const SectionView = SECTION_VIEWS[section];
  return (
    <>
      <ClubStretchHeader club={club} />
      <div className="mt-3">
        <Tabs
          idPrefix={prefix}
          label={t("fastForward.report.sectionsLabel")}
          tabs={SECTIONS.map((id) => ({
            id,
            label: t(`fastForward.report.tab.${id}`),
          }))}
          selectedId={section}
          onSelect={(id) => onSection(id as Section)}
        />
      </div>
      {SECTIONS.map((id) => (
        <TabPanel
          key={id}
          idPrefix={prefix}
          tabId={id}
          hidden={id !== section}
          className="mt-3 max-h-[50vh] overflow-y-auto pr-1 focus:outline-none focus-visible:ring-2 focus-visible:ring-focus"
        >
          {id === section ? <SectionView club={club} nav={nav} /> : null}
        </TabPanel>
      ))}
    </>
  );
}

/**
 * What happened in the days a simulation played, from the backend's own report. Opening it, or any
 * of its sections, changes nothing in the game: it only reads what the run handed back.
 */
export default function SimulationReportView({
  response,
  onBack,
  backLabel,
  nav,
}: SimulationReportViewProps) {
  const { t } = useTranslation();
  const prefix = useId();
  const clubsPrefix = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  // The dialog keeps its place when its phase changes; the report's title takes focus so a screen
  // reader announces where the player has arrived and the keyboard starts from there.
  useEffect(() => headingRef.current?.focus(), []);
  const clubs = reportClubs(response);
  const [clubIndex, setClubIndex] = useState(0);
  const [section, setSection] = useState<Section>("summary");
  const club = clubs[Math.min(clubIndex, clubs.length - 1)];
  const { stop } = response;

  return (
    <>
      <h2
        id="fast-forward-title"
        ref={headingRef}
        tabIndex={-1}
        className="font-heading text-title font-bold uppercase tracking-label text-fg focus:outline-none"
      >
        {t("fastForward.report.title")}
      </h2>

      {stop.kind === "error" ? (
        <p role="alert" className="mt-2 text-caption text-danger">
          {userErrorText(stop.error, t, resolveBackendError)}
        </p>
      ) : null}
      {isPartialRun(response) ? (
        <p role="status" className="mt-2 text-caption text-fg-muted">
          {t("fastForward.report.partial", {
            played: clubs.reduce((days, c) => days + c.days_played, 0),
            asked: response.progress.days_total,
          })}
        </p>
      ) : null}

      {response.report === undefined ? (
        <p role="status" className="mt-3 text-body text-fg-muted">
          {t("fastForward.report.unavailable")}
        </p>
      ) : club === undefined ? (
        <p role="status" className="mt-3 text-body text-fg-muted">
          {t("fastForward.report.noClub")}
        </p>
      ) : (
        <>
          {clubs.length > 1 ? (
            <div className="mt-3">
              <Tabs
                idPrefix={clubsPrefix}
                label={t("fastForward.report.clubsLabel")}
                look="segmented"
                tabs={clubs.map((c, i) => ({ id: String(i), label: c.club_name }))}
                selectedId={String(clubIndex)}
                onSelect={(id) => setClubIndex(Number(id))}
              />
            </div>
          ) : null}
          {clubs.length === 1 ? (
            <ClubBody
              club={club}
              section={section}
              onSection={setSection}
              prefix={prefix}
              nav={nav}
            />
          ) : null}
          {clubs.length > 1
            ? clubs.map((c, i) => (
                <TabPanel
                  key={`${c.club_id}-${c.from}`}
                  idPrefix={clubsPrefix}
                  tabId={String(i)}
                  hidden={c !== club}
                  className="focus:outline-none"
                >
                  {c === club ? (
                    <ClubBody
                      club={club}
                      section={section}
                      onSection={setSection}
                      prefix={prefix}
                      nav={nav}
                    />
                  ) : null}
                </TabPanel>
              ))
            : null}
        </>
      )}

      <div className="mt-6 flex justify-end">
        <Button onClick={onBack}>{backLabel}</Button>
      </div>
    </>
  );
}
