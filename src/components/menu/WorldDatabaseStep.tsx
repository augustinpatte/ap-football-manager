import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, ArrowLeft, ChevronRight, Database, Loader2, Shuffle, X } from "lucide-react";
import { Button } from "../ui";
import { FM24_WORLD_ID } from "../../lib/product";
import { resolveBackendError } from "../../utils/backendI18n";
import {
  findFm24WorldSource,
  installFm24World,
  listWorldDatabases,
  type WorldDatabaseInfo,
} from "../../services/worldDatabaseService";

/** What the player picked: the generated demo world, or a world file on this machine. */
export type WorldChoice =
  | { kind: "demo" }
  | { kind: "file"; path: string; baseYear: number | null };

interface WorldDatabaseStepProps {
  onNext: (choice: WorldChoice) => void;
  onBack: () => void;
  onClose: () => void;
}

const DEMO_ID = "random";

function formatStartDate(iso: string | null, language: string): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat(language, {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(date);
}

interface Fm24CardProps {
  world: WorldDatabaseInfo | null;
  selected: boolean;
  installing: boolean;
  error: string | null;
  onSelect: () => void;
  onInstall: () => void;
}

function Fm24Card({ world, selected, installing, error, onSelect, onInstall }: Fm24CardProps) {
  const { t, i18n } = useTranslation();
  const startDate = formatStartDate(world?.start_date ?? null, i18n.language);
  const frame = selected
    ? "border-primary-500 bg-primary-500/5"
    : "border-gray-200 dark:border-navy-600";

  return (
    <div className={`rounded-xl border p-4 flex flex-col gap-3 ${frame}`}>
      <div className="flex items-start gap-3">
        <Database className="w-5 h-5 mt-0.5 text-accent-500 dark:text-accent-400" />
        <div className="flex-1 min-w-0">
          <p className="font-heading font-bold text-gray-900 dark:text-white">
            {t("apfmWorld.fm24.name")}
          </p>
          <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
            {t("apfmWorld.fm24.description")}
          </p>
        </div>
        <span className="text-[10px] font-heading uppercase tracking-widest text-accent-600 dark:text-accent-400">
          {t("apfmWorld.fm24.badge")}
        </span>
      </div>

      {world ? (
        <>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs text-gray-600 dark:text-gray-300">
            <dt>{t("apfmWorld.scope")}</dt>
            <dd>
              {t("apfmWorld.scopeValue", {
                clubs: world.team_count,
                competitions: world.competitions.length,
                players: world.player_count,
              })}
            </dd>
            {startDate && (
              <>
                <dt>{t("apfmWorld.startDate")}</dt>
                <dd>{startDate}</dd>
              </>
            )}
          </dl>
          {world.competitions.length > 0 && (
            <details className="text-xs text-gray-600 dark:text-gray-300">
              <summary className="cursor-pointer font-medium">
                {t("apfmWorld.competitions", { count: world.competitions.length })}
              </summary>
              <ul className="mt-1 grid grid-cols-2 gap-x-4 list-disc list-inside">
                {world.competitions.map((name) => (
                  <li key={name}>{name}</li>
                ))}
              </ul>
            </details>
          )}
          <div className="flex gap-2 flex-wrap">
            <Button variant={selected ? "primary" : "outline"} size="sm" onClick={onSelect}>
              {selected ? t("apfmWorld.selected") : t("apfmWorld.select")}
            </Button>
            <Button variant="ghost" size="sm" disabled={installing} onClick={onInstall}>
              {installing ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
              {installing ? t("apfmWorld.installing") : t("apfmWorld.reinstall")}
            </Button>
          </div>
        </>
      ) : (
        <>
          <p className="text-xs text-gray-600 dark:text-gray-300">{t("apfmWorld.notInstalled")}</p>
          <div>
            <Button variant="primary" size="sm" disabled={installing} onClick={onInstall}>
              {installing ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
              {installing ? t("apfmWorld.installing") : t("apfmWorld.install")}
            </Button>
          </div>
        </>
      )}

      {error && (
        <p role="alert" className="flex items-start gap-2 text-xs text-red-600 dark:text-red-400">
          <AlertCircle className="w-4 h-4 shrink-0" />
          <span>{error}</span>
        </p>
      )}
      <p className="text-[11px] text-gray-400 dark:text-gray-500">{t("apfmWorld.localNotice")}</p>
    </div>
  );
}

export default function WorldDatabaseStep({ onNext, onBack, onClose }: WorldDatabaseStepProps) {
  const { t } = useTranslation();
  const [worlds, setWorlds] = useState<WorldDatabaseInfo[] | null>(null);
  const [selectedId, setSelectedId] = useState<string>(DEMO_ID);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async (): Promise<WorldDatabaseInfo[]> => {
    try {
      const found = await listWorldDatabases();
      setWorlds(found);
      return found;
    } catch (failure) {
      console.error("Failed to list worlds:", failure);
      setWorlds([]);
      return [];
    }
  }, []);

  useEffect(() => {
    void refresh().then((found) => {
      // A local world that is already installed is what this player came to play.
      const local = found.find((world) => world.world_id === FM24_WORLD_ID);
      if (local) setSelectedId(local.id);
    });
  }, [refresh]);

  const fm24 = worlds?.find((world) => world.world_id === FM24_WORLD_ID) ?? null;
  const others = (worlds ?? []).filter(
    (world) => world.id !== DEMO_ID && world.world_id !== FM24_WORLD_ID,
  );

  const handleInstall = async () => {
    setInstalling(true);
    setError(null);
    try {
      let source = await findFm24WorldSource();
      if (!source) {
        const picked = await open({
          filters: [{ name: "world.db", extensions: ["db"] }],
          multiple: false,
          title: t("apfmWorld.chooseSource"),
        });
        if (typeof picked !== "string") return;
        source = picked;
      }
      await installFm24World(source);
      const found = await refresh();
      const local = found.find((world) => world.world_id === FM24_WORLD_ID);
      if (local) setSelectedId(local.id);
    } catch (failure) {
      setError(resolveBackendError(failure));
    } finally {
      setInstalling(false);
    }
  };

  const handleNext = () => {
    const chosen = (worlds ?? []).find((world) => world.id === selectedId);
    if (!chosen || chosen.id === DEMO_ID) {
      onNext({ kind: "demo" });
      return;
    }
    onNext({ kind: "file", path: chosen.path, baseYear: chosen.base_year });
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex justify-between items-center mb-2">
        <div>
          <h2 className="text-xl font-heading font-bold uppercase tracking-wide text-gray-900 dark:text-white">
            {t("apfmWorld.title")}
          </h2>
          <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">{t("apfmWorld.subtitle")}</p>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-gray-400 hover:text-gray-700 dark:hover:text-white transition-colors p-1 rounded-lg hover:bg-gray-100 dark:hover:bg-navy-600"
          aria-label={t("common.close")}
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      {worlds === null ? (
        <div className="flex items-center gap-2 text-sm text-gray-500">
          <Loader2 className="w-4 h-4 animate-spin" />
          {t("apfmWorld.loading")}
        </div>
      ) : (
        <>
          <Fm24Card
            world={fm24}
            selected={fm24 !== null && selectedId === fm24.id}
            installing={installing}
            error={error}
            onSelect={() => fm24 && setSelectedId(fm24.id)}
            onInstall={() => void handleInstall()}
          />

          {others.map((world) => (
            <button
              key={world.id}
              type="button"
              onClick={() => setSelectedId(world.id)}
              className={`text-left rounded-xl border p-4 ${
                selectedId === world.id
                  ? "border-primary-500 bg-primary-500/5"
                  : "border-gray-200 dark:border-navy-600"
              }`}
            >
              <p className="font-heading font-bold text-gray-900 dark:text-white">{world.name}</p>
              <p className="text-xs text-gray-500 dark:text-gray-400">
                {t("apfmWorld.scopeValue", {
                  clubs: world.team_count,
                  competitions: world.competitions.length,
                  players: world.player_count,
                })}
              </p>
            </button>
          ))}

          <button
            type="button"
            onClick={() => setSelectedId(DEMO_ID)}
            aria-pressed={selectedId === DEMO_ID}
            className={`text-left rounded-xl border p-4 flex items-start gap-3 ${
              selectedId === DEMO_ID
                ? "border-primary-500 bg-primary-500/5"
                : "border-gray-200 dark:border-navy-600"
            }`}
          >
            <Shuffle className="w-5 h-5 mt-0.5 text-gray-400" />
            <span className="flex-1">
              <span className="block font-heading font-bold text-gray-900 dark:text-white">
                {t("apfmWorld.demo.name")}
              </span>
              <span className="block text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                {t("apfmWorld.demo.description")}
              </span>
            </span>
            <span className="text-[10px] font-heading uppercase tracking-widest text-gray-400">
              {t("apfmWorld.demo.badge")}
            </span>
          </button>
        </>
      )}

      <div className="flex justify-between gap-3 mt-2">
        <Button variant="ghost" onClick={onBack}>
          <ArrowLeft className="w-4 h-4" />
          {t("common.back")}
        </Button>
        <Button variant="primary" disabled={worlds === null || installing} onClick={handleNext}>
          {t("apfmWorld.next")}
          <ChevronRight className="w-4 h-4" />
        </Button>
      </div>
    </div>
  );
}
