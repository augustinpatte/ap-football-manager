import { useEffect, useState } from "react";
import { type GameStateData, type StaffData, useGameStore } from "../../store/gameStore";
import {
  getStaff,
  getStaffOverview,
  proposeStaffContract,
  renewStaffContract,
  searchStaffCandidates,
  type CandidateView,
  type StaffOverview,
  type StaffSlice,
} from "../../services/staffService";
import { resolveBackendError } from "../../utils/backendI18n";
import OfferDialog from "./OfferDialog";
import StaffManagementPanel from "./StaffManagementPanel";
import { Button, Card, CardBody, Badge, CountryFlag, Dialog, ProgressBar } from "../ui";
import {
  UserCog,
  Search,
  UserPlus,
  UserMinus,
  Briefcase,
  Eye,
  Stethoscope,
  GraduationCap,
  Star,
  Settings2,
} from "lucide-react";
import { getTeamName, calcAge, formatVal, formatWeeklyWage } from "../../lib/helpers";
import { countryName } from "../../lib/countries";
import { useTranslation } from "react-i18next";
import { releaseStaff } from "../../services/staffService";
import ContextMenu, { type ContextMenuItem } from "../ContextMenu";
import type { DashboardNavigateContext } from "../dashboard/dashboardProfileNavigation";

interface StaffTabProps {
  gameState: GameStateData | null;
  onGameUpdate?: (state: GameStateData) => void;
  onNavigate?: (tab: string, context?: DashboardNavigateContext) => void;
}

const ROLE_ICONS: Record<string, React.ReactNode> = {
  AssistantManager: <Briefcase className="w-4 h-4" />,
  Coach: <GraduationCap className="w-4 h-4" />,
  Scout: <Eye className="w-4 h-4" />,
  Physio: <Stethoscope className="w-4 h-4" />,
};
// The icon names the role; the colour only tells roles apart, so it carries no good-or-bad meaning.
const ROLE_COLORS: Record<string, string> = {
  AssistantManager: "text-info",
  Coach: "text-positive",
  Scout: "text-warning",
  Physio: "text-fg-muted",
};

const FOCUS_RING =
  "focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page";
const CHIP_ON = "bg-primary-700 text-on-brand shadow-md";
const CHIP_OFF = "bg-panel text-fg-muted border border-line hover:text-fg";
const CHIP_BASE = `rounded-lg font-heading font-bold uppercase tracking-label transition-all ${FOCUS_RING}`;
const VIEW_BUTTON = `${CHIP_BASE} flex items-center gap-1.5 px-4 py-2 text-body`;
const ROLE_BUTTON = `${CHIP_BASE} flex items-center gap-1 px-3 py-1.5 text-caption`;
const ICON_BUTTON = `rounded-lg p-2 transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${FOCUS_RING}`;
const LINK_BUTTON = `mt-3 rounded text-caption font-heading font-bold uppercase tracking-label text-positive hover:underline ${FOCUS_RING}`;

function bestAttr(s: StaffData): { key: string; value: number } {
  const attrs = [
    { key: "coaching", value: s.attributes.coaching },
    { key: "judgingAbility", value: s.attributes.judgingAbility },
    { key: "judgingPotential", value: s.attributes.judgingPotential },
    { key: "physiotherapy", value: s.attributes.physiotherapy },
  ];
  return attrs.reduce((a, b) => (b.value > a.value ? b : a));
}

/**
 * Per-role attribute weights, mirroring what the engine actually consumes.
 * A flat average over all four attributes rated a specialist on work their
 * role never does — an elite physio read as mediocre because coaching and
 * scouting dragged the number down.
 *
 * Sources, so these stay honest if the engine changes:
 * - Coach: `coaching` alone drives the training multiplier (`training.rs`).
 * - Physio: `physiotherapy` alone drives recovery (`training.rs`).
 * - Scout: `judgingAbility` sets assignment speed, `judgingPotential` sets
 *   potential accuracy (`scouting.rs`).
 * - AssistantManager: `(coaching*4 + judgingAbility*3 + judgingPotential*3)/10`
 *   is the engine's own `assistant_quality` (`delegated_renewals.rs`).
 */
const ROLE_ATTR_WEIGHTS: Record<string, Partial<Record<keyof StaffData["attributes"], number>>> = {
  Coach: { coaching: 10 },
  Physio: { physiotherapy: 10 },
  Scout: { judgingAbility: 5, judgingPotential: 5 },
  AssistantManager: { coaching: 4, judgingAbility: 3, judgingPotential: 3 },
};

