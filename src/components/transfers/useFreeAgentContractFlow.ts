import { useEffect, useState } from "react";

import type { FreeAgentTalkData, GameStateData, PlayerData, TeamData } from "../../store/gameStore";
import {
  acceptFreeAgentCounter,
  offerFreeAgentContract,
  previewFreeAgentContractImpact,
  withdrawFreeAgentOffer,
  type FreeAgentContractProjection,
  type FreeAgentContractResponseData,
  type FreeAgentInterestData,
  type SquadCapacityData,
} from "../../services/freeAgentService";
import { calcAgeOnDate } from "../../lib/valueFormatting";
import { resolveBackendError } from "../../utils/backendI18n";

interface UseFreeAgentContractFlowArgs {
  gameState: GameStateData;
  onGameUpdate?: (game: GameStateData) => void;
}

const MARKET_VALUE_TO_WAGE_RATIO = 200;
const MINIMUM_DEFAULT_WAGE = 500;
const MAX_CONTRACT_YEARS = 5;

/** What the talk panel of the contract form shows and does: the negotiation, not the terms. */
export interface FreeAgentTalkPanelData {
  /** The club's senior squad list; a full list is a registration limit, not the player's answer. */
  squadCapacity: SquadCapacityData | null;
  /** How the player would take the terms now; absent until the preview has answered. */
  interest: FreeAgentInterestData | null;
  /** The manager's club's talk with him: the open one, else the latest that ended. */
  talk: FreeAgentTalkData | null;
  /** Other clubs with an offer to him open. */
  rivals: { club_id: string; club_name: string }[];
  busy: boolean;
  onAcceptCounter: () => void;
  onWithdraw: () => void;
}

interface UseFreeAgentContractFlowResult {
  freeAgentTarget: PlayerData | null;
  contractWage: string;
  setContractWage: (value: string) => void;
  contractLength: string;
  setContractLength: (value: string) => void;
  steppingStoneAvailable: boolean;
  signingBonus: string;
  setSigningBonus: (value: string) => void;
  /** A translation key: why a bonus of that size cannot be paid, when the backend says so. */
  signingBonusError: string | null;
  promiseSteppingStone: boolean;
  setPromiseSteppingStone: (value: boolean) => void;
  /** A resolved, player-readable error from the last action, if it failed. */
  contractError: string | null;
  contractProjection: FreeAgentContractProjection | null;
  contractSubmitting: boolean;
  contractSubmitDisabled: boolean;
  talkPanel: FreeAgentTalkPanelData;
  myTeam: TeamData | null;
  openFreeAgentContract: (player: PlayerData) => void;
  closeFreeAgentContract: () => void;
  submitFreeAgentContract: () => Promise<void>;
}

/** The club's talk with the player: the open one, else the one that ended last. */
function talkOfClub(player: PlayerData | null, clubId: string | null): FreeAgentTalkData | null {
  const talks = (player?.morale_core?.free_agent_talks ?? []).filter(
    (talk) => talk.club_id === clubId,
  );
  const open = talks.find(
    (talk) => talk.status === "awaiting_response" || talk.status === "counter_offered",
  );
  if (open) return open;
  return [...talks].sort((a, b) => b.updated_on.localeCompare(a.updated_on))[0] ?? null;
}

function defaultContractYears(dateOfBirth: string, asOfDate: string): string {
  const age = calcAgeOnDate(dateOfBirth, asOfDate);

  if (age <= 28) return "3";
  if (age <= 32) return "2";
  return "1";
}

function defaultContractWage(player: PlayerData): string {
  const baseline =
    player.wage > 0
      ? player.wage
      : Math.max(
          Math.round(player.market_value / MARKET_VALUE_TO_WAGE_RATIO),
          MINIMUM_DEFAULT_WAGE,
        );
  return String(Math.ceil(baseline / 1000) * 1000);
}

