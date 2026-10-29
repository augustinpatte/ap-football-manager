import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import type { OfferResponse } from "../../services/staffService";
import { resolveBackendError } from "../../utils/backendI18n";
import { formatVal, formatWeeklyWage } from "../../lib/helpers";
import { Button, Dialog } from "../ui";

interface OfferDialogProps {
  mode: "hire" | "renew";
  name: string;
  /** What they ask for, per week; the field starts here. */
  askingWage: number;
  maxYears: number;
  /** The signing fee in weeks of wage (hire only). */
  signingFeeWeeks: number;
  onSubmit: (wage: number, years: number) => Promise<OfferResponse>;
  onClose: () => void;
}

export default function OfferDialog({
  mode,
  name,
  askingWage,
  maxYears,
  signingFeeWeeks,
  onSubmit,
  onClose,
}: OfferDialogProps) {
  const { t } = useTranslation();
  const titleId = useId();
  const [wage, setWage] = useState(askingWage);
  const [years, setYears] = useState(Math.min(2, maxYears));
  const [busy, setBusy] = useState(false);
  const [response, setResponse] = useState<OfferResponse | null>(null);
  const [error, setError] = useState("");

  const submit = async (offered: number) => {
    setBusy(true);
    setError("");
    try {
      const answer = await onSubmit(offered, years);
      setResponse(answer);
      if (answer.status === "accepted") onClose();
    } catch (err) {
      setError(resolveBackendError(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog labelledBy={titleId} onClose={onClose} busy={busy} className="w-full max-w-md p-5">
      <h2 id={titleId} className="font-heading text-heading uppercase">
        {t(mode === "hire" ? "staff.offer.titleHire" : "staff.offer.titleRenew", { name })}
      </h2>
      <div className="mt-4 grid gap-3">
        <label className="grid gap-1 text-body">
          <span className="text-fg-muted">{t("staff.offer.wage")}</span>
          <input
            type="number"
            min={0}
            step={100}
            value={wage}
            onChange={(e) => setWage(Math.max(0, Number(e.target.value)))}
            className="rounded-lg border border-line bg-sunken px-3 py-2 text-fg"
          />
        </label>
        <label className="grid gap-1 text-body">
          <span className="text-fg-muted">{t("staff.offer.years")}</span>
          <input
            type="number"
            min={1}
            max={maxYears}
            value={years}
            onChange={(e) =>
              setYears(Math.min(maxYears, Math.max(1, Math.round(Number(e.target.value)))))
            }
            className="rounded-lg border border-line bg-sunken px-3 py-2 text-fg"
          />
        </label>
        {mode === "hire" ? (
          <p className="text-caption text-fg-muted">
            {t("staff.offer.signingFee", {
              amount: formatVal(wage * signingFeeWeeks),
            })}
          </p>
        ) : null}
        {response?.status === "refused" ? (
          <p role="alert" className="text-body text-danger">
            {t("staff.offer.refused", { name })}
          </p>
        ) : null}
        {response?.status === "counter" ? (
          <div role="status" className="grid gap-2 text-body text-warning">
            <p>
              {t("staff.offer.counter", { name, wage: formatWeeklyWage(response.wage, "compact") })}
            </p>
            <Button
              size="sm"
              disabled={busy}
              onClick={() => {
                setWage(response.wage);
                void submit(response.wage);
              }}
            >
              {t("staff.offer.acceptCounter", { wage: formatWeeklyWage(response.wage, "compact") })}
            </Button>
          </div>
        ) : null}
        {error ? (
          <p role="alert" className="text-body text-danger">
            {error}
          </p>
        ) : null}
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose} disabled={busy}>
          {t("common.cancel")}
        </Button>
        <Button onClick={() => void submit(wage)} disabled={busy || wage <= 0}>
          {t("staff.offer.submit")}
        </Button>
      </div>
    </Dialog>
  );
}
