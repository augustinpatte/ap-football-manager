import { useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  FormationShape,
  MatchPlayerView,
  MatchTeamView,
  UserOrder,
} from "../../services/interactiveMatchService";
import { Badge, Button, Card, CardBody, CardHeader, Select } from "../ui";
import { FormationPitch } from "./FormationPitch";
import {
  DepartedList,
  PlayerDetail,
  QueuedSubstitutions,
  usePlayerStatus,
} from "./FormationPanelParts";
import { translatePositionAbbreviation } from "./../squad/SquadTab.helpers";
import { placeOnPitch, substitutionAllowance, toPitchPlayer } from "./formationPanel.helpers";

interface FormationSubstitutionsPanelProps {
  /** The manager's own team, as the engine has it now. */
  team: MatchTeamView;
  shapes: FormationShape[];
  supportedFormations: string[];
  /** False while the picture is behind the engine or the match is over: orders are not accepted. */
  canOrder: boolean;
  onOrder: (order: UserOrder) => void;
  onBackToLive: () => void;
}

const surname = (name: string) => name.split(" ").pop() ?? name;

/**
 * Manage the match on the tactical pitch: pick who goes off on the pitch, who comes on from the
 * bench, and send the order. The panel holds only the manager's selection; the lineup, the queue
 * and the allowance are the engine's own view, and nothing is changed here until the engine has
 * accepted an order and the next view arrives.
 */
