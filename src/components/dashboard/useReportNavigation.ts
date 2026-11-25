import type { DashboardNavigateContext } from "./dashboardProfileNavigation";
import type { ReportNavigation } from "./simulationReport/ReportLines";

/**
 * Where a name in the simulation report leads, on the dashboard behind the dialog: the dialog is
 * left first, then the existing profile, inbox or schedule opens.
 */
export function reportNavigation(
  navigate: (tab: string, context?: DashboardNavigateContext) => void,
  leaveDialog: () => void,
): ReportNavigation {
  const go = (tab: string, messageId?: string) => {
    leaveDialog();
    navigate(tab, messageId ? { messageId } : undefined);
  };
  return {
    openPlayer: (playerId) => go("__selectPlayer", playerId),
    openClub: (clubId) => go("__selectTeam", clubId),
    openInboxMessage: (messageId) => go("Inbox", messageId),
    openSchedule: () => go("Schedule"),
  };
}
