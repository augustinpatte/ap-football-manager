import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge, Button, Checkbox, Select } from "../ui";
import { resolveBackendKeyError } from "../../utils/backendI18n";
import {
  advanceInteractiveMatch,
  finishInteractiveMatch,
  getInteractiveMatchState,
  setInteractiveAssistant,
  setInteractivePause,
  startInteractiveMatch,
  submitMatchCommand,
  type InteractiveSnapshot,
  type MatchKeyEvent,
  type OrderSnapshot,
  type TacticalChange,
  type UserOrder,
} from "../../services/interactiveMatchService";
import { eventLine, matchMinute, playerNames, rejectionMessage } from "./interactiveMatch.helpers";
import {
  EventFeed,
  Field,
  SubstitutionPanel,
  TacticsPanel,
  TeamLineup,
} from "./InteractiveMatchPanels";

/** Pause between two slices of play while the match is running by itself (screen time only). */
const SLICE_PAUSE_MS = 350;
/** Simulated minutes per slice at each speed; the match is the same at any of them. */
const MINUTES_PER_SLICE = { slow: 1, normal: 3, fast: 6 } as const;
type Speed = keyof typeof MINUTES_PER_SLICE;

interface InteractiveMatchScreenProps<TFinish> {
  homeName: string;
  awayName: string;
  /** Receives what the career's own finalisation returned once the result has been applied. */
  onFinished: (response: TFinish) => void;
}

/**
 * Manage a real match. This screen shows what the match engine reports and sends the manager's
 * orders back; it holds no football state of its own and predicts nothing. The clock, the
 * score, the lineups and the conditions on it are the engine's, as of the last answer.
 */
