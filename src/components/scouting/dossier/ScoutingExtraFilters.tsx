import { useId } from "react";
import { useTranslation } from "react-i18next";

import type { PlayersPageQuery, ScoutingFilter } from "../../../services/playersService";
import { Checkbox, Select } from "../../ui";

interface ScoutingExtraFiltersProps {
  query: PlayersPageQuery;
  onChange: (patch: Partial<PlayersPageQuery>) => void;
}

const SCOUTING_OPTIONS: ScoutingFilter[] = ["any", "unscouted", "scouted", "shortlisted"];

/** The scouting-specific filters beside the shared list filters: what the club knows about him. */
export default function ScoutingExtraFilters({ query, onChange }: ScoutingExtraFiltersProps) {
  const { t } = useTranslation();
  const statusId = useId();
  const potentialId = useId();
  const expiringId = useId();
  const potentialMin = query.potential_min ?? "";

  return (
    <div className="mb-3 flex flex-wrap items-end gap-4">
      <div className="flex flex-col gap-1">
        <label htmlFor={statusId} className="text-caption text-fg-muted">
          {t("scouting.filters.status")}
        </label>
        <Select
          id={statusId}
          selectSize="sm"
          value={query.scouting ?? "any"}
          onChange={(event) =>
            onChange({ scouting: event.target.value as ScoutingFilter, page: 1 })
          }
        >
          {SCOUTING_OPTIONS.map((option) => (
            <option key={option} value={option}>
              {t(`scouting.filters.${option}`)}
            </option>
          ))}
        </Select>
      </div>
      <div className="flex flex-col gap-1">
        <label htmlFor={potentialId} className="text-caption text-fg-muted">
          {t("scouting.filters.potentialMin")}
        </label>
        <input
          id={potentialId}
          type="number"
          min={1}
          max={99}
          inputMode="numeric"
          value={potentialMin}
          onChange={(event) => {
            const parsed = Number.parseInt(event.target.value, 10);
            onChange({
              potential_min: Number.isNaN(parsed) ? null : Math.min(99, Math.max(1, parsed)),
              page: 1,
            });
          }}
          aria-describedby={`${potentialId}-hint`}
          className="w-24 rounded-lg border border-line bg-panel px-2 py-1.5 text-body text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
        />
        <span id={`${potentialId}-hint`} className="text-caption text-fg-subtle">
          {t("scouting.filters.potentialHint")}
        </span>
      </div>
      <div className="flex items-center gap-2 pb-1.5">
        <Checkbox
          id={expiringId}
          checked={query.contract_expiring ?? false}
          onChange={(event) => onChange({ contract_expiring: event.target.checked, page: 1 })}
        />
        <label htmlFor={expiringId} className="text-body text-fg">
          {t("scouting.filters.contractExpiring")}
        </label>
      </div>
    </div>
  );
}
