import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { type GameStateData, useGameStore } from "../../store/gameStore";
import {
  type CustomTactic,
  deleteCustomTactic,
  legacyTacticsImportPending,
  listCustomTactics,
  markLegacyTacticsImported,
  saveCustomTactic,
} from "../../services/customTacticsService";
import { getTacticsBlueprintStatus } from "../../services/squadService";
import type { TacticsPhaseSettings } from "../../store/types";
import {
  TACTICS_PRESETS,
  findTacticsPresetBySetup,
  type TacticsPresetDefinition,
} from "./TacticsTab.helpers";
import {
  importLegacyTactics,
  toCustomTactic,
  toLibraryEntries,
} from "./TacticsCustomTactics.helpers";
import type { TacticsLibraryEntry } from "./TacticsCommandBar";

interface UseTacticsLibraryArgs {
  gameState: GameStateData | null;
  formation: string;
  activePlayStyle: string;
  outOfPossessionFormation?: string;
  tacticsPhase?: TacticsPhaseSettings;
  initialPreset: TacticsPresetDefinition | null;
  onFormationChange: (nextFormation: string) => Promise<boolean>;
  onPlayStyleChange: (playStyle: string) => Promise<boolean>;
  onApplyPreset: (
    formation: string,
    playStyle: string,
    tacticsPhase?: TacticsPhaseSettings,
    outOfPossessionFormation?: string,
  ) => Promise<boolean>;
  onApplyCustomTactic?: (id: string) => Promise<boolean>;
}