export default function FormationSubstitutionsPanel({
  team,
  shapes,
  supportedFormations,
  canOrder,
  onOrder,
  onBackToLive,
}: FormationSubstitutionsPanelProps) {
  const { t } = useTranslation();
  const headingRef = useRef<HTMLSpanElement>(null);
  const statusOf = usePlayerStatus();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [incomingId, setIncomingId] = useState<string | null>(null);
  const [formationDraft, setFormationDraft] = useState<string | null>(null);

  const placed = useMemo(() => placeOnPitch(team, shapes), [team, shapes]);
  const pitchPlayers = useMemo(() => placed.map(({ player }) => toPitchPlayer(player)), [placed]);
  const slots = useMemo(
    () =>
      placed.map(({ player, x, y }, i) => ({
        player: pitchPlayers[i],
        x,
        y,
        slotPosition: player.role ?? undefined,
      })),
    [placed, pitchPlayers],
  );
  const playersById = useMemo(
    () => new Map([...team.on_pitch, ...team.bench].map((p) => [p.id, p])),
    [team],
  );
  const nameOf = (id: string) => playersById.get(id)?.name ?? t("match.replayUnknownPlayer");

  const allowance = substitutionAllowance(team);
  const queuedOff = new Set(team.pending_substitutions.map((p) => p.off));
  const queuedOn = new Set(team.pending_substitutions.map((p) => p.on));
  // A selection of a player the engine has since taken off, or queued, no longer stands.
  const selected = selectedId ? (team.on_pitch.find((p) => p.id === selectedId) ?? null) : null;
  const selectedQueued = selected ? queuedOff.has(selected.id) : false;
  const incoming =
    incomingId && !queuedOn.has(incomingId)
      ? (team.bench.find((p) => p.id === incomingId) ?? null)
      : null;
  const canChoose = canOrder && selected !== null && !selectedQueued && allowance.free > 0;
  const canConfirm = canChoose && incoming !== null;

  const select = (id: string) => {
    setSelectedId(id === selectedId ? null : id);
    setIncomingId(null);
  };
  const clear = () => {
    setSelectedId(null);
    setIncomingId(null);
  };
  const clearAndKeepFocus = () => {
    // The button disables itself when the selection goes: focus moves first, never onto a disabled control.
    headingRef.current?.focus();
    clear();
  };
  const confirm = () => {
    if (!selected || !incoming) return;
    headingRef.current?.focus();
    onOrder({ Substitute: { player_off_id: selected.id, player_on_id: incoming.id } });
    clear();
  };
  const cancelQueued = (offId: string) => {
    headingRef.current?.focus();
    onOrder({ CancelSubstitution: { player_off_id: offId } });
  };
  const chosenFormation = formationDraft ?? team.formation;
  const changeFormation = () => {
    headingRef.current?.focus();
    onOrder({ Tactics: { formation: chosenFormation } });
    setFormationDraft(null);
  };

  return (
    <Card>
      <CardHeader
        action={
          <Button size="sm" variant="outline" onClick={onBackToLive}>
            {t("match.formationPanel.backToLive")}
          </Button>
        }
      >
        {/* The card header is the heading; this takes focus after an order, so a screen reader reads
            on from the panel's title instead of from the top of the page. */}
        <span ref={headingRef} tabIndex={-1} className="focus:outline-none">
          {t("match.formationPanel.title")}
        </span>
      </CardHeader>
      <CardBody className="@container">
        <div className="grid gap-4 @min-[44rem]:grid-cols-[minmax(0,26rem)_minmax(0,1fr)]">
          <section aria-label={t("match.formationPanel.pitchLabel", { formation: team.formation })}>
            <FormationPitch
              formation={team.formation}
              players={pitchPlayers}
              slots={slots}
              selectedId={selected?.id ?? null}
              onPlayerClick={select}
              className="mx-auto aspect-[2/3] w-full max-w-[26rem]"
              renderToken={(pitchPlayer, { isSelected, descriptionId }) => {
                const player = playersById.get(pitchPlayer.id);
                if (!player) return null;
                return (
                  <PlayerToken
                    player={player}
                    isSelected={isSelected}
                    queued={queuedOff.has(player.id)}
                    status={statusOf(team, player)}
                    descriptionId={descriptionId}
                  />
                );
              }}
            />
          </section>

          <div className="flex min-w-0 flex-col gap-4">
            <FormationControl
              current={team.formation}
              value={chosenFormation}
              formations={supportedFormations}
              disabled={!canOrder}
              onChange={setFormationDraft}
              onApply={changeFormation}
              changed={chosenFormation !== team.formation}
            />
            <SelectionBlock
              team={team}
              selected={selected}
              selectedQueued={selectedQueued}
              exhausted={allowance.free === 0}
              nameOf={nameOf}
            />

            <BenchList
              bench={team.bench}
              queuedOn={queuedOn}
              incomingId={incoming?.id ?? null}
              disabled={!canChoose}
              onPick={(id) => setIncomingId(id === incomingId ? null : id)}
            />
            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" disabled={!canConfirm} onClick={confirm}>
                {canConfirm && selected && incoming
                  ? t("match.formationPanel.confirmNamed", {
                      off: selected.name,
                      on: incoming.name,
                    })
                  : t("match.confirmSubstitution")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={selected === null}
                onClick={clearAndKeepFocus}
              >
                {t("match.formationPanel.clearSelection")}
              </Button>
            </div>
            <p className="text-caption text-fg-muted">
              {t("match.formationPanel.allowance", {
                used: allowance.used,
                max: allowance.max,
                queued: allowance.queued,
                windows: team.windows_remaining,
              })}
            </p>
            <QueuedSubstitutions
              pending={team.pending_substitutions}
              nameOf={nameOf}
              disabled={!canOrder}
              onCancel={cancelQueued}
            />
            <DepartedList team={team} />
          </div>
        </div>
      </CardBody>
    </Card>
  );
}

interface SelectionBlockProps {
  team: MatchTeamView;
  selected: MatchPlayerView | null;
  selectedQueued: boolean;
  exhausted: boolean;
  nameOf: (id: string) => string;
}

/** The chosen player's details and what to do next. The hint sits in one live region that is always there, so a change of its text is announced. */
function SelectionBlock({
  team,
  selected,
  selectedQueued,
  exhausted,
  nameOf,
}: SelectionBlockProps) {
  const { t } = useTranslation();
  const statusOf = usePlayerStatus();
  let hint = t("match.formationPanel.hint.selectOff");
  if (selected) {
    if (selectedQueued) hint = t("match.formationPanel.hint.queued");
    else if (exhausted) hint = t("match.formationPanel.hint.exhausted");
    else hint = t("match.formationPanel.hint.chooseReplacement", { name: selected.name });
  }
  const replacement = selected
    ? team.pending_substitutions.find((p) => p.off === selected.id)
    : undefined;
  return (
    <section aria-labelledby="formation-panel-selection" className="flex flex-col gap-2">
      <h3
        id="formation-panel-selection"
        className="text-caption font-bold uppercase tracking-label text-fg-muted"
      >
        {t("match.formationPanel.selection")}
      </h3>
      {selected ? (
        <PlayerDetail
          player={selected}
          status={statusOf(team, selected)}
          replacementName={replacement ? nameOf(replacement.on) : undefined}
        />
      ) : null}
      <p role="status" className="text-body text-fg-muted">
        {hint}
      </p>
    </section>
  );
}

interface PlayerTokenProps {
  player: MatchPlayerView;
  isSelected: boolean;
  queued: boolean;
  status: string | null;
  descriptionId: string;
}

/**
 * A player on the pitch: shirt number, surname and condition in a footprint small enough that two
 * neighbouring slots never overlap. Everything else is in the description a screen reader reads.
 */
function PlayerToken({ player, isSelected, queued, status, descriptionId }: PlayerTokenProps) {
  const { t } = useTranslation();
  const condition = Math.round(player.condition * 100);
  const position = translatePositionAbbreviation(t, player.role ?? player.natural_position);
  const description = [position, t("match.formationPanel.token.condition", { condition }), status]
    .filter(Boolean)
    .join(", ");
  return (
    <span className="flex w-[4.5rem] flex-col items-center text-center">
      <span
        className={`relative flex size-8 items-center justify-center rounded-full border-2 font-heading text-body font-bold ${
          isSelected
            ? "border-focus bg-panel text-fg ring-4 ring-on-brand"
            : "border-on-brand/60 bg-chrome/80 text-on-brand"
        }`}
      >
        {player.shirt_number > 0 ? player.shirt_number : ""}
        {player.injured && (
          <span
            aria-hidden="true"
            className="absolute -right-1.5 -top-1.5 flex size-4 items-center justify-center rounded-full border-2 border-danger bg-panel text-caption font-bold leading-none text-danger"
          >
            +
          </span>
        )}
        {queued && (
          <span
            aria-hidden="true"
            className="absolute -bottom-1.5 -right-1.5 flex size-4 items-center justify-center rounded-full border-2 border-warning bg-panel text-caption font-bold leading-none text-warning"
          >
            ▼
          </span>
        )}
      </span>
      <span
        className={`max-w-full truncate rounded bg-chrome/75 px-0.5 text-caption font-bold text-on-brand ${isSelected ? "underline underline-offset-2" : ""}`}
      >
        {surname(player.name)}
      </span>
      <span
        aria-hidden="true"
        className="mt-0.5 rounded bg-chrome/75 px-0.5 text-caption leading-none text-on-brand"
      >
        {position} {condition}
      </span>
      <span id={descriptionId} className="sr-only">
        {description}
      </span>
    </span>
  );
}

interface BenchListProps {
  bench: MatchPlayerView[];
  queuedOn: Set<string>;
  incomingId: string | null;
  disabled: boolean;
  onPick: (id: string) => void;
}

/** The substitutes: pickable once a player to take off is chosen; a queued one is shown as taken. */
function BenchList({ bench, queuedOn, incomingId, disabled, onPick }: BenchListProps) {
  const { t } = useTranslation();
  const headingId = useId();
  return (
    <section aria-labelledby={headingId}>
      <h3 id={headingId} className="text-caption font-bold uppercase tracking-label text-fg-muted">
        {t("match.formationPanel.bench")}
      </h3>
      {bench.length === 0 ? (
        <p className="mt-1 text-body text-fg-muted">{t("match.formationPanel.benchEmpty")}</p>
      ) : (
        <ul className="mt-1 grid gap-1">
          {bench.map((player) => {
            const taken = queuedOn.has(player.id);
            const chosen = incomingId === player.id;
            return (
              <li key={player.id}>
                <button
                  type="button"
                  aria-pressed={chosen}
                  disabled={disabled || taken}
                  onClick={() => onPick(player.id)}
                  className={`flex h-row w-full items-center gap-2 rounded border px-2 text-left text-body focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page disabled:opacity-50 ${
                    chosen
                      ? "border-focus bg-hover font-bold text-fg ring-2 ring-focus"
                      : "border-line bg-panel text-fg"
                  }`}
                >
                  <span className="w-6 shrink-0 text-caption text-fg-muted">
                    {chosen ? <span aria-hidden="true">✓</span> : null}
                    {chosen || player.shirt_number <= 0 ? "" : player.shirt_number}
                  </span>
                  <span className="min-w-0 flex-1 truncate">{player.name}</span>
                  <span className="shrink-0 text-caption text-fg-muted">
                    {translatePositionAbbreviation(t, player.natural_position)}{" "}
                    {Math.round(player.condition * 100)}
                  </span>
                  {player.injured && (
                    <Badge variant="danger">
                      <span className="sr-only">{t("common.injured")}</span>
                      <span aria-hidden="true">+</span>
                    </Badge>
                  )}
                  {taken && (
                    <span className="shrink-0 text-caption text-fg-muted">
                      {t("match.formationPanel.status.comingOn")}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

interface FormationControlProps {
  current: string;
  value: string;
  formations: string[];
  disabled: boolean;
  changed: boolean;
  onChange: (formation: string) => void;
  onApply: () => void;
}

/** The formation in force and the supported ones to move to, through the ordinary tactics order. */
function FormationControl({
  current,
  value,
  formations,
  disabled,
  changed,
  onChange,
  onApply,
}: FormationControlProps) {
  const { t } = useTranslation();
  const selectId = useId();
  return (
    <section className="flex flex-wrap items-end gap-2">
      <div className="flex flex-col gap-1">
        <label htmlFor={selectId} className="text-caption text-fg-muted">
          {t("match.formationPanel.currentFormation", { formation: current })}
        </label>
        <Select
          id={selectId}
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value)}
        >
          {[...new Set([current, ...formations])].map((formation) => (
            <option key={formation} value={formation} disabled={!formations.includes(formation)}>
              {formation}
            </option>
          ))}
        </Select>
      </div>
      <Button size="sm" disabled={disabled || !changed} onClick={onApply}>
        {t("match.formationPanel.changeFormation")}
      </Button>
    </section>
  );
}
