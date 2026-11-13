import type { SupportedLanguageCode } from "../../i18n";
import type {
  ExternalNegotiationRequest,
  RegistrySearchRequest,
} from "../../services/registryService";
import type { SettingsResponse } from "../../store/settingsStore";
import type { StyleGuideOptions } from "../styleGuide/styleGuideOptions";
import * as fixture from "./fixture.generated";
import {
  marketFinalize,
  marketNegotiation,
  marketPlayerDetail,
  marketSearchPage,
} from "./globalMarketFixture";
import { type CommandHandlers, UNANSWERED } from "./harnessIpc";

/** The fixture holds one club, the user's: anything asked about another club is declined. */
const USER_CLUB = fixture.gameState.manager.team_id;

function forUserClub(teamId: unknown, answer: () => unknown): unknown {
  return teamId === USER_CLUB ? answer() : UNANSWERED;
}

/** The settings the page sees, matching what the URL put on the document. */
export function harnessSettings(
  options: StyleGuideOptions,
  language: SupportedLanguageCode,
): SettingsResponse {
  const currency = { code: "EUR", symbol: "€", exchange_rate: 1 } as const;
  return {
    settings: {
      theme: options.theme,
      ui_scale: options.scale,
      high_contrast: options.highContrast,
      language,
    },
    currency,
    supported_currencies: [currency],
  };
}

/**
 * The answers to every command the dashboard shell, Home and Squad ask for on mount, from the
 * generated career. A page that needs another one shows it in the harness's unanswered list.
 */
export function fixtureHandlers(
  options: StyleGuideOptions,
  language: SupportedLanguageCode,
): CommandHandlers {
  return {
    get_settings: () => harnessSettings(options, language),
    get_active_game: () => fixture.gameState,
    get_active_save_id: () => "page-harness",
    get_career_stage: () => fixture.careerStage,
    // No team id means the manager's club, as in the Rust command.
    get_finance_snapshot: ({ teamId }) =>
      forUserClub(teamId ?? USER_CLUB, () => ({
        snapshot: fixture.financeSnapshot,
        previews: fixture.financePreviews,
      })),
    get_club_league_standing: ({ teamId }) => forUserClub(teamId, () => fixture.clubLeagueStanding),
    get_squad: ({ teamId }) => forUserClub(teamId, () => fixture.squad),
    // The dashboard's staff warnings read fitness; every fixture player is fit, and the rating
    // tiers are the backend's fixed rule (A from 80, B from 60, S from 90).
    get_squad_fitness: () => ({
      players: fixture.squad.length,
      average_condition: 88,
      exhausted_count: 0,
      critical_count: 0,
      statuses: Object.fromEntries(fixture.squad.map((player) => [player.id, "Fit"])),
    }),
    get_rating_tiers: () =>
      Array.from({ length: 100 }, (_, rating) =>
        rating >= 90 ? "S" : rating >= 80 ? "A" : rating >= 60 ? "B" : "Rest",
      ),
    get_available_jobs: () => fixture.availableJobs,
    // Only the players asked about; Rust leaves out an id no player has.
    get_contract_warning_stages: ({ playerIds }) => {
      const asked = new Set(Array.isArray(playerIds) ? playerIds : []);
      return Object.fromEntries(
        Object.entries(fixture.contractWarningStages).filter(([id]) => asked.has(id)),
      );
    },
    // The global market is an external registry: it answers about its own players, never the
    // career's squads, and declines an id it does not hold the way the Rust command returns none.
    search_external_players: ({ request }) => marketSearchPage(request as RegistrySearchRequest),
    get_external_player: ({ playerId }) => marketPlayerDetail(String(playerId)) ?? UNANSWERED,
    negotiate_external_transfer: ({ playerId, request }) =>
      marketNegotiation(String(playerId), request as ExternalNegotiationRequest) ?? UNANSWERED,
    finalize_external_transfer: ({ playerId, request }) =>
      marketFinalize(String(playerId), request as ExternalNegotiationRequest) ?? UNANSWERED,
  };
}
