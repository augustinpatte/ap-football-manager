import { ArrowDown, ArrowUp, Search } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button, Card, CardBody } from "../ui";
import { DEFAULT_MARKET_QUERY, type GlobalMarketQuery } from "./globalMarketModel";
import type { RegistrySearchRequest } from "../../services/registryService";

const FIELD_CLASS =
  "w-full rounded-lg border border-line bg-panel px-3 py-2 text-body text-fg placeholder:text-fg-subtle focus:outline-none focus-visible:ring-2 focus-visible:ring-focus";

const SORT_KEYS: RegistrySearchRequest["sort"][] = ["ovr", "value", "age", "name", "club"];
const POSITIONS: RegistrySearchRequest["position"][] = [
  "any",
  "goalkeeper",
  "defender",
  "midfielder",
  "forward",
];
const AVAILABILITY: RegistrySearchRequest["availability"][] = ["all", "free_agent", "contracted"];

interface GlobalMarketFiltersProps {
  query: GlobalMarketQuery;
  onChange: (patch: Partial<GlobalMarketQuery>) => void;
  onSearch: () => void;
  resultCount: number;
}

export default function GlobalMarketFilters({
  query,
  onChange,
  onSearch,
  resultCount,
}: GlobalMarketFiltersProps) {
  const { t } = useTranslation();

  return (
    <Card>
      <CardBody>
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          <label className="block text-caption text-fg-muted xl:col-span-2">
            <span className="mb-1 block">{t("globalMarket.searchLabel")}</span>
            <input
              type="search"
              className={FIELD_CLASS}
              value={query.text}
              placeholder={t("globalMarket.searchPlaceholder")}
              onChange={(event) => onChange({ text: event.target.value })}
            />
          </label>
          <label className="block text-caption text-fg-muted">
            <span className="mb-1 block">{t("globalMarket.nationLabel")}</span>
            <input
              type="text"
              maxLength={3}
              className={FIELD_CLASS}
              value={query.nation}
              placeholder={t("globalMarket.nationPlaceholder")}
              onChange={(event) => onChange({ nation: event.target.value.toUpperCase() })}
            />
          </label>
          <label className="block text-caption text-fg-muted">
            <span className="mb-1 block">{t("globalMarket.club")}</span>
            <input
              type="text"
              className={FIELD_CLASS}
              value={query.club}
              placeholder={t("globalMarket.clubPlaceholder")}
              onChange={(event) => onChange({ club: event.target.value })}
            />
          </label>
          <SelectField
            label={t("common.position")}
            value={query.position}
            onChange={(value) => onChange({ position: value as GlobalMarketQuery["position"] })}
            options={POSITIONS.map((value) => ({
              value,
              label: t(`globalMarket.position.${value}`),
            }))}
          />
          <SelectField
            label={t("globalMarket.availabilityLabel")}
            value={query.availability}
            onChange={(value) =>
              onChange({ availability: value as GlobalMarketQuery["availability"] })
            }
            options={AVAILABILITY.map((value) => ({
              value,
              label: t(`globalMarket.availability.${value}`),
            }))}
          />
          <label className="block text-caption text-fg-muted">
            <span className="mb-1 block">{t("globalMarket.minAge")}</span>
            <input
              type="number"
              min={0}
              max={99}
              className={FIELD_CLASS}
              value={query.minAge}
              onChange={(event) => onChange({ minAge: event.target.value })}
            />
          </label>
          <label className="block text-caption text-fg-muted">
            <span className="mb-1 block">{t("globalMarket.maxAge")}</span>
            <input
              type="number"
              min={0}
              max={99}
              className={FIELD_CLASS}
              value={query.maxAge}
              onChange={(event) => onChange({ maxAge: event.target.value })}
            />
          </label>
          <SelectField
            label={t("globalMarket.sortBy")}
            value={query.sort}
            onChange={(value) => onChange({ sort: value as GlobalMarketQuery["sort"] })}
            options={SORT_KEYS.map((value) => ({ value, label: t(`globalMarket.sort.${value}`) }))}
          />
          <div className="flex items-end gap-2">
            <Button
              variant="outline"
              className="flex-1"
              onClick={() => onChange({ descending: !query.descending })}
              icon={query.descending ? <ArrowDown /> : <ArrowUp />}
            >
              {query.descending ? t("globalMarket.descending") : t("globalMarket.ascending")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => onChange({ ...DEFAULT_MARKET_QUERY })}
              title={t("globalMarket.reset")}
            >
              {t("globalMarket.reset")}
            </Button>
          </div>
        </div>
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
          <p className="text-caption text-fg-muted">
            {t("globalMarket.results", { count: resultCount })}
          </p>
          <Button onClick={onSearch} icon={<Search />}>
            {t("globalMarket.search")}
          </Button>
        </div>
      </CardBody>
    </Card>
  );
}

function SelectField({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
}) {
  return (
    <label className="block text-caption text-fg-muted">
      <span className="mb-1 block">{label}</span>
      <select
        className={FIELD_CLASS}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}
