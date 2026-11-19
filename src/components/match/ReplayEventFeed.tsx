import { useTranslation } from "react-i18next";

import type { FeedEntry } from "./replayDerived";

interface ReplayEventFeedProps {
  entries: FeedEntry[];
  teamNames: { home: string; away: string };
  nameOf: (playerId: string | undefined) => string;
  /** The stopwatch reading of an instant (`45+2'`); the plain minute when absent. */
  minuteLabel?: (t_ms: number) => string;
  /**
   * Whether new entries are announced as they appear. On by default for a replay; the live screen
   * turns it off, because a list that grows while a match is played would be read out again and
   * again, and announces the score and goals through its one status line instead.
   */
  announce?: boolean;
}

/** The match events a player reads, each in the language of the game and with real names. */
export default function ReplayEventFeed({
  entries,
  teamNames,
  nameOf,
  minuteLabel,
  announce = true,
}: ReplayEventFeedProps) {
  const { t } = useTranslation();
  return (
    <section aria-label={t("match.replayFeedTitle")}>
      <h3 className="font-heading text-body font-bold uppercase tracking-label text-fg-muted">
        {t("match.replayFeedTitle")}
      </h3>
      <ul aria-live={announce ? "polite" : "off"} className="mt-2 space-y-1 text-body">
        {entries.map((entry) => (
          <li
            // Never the position: a newest-first list would renumber every entry when one arrives.
            key={`${entry.t_ms}-${entry.kind}-${entry.playerId ?? ""}-${entry.otherPlayerId ?? ""}`}
            className="flex flex-wrap items-baseline gap-x-2 text-fg"
          >
            <span className="w-14 shrink-0 text-right font-heading text-fg-muted">
              {minuteLabel ? minuteLabel(entry.t_ms) : `${Math.floor(entry.t_ms / 60_000)}'`}
            </span>
            <span>
              {t(`match.replayFeed.${entry.kind}`, {
                player: nameOf(entry.playerId),
                other: nameOf(entry.otherPlayerId),
              })}
            </span>
            {entry.side ? (
              <span className="text-caption text-fg-muted">({teamNames[entry.side]})</span>
            ) : null}
          </li>
        ))}
      </ul>
    </section>
  );
}
