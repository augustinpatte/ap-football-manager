import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { getAcademyReview, type AcademyReview } from "../../../services/academyService";
import { getErrorMessage, resolveTranslatedErrorMessage } from "../../../utils/errorMessage";

interface AcademyReviewState {
  review: AcademyReview | null;
  loading: boolean;
  error: string | null;
  refresh: () => void;
}

/** The academy review; asked again when `refreshKey` changes or after an action of the screen's own. */
export function useAcademyReview(refreshKey: unknown): AcademyReviewState {
  const { t } = useTranslation();
  // The effect must not re-run when the translator's identity changes, only when the data may have.
  const translate = useRef(t);
  translate.current = t;
  const [review, setReview] = useState<AcademyReview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const refresh = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    let current = true;
    setLoading(true);
    void (async () => {
      try {
        const answer = await getAcademyReview();
        if (!current) return;
        setReview(answer ?? null);
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

  return { review, loading, error, refresh };
}
