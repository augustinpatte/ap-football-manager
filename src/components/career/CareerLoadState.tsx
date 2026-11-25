import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { CareerHydration } from "../../hooks/useCareerHydration";

interface CareerLoadStateProps {
  status: Extract<CareerHydration, "loading" | "failed" | "none">;
  onRetry: () => void;
  onBackToMenu: () => void;
}

/**
 * What a career screen shows while it has no game to draw: a spinner while the backend is asked,
 * and an alert with a way forward if the question itself failed. It never redirects by itself, so a
 * slow or failed lookup cannot throw the player out of a career that is still there.
 */
export function CareerLoadState({ status, onRetry, onBackToMenu }: CareerLoadStateProps) {
  const { t } = useTranslation();
  const buttonClass =
    "rounded-lg border border-line px-4 py-2 text-body font-semibold text-fg hover:bg-hover focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page";

  if (status === "failed") {
    return (
      <div className="flex min-h-screen items-center justify-center bg-page p-6">
        <div
          role="alert"
          className="max-w-md rounded-xl border border-line bg-panel p-6 text-fg shadow-sm"
        >
          <h1 className="font-heading text-title font-bold">{t("career.loadFailed.title")}</h1>
          <p className="mt-2 text-body text-fg-muted">{t("career.loadFailed.body")}</p>
          <div className="mt-4 flex gap-3">
            <button type="button" className={buttonClass} onClick={onRetry}>
              {t("career.loadFailed.retry")}
            </button>
            <button type="button" className={buttonClass} onClick={onBackToMenu}>
              {t("career.loadFailed.backToMenu")}
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-page">
      <div role="status" className="flex items-center gap-3 text-body text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin motion-reduce:animate-none" aria-hidden="true" />
        <span>{t("career.loading")}</span>
      </div>
    </div>
  );
}
