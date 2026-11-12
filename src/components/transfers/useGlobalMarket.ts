import { useCallback, useEffect, useRef, useState } from "react";

import type { GameStateData } from "../../store/gameStore";
import {
  EMPTY_NEGOTIATION_FORM,
  type GlobalMarketQuery,
  type NegotiationForm,
  DEFAULT_MARKET_QUERY,
  canSign,
  formFromOutcome,
  requestFromForm,
  searchRequest,
  withQueryChange,
} from "./globalMarketModel";
import {
  finalizeExternalTransfer,
  getExternalPlayer,
  negotiateExternalTransfer,
  searchExternalPlayers,
  type ExternalNegotiationOutcome,
  type ExternalNegotiationRequest,
  type RegistryPage,
  type RegistryPlayerDetail,
} from "../../services/registryService";

export interface GlobalMarketNotice {
  key: string;
  params?: Record<string, string | number>;
}

export interface GlobalMarketController {
  query: GlobalMarketQuery;
  page: RegistryPage | null;
  loading: boolean;
  error: unknown;
  selectedId: string | null;
  detail: RegistryPlayerDetail | null;
  detailLoading: boolean;
  form: NegotiationForm;
  outcome: ExternalNegotiationOutcome | null;
  quoting: boolean;
  signing: boolean;
  actionError: unknown;
  notice: GlobalMarketNotice | null;
  confirmOpen: boolean;
  setFilter: (patch: Partial<GlobalMarketQuery>) => void;
  reload: () => void;
  selectPlayer: (id: string) => void;
  closeDetail: () => void;
  setFormField: (field: keyof NegotiationForm, value: string) => void;
  checkTerms: () => void;
  applyCounter: () => void;
  requestSign: () => void;
  cancelSign: () => void;
  confirmSign: () => void;
}

const SEARCH_DEBOUNCE_MS = 250;

export function useGlobalMarket(
  onGameUpdate: (state: GameStateData) => void,
): GlobalMarketController {
  const [query, setQuery] = useState<GlobalMarketQuery>(DEFAULT_MARKET_QUERY);
  const [page, setPage] = useState<RegistryPage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [reloadToken, setReloadToken] = useState(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<RegistryPlayerDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [form, setForm] = useState<NegotiationForm>(EMPTY_NEGOTIATION_FORM);
  const [outcome, setOutcome] = useState<ExternalNegotiationOutcome | null>(null);
  const [quoting, setQuoting] = useState(false);
  const [signing, setSigning] = useState(false);
  const [actionError, setActionError] = useState<unknown>(null);
  const [notice, setNotice] = useState<GlobalMarketNotice | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const searchSeq = useRef(0);
  const detailSeq = useRef(0);

  useEffect(() => {
    const seq = ++searchSeq.current;
    const timer = setTimeout(() => {
      void (async () => {
        setLoading(true);
        setError(null);
        try {
          const result = await searchExternalPlayers(searchRequest(query));
          if (seq === searchSeq.current) setPage(result);
        } catch (readError) {
          if (seq === searchSeq.current) {
            setError(readError);
            setPage(null);
          }
        } finally {
          if (seq === searchSeq.current) setLoading(false);
        }
      })();
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query, reloadToken]);

  const setFilter = useCallback((patch: Partial<GlobalMarketQuery>) => {
    setQuery((current) => withQueryChange(current, patch));
  }, []);

  const reload = useCallback(() => setReloadToken((token) => token + 1), []);

  const selectPlayer = useCallback((id: string) => {
    const seq = ++detailSeq.current;
    setSelectedId(id);
    setDetail(null);
    setDetailLoading(true);
    setOutcome(null);
    setActionError(null);
    setNotice(null);
    setForm(EMPTY_NEGOTIATION_FORM);
    void (async () => {
      try {
        const loaded = await getExternalPlayer(id);
        if (seq === detailSeq.current) setDetail(loaded);
      } catch (readError) {
        if (seq === detailSeq.current) setActionError(readError);
      } finally {
        if (seq === detailSeq.current) setDetailLoading(false);
      }
    })();
  }, []);

  const closeDetail = useCallback(() => {
    detailSeq.current += 1;
    setSelectedId(null);
    setDetail(null);
    setDetailLoading(false);
    setOutcome(null);
    setActionError(null);
    setForm(EMPTY_NEGOTIATION_FORM);
  }, []);

  const setFormField = useCallback((field: keyof NegotiationForm, value: string) => {
    setForm((current) => ({ ...current, [field]: value }));
    // Edited terms are no longer the terms that were agreed.
    setOutcome(null);
    setActionError(null);
  }, []);

  const checkWith = useCallback(
    async (request: ExternalNegotiationRequest) => {
      const target = detail;
      if (!target) return;
      setQuoting(true);
      setActionError(null);
      try {
        setOutcome(await negotiateExternalTransfer(target.id, request));
      } catch (negotiationError) {
        setOutcome(null);
        setActionError(negotiationError);
      } finally {
        setQuoting(false);
      }
    },
    [detail],
  );

  const checkTerms = useCallback(() => {
    void checkWith(requestFromForm(form));
  }, [checkWith, form]);

  const applyCounter = useCallback(() => {
    if (!outcome) return;
    const accepted = formFromOutcome(outcome);
    setForm(accepted);
    void checkWith(requestFromForm(accepted));
  }, [checkWith, outcome]);

  const requestSign = useCallback(() => {
    if (!canSign(outcome)) return;
    setConfirmOpen(true);
  }, [outcome]);

  const cancelSign = useCallback(() => {
    if (!signing) setConfirmOpen(false);
  }, [signing]);

  const confirmSign = useCallback(async () => {
    const target = detail;
    if (!target || !canSign(outcome)) return;
    setSigning(true);
    setActionError(null);
    try {
      const response = await finalizeExternalTransfer(target.id, requestFromForm(form));
      const club =
        response.game.teams.find((team) => team.id === response.game.manager.team_id)?.name ?? "";
      onGameUpdate(response.game);
      setConfirmOpen(false);
      closeDetail();
      setNotice({
        key: "globalMarket.signed",
        params: { player: target.name, club },
      });
      reload();
    } catch (signError) {
      setActionError(signError);
      setConfirmOpen(false);
    } finally {
      setSigning(false);
    }
  }, [closeDetail, detail, form, onGameUpdate, outcome, reload]);

  return {
    query,
    page,
    loading,
    error,
    selectedId,
    detail,
    detailLoading,
    form,
    outcome,
    quoting,
    signing,
    actionError,
    notice,
    confirmOpen,
    setFilter,
    reload,
    selectPlayer,
    closeDetail,
    setFormField,
    checkTerms,
    applyCounter,
    requestSign,
    cancelSign,
    confirmSign,
  };
}
