import { getResumableMatch, takeMatchRestoreNotice } from "../services/interactiveMatchService";
import { routeForActiveCareer } from "../services/careerService";
import { useManagerDobError } from "../hooks/useManagerDobError";
import { Suspense, lazy, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useGameStore, type GameStateData } from "../store/gameStore";
import { ThemeToggle } from "../components/ui/ThemeToggle";
import type { CareerStartPhase, CreateManagerFormData } from "../components/menu/CreateManagerForm";
import type { PackageInfo, PackageIssue } from "../components/menu/WorldSelect";
import type { WorldChoice } from "../components/menu/WorldDatabaseStep";
import { UPSTREAM_BUG_REPORT_ENABLED } from "../lib/product";
import type { ManagerProfile } from "../components/menu/types";
import { applyExtraTranslations } from "../lib/extraTranslations";
import { formatAppVersion } from "../lib/appVersion";
import { resolveBackendError } from "../utils/backendI18n";
import { prewarmManagerSquadPortraits } from "../services/portraitService";
import { FolderOpen, Settings, PlusCircle, ChevronRight, Power, Package } from "lucide-react";
import { ReportBugModal } from "../components/diagnostics/ReportBugModal";
import { CrashReportPrompt } from "../components/diagnostics/CrashReportPrompt";
import { showError } from "../lib/errorDialog";

const CreateManagerForm = lazy(() => import("../components/menu/CreateManagerForm"));
const ProfileSaveConfirm = lazy(() => import("../components/menu/ProfileSaveConfirm"));
const SavesList = lazy(() => import("../components/menu/SavesList"));
const PackageBuildStep = lazy(() => import("../components/menu/PackageBuildStep"));
const WorldDatabaseStep = lazy(() => import("../components/menu/WorldDatabaseStep"));
const GenerationStep = lazy(() => import("../components/menu/WorldSelect"));

interface SaveEntry {
  id: string;
  name: string;
  manager_name: string;
  team_name: string;
  db_filename: string;
  checksum: string;
  created_at: string;
  last_played_at: string;
}

/**
 * Earliest year a career may start. Historical world packages recreate eras
 * decades before the modern game, so the floor only keeps the clock inside a
 * sane calendar range. Must match `MIN_START_YEAR` in `commands/game.rs`.
 */
const MIN_CAREER_START_YEAR = 1900;
const DEFAULT_GENERATED_HISTORY_DEPTH_YEARS = 12;
const MAX_GENERATED_HISTORY_DEPTH_YEARS = 24;
const GENERATED_HISTORY_DEPTH_STORAGE_KEY = "ofm-generated-history-depth-years";

type StartupOptionsPayload = {
  startYear: number;
  startPhase: CareerStartPhase;
  historyDepthYears: number;
};

function defaultCareerStartYear(): string {
  return String(new Date().getFullYear());
}

function parseCareerStartYear(rawValue: string): number | null {
  const trimmed = rawValue.trim();
  if (!/^\d+$/.test(trimmed)) return null;

  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed)) return null;
  return parsed;
}

function isCareerStartPhase(value: string): value is CareerStartPhase {
  return value === "seasonStart" || value === "midSeason";
}

function normalizeHistoryDepthYears(value: number): number | null {
  if (!Number.isInteger(value)) return null;
  if (value < 0 || value > MAX_GENERATED_HISTORY_DEPTH_YEARS) return null;
  return value;
}

function initialHistoryDepthYears(): number {
  if (typeof window === "undefined") {
    return DEFAULT_GENERATED_HISTORY_DEPTH_YEARS;
  }

  const storedValue = window.localStorage.getItem(GENERATED_HISTORY_DEPTH_STORAGE_KEY);
  if (storedValue === null) {
    return DEFAULT_GENERATED_HISTORY_DEPTH_YEARS;
  }

  const parsedValue = Number(storedValue);
  return normalizeHistoryDepthYears(parsedValue) ?? DEFAULT_GENERATED_HISTORY_DEPTH_YEARS;
}

function buildStartupOptions(
  formData: CreateManagerFormData,
  historyDepthYears: number,
): StartupOptionsPayload | null {
  const startYear = parseCareerStartYear(formData.startYear);
  if (startYear === null || startYear < MIN_CAREER_START_YEAR) {
    return null;
  }
  if (!isCareerStartPhase(formData.startPhase)) {
    return null;
  }
  const normalizedHistoryDepthYears = normalizeHistoryDepthYears(historyDepthYears);
  if (normalizedHistoryDepthYears === null) {
    return null;
  }

  return {
    startYear,
    startPhase: formData.startPhase,
    historyDepthYears: normalizedHistoryDepthYears,
  };
}

