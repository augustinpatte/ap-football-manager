import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { getScoutingDossier, type ScoutingDossierView } from "../../../services/scoutingService";
import { getErrorMessage, resolveTranslatedErrorMessage } from "../../../utils/errorMessage";

interface DossierState {
  dossier: ScoutingDossierView | null;
  loading: boolean;
  error: string | null;
  refresh: () => void;
}

/**
 * The club's reports and shortlist. Asks again whenever `refreshKey` changes (a day passing, a
 * shortlist edit or a new report all change the game the screen shows), and never shows a stale
 * answer to a request that was overtaken.
 */
export function useScoutingDossier(refreshKey: unknown): DossierState {
  const { t } = useTranslation();
  // The effect must not re-run when the translator's identity changes, only when the data may have.
  const translate = useRef(t);
  translate.current = t;
  const [dossier, setDossier] = useState<ScoutingDossierView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const refresh = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    let current = true;
    setLoading(true);
    void (async () => {
      try {
        const view = await getScoutingDossier();
        if (!current) return;
        setDossier(view ?? null);
        setError(null);
      } catch (err) {
        if (current)
          setError(resolveTranslatedErrorMessage(getErrorMessage(err), translate.current));
      } finally {
        if (current) setLoading(false);
      }
    })();
    return () => {
      current = false;
    };
  }, [refreshKey, nonce]);

  return { dossier, loading, error, refresh };
}
