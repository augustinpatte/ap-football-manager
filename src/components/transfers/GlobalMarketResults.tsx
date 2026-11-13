import { Check, ChevronLeft, ChevronRight } from "lucide-react";
import { type ReactNode, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import { Badge, Button, Card, CardBody } from "../ui";
import { formatVal } from "../../lib/valueFormatting";
import { translatePositionAbbreviation } from "../squad/SquadTab.helpers";
import { pageRange, totalPages } from "./globalMarketModel";
import type { GlobalMarketController } from "./useGlobalMarket";
import type { RegistrySummary } from "../../services/registryService";

interface GlobalMarketResultsProps {
  controller: GlobalMarketController;
}

export default function GlobalMarketResults({ controller }: GlobalMarketResultsProps) {
  const { t } = useTranslation();
  const { page, loading, error, selectedId, notice } = controller;
  const previousRef = useRef<HTMLButtonElement>(null);
  const nextRef = useRef<HTMLButtonElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const pendingPaginationFocus = useRef<"next" | "previous" | null>(null);

  const pages = page ? totalPages(page.total, page.pageSize) : 1;

  // A page control that disables itself must not take the keyboard's focus with it. When paging
  // reaches either end, the control in the other direction is the one that stays usable.
  useEffect(() => {
    const direction = pendingPaginationFocus.current;
    if (!direction || !page) return;
    pendingPaginationFocus.current = null;
    if (direction === "next" && page.page >= pages) previousRef.current?.focus();
    if (direction === "previous" && page.page <= 1) nextRef.current?.focus();
  }, [page, pages]);

  // Closing the profile returns the keyboard where it was, unless a signing moved focus on to the
  // confirmation. The row may have been redrawn, so the element is checked before it is used.
  useEffect(() => {
    if (notice) {
      returnFocusRef.current = null;
      return;
    }
    if (selectedId !== null) return;
    const target = returnFocusRef.current;
    returnFocusRef.current = null;
    if (target?.isConnected) target.focus();
  }, [selectedId, notice]);

  if (error) return null;

  if (loading && !page) {
    return (
      <Card>
        <CardBody>
          <p className="text-body text-fg-muted">{t("globalMarket.loading")}</p>
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

  const range = pageRange(page.page, page.total, page.pageSize);
  const pageLabel = t("globalMarket.tableCaption", { page: page.page, pages });

  return (
    <Card className="min-w-0">
      <CardBody className="p-0">
        <div
          role="region"
          aria-label={pageLabel}
          // biome-ignore lint/a11y/noNoninteractiveTabindex: a sideways scroller has to be focusable before the keyboard can scroll it; the region is named
          tabIndex={0}
          className="overflow-x-auto focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-panel"
        >
          <table className="w-full min-w-[48rem] border-collapse text-left">
            <caption className="sr-only">{pageLabel}</caption>
            <thead>
              <tr className="border-b border-line bg-sunken text-caption">
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
                  onSelect={(opener) => {
                    returnFocusRef.current = opener;
                    controller.selectPlayer(item.id);
                  }}
                />
              ))}
            </tbody>
          </table>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line px-4 py-3">
          <p
            role="status"
            aria-live="polite"
            aria-atomic="true"
            className="text-caption tabular-nums text-fg-muted"
          >
            {t("globalMarket.range", { from: range.from, to: range.to, total: page.total })}
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              ref={previousRef}
              variant="ghost"
              size="sm"
              aria-label={t("globalMarket.previousPage")}
              disabled={page.page <= 1}
              onClick={() => {
                if (page.page <= 1) return;
                pendingPaginationFocus.current = "previous";
                controller.setFilter({ page: page.page - 1 });
              }}
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <span className="text-caption font-heading font-bold text-fg-muted">
              {t("globalMarket.pageOf", { page: page.page, pages })}
            </span>
            <Button
              ref={nextRef}
              variant="ghost"
              size="sm"
              aria-label={t("globalMarket.nextPage")}
              disabled={page.page >= pages}
              onClick={() => {
                if (page.page >= pages) return;
                pendingPaginationFocus.current = "next";
                controller.setFilter({ page: page.page + 1 });
              }}
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
    <th
      scope="col"
      className="px-3 py-3 font-heading font-bold uppercase tracking-wider text-fg-muted"
    >
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
  onSelect: (opener: HTMLButtonElement) => void;
}) {
  const { t } = useTranslation();
  return (
    <tr
      aria-current={selected ? "true" : undefined}
      className={selected ? "bg-primary-50 dark:bg-primary-900/20" : undefined}
    >
      <td
        className={`border-l-4 px-3 py-2 align-top ${
          selected ? "border-primary-500" : "border-transparent"
        }`}
      >
        <div className="flex items-start gap-1.5">
          {selected ? (
            <Check
              aria-hidden="true"
              className="mt-0.5 h-4 w-4 shrink-0 text-primary-600 dark:text-primary-300"
            />
          ) : null}
          <button
            type="button"
            onClick={(event) => onSelect(event.currentTarget)}
            aria-label={t("globalMarket.openProfile", { player: item.name })}
            className="break-words text-left font-heading font-bold text-fg transition-colors hover:text-positive focus:outline-none focus-visible:ring-2 focus-visible:ring-focus"
          >
            {item.name}
          </button>
        </div>
        <p className="text-caption text-fg-subtle">
          {item.nationality ?? "—"}
          {selected ? <span className="sr-only"> · {t("globalMarket.selected")}</span> : null}
        </p>
      </td>
      <td className="px-3 py-2 align-top text-caption text-fg-muted">
        <Badge variant="neutral">
          {item.position ? translatePositionAbbreviation(t, item.position) : "—"}
        </Badge>
      </td>
      <td className="px-3 py-2 align-top text-caption tabular-nums text-fg-muted">
        {item.age ?? "—"}
      </td>
      <td className="px-3 py-2 align-top text-caption text-fg-muted">
        {item.club ?? t("globalMarket.freeAgent")}
      </td>
      <td className="px-3 py-2 align-top font-heading font-bold tabular-nums text-fg">
        {item.ovr}
      </td>
      <td className="px-3 py-2 align-top text-caption tabular-nums text-fg-muted">
        {formatVal(item.marketValue)}
      </td>
      <td className="px-3 py-2 align-top text-caption tabular-nums text-fg-muted">
        {formatVal(item.weeklyWage)}
      </td>
      <td className="px-3 py-2 align-top text-caption tabular-nums text-fg-muted">
        {item.ownership === "free_agent"
          ? t("globalMarket.freeAgent")
          : formatVal(item.askingPrice)}
      </td>
    </tr>
  );
}