export function useTacticsLibrary({
  gameState,
  formation,
  activePlayStyle,
  outOfPossessionFormation,
  tacticsPhase,
  initialPreset,
  onFormationChange,
  onPlayStyleChange,
  onApplyPreset,
  onApplyCustomTactic,
}: UseTacticsLibraryArgs) {
  const { t } = useTranslation();
  // The library is the career's, kept in the save: it follows a change of club and a reload. It
  // starts empty and is filled from the save (and from an old browser library, once) below.
  const [customTactics, setCustomTactics] = useState<TacticsLibraryEntry[]>([]);
  const [activeTacticId, setActiveTacticId] = useState<string | null>(
    initialPreset ? `preset:${initialPreset.id}` : null,
  );
  const [draftTacticName, setDraftTacticName] = useState(
    initialPreset?.id
      ? t(`tactics.presetNames.${initialPreset.id}`, initialPreset.id)
      : t("tactics.customTactic"),
  );
  const [presetAnchorId, setPresetAnchorId] = useState<string | null>(initialPreset?.id ?? null);
  const managerId = gameState?.manager.id ?? null;
  // Handed back with every write so the backend can refuse one meant for another career.
  const careerToken = gameState?.career_token ?? 0;
  const careerStartDate = gameState?.clock.start_date ?? null;

  // Whether the phase dials still equal the style's blueprint is the backend's rule (`blueprint_for`);
  // it is asked again whenever the dials or the style change.
  const [blueprintModified, setBlueprintModified] = useState(false);
  const userTeam = gameState?.teams.find((team) => team.id === gameState.manager.team_id);
  const blueprintKey = `${activePlayStyle}|${JSON.stringify(userTeam?.tactics_phase ?? null)}`;

  const hasClub = Boolean(userTeam);

  useEffect(() => {
    // No game or no club has no blueprint to ask about.
    if (!hasClub) {
      setBlueprintModified(false);
      return;
    }

    let cancelled = false;
    getTacticsBlueprintStatus()
      .then(({ modified }) => {
        if (!cancelled) setBlueprintModified(modified);
      })
      .catch(() => {
        if (!cancelled) setBlueprintModified(false);
      });
    return () => {
      cancelled = true;
    };
  }, [blueprintKey, hasClub]);

  // Everything that talks to the save about the library runs one step at a time, in the order it
  // was asked: the first load, the import of an old browser library, and every save. Replies from
  // overlapping calls are not promised to arrive in order, and a load that began before a save must
  // not put its older list over the save's newer one. Each step's reply is the whole list, so the
  // last step to finish is the library as the save has it.
  const savesInOrder = useRef<Promise<void>>(Promise.resolve());
  // The commands act on whichever game is active, and carry no career with them. A step still
  // waiting in the queue when the career changes (another game loaded or the game left) would
  // write one career's tactic into the next, so each step remembers the store's career epoch it
  // was asked in and does nothing once that is no longer the current one. Leaving this screen
  // while the same career stays open changes nothing: the step is still that career's.
  const careerEpoch = useGameStore((state) => state.careerEpoch);

  useEffect(() => {
    if (!managerId || !careerStartDate) return;

    const askedIn = useGameStore.getState().careerEpoch;
    const stale = () => useGameStore.getState().careerEpoch !== askedIn;
    savesInOrder.current = savesInOrder.current
      .then(async () => {
        if (stale()) return;
        let tactics = await listCustomTactics();
        // Once per career: the save records that the old browser library was copied in, so a
        // tactic the player has since deleted is not copied back from the browser's key.
        if (await legacyTacticsImportPending()) {
          const imported = await importLegacyTactics(
            managerId,
            careerStartDate,
            tactics,
            async (tactic) => {
              if (stale()) throw new Error("the career changed");
              return saveCustomTactic(tactic, careerToken);
            },
          );
          tactics = imported.tactics;
          if (imported.complete && !stale()) await markLegacyTacticsImported(careerToken);
        }
        if (!stale()) setCustomTactics(toLibraryEntries(tactics));
      })
      .catch(() => {});
  }, [managerId, careerStartDate, careerEpoch, careerToken]);

  // The screen answers at once; the save's own list then replaces it, or, if the save refused the
  // change (a shape the game cannot lay out, an id that is already gone), puts the library back as
  // the save has it.
  function changeSave(change: () => Promise<CustomTactic[]>): void {
    const askedIn = useGameStore.getState().careerEpoch;
    const stale = () => useGameStore.getState().careerEpoch !== askedIn;
    savesInOrder.current = savesInOrder.current
      .then(() => (stale() ? null : change().catch(() => listCustomTactics())))
      .then((saved) => {
        if (saved && !stale()) setCustomTactics(toLibraryEntries(saved));
      })
      .catch(() => {});
  }

  function persistTactic(tactic: TacticsLibraryEntry): void {
    changeSave(() => saveCustomTactic(toCustomTactic(tactic), careerToken));
  }

  const matchedPreset = findTacticsPresetBySetup(
    formation,
    activePlayStyle,
    tacticsPhase,
    outOfPossessionFormation,
  );
  const anchoredPreset = presetAnchorId
    ? (TACTICS_PRESETS.find((preset) => preset.id === presetAnchorId) ?? null)
    : null;
  const isPresetDirty = Boolean(
    anchoredPreset &&
      (formation !== anchoredPreset.formation ||
        activePlayStyle !== anchoredPreset.playStyle ||
        outOfPossessionFormation !== anchoredPreset.outOfPossessionFormation ||
        (tacticsPhase != null &&
          JSON.stringify(tacticsPhase) !== JSON.stringify(anchoredPreset.tacticsPhase))),
  );
  const translatedPresetLibrary = useMemo<TacticsLibraryEntry[]>(
    () =>
      TACTICS_PRESETS.map((preset) => ({
        description: t(preset.descriptionKey),
        formation: preset.formation,
        id: `preset:${preset.id}`,
        name: t(`tactics.presetNames.${preset.id}`, preset.id),
        outOfPossessionFormation: preset.outOfPossessionFormation,
        playStyle: preset.playStyle,
        sourcePresetName: null,
        tacticsPhase: preset.tacticsPhase,
        type: "preset",
      })),
    [t],
  );
  const tacticLibrary = useMemo(
    () => [...customTactics, ...translatedPresetLibrary],
    [customTactics, translatedPresetLibrary],
  );
  const currentSetupFallbackTactic = useMemo<TacticsLibraryEntry>(
    () => ({
      description: t("tactics.customTacticDescription"),
      formation,
      id: "current:setup",
      name: t("tactics.customTactic"),
      outOfPossessionFormation,
      playStyle: activePlayStyle,
      sourcePresetName: null,
      tacticsPhase,
      type: "custom",
    }),
    [activePlayStyle, formation, outOfPossessionFormation, t, tacticsPhase],
  );
  const activeTactic =
    tacticLibrary.find((entry) => entry.id === activeTacticId) ??
    translatedPresetLibrary.find((entry) => entry.id === `preset:${matchedPreset?.id}`) ??
    currentSetupFallbackTactic;
  const isActiveCustomTactic = activeTactic?.type === "custom";
  const isActiveTacticDirty = Boolean(
    activeTactic &&
      (formation !== activeTactic.formation ||
        activePlayStyle !== activeTactic.playStyle ||
        (isActiveCustomTactic &&
          (outOfPossessionFormation !== (activeTactic.outOfPossessionFormation ?? formation) ||
            JSON.stringify(tacticsPhase ?? null) !==
              JSON.stringify(activeTactic.tacticsPhase ?? null) ||
            (draftTacticName.trim().length > 0 && draftTacticName.trim() !== activeTactic.name)))),
  );
  // A custom tactic saves the whole manager configuration, so every changed
  // phase dial and defensive shape is an unsaved change.
  const isCommandBarDirty = isActiveCustomTactic
    ? isActiveTacticDirty
    : isActiveTacticDirty || isPresetDirty || blueprintModified;

  useEffect(() => {
    if (!matchedPreset) {
      return;
    }

    if (matchedPreset.id !== presetAnchorId) {
      setPresetAnchorId(matchedPreset.id);
    }

    const nextActivePresetId = `preset:${matchedPreset.id}`;
    setActiveTacticId((current) =>
      tacticLibrary.find((entry) => entry.id === current)?.type === "custom" ||
      current === nextActivePresetId
        ? current
        : nextActivePresetId,
    );
  }, [matchedPreset, presetAnchorId, tacticLibrary]);

  useEffect(() => {
    if (!activeTactic) {
      return;
    }

    const nextName =
      activeTactic.type === "custom"
        ? activeTactic.name
        : t(`tactics.presetNames.${activeTactic.id.replace("preset:", "")}`);
    setDraftTacticName(nextName);
  }, [activeTactic?.id, activeTactic?.name, activeTactic?.type, t]);

  function createCustomTacticEntry(
    overrides: Partial<TacticsLibraryEntry> = {},
  ): TacticsLibraryEntry {
    const customCount = customTactics.length + 1;
    const sourcePresetName = matchedPreset
      ? t(`tactics.presetNames.${matchedPreset.id}`, matchedPreset.id)
      : null;

    return {
      description: overrides.description ?? t("tactics.customTacticDescription"),
      formation: overrides.formation ?? formation,
      id: overrides.id ?? `custom:${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      name: overrides.name ?? t("tactics.customTacticNumber", { count: customCount }),
      outOfPossessionFormation: overrides.outOfPossessionFormation ?? outOfPossessionFormation,
      playStyle: overrides.playStyle ?? activePlayStyle,
      sourcePresetName:
        overrides.sourcePresetName === undefined ? sourcePresetName : overrides.sourcePresetName,
      type: "custom",
      tacticsPhase: overrides.tacticsPhase ?? tacticsPhase,
    };
  }

  async function applyFormationAndStyle(nextTactic: TacticsLibraryEntry): Promise<boolean> {
    if (formation !== nextTactic.formation && !(await onFormationChange(nextTactic.formation))) {
      return false;
    }

    return activePlayStyle === nextTactic.playStyle || onPlayStyleChange(nextTactic.playStyle);
  }

  async function applyTacticSelection(nextTactic: TacticsLibraryEntry): Promise<void> {
    const isPreset = nextTactic.type === "preset";
    // A custom tactic is also a whole setup: the backend restores its persisted
    // defensive formation and phase blueprint in the same transaction.
    const didApply = isPreset
      ? await onApplyPreset(
          nextTactic.formation,
          nextTactic.playStyle,
          nextTactic.tacticsPhase ?? undefined,
          nextTactic.outOfPossessionFormation,
        )
      : onApplyCustomTactic &&
          (nextTactic.tacticsPhase != null || nextTactic.outOfPossessionFormation != null)
        ? await onApplyCustomTactic(nextTactic.id)
        : await applyFormationAndStyle(nextTactic);
    if (!didApply) {
      return;
    }

    setActiveTacticId(nextTactic.id);
    setDraftTacticName(nextTactic.name);

    if (isPreset) {
      setPresetAnchorId(nextTactic.id.replace("preset:", ""));
    }
  }

  function handleCreateCustomTactic(): void {
    const nextTactic = createCustomTacticEntry();
    setCustomTactics((current) => [nextTactic, ...current]);
    persistTactic(nextTactic);
    setActiveTacticId(nextTactic.id);
    setDraftTacticName(nextTactic.name);
  }

  function handleDuplicateTactic(): void {
    const nextTactic = createCustomTacticEntry({
      description: activeTactic?.description,
      formation,
      name: t("tactics.copyOfTactic", {
        name: draftTacticName.trim() || activeTactic?.name || t("tactics.customTactic"),
      }),
      outOfPossessionFormation,
      playStyle: activePlayStyle,
      sourcePresetName: activeTactic?.sourcePresetName ?? activeTactic?.name ?? null,
      tacticsPhase,
    });

    setCustomTactics((current) => [nextTactic, ...current]);
    persistTactic(nextTactic);
    setActiveTacticId(nextTactic.id);
    setDraftTacticName(nextTactic.name);
  }

  // Only a tactic the save holds can be deleted: not a preset, and not the unsaved setup that
  // stands in when nothing else matches.
  const canDeleteTactic =
    isActiveCustomTactic && customTactics.some((entry) => entry.id === activeTactic.id);

  // The team keeps the formation and play style it has. With the tactic gone from the library the
  // setup shown falls back to the preset it matches, or to the current setup, applying nothing.
  function handleDeleteTactic(): void {
    if (!canDeleteTactic) return;

    const { id } = activeTactic;
    setCustomTactics((current) => current.filter((entry) => entry.id !== id));
    changeSave(() => deleteCustomTactic(id.trim(), careerToken));
  }

  function handleSaveTactic(): void {
    const nextName = draftTacticName.trim() || t("tactics.customTactic");

    if (
      isActiveCustomTactic &&
      activeTactic &&
      customTactics.some((e) => e.id === activeTactic.id)
    ) {
      const updated: TacticsLibraryEntry = {
        ...activeTactic,
        description: activeTactic.description,
        formation,
        name: nextName,
        outOfPossessionFormation,
        playStyle: activePlayStyle,
        tacticsPhase,
      };
      setCustomTactics((current) =>
        current.map((entry) => (entry.id === activeTactic.id ? updated : entry)),
      );
      persistTactic(updated);
      return;
    }

    const nextTactic = createCustomTacticEntry({
      description: activeTactic?.description,
      formation,
      name: nextName,
      outOfPossessionFormation,
      playStyle: activePlayStyle,
      sourcePresetName: activeTactic?.name ?? null,
      tacticsPhase,
    });

    setCustomTactics((current) => [nextTactic, ...current]);
    persistTactic(nextTactic);
    setActiveTacticId(nextTactic.id);
    setDraftTacticName(nextTactic.name);
  }

  // Reset applies the full persisted preset or custom configuration.
  const anchoredPresetEntry =
    translatedPresetLibrary.find((entry) => entry.id === `preset:${presetAnchorId}`) ?? null;
  const resetTarget = isActiveCustomTactic
    ? (customTactics.find((entry) => entry.id === activeTactic.id) ?? anchoredPresetEntry)
    : activeTactic;
  // Reset is offered exactly when the badge says something differs: a preset against its whole
  // definition (formation, style and the backend's verdict on the dials), a saved custom tactic
  // against what it was saved with.
  const canResetTactic =
    resetTarget !== null &&
    (resetTarget.type === "preset"
      ? isActiveTacticDirty || isPresetDirty || blueprintModified
      : isActiveTacticDirty);

  async function handleResetTactic(): Promise<void> {
    if (resetTarget) {
      await applyTacticSelection(resetTarget);
    }
  }

  return {
    activeTactic,
    tacticLibrary,
    isCommandBarDirty,
    canResetTactic,
    handleResetTactic,
    applyTacticSelection,
    handleCreateCustomTactic,
    handleDuplicateTactic,
    handleSaveTactic,
    canDeleteTactic,
    handleDeleteTactic,
  };
}