export function useFreeAgentContractFlow({
  gameState,
  onGameUpdate,
}: UseFreeAgentContractFlowArgs): UseFreeAgentContractFlowResult {
  const myTeam = gameState.teams.find((team) => team.id === gameState.manager.team_id) ?? null;
  // The id, not the player: read from the game state each render (see `useTransferBidFlow`).
  const [freeAgentTargetId, setFreeAgentTargetId] = useState<string | null>(null);
  const freeAgentTarget =
    freeAgentTargetId === null
      ? null
      : (gameState.players.find((player) => player.id === freeAgentTargetId) ?? null);
  const [contractWage, setContractWage] = useState("");
  const [contractLength, setContractLength] = useState("");
  const [steppingStoneAvailable, setSteppingStoneAvailable] = useState(false);
  const [signingBonus, setSigningBonus] = useState("0");
  const [signingBonusError, setSigningBonusError] = useState<string | null>(null);
  const [squadCapacity, setSquadCapacity] = useState<SquadCapacityData | null>(null);
  const [promiseSteppingStone, setPromiseSteppingStone] = useState(false);
  const [contractError, setContractError] = useState<string | null>(null);
  const [interest, setInterest] = useState<FreeAgentInterestData | null>(null);
  const [contractProjection, setContractProjection] = useState<FreeAgentContractProjection | null>(
    null,
  );
  const [contractSubmitting, setContractSubmitting] = useState(false);

  const offeredWage = Number(contractWage);
  const offeredYears = Number(contractLength);
  const isContractWageValid = Number.isFinite(offeredWage) && offeredWage > 0;
  const offeredBonus = Number(signingBonus);
  const isSigningBonusValid = Number.isInteger(offeredBonus) && offeredBonus >= 0;
  const isContractLengthValid =
    Number.isInteger(offeredYears) && offeredYears > 0 && offeredYears <= MAX_CONTRACT_YEARS;
  const contractViolatesSoftCap =
    isContractWageValid && contractProjection !== null && !contractProjection.policy_allows;
  const talk = talkOfClub(freeAgentTarget, gameState.manager.team_id ?? null);
  const talkIsAwaitingAnswer = talk?.status === "awaiting_response";
  const today = gameState.clock.current_date;

  useEffect(() => {
    if (!freeAgentTarget || !isContractWageValid) {
      setContractProjection(null);
      setInterest(null);
      return;
    }

    let cancelled = false;

    const loadProjection = async (): Promise<void> => {
      try {
        const result = await previewFreeAgentContractImpact(
          freeAgentTarget.id,
          offeredWage,
          isSigningBonusValid ? offeredBonus : 0,
          isContractLengthValid ? offeredYears : undefined,
          promiseSteppingStone,
        );

        if (!cancelled) {
          setContractProjection(result.projection ?? null);
          setInterest(result.interest ?? null);
          setSigningBonusError(result.signing_bonus_error ?? null);
          setSquadCapacity(result.squad_capacity ?? null);
          const available = result.stepping_stone_available === true;
          setSteppingStoneAvailable(available);
          // A promise the player would not value is refused by the backend: never carry it over.
          if (!available) {
            setPromiseSteppingStone(false);
          }
        }
      } catch {
        if (!cancelled) {
          setContractProjection(null);
          setInterest(null);
        }
      }
    };

    void loadProjection();

    return () => {
      cancelled = true;
    };
    // `today` and the talk's status: the answer to "how would he take it" moves with the days.
  }, [
    freeAgentTarget,
    isContractWageValid,
    offeredWage,
    isSigningBonusValid,
    offeredBonus,
    isContractLengthValid,
    offeredYears,
    promiseSteppingStone,
    today,
    talk?.status,
  ]);

  const resetOffer = (): void => {
    setPromiseSteppingStone(false);
    setSteppingStoneAvailable(false);
    setSigningBonus("0");
    setSigningBonusError(null);
    setContractError(null);
    setContractProjection(null);
    setInterest(null);
  };

  const openFreeAgentContract = (player: PlayerData): void => {
    setFreeAgentTargetId(player.id);
    // A counter on the table is the best place to start: it is what he asked for.
    const counter = talkOfClub(player, gameState.manager.team_id ?? null);
    const counterTerms = counter?.status === "counter_offered" ? counter : null;
    setContractWage(
      counterTerms?.counter_wage != null
        ? String(counterTerms.counter_wage)
        : defaultContractWage(player),
    );
    setContractLength(
      counterTerms?.counter_years != null
        ? String(counterTerms.counter_years)
        : defaultContractYears(player.date_of_birth, gameState.clock.current_date),
    );
    resetOffer();
  };

  const closeFreeAgentContract = (): void => {
    if (contractSubmitting) {
      return;
    }

    setFreeAgentTargetId(null);
    setContractWage("");
    setContractLength("");
    resetOffer();
  };

  /** Runs one action against the talk, publishing the game it returns or the error it fails with. */
  const runTalkAction = async (
    action: () => Promise<FreeAgentContractResponseData>,
  ): Promise<void> => {
    setContractSubmitting(true);
    setContractError(null);
    try {
      const result = await action();
      onGameUpdate?.(result.game);
    } catch (error) {
      setContractError(resolveBackendError(error));
    } finally {
      setContractSubmitting(false);
    }
  };

  const submitFreeAgentContract = async (): Promise<void> => {
    if (
      !freeAgentTarget ||
      !isContractWageValid ||
      !isContractLengthValid ||
      !isSigningBonusValid ||
      talkIsAwaitingAnswer
    ) {
      return;
    }
    await runTalkAction(() =>
      offerFreeAgentContract(
        freeAgentTarget.id,
        offeredWage,
        offeredYears,
        promiseSteppingStone,
        offeredBonus,
      ),
    );
  };

  const rivals = (freeAgentTarget?.morale_core?.free_agent_talks ?? [])
    .filter(
      (other) =>
        other.club_id !== (gameState.manager.team_id ?? null) &&
        (other.status === "awaiting_response" || other.status === "counter_offered"),
    )
    .map((other) => ({
      club_id: other.club_id,
      club_name: gameState.teams.find((team) => team.id === other.club_id)?.name ?? other.club_id,
    }));

  const talkPanel: FreeAgentTalkPanelData = {
    squadCapacity,
    interest,
    talk,
    rivals,
    busy: contractSubmitting,
    onAcceptCounter: () => {
      if (freeAgentTarget) {
        void runTalkAction(() => acceptFreeAgentCounter(freeAgentTarget.id));
      }
    },
    onWithdraw: () => {
      if (freeAgentTarget) {
        void runTalkAction(() => withdrawFreeAgentOffer(freeAgentTarget.id));
      }
    },
  };

  return {
    freeAgentTarget,
    contractWage,
    setContractWage,
    contractLength,
    setContractLength,
    steppingStoneAvailable,
    signingBonus,
    setSigningBonus,
    signingBonusError,
    promiseSteppingStone,
    setPromiseSteppingStone,
    contractError,
    contractProjection,
    contractSubmitting,
    contractSubmitDisabled:
      contractSubmitting ||
      talkIsAwaitingAnswer ||
      !isContractWageValid ||
      !isContractLengthValid ||
      !isSigningBonusValid ||
      signingBonusError !== null ||
      squadCapacity?.blocks_signing === true ||
      contractViolatesSoftCap,
    talkPanel,
    myTeam,
    openFreeAgentContract,
    closeFreeAgentContract,
    submitFreeAgentContract,
  };
}
