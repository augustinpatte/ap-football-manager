import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import type { DelegationLimits, DelegationMode } from "../../services/staffService";
import { Badge, Button, Card, CardBody, SegmentedControl } from "../ui";

interface DelegationModeCardProps {
  mode: DelegationMode;
  limits: DelegationLimits;
  hasAssistant: boolean;
  busy: boolean;
  onModeChange: (mode: DelegationMode) => void;
  onLimitsSave: (limits: DelegationLimits) => void;
}

const MODES: DelegationMode[] = ["manual", "approval", "full"];

const LIMIT_FIELDS: { key: keyof DelegationLimits; step: number }[] = [
  { key: "max_staff_wage", step: 100 },
  { key: "max_player_wage", step: 500 },
  { key: "max_commitment", step: 100_000 },
  { key: "max_transfer_fee", step: 100_000 },
];

/** The whole-job switch (manual / approval / full) and the spending ceilings behind it. */
export default function DelegationModeCard({
  mode,
  limits,
  hasAssistant,
  busy,
  onModeChange,
  onLimitsSave,
}: DelegationModeCardProps) {
  const { t } = useTranslation();
  const labelId = useId();
  const [draft, setDraft] = useState<DelegationLimits>(limits);
  // The saved limits are the source of truth: a save or a reload replaces the draft.
  useEffect(() => setDraft(limits), [limits]);
  const dirty = LIMIT_FIELDS.some(({ key }) => draft[key] !== limits[key]);

  return (
    <Card>
      <CardBody>
        <h3 id={labelId} className="font-heading text-lead uppercase">
          {t("staff.delegation.mode.title")}
        </h3>
        <p className="mt-1 text-body text-fg-muted">
          {hasAssistant
            ? t(`staff.delegation.mode.${mode}Desc`)
            : t("staff.delegation.noAssistant")}
        </p>
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <SegmentedControl<DelegationMode>
            labelledBy={labelId}
            emphasis="strong"
            options={MODES.map((value) => ({
              value,
              label: t(`staff.delegation.mode.${value}`),
            }))}
            value={mode}
            onChange={(next) => {
              if (!busy && next !== mode) onModeChange(next);
            }}
          />
          {mode === "custom" ? (
            <Badge variant="neutral" size="sm">
              {t("staff.delegation.mode.custom")}
            </Badge>
          ) : null}
        </div>

        <h4 className="mt-4 text-body font-medium">{t("staff.delegation.limits.title")}</h4>
        <p className="text-caption text-fg-muted">{t("staff.delegation.limits.intro")}</p>
        <div className="mt-2 grid gap-3 sm:grid-cols-2">
          {LIMIT_FIELDS.map(({ key, step }) => (
            <label key={key} className="grid gap-1 text-body">
              <span className="text-fg-muted">{t(`staff.delegation.limits.${key}`)}</span>
              <input
                type="number"
                min={0}
                step={step}
                value={draft[key]}
                onChange={(event) =>
                  setDraft({ ...draft, [key]: Math.max(0, Math.round(Number(event.target.value))) })
                }
                className="rounded-lg border border-line bg-sunken px-3 py-2 text-fg"
              />
            </label>
          ))}
        </div>
        <p className="mt-1 text-caption text-fg-muted">
          {t("staff.delegation.limits.transferNote")}
        </p>
        <div className="mt-2">
          <Button size="sm" disabled={busy || !dirty} onClick={() => onLimitsSave(draft)}>
            {t("staff.delegation.limits.save")}
          </Button>
        </div>
      </CardBody>
    </Card>
  );
}
