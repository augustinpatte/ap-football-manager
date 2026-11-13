import { useTranslation } from "react-i18next";

import type { FeedEntry } from "./replayDerived";

interface ReplayEventFeedProps {
  entries: FeedEntry[];
  teamNames: { home: string; away: string };
  nameOf: (playerId: string | undefined) => string;
}

/** The match events a player reads, each in the language of the game and with real names. */
export default function ReplayEventFeed({ entries, teamNames, nameOf }: ReplayEventFeedProps) {
  const { t } = useTranslation();
  return (
    <section aria-label={t("match.replayFeedTitle")}>
      <h3 className="font-heading text-body font-bold uppercase tracking-label text-fg-muted">
        {t("match.replayFeedTitle")}
      </h3>
      <ul aria-live="polite" className="mt-2 space-y-1 text-body">
        {entries.map((entry, index) => (
          <li
            key={`${entry.t_ms}-${entry.kind}-${index}`}
            className="flex flex-wrap items-baseline gap-x-2 text-fg"
          >
            <span className="w-10 shrink-0 text-right font-heading text-fg-muted">
              {Math.floor(entry.t_ms / 60_000)}'
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