export default function InteractiveMatchScreen<TFinish>({
  homeName,
  awayName,
  onFinished,
}: InteractiveMatchScreenProps<TFinish>) {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<InteractiveSnapshot | null>(null);
  const [events, setEvents] = useState<MatchKeyEvent[]>([]);
  const [running, setRunning] = useState(false);
  const [speed, setSpeed] = useState<Speed>("normal");
  const [notice, setNotice] = useState<string | null>(null);
  const busy = useRef(false);
  const cursor = useRef(0);
  const orderCount = useRef(0);

  const failure = useCallback(
    (error: unknown) =>
      setNotice(resolveBackendKeyError(error, t("match.interactive.matchFailed"))),
    [t],
  );

  const take = useCallback((next: InteractiveSnapshot) => {
    cursor.current = next.next_event_cursor;
    setSnapshot(next);
    if (next.events.length > 0) setEvents((old) => [...old, ...next.events]);
  }, []);

  /** Runs one request at a time; a second click while one is in flight is ignored. */
  const exclusive = useCallback(
    async <T,>(call: () => Promise<T>): Promise<T | null> => {
      if (busy.current) return null;
      busy.current = true;
      try {
        return await call();
      } catch (error) {
        failure(error);
        setRunning(false);
        // Whatever went wrong, show the match as the engine has it now, not as it was.
        await getInteractiveMatchState(cursor.current)
          .then(take)
          .catch(() => undefined);
        return null;
      } finally {
        busy.current = false;
      }
    },
    [failure, take],
  );

  useEffect(() => {
    void exclusive(startInteractiveMatch).then((first) => first && take(first));
  }, [exclusive, take]);

  const view = snapshot?.view;
  const session = view?.session;

  // While running, ask for the next slice of play; stop at the break so orders can be given.
  useEffect(() => {
    if (!running || !session || session.finished) return;
    const timer = window.setTimeout(() => {
      void exclusive(() =>
        advanceInteractiveMatch(
          { kind: "for_minutes", value: MINUTES_PER_SLICE[speed] },
          cursor.current,
        ),
      ).then((result) => {
        if (!result) return;
        take(result.snapshot);
        if (["period_break", "finished", "paused"].includes(result.stop)) setRunning(false);
      });
    }, SLICE_PAUSE_MS);
    return () => window.clearTimeout(timer);
  }, [running, session, speed, exclusive, take]);

  const togglePlay = useCallback(async () => {
    if (!session) return;
    if (running) {
      setRunning(false);
      const paused = await exclusive(() => setInteractivePause(true, cursor.current));
      if (paused) take(paused);
      return;
    }
    if (session.paused) {
      const resumed = await exclusive(() => setInteractivePause(false, cursor.current));
      if (!resumed) return;
      take(resumed);
    }
    setNotice(null);
    setRunning(true);
  }, [session, running, exclusive, take]);

  const playToEnd = useCallback(async () => {
    setRunning(false);
    const result = await exclusive(() =>
      advanceInteractiveMatch({ kind: "to_end" }, cursor.current),
    );
    if (result) take(result.snapshot);
  }, [exclusive, take]);

  const giveOrder = useCallback(
    async (order: UserOrder) => {
      if (!session) return;
      orderCount.current += 1;
      const id = `${session.session_id}:${orderCount.current}`;
      const answer: OrderSnapshot | null = await exclusive(() =>
        submitMatchCommand(id, session.revision, order, cursor.current),
      );
      if (!answer) return;
      take(answer.snapshot);
      setNotice(
        answer.receipt.accepted
          ? null
          : t("match.interactive.orderRefused", {
              reason: rejectionMessage(answer.receipt.rejection ?? "other", t),
            }),
      );
    },
    [session, exclusive, take, t],
  );

  const toggleAssistant = useCallback(
    async (active: boolean) => {
      const next = await exclusive(() => setInteractiveAssistant(active, cursor.current));
      if (next) take(next);
    },
    [exclusive, take],
  );

  const finish = useCallback(async () => {
    const response = await exclusive(() => finishInteractiveMatch<TFinish>());
    if (response) onFinished(response);
  }, [exclusive, onFinished]);

  const names = useMemo(() => (view ? playerNames(view) : {}), [view]);
  const lines = useMemo(
    () =>
      view
        ? events
            .map((e) =>
              eventLine(e, {
                names,
                teamNames: { home: homeName, away: awayName },
                regulationMinutes: view.regulation_minutes,
                t,
              }),
            )
            .filter((line): line is string => line !== null)
        : [],
    [events, view, names, homeName, awayName, t],
  );

  if (!view || !session) {
    return (
      <main className="min-h-screen bg-page p-6">
        {notice ? (
          <p role="alert" className="text-body text-danger">
            {notice}
          </p>
        ) : (
          <p className="text-body text-fg-muted">{t("dashboard.loading")}</p>
        )}
      </main>
    );
  }

  const mine = view.user_side === "away" ? session.away : session.home;
  const myPlan = view.user_side === "away" ? view.away_plan : view.home_plan;
  const controllable = view.user_side !== null && !session.finished;
  const phaseLabel = session.finished
    ? t("match.fullTime")
    : session.phase === "half_time"
      ? t("match.halfTime")
      : session.paused
        ? t("match.paused")
        : t("match.live");
  const playLabel = running
    ? t("match.pause")
    : session.paused
      ? t("match.interactive.resume")
      : t("match.interactive.play");

  return (
    <main className="min-h-screen bg-page text-fg">
      <header className="bg-chrome px-6 py-4 text-on-brand">
        <h1 className="sr-only">{t("match.interactive.heading")}</h1>
        <div className="flex items-center justify-center gap-6 font-heading text-title uppercase tracking-label">
          <span>{homeName}</span>
          <span aria-live="polite">
            {session.score_home} – {session.score_away}
          </span>
          <span>{awayName}</span>
        </div>
        <p className="mt-1 text-center text-body" aria-live="polite">
          {matchMinute(session.clock_ms)}&apos; <Badge variant="neutral">{phaseLabel}</Badge>
        </p>
      </header>

      <section className="flex flex-wrap items-center gap-3 border-b border-line bg-sunken px-6 py-3">
        <Button size="sm" onClick={() => void togglePlay()} disabled={session.finished}>
          {playLabel}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void playToEnd()}
          disabled={session.finished}
        >
          {t("match.interactive.toEnd")}
        </Button>
        <Field label={t("match.simSpeed")}>
          {(id) => (
            <Select
              id={id}
              selectSize="sm"
              value={speed}
              onChange={(e) => setSpeed(e.target.value as Speed)}
            >
              <option value="slow">{t("match.slow")}</option>
              <option value="normal">{t("match.normal")}</option>
              <option value="fast">{t("match.fast")}</option>
            </Select>
          )}
        </Field>
        {view.user_side !== null && (
          <div className="flex items-center gap-2 text-body text-fg">
            <Checkbox
              id="interactive-assistant"
              checked={view.assistant_active}
              disabled={!view.assistant_available && !view.assistant_active}
              onChange={(e) => void toggleAssistant(e.target.checked)}
              aria-describedby="assistant-hint"
            />
            <label htmlFor="interactive-assistant">{t("match.interactive.assistant")}</label>
          </div>
        )}
        {view.user_side !== null && (
          <span id="assistant-hint" className="text-caption text-fg-muted">
            {view.assistant_available
              ? t("match.interactive.assistantHint")
              : t("match.interactive.assistantUnavailable")}
          </span>
        )}
        <div className="ml-auto">
          <Button size="sm" onClick={() => void finish()} disabled={!session.finished}>
            {t("match.interactive.finish")}
          </Button>
        </div>
      </section>

      {session.finished && (
        <p className="bg-positive-tint px-6 py-2 text-body text-positive">
          {t("match.interactive.fullTimeHint")}
        </p>
      )}
      {notice && (
        <p role="alert" className="bg-danger-tint px-6 py-2 text-body text-danger">
          {notice}
        </p>
      )}

      <div className="grid gap-4 p-6 lg:grid-cols-2">
        <TeamLineup name={homeName} team={session.home} />
        <TeamLineup name={awayName} team={session.away} />
        {controllable && (
          <TacticsPanel
            plan={myPlan}
            supportedFormations={view.supported_formations}
            disabled={false}
            onApply={(change: TacticalChange) => void giveOrder({ Tactics: change })}
          />
        )}
        {controllable && (
          <SubstitutionPanel
            team={mine}
            names={names}
            disabled={false}
            onSubstitute={(off, on) =>
              void giveOrder({ Substitute: { player_off_id: off, player_on_id: on } })
            }
            onCancel={(off) => void giveOrder({ CancelSubstitution: { player_off_id: off } })}
          />
        )}
        <div className="lg:col-span-2">
          <EventFeed lines={lines} />
        </div>
      </div>
    </main>
  );
}
