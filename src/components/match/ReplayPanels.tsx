import { useTranslation } from "react-i18next";

import { ProgressBar } from "../ui";
import type { ConditionEntry, LineupSide, Side, SideCounts } from "./replayDerived";

type TeamNames = { home: string; away: string };
type NameOf = (playerId: string | undefined) => string;

const SIDES: Side[] = ["home", "away"];

const STAT_ROWS: Array<{ key: keyof SideCounts; label: string; percent?: boolean }> = [
  { key: "possessionPct", label: "possession", percent: true },
  { key: "goals", label: "goals" },
  { key: "shots", label: "shots" },
  { key: "shotsOnTarget", label: "shotsOnTarget" },
  { key: "corners", label: "corners" },
  { key: "fouls", label: "fouls" },
  { key: "offsides", label: "offsides" },
  { key: "yellowCards", label: "yellowCards" },
  { key: "redCards", label: "redCards" },
];

export function ReplayStatsPanel({
  stats,
  teamNames,
  minute,
}: {
  stats: Record<Side, SideCounts>;
  teamNames: TeamNames;
  minute: number;
}) {
  const { t } = useTranslation();
  const show = (counts: SideCounts, key: keyof SideCounts, percent?: boolean) =>
    percent ? `${Math.round(counts[key])}%` : String(counts[key]);
  return (
    <table className="w-full text-body">
      <caption className="mb-2 text-left font-heading text-body font-bold uppercase tracking-label text-fg-muted">
        {t("match.replayStats.title", { minute })}
      </caption>
      <thead>
        <tr className="text-fg-muted">
          <th scope="col" className="w-1/3 py-1 text-right font-heading">
            {teamNames.home}
          </th>
          <th scope="col" className="py-1">
            <span className="sr-only">{t("match.replayTab.stats")}</span>
          </th>
          <th scope="col" className="w-1/3 py-1 text-left font-heading">
            {teamNames.away}
          </th>
        </tr>
      </thead>
      <tbody>
        {STAT_ROWS.map((row) => (
          <tr key={row.key} className="border-t border-line">
            <td className="py-1 text-right font-heading text-heading">
              {show(stats.home, row.key, row.percent)}
            </td>
            <th scope="row" className="py-1 text-center font-normal text-fg-muted">
              {t(`match.replayStats.${row.label}`)}
            </th>
            <td className="py-1 text-left font-heading text-heading">
              {show(stats.away, row.key, row.percent)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function ReplayConditionPanel({
  entries,
  teamNames,
  nameOf,
}: {
  entries: ConditionEntry[];
  teamNames: TeamNames;
  nameOf: NameOf;
}) {
  const { t } = useTranslation();
  if (entries.length === 0) {
    return <p className="text-body text-fg-muted">{t("match.replayCondition.none")}</p>;
  }
  return (
    <div>
      <h3 className="font-heading text-body font-bold uppercase tracking-label text-fg-muted">
        {t("match.replayCondition.title")}
      </h3>
      <p className="mb-3 text-caption text-fg-muted">{t("match.replayCondition.hint")}</p>
      <div className="grid gap-4 @split:grid-cols-2 md:grid-cols-2">
        {SIDES.map((side) => (
          <section key={side} aria-label={teamNames[side]}>
            <h4 className="mb-1 font-heading text-body font-bold">{teamNames[side]}</h4>
            <ul className="space-y-1">
              {entries
                .filter((entry) => entry.side === side)
                .sort((a, b) => a.condition - b.condition)
                .map((entry) => (
                  <li key={entry.playerId} className="flex items-center gap-2 text-body">
                    <span className="w-40 shrink-0 truncate">{nameOf(entry.playerId)}</span>
                    <ProgressBar
                      value={entry.condition}
                      variant="primary"
                      size="md"
                      className="flex-1"
                    />
                    <span className="w-9 shrink-0 text-right font-heading">
                      {Math.round(entry.condition)}
                    </span>
                  </li>
                ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}

export function ReplayLineupPanel({
  lineup,
  teamNames,
  nameOf,
}: {
  lineup: Record<Side, LineupSide>;
  teamNames: TeamNames;
  nameOf: NameOf;
}) {
  const { t } = useTranslation();
  return (
    <div>
      <p className="mb-3 rounded border border-line bg-sunken p-2 text-caption text-fg-muted">
        {t("match.replayLineup.replayOnly")}
      </p>
      <div className="grid gap-4 md:grid-cols-2">
        {SIDES.map((side) => (
          <section key={side} aria-label={teamNames[side]}>
            <h4 className="mb-1 font-heading text-body font-bold">{teamNames[side]}</h4>
            <PlayerGroup
              title={t("match.replayLineup.onPitch")}
              ids={lineup[side].onPitch}
              nameOf={nameOf}
            />
            <PlayerGroup
              title={t("match.replayLineup.bench")}
              ids={lineup[side].bench}
              nameOf={nameOf}
            />
            {lineup[side].gone.length > 0 ? (
              <div className="mt-2">
                <h5 className="text-caption font-bold uppercase tracking-label text-fg-muted">
                  {t("match.replayLineup.gone")}
                </h5>
                <ul className="text-body">
                  {lineup[side].gone.map((entry) => (
                    <li key={entry.playerId}>
                      {nameOf(entry.playerId)}{" "}
                      <span className="text-fg-muted">
                        ({t(`match.replayLineup.${entry.reason}`)})
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
          </section>
        ))}
      </div>
    </div>
  );
}

function PlayerGroup({ title, ids, nameOf }: { title: string; ids: string[]; nameOf: NameOf }) {
  return (
    <div className="mt-2">
      <h5 className="text-caption font-bold uppercase tracking-label text-fg-muted">{title}</h5>
      <ul className="text-body">
        {ids.map((id) => (
          <li key={id}>{nameOf(id)}</li>
        ))}
      </ul>
    </div>
  );
}
