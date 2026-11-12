import { ChevronLeft, ChevronRight } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Badge, Button, Card, CardBody } from "../ui";
import { formatVal } from "../../lib/valueFormatting";
import { getErrorMessage, resolveTranslatedErrorMessage } from "../../utils/errorMessage";
import { translatePositionAbbreviation } from "../squad/SquadTab.helpers";
import { pageRange, totalPages } from "./globalMarketModel";
import type { GlobalMarketController } from "./useGlobalMarket";
import type { RegistrySummary } from "../../services/registryService";

interface GlobalMarketResultsProps {
  controller: GlobalMarketController;
}

export default function GlobalMarketResults({ controller }: GlobalMarketResultsProps) {
  const { t } = useTranslation();
  const { page, loading, error, selectedId } = controller;

  if (error) {
    return (
      <Card>
        <CardBody className="space-y-3">
          <p role="alert" className="text-body text-danger">
            {resolveTranslatedErrorMessage(getErrorMessage(error), t)}
          </p>
          <Button onClick={controller.reload}>{t("globalMarket.retry")}</Button>
        </CardBody>
      </Card>
    );
  }

  if (loading && !page) {
    return (
      <Card>
        <CardBody>
          <p role="status" className="text-body text-fg-muted">
            {t("globalMarket.loading")}
          </p>
        </CardBody>
      </Card>
    );
  }

  if (!page || page.items.length === 0) {
    return (
      <Card>
        <CardBody>
          <p className="text-body text-fg-muted">{t("globalMarket.noResults")}</p>
        </CardBody>
      </Card>
    );
  }

  const pages = totalPages(page.total, page.pageSize);
  const range = pageRange(page.page, page.total, page.pageSize);

  return (
    <Card>
      <CardBody className="p-0">
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-line bg-sunken text-[11px]">
                <Th>{t("common.player")}</Th>
                <Th>{t("common.position")}</Th>
                <Th>{t("common.age")}</Th>
                <Th>{t("globalMarket.club")}</Th>
                <Th>{t("common.ovr")}</Th>
                <Th>{t("globalMarket.marketValue")}</Th>
                <Th>{t("common.wage")}</Th>
                <Th>{t("globalMarket.askingPrice")}</Th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {page.items.map((item) => (
                <ResultRow
                  key={item.id}
                  item={item}
                  selected={item.id === selectedId}
                  onSelect={() => controller.selectPlayer(item.id)}
                />
              ))}
            </tbody>
          </table>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line px-4 py-3">
          <p className="text-caption text-fg-muted">
            {t("globalMarket.range", { from: range.from, to: range.to, total: page.total })}
          </p>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              aria-label={t("globalMarket.previousPage")}
              disabled={page.page <= 1}
              onClick={() => controller.setFilter({ page: page.page - 1 })}
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <span className="text-caption font-heading font-bold text-fg-muted">
              {t("globalMarket.pageOf", { page: page.page, pages })}
            </span>
            <Button
              variant="ghost"
              size="sm"
              aria-label={t("globalMarket.nextPage")}
              disabled={page.page >= pages}
              onClick={() => controller.setFilter({ page: page.page + 1 })}
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </CardBody>
    </Card>
  );
}

function Th({ children }: { children: ReactNode }) {
  return (
    <th className="px-3 py-3 font-heading font-bold uppercase tracking-wider text-fg-muted">
      {children}
    </th>
  );
}

function ResultRow({
  item,
  selected,
  onSelect,
}: {
  item: RegistrySummary;
  selected: boolean;
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  return (
    <tr className={selected ? "bg-primary-50 dark:bg-primary-900/20" : undefined}>
      <td className="px-3 py-2">
        <button
          type="button"
          onClick={onSelect}
          aria-label={t("globalMarket.openProfile", { player: item.name })}
          className="text-left font-heading font-bold text-fg transition-colors hover:text-positive focus:outline-none focus-visible:ring-2 focus-visible:ring-focus"
        >
          {item.name}
        </button>
        <p className="text-[11px] text-fg-subtle">{item.nationality ?? "—"}</p>
      </td>
      <td className="px-3 py-2 text-caption text-fg-muted">
        <Badge variant="neutral">
          {item.position ? translatePositionAbbreviation(t, item.position) : "—"}
        </Badge>
      </td>
      <td className="px-3 py-2 text-caption tabular-nums text-fg-muted">{item.age ?? "—"}</td>
      <td className="px-3 py-2 text-caption text-fg-muted">
        {item.club ?? t("globalMarket.freeAgent")}
      </td>
      <td className="px-3 py-2 font-heading font-bold tabular-nums text-fg">{item.ovr}</td>
      <td className="px-3 py-2 text-caption tabular-nums text-fg-muted">
        {formatVal(item.marketValue)}
      </td>
      <td className="px-3 py-2 text-caption tabular-nums text-fg-muted">
        {formatVal(item.weeklyWage)}
      </td>
      <td className="px-3 py-2 text-caption tabular-nums text-fg-muted">
        {item.ownership === "free_agent"
          ? t("globalMarket.freeAgent")
          : formatVal(item.askingPrice)}
      </td>
    </tr>
  );
}
