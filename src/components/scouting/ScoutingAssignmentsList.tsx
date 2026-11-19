import { Clock, RefreshCcw, XCircle } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { PlayerData, ScoutingAssignment, StaffData, TeamData } from "../../store/gameStore";
import { getTeamName } from "../../lib/helpers";
import ContextMenu from "../ContextMenu";
import {
  buildViewProfileMenuItem,
  buildViewTeamMenuItem,
} from "../playerActions/playerContextMenuItems";
import { translatePositionLabel } from "../squad/SquadTab.helpers";
import { Button, Card, CardBody, CardHeader, Select } from "../ui";
import { naturalPositionOf } from "../../lib/positions";

interface ScoutingAssignmentsListProps {
  assignments: ScoutingAssignment[];
  scouts: StaffData[];
  players: PlayerData[];
  teams: TeamData[];
  onSelectPlayer?: (id: string) => void;
  onSelectTeam?: (id: string) => void;
  /** Stops a job; no report is written for it. */
  onCancel?: (assignmentId: string) => void;
  /** Hands a job to another scout, who must be free; `idleScouts` are the candidates. */
  onReassign?: (assignmentId: string, scoutId: string) => void;
  idleScouts?: StaffData[];
}

export default function ScoutingAssignmentsList({
  assignments,
  scouts,
  players,
  teams,
  onSelectPlayer,
  onSelectTeam,
  onCancel,
  onReassign,
  idleScouts = [],
}: ScoutingAssignmentsListProps) {
  const { t } = useTranslation();
  const [reassignTargets, setReassignTargets] = useState<Record<string, string>>({});

  if (assignments.length === 0) {
    return null;
  }

  return (
    <Card>
      <CardHeader>{t("scouting.activeScoutingAssignments")}</CardHeader>
      <CardBody>
        <div className="flex flex-col gap-2">
          {assignments.map((assignment) => {
            const scout = scouts.find((staffMember) => staffMember.id === assignment.scout_id);
            const player = players.find((squadPlayer) => squadPlayer.id === assignment.player_id);

            if (!scout || !player) {
              return null;
            }

            const team = player.team_id
              ? getTeamName(teams, player.team_id)
              : t("common.freeAgent");
            const contextItems = [];

            if (onSelectPlayer) {
              contextItems.push(buildViewProfileMenuItem(t, () => onSelectPlayer(player.id)));
            }

            const teamId = player.team_id;
            if (teamId && onSelectTeam) {
              contextItems.push(buildViewTeamMenuItem(t, () => onSelectTeam(teamId)));
            }

            const row = (
              <div
                className="flex items-center gap-4 p-3 rounded-lg bg-gray-50 dark:bg-navy-700/50"
                data-testid={`scouting-assignment-${assignment.id}`}
                key={assignment.id}
              >
                <div className="flex-1 min-w-0">
                  <button
                    type="button"
                    onClick={() => onSelectPlayer?.(player.id)}
                    className="font-heading font-bold text-sm text-gray-800 dark:text-gray-100 hover:text-primary-500 transition-colors truncate block"
                  >
                    {player.full_name}
                  </button>
                  <p className="text-xs text-gray-500 dark:text-gray-400">
                    {translatePositionLabel(t, naturalPositionOf(player))} · {team}
                  </p>
                </div>
                <div className="text-right">
                  <p className="text-xs text-gray-500 dark:text-gray-400">
                    {t("scouting.scoutLabel", {
                      name: scout.display_name,
                    })}
                  </p>
                  <div className="flex items-center gap-1.5 justify-end mt-0.5">
                    <Clock className="w-3 h-3 text-accent-500" />
                    <span className="text-xs font-heading font-bold text-accent-500">
                      {t("scouting.daysLeft", { days: assignment.days_remaining })}
                    </span>
                  </div>
                </div>
                {(onCancel || onReassign) && (
                  <div className="flex flex-wrap items-center justify-end gap-2">
                    {onReassign && idleScouts.length > 0 && (
                      <>
                        <Select
                          selectSize="sm"
                          value={reassignTargets[assignment.id] ?? idleScouts[0].id}
                          aria-label={t("scouting.reassignSearchFor", { player: player.full_name })}
                          onChange={(event) =>
                            setReassignTargets((current) => ({
                              ...current,
                              [assignment.id]: event.target.value,
                            }))
                          }
                        >
                          {idleScouts.map((candidate) => (
                            <option key={candidate.id} value={candidate.id}>
                              {candidate.display_name}
                            </option>
                          ))}
                        </Select>
                        <Button
                          size="sm"
                          variant="outline"
                          icon={<RefreshCcw />}
                          onClick={() =>
                            onReassign(
                              assignment.id,
                              reassignTargets[assignment.id] ?? idleScouts[0].id,
                            )
                          }
                        >
                          {t("scouting.reassignSearch")}
                        </Button>
                      </>
                    )}
                    {onCancel && (
                      <Button
                        size="sm"
                        variant="ghost"
                        icon={<XCircle />}
                        aria-label={t("scouting.cancelSearchFor", { player: player.full_name })}
                        onClick={() => onCancel(assignment.id)}
                      >
                        {t("scouting.cancelSearch")}
                      </Button>
                    )}
                  </div>
                )}
              </div>
            );

            if (contextItems.length === 0) {
              return row;
            }

            return (
              <ContextMenu items={contextItems} key={assignment.id}>
                {row}
              </ContextMenu>
            );
          })}
        </div>
      </CardBody>
    </Card>
  );
}