const CREATE_MANAGER_FIELD_ORDER = [
  "firstName",
  "lastName",
  "dob",
  "startYear",
  "startPhase",
  "nationality",
] as const satisfies ReadonlyArray<keyof CreateManagerFormData>;

function prefersReducedMotion(): boolean {
  if (typeof window === "undefined") return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function deferFocusToNextPaint(callback: () => void): void {
  requestAnimationFrame(() => {
    requestAnimationFrame(callback);
  });
}

function focusFirstCreateManagerError(
  errors: Partial<Record<keyof CreateManagerFormData, string>>,
): void {
  const first = CREATE_MANAGER_FIELD_ORDER.find((k) => errors[k]);
  if (!first) return;
  const root = document.getElementById(`create-manager-field-${first}`);
  root?.scrollIntoView?.({
    behavior: prefersReducedMotion() ? "auto" : "smooth",
    block: "center",
  });
  const focusable = root?.querySelector<HTMLElement>(
    "input:not([type=hidden]), button:not([disabled]), select, textarea",
  );
  focusable?.focus({ preventScroll: true });
}

function MenuPanelFallback() {
  return (
    <div className="flex min-h-64 items-center justify-center">
      <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary-500 border-t-transparent" />
    </div>
  );
}

export default function MainMenu() {
  const navigate = useNavigate();
  const setGameActive = useGameStore((state) => state.setGameActive);
  const setGameState = useGameStore((state) => state.setGameState);
  const { t } = useTranslation();

  const [menuState, setMenuState] = useState<
    "main" | "create" | "worlds" | "packages" | "generation" | "load"
  >("main");
  const [showProfileConfirm, setShowProfileConfirm] = useState(false);
  const [saves, setSaves] = useState<SaveEntry[]>([]);
  const [isLoadingSaves, setIsLoadingSaves] = useState(false);
  const [loadingSaveId, setLoadingSaveId] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const [reportingBug, setReportingBug] = useState(false);

  const [profiles, setProfiles] = useState<ManagerProfile[]>([]);
  const [loadedProfile, setLoadedProfile] = useState<ManagerProfile | null>(null);

  const [formData, setFormData] = useState<CreateManagerFormData>({
    firstName: "",
    lastName: "",
    dob: "",
    startYear: defaultCareerStartYear(),
    startPhase: "seasonStart",
    nationality: "",
  });
  const [formErrors, setFormErrors] = useState<
    Partial<Record<keyof CreateManagerFormData, string>>
  >({});

  // Installed packages state
  const [installedPackages, setInstalledPackages] = useState<PackageInfo[]>([]);
  const [activePackageIds, setActivePackageIds] = useState<string[]>([]);
  const [isInstallingPackage, setIsInstallingPackage] = useState(false);
  const [packageStackErrors, setPackageStackErrors] = useState<PackageIssue[]>([]);
  const [historyDepthYears, setHistoryDepthYears] = useState(initialHistoryDepthYears);

  useEffect(() => {
    window.localStorage.setItem(GENERATED_HISTORY_DEPTH_STORAGE_KEY, String(historyDepthYears));
  }, [historyDepthYears]);

  useEffect(() => {
    invoke<ManagerProfile[]>("get_manager_profiles")
      .then((p) => setProfiles(p ?? []))
      .catch((error) => console.error("Failed to load manager profiles:", error));
  }, []);

  // Check if a game is already active (e.g. loaded by MCP --mcp-auto-start before frontend mounted)
  useEffect(() => {
    invoke<GameStateData>("get_active_game")
      .then((state) => {
        const mgrName = state.manager.display_name;
        applyExtraTranslations(state.extra_translations);
        setGameState(state);
        setGameActive(true, mgrName);
        return routeForActiveCareer().then(navigate);
      })
      .catch(() => {
        // No active game — stay on menu
      });
  }, [setGameState, setGameActive, navigate]);

  // Listen for game loaded by MCP auto-start (event may arrive after mount)
  useEffect(() => {
    const unlisten = listen("game-state-changed", async () => {
      try {
        const state = await invoke<GameStateData>("get_active_game");
        const mgrName = state.manager.display_name;
        applyExtraTranslations(state.extra_translations);
        setGameState(state);
        setGameActive(true, mgrName);
        navigate(await routeForActiveCareer());
      } catch {
        // Game not actually active — ignore
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [setGameState, setGameActive, navigate]);

  /** The backend's answer about the date of birth, shown as the user edits and again on submit. */
  const dobStartup = buildStartupOptions(formData, historyDepthYears);
  const dobRuleKey = useManagerDobError(
    formData.dob,
    dobStartup?.startYear ?? null,
    dobStartup?.startPhase ?? null,
  );
  const dobLiveRuleMessage = dobRuleKey ? resolveBackendError(dobRuleKey) : null;
  const dobDisplayedError = formErrors.dob || dobLiveRuleMessage;

  const updateFormField = (field: keyof CreateManagerFormData, value: string) => {
    setFormData((previous) => ({
      ...previous,
      [field]: value,
    }));
  };

  const clearFormError = (field: keyof CreateManagerFormData) => {
    setFormErrors((previous) => ({
      ...previous,
      [field]: "",
    }));
  };

  const validateForm = (): {
    ok: boolean;
    errors: Partial<Record<keyof CreateManagerFormData, string>>;
  } => {
    const errors: Partial<Record<keyof CreateManagerFormData, string>> = {};
    if (!formData.firstName.trim()) {
      errors.firstName = t("validation.required", {
        field: t("createManager.firstName"),
      });
    } else if (formData.firstName.length > 30) {
      errors.firstName = t("validation.maxLength", {
        field: t("createManager.firstName"),
        max: 30,
      });
    }

    if (!formData.lastName.trim()) {
      errors.lastName = t("validation.required", {
        field: t("createManager.lastName"),
      });
    } else if (formData.lastName.length > 30) {
      errors.lastName = t("validation.maxLength", {
        field: t("createManager.lastName"),
        max: 30,
      });
    }

    if (!formData.dob) {
      errors.dob = t("validation.required", { field: t("createManager.dob") });
    } else {
      if (dobLiveRuleMessage) {
        errors.dob = dobLiveRuleMessage;
      }
    }
    if (!formData.startYear.trim()) {
      errors.startYear = t("validation.required", {
        field: t("createManager.startYear"),
      });
    } else {
      const startYear = parseCareerStartYear(formData.startYear);
      if (startYear === null || startYear < MIN_CAREER_START_YEAR) {
        errors.startYear = t("validation.minStartYear", {
          min: MIN_CAREER_START_YEAR,
        });
      }
    }
    if (!isCareerStartPhase(formData.startPhase)) {
      errors.startPhase = t("validation.required", {
        field: t("createManager.startPhase"),
      });
    }
    if (!formData.nationality)
      errors.nationality = t("validation.required", {
        field: t("createManager.countryOfOrigin"),
      });
    setFormErrors(errors);
    return {
      ok: Object.keys(errors).length === 0,
      errors,
    };
  };

  const handleGoToWorldSelect = (e: React.FormEvent) => {
    e.preventDefault();
    const validation = validateForm();
    if (!validation.ok) {
      deferFocusToNextPaint(() => focusFirstCreateManagerError(validation.errors));
      return;
    }
    if (loadedProfile && formDiffersFromProfile(formData, loadedProfile)) {
      setShowProfileConfirm(true);
      return;
    }
    void autoSaveProfile();
    proceedToWorlds();
  };

  const loadInstalledPackages = async () => {
    try {
      const pkgs = await invoke<PackageInfo[]>("list_installed_packages");
      // Tauri commands can resolve to null; never let installedPackages become
      // null or downstream `.filter` calls (here and in PackageBuildStep) throw.
      setInstalledPackages(pkgs ?? []);
    } catch (err) {
      console.error("Failed to list packages:", err);
    }
  };

  useEffect(() => {
    if (menuState === "packages") {
      void loadInstalledPackages();
    }
  }, [menuState]);

  const handleInstallPackage = async () => {
    const selected = await open({
      filters: [{ name: "OFM Package", extensions: ["ofm"] }],
      multiple: false,
      title: t("worldSelect.installPackage"),
    });
    if (typeof selected !== "string") return;
    setIsInstallingPackage(true);
    try {
      await invoke<PackageInfo>("install_package", { path: selected });
      await loadInstalledPackages();
    } catch (err) {
      console.error("Failed to install package:", err);
      await showError(t("errors.title"), resolveBackendError(err));
    } finally {
      setIsInstallingPackage(false);
    }
  };

  const handleUninstallPackage = async (id: string) => {
    try {
      await invoke("uninstall_package", { id });
      setInstalledPackages((prev) => prev.filter((p) => p.id !== id));
      setActivePackageIds((prev) => prev.filter((pid) => pid !== id));
      setPackageStackErrors([]);
    } catch (err) {
      console.error("Failed to uninstall package:", err);
      await showError(t("errors.title"), resolveBackendError(err));
    }
  };

  const handleTogglePackage = (id: string) => {
    setActivePackageIds((prev) =>
      prev.includes(id) ? prev.filter((pid) => pid !== id) : [...prev, id],
    );
    setPackageStackErrors([]);
  };

  const handleWorldChosen = (choice: WorldChoice) => {
    if (choice.kind === "demo") {
      setMenuState("packages");
      return;
    }
    void startGame({ worldSource: choice.path, baseYear: choice.baseYear });
  };

  const handleStartGame = () => startGame();

  const startGame = async (fileWorld?: { worldSource: string; baseYear: number | null }) => {
    // A fixed-roster world opens in the season its data describes, whatever the form says: a
    // 2026/27 squad list in another year would age every player wrongly.
    const startForm =
      fileWorld?.baseYear != null
        ? { ...formData, startYear: String(fileWorld.baseYear), startPhase: "seasonStart" as const }
        : formData;
    const startupOptions = buildStartupOptions(startForm, historyDepthYears);
    if (!startupOptions) {
      const validation = validateForm();
      if (validation.ok) {
        // The two disagreed: `buildStartupOptions` rejected something `validateForm` does not
        // check. Today that is only the generated-history depth, which lives on the world screen
        // and has no field on this form — so bouncing to the create step would put the player in
        // front of a form with no errors on it and no way to tell what was wrong. Say so instead.
        await showError(t("errors.title"), t("menu.startupOptionsRejected"));
        return;
      }
      setMenuState("create");
      deferFocusToNextPaint(() => focusFirstCreateManagerError(validation.errors));
      return;
    }

    setIsStarting(true);
    try {
      const game = await invoke<GameStateData>("start_new_game", {
        firstName: formData.firstName,
        lastName: formData.lastName,
        dob: formData.dob,
        nationality: formData.nationality,
        startupOptions,
        worldSource: fileWorld?.worldSource,
        packageIds: !fileWorld && activePackageIds.length > 0 ? activePackageIds : undefined,
      });
      applyExtraTranslations(game.extra_translations);
      setGameState(game);
      navigate("/select-team");
    } catch (error) {
      console.error("Failed to start game:", error);
      await showError(
        t("errors.title"),
        t("menu.failedStartGame", { error: resolveBackendError(error) }),
      );
    } finally {
      setIsStarting(false);
    }
  };

  const handleOpenLoadMenu = async () => {
    setMenuState("load");
    setIsLoadingSaves(true);
    try {
      const dbSaves = await invoke<SaveEntry[]>("get_saves");
      setSaves(dbSaves);
    } catch (error) {
      console.error("Failed to load saves:", error);
    } finally {
      setIsLoadingSaves(false);
    }
  };

  const handleLoadGame = async (saveId: string) => {
    setLoadingSaveId(saveId);
    try {
      const managerName = await invoke<string>("load_game", { saveId });
      const activeGame = await invoke<GameStateData>("get_active_game");
      try {
        await prewarmManagerSquadPortraits(activeGame);
      } catch (portraitError) {
        console.warn("Portrait prewarm failed during save load:", portraitError);
      }
      setGameState(activeGame);
      setGameActive(true, managerName);
      const restoreNotice = await takeMatchRestoreNotice().catch(() => null);
      if (restoreNotice) {
        await showError(t("errors.title"), resolveBackendError(restoreNotice));
      }
      const resumable = await getResumableMatch().catch(() => null);
      if (resumable) {
        navigate("/match", {
          state: {
            fixtureIndex: resumable.fixture_index,
            competitionId: resumable.competition_id,
            fixtureId: resumable.fixture_id,
            mode: "live",
            resume: true,
          },
        });
        return;
      }
      navigate(await routeForActiveCareer());
    } catch (error) {
      console.error("Failed to load game:", error);
      setLoadingSaveId(null);
      await showError(
        t("errors.title"),
        t("menu.loadGameFailed", { error: resolveBackendError(error) }),
      );
    }
  };

  const handleDeleteSave = async (saveId: string) => {
    try {
      await invoke<boolean>("delete_save", { saveId });
      setSaves((prev) => prev.filter((s) => s.id !== saveId));
      setConfirmDeleteId(null);
    } catch (error) {
      console.error("Failed to delete save:", error);
    }
  };

  const handleSelectProfile = (profile: ManagerProfile) => {
    setFormData((prev) => ({
      ...prev,
      firstName: profile.first_name,
      lastName: profile.last_name,
      dob: profile.date_of_birth,
      nationality: profile.nationality,
    }));
    setFormErrors({});
    setLoadedProfile(profile);
    void invoke("touch_manager_profile", { id: profile.id });
  };

  const formDiffersFromProfile = (form: CreateManagerFormData, profile: ManagerProfile) =>
    form.firstName !== profile.first_name ||
    form.lastName !== profile.last_name ||
    form.dob !== profile.date_of_birth ||
    form.nationality !== profile.nationality;

  const proceedToWorlds = () => {
    setShowProfileConfirm(false);
    setMenuState("worlds");
  };

  const handleUpdateProfile = async () => {
    if (!loadedProfile) return;
    try {
      const updated = await invoke<ManagerProfile | null>("update_manager_profile", {
        id: loadedProfile.id,
        firstName: formData.firstName,
        lastName: formData.lastName,
        dob: formData.dob,
        nationality: formData.nationality,
      });
      if (updated) {
        setProfiles((prev) => prev.map((p) => (p.id === updated.id ? updated : p)));
        setLoadedProfile(updated);
      }
    } catch (error) {
      console.error("Failed to update manager profile:", error);
    }
    proceedToWorlds();
  };

  const handleSaveAsNewProfile = () => {
    void autoSaveProfile(true);
    setLoadedProfile(null);
    proceedToWorlds();
  };

  const handleDeleteProfile = async (id: string) => {
    try {
      await invoke<boolean>("delete_manager_profile", { id });
      setProfiles((prev) => prev.filter((p) => p.id !== id));
      if (loadedProfile?.id === id) setLoadedProfile(null);
    } catch (error) {
      console.error("Failed to delete manager profile:", error);
    }
  };

  const autoSaveProfile = async (forceNew = false) => {
    try {
      const saved = await invoke<ManagerProfile>("save_manager_profile", {
        firstName: formData.firstName,
        lastName: formData.lastName,
        dob: formData.dob,
        nationality: formData.nationality,
        force: forceNew || undefined,
      });
      setProfiles((prev) => {
        const exists = prev.some((p) => p.id === saved.id);
        const next =
          !forceNew && exists ? prev.map((p) => (p.id === saved.id ? saved : p)) : [...prev, saved];
        return next.sort((a, b) => {
          const aDate = a.last_used_at ?? a.created_at;
          const bDate = b.last_used_at ?? b.created_at;
          return bDate.localeCompare(aDate);
        });
      });
    } catch (error) {
      console.error("Failed to auto-save manager profile:", error);
    }
  };

  const handleExitApp = async (): Promise<void> => {
    try {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
      }
      await getCurrentWindow().destroy();
    } catch (error) {
      console.error("Failed to exit app:", error);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-100 dark:bg-navy-900 transition-colors duration-500 relative overflow-x-hidden">
      {/* Background gradient accents */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -top-40 -right-40 w-96 h-96 bg-primary-500/10 dark:bg-primary-500/5 rounded-full blur-3xl" />
        <div className="absolute -bottom-40 -left-40 w-96 h-96 bg-accent-400/10 dark:bg-accent-400/5 rounded-full blur-3xl" />
      </div>

      {/* Theme Toggle */}
      <ThemeToggle className="absolute top-6 right-6 z-20" />

      {/* Main Card */}
      <div className="relative z-10 w-full max-w-md">
        {/* Top accent bar */}
        <div className="h-1.5 bg-gradient-to-r from-primary-500 via-accent-400 to-primary-500 rounded-t-2xl" />

        <div className="bg-white dark:bg-navy-800 p-8 rounded-b-2xl shadow-xl dark:shadow-2xl border border-gray-200 dark:border-navy-600 border-t-0 transition-all duration-500">
          {/* Wordmark: typographic on purpose, until the product has a logo of its own */}
          <h1 className="text-center font-heading font-extrabold uppercase leading-none text-gray-900 dark:text-white">
            <span className="block text-6xl text-primary-500">AP</span>
            <span className="block text-2xl tracking-[0.2em] mt-2">Football Manager</span>
          </h1>

          <div className="border-t border-gray-200 dark:border-navy-600 my-8 transition-colors duration-500" />

          {/* Main Menu */}
          {menuState === "main" && (
            <div className="flex flex-col gap-3">
              <button
                type="button"
                onClick={() => setMenuState("create")}
                className="group flex items-center justify-between w-full p-4 bg-gradient-to-r from-primary-500 to-primary-600 hover:from-primary-600 hover:to-primary-700 text-white rounded-xl transition-all duration-300 shadow-md hover:shadow-lg hover:shadow-primary-500/20"
              >
                <div className="flex items-center gap-3">
                  <PlusCircle className="w-6 h-6" />
                  <span className="font-heading font-bold text-lg uppercase tracking-wide">
                    {t("menu.newGame")}
                  </span>
                </div>
                <ChevronRight className="w-5 h-5 opacity-70 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all" />
              </button>

              <button
                type="button"
                onClick={handleOpenLoadMenu}
                className="group flex items-center justify-between w-full p-4 bg-white dark:bg-navy-700 hover:bg-gray-50 dark:hover:bg-navy-600 text-gray-800 dark:text-gray-200 rounded-xl transition-all duration-300 border border-gray-200 dark:border-navy-600 hover:border-accent-400 dark:hover:border-accent-400 shadow-sm"
              >
                <div className="flex items-center gap-3">
                  <FolderOpen className="w-6 h-6 text-accent-500 dark:text-accent-400" />
                  <span className="font-heading font-bold text-lg uppercase tracking-wide">
                    {t("menu.loadGame")}
                  </span>
                </div>
                <ChevronRight className="w-5 h-5 opacity-0 group-hover:opacity-70 group-hover:translate-x-0.5 transition-all text-accent-500" />
              </button>

              <button
                type="button"
                onClick={() => navigate("/world-editor")}
                className="group flex items-center justify-between w-full p-4 bg-white dark:bg-navy-700 hover:bg-gray-50 dark:hover:bg-navy-600 text-gray-800 dark:text-gray-200 rounded-xl transition-all duration-300 border border-gray-200 dark:border-navy-600 hover:border-accent-400 dark:hover:border-accent-400 shadow-sm"
              >
                <div className="flex items-center gap-3">
                  <Package className="w-6 h-6 text-accent-500 dark:text-accent-400" />
                  <span className="font-heading font-bold text-lg uppercase tracking-wide">
                    {t("menu.worldEditor")}
                  </span>
                </div>
                <ChevronRight className="w-5 h-5 opacity-0 group-hover:opacity-70 group-hover:translate-x-0.5 transition-all text-accent-500" />
              </button>

              <button
                type="button"
                onClick={() => navigate("/settings", { state: { from: "/" } })}
                className="group flex items-center justify-between w-full p-4 bg-white dark:bg-navy-700 hover:bg-gray-50 dark:hover:bg-navy-600 text-gray-800 dark:text-gray-200 rounded-xl transition-all duration-300 border border-gray-200 dark:border-navy-600 hover:border-gray-300 dark:hover:border-navy-600 shadow-sm"
              >
                <div className="flex items-center gap-3">
                  <Settings className="w-6 h-6 text-gray-400 dark:text-gray-500" />
                  <span className="font-heading font-bold text-lg uppercase tracking-wide">
                    {t("menu.settings")}
                  </span>
                </div>
                <ChevronRight className="w-5 h-5 opacity-0 group-hover:opacity-70 group-hover:translate-x-0.5 transition-all text-gray-400" />
              </button>

              <button
                type="button"
                onClick={() => {
                  void handleExitApp();
                }}
                className="group flex items-center justify-between w-full p-4 bg-white dark:bg-navy-700 hover:bg-red-50 dark:hover:bg-red-500/10 text-gray-800 dark:text-gray-200 rounded-xl transition-all duration-300 border border-gray-200 dark:border-navy-600 hover:border-red-200 dark:hover:border-red-500/30 shadow-sm"
              >
                <div className="flex items-center gap-3">
                  <Power className="w-6 h-6 text-red-500 dark:text-red-400" />
                  <span className="font-heading font-bold text-lg uppercase tracking-wide">
                    {t("menu.exitGame")}
                  </span>
                </div>
              </button>
            </div>
          )}

          {/* Step 1: Create Manager Form */}
          {menuState === "create" && (
            <Suspense fallback={<MenuPanelFallback />}>
              <CreateManagerForm
                formData={formData}
                formErrors={formErrors}
                dobError={dobDisplayedError}
                profiles={profiles}
                selectedProfileId={loadedProfile?.id}
                onChange={updateFormField}
                onClearError={clearFormError}
                onClose={() => {
                  setMenuState("main");
                  setFormErrors({});
                  setLoadedProfile(null);
                }}
                onSelectProfile={handleSelectProfile}
                onDeleteProfile={handleDeleteProfile}
                onSubmit={handleGoToWorldSelect}
              />
            </Suspense>
          )}

          {/* Profile save confirmation modal */}
          {showProfileConfirm && loadedProfile && (
            <Suspense fallback={null}>
              <ProfileSaveConfirm
                loadedProfile={loadedProfile}
                onUpdate={() => {
                  void handleUpdateProfile();
                }}
                onSaveNew={() => {
                  void handleSaveAsNewProfile();
                }}
                onSkip={proceedToWorlds}
                onClose={() => setShowProfileConfirm(false)}
              />
            </Suspense>
          )}

          {/* Step 2: which world to play in (the FM24 world on this machine, or the demo world) */}
          {menuState === "worlds" && (
            <Suspense fallback={<MenuPanelFallback />}>
              <WorldDatabaseStep
                onNext={handleWorldChosen}
                onBack={() => setMenuState("create")}
                onClose={() => setMenuState("main")}
              />
            </Suspense>
          )}

          {/* Step 2a: Build Your World (package selection) */}
          {menuState === "packages" && (
            <Suspense fallback={<MenuPanelFallback />}>
              <PackageBuildStep
                installedPackages={installedPackages}
                activePackageIds={activePackageIds}
                isInstallingPackage={isInstallingPackage}
                packageStackErrors={packageStackErrors}
                onTogglePackage={handleTogglePackage}
                onInstallPackage={handleInstallPackage}
                onUninstallPackage={handleUninstallPackage}
                onNext={() => setMenuState("generation")}
                onBack={() => setMenuState("worlds")}
                onClose={() => setMenuState("main")}
              />
            </Suspense>
          )}

          {/* Step 2b: Generation & Completion */}
          {menuState === "generation" && (
            <Suspense fallback={<MenuPanelFallback />}>
              <GenerationStep
                isStarting={isStarting}
                startYear={parseCareerStartYear(formData.startYear) ?? MIN_CAREER_START_YEAR}
                startPhase={formData.startPhase}
                historyDepthYears={historyDepthYears}
                onChangeHistoryDepthYears={setHistoryDepthYears}
                onStart={handleStartGame}
                onBack={() => setMenuState("packages")}
                onClose={() => setMenuState("main")}
                activePackages={installedPackages.filter((p) => activePackageIds.includes(p.id))}
              />
            </Suspense>
          )}

          {/* Load Game List */}
          {menuState === "load" && (
            <Suspense fallback={<MenuPanelFallback />}>
              <SavesList
                loadingSaveId={loadingSaveId}
                saves={saves}
                isLoading={isLoadingSaves}
                confirmDeleteId={confirmDeleteId}
                onLoad={handleLoadGame}
                onDelete={handleDeleteSave}
                onConfirmDelete={setConfirmDeleteId}
                onClose={() => setMenuState("main")}
              />
            </Suspense>
          )}

          {/* Package Editor */}
        </div>
      </div>

      {/* Version */}
      <div className="absolute bottom-4 right-4 text-gray-400 dark:text-gray-600 text-xs font-heading uppercase tracking-widest transition-colors">
        {formatAppVersion()}
      </div>

      {UPSTREAM_BUG_REPORT_ENABLED && (
        <>
          <CrashReportPrompt onReport={() => setReportingBug(true)} />
          {reportingBug && <ReportBugModal onClose={() => setReportingBug(false)} />}
        </>
      )}
    </div>
  );
}