/**
 * Weighting for a role we do not recognise. An even split says "no opinion",
 * which is the honest answer; borrowing another role's weighting would rate
 * someone confidently on work their role may never do.
 */
const UNKNOWN_ROLE_WEIGHTS = {
  coaching: 1,
  judgingAbility: 1,
  judgingPotential: 1,
  physiotherapy: 1,
} as const;

function ovrRating(s: StaffData): number {
  const weights = ROLE_ATTR_WEIGHTS[s.role] ?? UNKNOWN_ROLE_WEIGHTS;
  const total = Object.values(weights).reduce((sum, w) => sum + (w ?? 0), 0);
  if (total === 0) return 0;
  const weighted = Object.entries(weights).reduce(
    (sum, [key, weight]) =>
      sum + s.attributes[key as keyof StaffData["attributes"]] * (weight ?? 0),
    0,
  );
  return Math.round(weighted / total);
}

export default function StaffTab({ gameState, onGameUpdate, onNavigate }: StaffTabProps) {
  const { t, i18n } = useTranslation();
  const { sessionState } = useGameStore();
  const [fetchedStaff, setFetchedStaff] = useState<StaffSlice | null>(null);
  // The dashboard alert for waiting proposals sends the manager here; the answers live on the
  // management view, so that is where the tab opens while any are pending.
  const [view, setView] = useState<"mystaff" | "available" | "management">(() =>
    (gameState?.delegation?.proposals?.length ?? 0) > 0 ? "management" : "mystaff",
  );
  const [search, setSearch] = useState("");
  const [roleFilter, setRoleFilter] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [overview, setOverview] = useState<StaffOverview | null>(null);
  const [candidates, setCandidates] = useState<Record<string, CandidateView>>({});
  const [offer, setOffer] = useState<{ staff: StaffData; mode: "hire" | "renew" } | null>(null);
  const [releasing, setReleasing] = useState<StaffData | null>(null);
  const [actionError, setActionError] = useState("");

  const teamId = sessionState?.manager?.team_id ?? gameState?.manager?.team_id ?? null;

  useEffect(() => {
    if (!teamId) return;
    void getStaff(teamId)
      .then(setFetchedStaff)
      .catch(() => {});
  }, [teamId]);

  // The seats, budget, contract terms and delegation, and the asking wages of free agents: all of
  // it comes from the backend, so it is read again after every change to the game.
  const refreshManagement = () => {
    if (!teamId) return;
    void getStaffOverview()
      .then(setOverview)
      .catch(() => {});
    void searchStaffCandidates(null, 0)
      .then((found) => setCandidates(Object.fromEntries(found.map((c) => [c.staff.id, c]))))
      .catch(() => {});
  };
  useEffect(refreshManagement, [teamId]);

  const openScoutingWorkflowLabel = t("staff.openScoutingWorkflow");

  const myStaff =
    fetchedStaff?.team_staff ?? gameState?.staff.filter((s) => s.team_id === teamId) ?? [];
  const availableStaff =
    fetchedStaff?.available_staff ?? gameState?.staff.filter((s) => !s.team_id) ?? [];
  const assignments = fetchedStaff?.scouting_assignments ?? gameState?.scouting_assignments ?? [];
  const youthAssignments =
    fetchedStaff?.youth_scouting_assignments ?? gameState?.youth_scouting_assignments ?? [];

  const applyStaffUpdate = (updated: GameStateData) => {
    onGameUpdate?.(updated);
    refreshManagement();
    setFetchedStaff({
      team_staff: updated.staff.filter((s) => s.team_id === teamId),
      available_staff: updated.staff.filter((s) => !s.team_id),
      scouting_assignments: updated.scouting_assignments,
      youth_scouting_assignments: updated.youth_scouting_assignments ?? [],
    });
  };

  const handleOffer = async (wage: number, years: number) => {
    if (!offer) throw new Error("be.error.staffMemberNotFound");
    const outcome = await (offer.mode === "hire" ? proposeStaffContract : renewStaffContract)(
      offer.staff.id,
      wage,
      years,
    );
    if (outcome.response.status === "accepted") applyStaffUpdate(outcome.game);
    return outcome.response;
  };

  const handleRelease = async (staffId: string) => {
    setActionLoading(staffId);
    setActionError("");
    try {
      applyStaffUpdate(await releaseStaff(staffId));
      setReleasing(null);
    } catch (err) {
      setActionError(resolveBackendError(err));
    } finally {
      setActionLoading(null);
    }
  };

  const memberOf = (staffId: string) => overview?.members.find((m) => m.id === staffId);

  const displayStaff = view === "mystaff" ? myStaff : availableStaff;

  const filtered = displayStaff.filter((s) => {
    if (roleFilter && s.role !== roleFilter) return false;
    if (search.length >= 2) {
      const q = search.toLowerCase();
      // The name shown and the full name both find the person; which one is shown is the
      // backend's call (`display_name`).
      const names = [s.display_name, `${s.first_name} ${s.last_name}`].map((name) =>
        name.toLowerCase(),
      );
      if (!names.some((name) => name.includes(q))) return false;
    }
    return true;
  });

  const roles = ["AssistantManager", "Coach", "Scout", "Physio"];

  return (
    <div>
      {/* View toggle */}
      <div className="flex flex-wrap gap-3 mb-4 items-center">
        <div className="flex gap-2">
          <button
            type="button"
            aria-pressed={view === "mystaff"}
            onClick={() => setView("mystaff")}
            className={`${VIEW_BUTTON} ${view === "mystaff" ? CHIP_ON : CHIP_OFF}`}
          >
            <UserCog className="w-4 h-4" /> {t("staff.myStaff", { count: myStaff.length })}
          </button>
          <button
            type="button"
            aria-pressed={view === "available"}
            onClick={() => setView("available")}
            className={`${VIEW_BUTTON} ${view === "available" ? CHIP_ON : CHIP_OFF}`}
          >
            <UserPlus className="w-4 h-4" />{" "}
            {t("staff.available", { count: availableStaff.length })}
          </button>
          <button
            type="button"
            aria-pressed={view === "management"}
            onClick={() => setView("management")}
            className={`${VIEW_BUTTON} ${view === "management" ? CHIP_ON : CHIP_OFF}`}
          >
            <Settings2 className="w-4 h-4" /> {t("staff.mgmt.tab")}
            {overview && overview.delegation.proposals.length > 0
              ? ` (${overview.delegation.proposals.length})`
              : ""}
          </button>
        </div>

        <div className="relative flex-1 min-w-[180px] max-w-xs">
          <Search
            aria-hidden="true"
            className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-fg-subtle"
          />
          <input
            type="search"
            aria-label={t("staff.searchStaff")}
            placeholder={t("staff.searchStaff")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className={`w-full rounded-lg border border-line bg-panel py-2 pl-9 pr-3 text-body text-fg placeholder:text-fg-subtle ${FOCUS_RING}`}
          />
        </div>

        <div className="flex gap-1.5">
          <button
            type="button"
            aria-pressed={!roleFilter}
            onClick={() => setRoleFilter(null)}
            className={`${ROLE_BUTTON} ${!roleFilter ? CHIP_ON : CHIP_OFF}`}
          >
            {t("common.all")}
          </button>
          {roles.map((r) => (
            <button
              type="button"
              key={r}
              aria-pressed={roleFilter === r}
              onClick={() => setRoleFilter(roleFilter === r ? null : r)}
              className={`${ROLE_BUTTON} ${roleFilter === r ? CHIP_ON : CHIP_OFF}`}
            >
              {ROLE_ICONS[r]} {t(`staff.roles.${r}`)}
            </button>
          ))}
        </div>
      </div>

      {actionError ? (
        <p role="alert" className="mb-3 text-body text-danger">
          {actionError}
        </p>
      ) : null}

      {view === "management" ? (
        <StaffManagementPanel
          overview={overview}
          onGameUpdate={(updated) => {
            onGameUpdate?.(updated);
            refreshManagement();
          }}
        />
      ) : filtered.length === 0 ? (
        <div className="py-12 text-center">
          <UserCog aria-hidden="true" className="w-12 h-12 text-fg-subtle mx-auto mb-3" />
          <p role="status" className="text-body text-fg-muted">
            {view === "mystaff" ? t("staff.noStaffMatch") : t("staff.noAvailableStaff")}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {filtered.map((staff) => {
            const roleIcon = ROLE_ICONS[staff.role] || ROLE_ICONS.Coach;
            const roleColor = ROLE_COLORS[staff.role] || ROLE_COLORS.Coach;
            const age = calcAge(staff.date_of_birth);
            const ovr = ovrRating(staff);
            const best = bestAttr(staff);
            const isLoading = actionLoading === staff.id;
            const scoutingLoad =
              assignments.filter((a) => a.scout_id === staff.id).length +
              youthAssignments.filter((a) => a.scout_id === staff.id).length;
            const youthLoad = youthAssignments.filter((a) => a.scout_id === staff.id).length;
            const scoutingLoadLabel = `${scoutingLoad} ${t(
              scoutingLoad === 1 ? "staff.activeAssignment" : "staff.activeAssignments",
            )}`;
            const youthLoadLabel = `${youthLoad} ${t(
              youthLoad === 1 ? "staff.youthSearch" : "staff.youthSearches",
            )}`;
            const contextItems: ContextMenuItem[] =
              view === "mystaff"
                ? [
                    ...(staff.role === "Scout" && onNavigate
                      ? [
                          {
                            label: openScoutingWorkflowLabel,
                            icon: <Eye className="w-4 h-4" />,
                            onClick: () => onNavigate("Scouting"),
                            disabled: false,
                          } satisfies ContextMenuItem,
                        ]
                      : []),
                    {
                      label: t("staff.releaseStaff"),
                      icon: <UserMinus className="w-4 h-4" />,
                      onClick: () => setReleasing(staff),
                      danger: true,
                      disabled: isLoading,
                    },
                  ]
                : [
                    {
                      label: t("staff.hireStaff"),
                      icon: <UserPlus className="w-4 h-4" />,
                      onClick: () => setOffer({ staff, mode: "hire" }),
                      disabled: isLoading,
                    },
                  ];
            const staffCard = (
              <div data-testid={`staff-card-${staff.id}`} className="h-full">
                <Card className="h-full">
                  <CardBody>
                    <div className="flex items-start gap-4">
                      {/* Avatar */}
                      <div
                        className={`w-12 h-12 rounded-xl flex items-center justify-center ${roleColor} bg-sunken`}
                      >
                        {roleIcon}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <h3 className="font-heading font-bold text-body text-fg uppercase tracking-label truncate">
                            {staff.display_name}
                          </h3>
                          <Badge
                            variant={ovr >= 65 ? "success" : ovr >= 45 ? "primary" : "neutral"}
                            size="sm"
                          >
                            {ovr} OVR
                          </Badge>
                        </div>
                        <p className="text-caption text-fg-muted mt-0.5">
                          {t(`staff.roles.${staff.role}`)} — {t("common.age")} {age}
                          <span className="ml-1.5 inline-flex items-center gap-1 align-middle">
                            <CountryFlag
                              code={staff.nationality}
                              locale={i18n.language}
                              className="text-caption leading-none"
                            />
                            <span>{countryName(staff.nationality, i18n.language)}</span>
                          </span>
                          {staff.team_id && view === "available" && (
                            <span className="ml-1.5">
                              @ {getTeamName(gameState?.teams ?? [], staff.team_id)}
                            </span>
                          )}
                        </p>

                        {/* Specialization + Wage */}
                        <div className="flex flex-wrap gap-1.5 mt-1.5">
                          {staff.specialization && (
                            <Badge variant="accent" size="sm" className="gap-1">
                              <Star aria-hidden="true" className="w-3 h-3" />{" "}
                              {t(`staff.specializations.${staff.specialization}`)}
                            </Badge>
                          )}
                          {staff.wage > 0 && (
                            <Badge variant="neutral" size="sm">
                              {formatWeeklyWage(staff.wage, "compact")}
                            </Badge>
                          )}
                          {staff.role === "Scout" ? (
                            <Badge variant="primary" size="sm">
                              {scoutingLoadLabel}
                            </Badge>
                          ) : null}
                          {staff.role === "Scout" && youthLoad > 0 ? (
                            <Badge variant="accent" size="sm">
                              {youthLoadLabel}
                            </Badge>
                          ) : null}
                        </div>

                        {/* Contract terms (own staff) or asking wage (free agents) */}
                        {view === "mystaff" && memberOf(staff.id) ? (
                          <p className="text-caption text-fg-muted mt-2">
                            {t(`staff.posts.${memberOf(staff.id)?.post}`)} ·{" "}
                            {memberOf(staff.id)?.days_left == null
                              ? t("staff.contract.openEnded")
                              : t("staff.contract.daysLeft", {
                                  days: Math.max(0, memberOf(staff.id)?.days_left ?? 0),
                                })}
                          </p>
                        ) : null}
                        {view === "available" && candidates[staff.id] ? (
                          <p className="text-caption text-fg-muted mt-2">
                            {t("staff.contract.asks", {
                              wage: formatWeeklyWage(candidates[staff.id].asking_wage, "compact"),
                            })}
                            {candidates[staff.id].blocker
                              ? ` — ${resolveBackendError(candidates[staff.id].blocker)}`
                              : ""}
                          </p>
                        ) : null}

                        {/* Attributes */}
                        <div className="grid grid-cols-2 gap-x-4 gap-y-1.5 mt-3">
                          <AttrBar
                            label={t("staff.attrs.coaching")}
                            value={staff.attributes.coaching}
                          />
                          <AttrBar
                            label={t("staff.attrs.judgingAbility")}
                            value={staff.attributes.judgingAbility}
                          />
                          <AttrBar
                            label={t("staff.attrs.judgingPotential")}
                            value={staff.attributes.judgingPotential}
                          />
                          <AttrBar
                            label={t("staff.attrs.physiotherapy")}
                            value={staff.attributes.physiotherapy}
                          />
                        </div>

                        <p className="text-caption text-fg-subtle mt-2">
                          {t("staff.best")}:{" "}
                          <span className="font-medium text-fg-muted">
                            {t(`staff.attrs.${best.key}`)} ({best.value})
                          </span>
                        </p>

                        {staff.role === "Scout" && onNavigate ? (
                          <button
                            type="button"
                            className={LINK_BUTTON}
                            onClick={() => onNavigate("Scouting")}
                          >
                            {openScoutingWorkflowLabel}
                          </button>
                        ) : null}
                      </div>

                      {view === "mystaff" && memberOf(staff.id) ? (
                        <button
                          type="button"
                          disabled={isLoading}
                          onClick={() => setOffer({ staff, mode: "renew" })}
                          className={`${ICON_BUTTON} bg-sunken text-fg-muted hover:text-fg`}
                          title={t("staff.contract.renew")}
                          aria-label={t("staff.contract.renew")}
                        >
                          <Briefcase aria-hidden="true" className="w-4 h-4" />
                        </button>
                      ) : null}
                      {/* Action button */}
                      {view === "mystaff" && (
                        <button
                          type="button"
                          disabled={isLoading}
                          onClick={() => setReleasing(staff)}
                          className={`${ICON_BUTTON} bg-danger-tint text-danger hover:bg-hover`}
                          title={t("staff.releaseStaff")}
                          aria-label={t("staff.releaseStaff")}
                        >
                          <UserMinus aria-hidden="true" className="w-4 h-4" />
                        </button>
                      )}
                      {view === "available" && (
                        <button
                          type="button"
                          disabled={isLoading}
                          onClick={() => setOffer({ staff, mode: "hire" })}
                          className={`${ICON_BUTTON} bg-positive-tint text-positive hover:bg-hover`}
                          title={t("staff.hireStaff")}
                          aria-label={t("staff.hireStaff")}
                        >
                          <UserPlus aria-hidden="true" className="w-4 h-4" />
                        </button>
                      )}
                    </div>
                  </CardBody>
                </Card>
              </div>
            );

            return (
              <ContextMenu items={contextItems} key={staff.id}>
                {staffCard}
              </ContextMenu>
            );
          })}
        </div>
      )}
      {offer ? (
        <OfferDialog
          mode={offer.mode}
          name={offer.staff.display_name}
          askingWage={
            offer.mode === "hire"
              ? (candidates[offer.staff.id]?.asking_wage ?? offer.staff.wage)
              : (memberOf(offer.staff.id)?.renewal_ask ?? offer.staff.wage)
          }
          maxYears={overview?.budget.max_years ?? 3}
          signingFeeWeeks={overview?.budget.signing_fee_weeks ?? 1}
          onSubmit={handleOffer}
          onClose={() => setOffer(null)}
        />
      ) : null}
      {releasing ? (
        <Dialog
          label={t("staff.release.confirmTitle", { name: releasing.display_name })}
          onClose={() => setReleasing(null)}
          className="w-full max-w-md p-5"
        >
          <h2 className="font-heading text-heading uppercase">
            {t("staff.release.confirmTitle", { name: releasing.display_name })}
          </h2>
          <p className="mt-3 text-body text-fg-muted">
            {t("staff.release.confirmBody", {
              amount: formatVal(memberOf(releasing.id)?.severance ?? 0),
            })}
          </p>
          <div className="mt-5 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setReleasing(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="accent"
              disabled={actionLoading === releasing.id}
              onClick={() => void handleRelease(releasing.id)}
            >
              {t("staff.release.confirm")}
            </Button>
          </div>
        </Dialog>
      ) : null}
    </div>
  );
}

function AttrBar({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div className="flex justify-between text-caption mb-0.5">
        <span className="text-fg-muted">{label}</span>
        <span className="font-heading font-bold tabular-nums text-fg">{value}</span>
      </div>
      <ProgressBar
        value={value}
        variant={value >= 70 ? "success" : value >= 50 ? "primary" : "accent"}
        size="sm"
      />
    </div>
  );
}
