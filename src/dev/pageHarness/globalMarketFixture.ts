import type {
  ExternalNegotiationOutcome,
  ExternalNegotiationRequest,
  RegistryPage,
  RegistryPlayerDetail,
  RegistrySearchRequest,
  RegistrySummary,
} from "../../services/registryService";
import { gameState } from "./fixture.generated";

/**
 * A deterministic market for the dev harness. It is deliberately independent of the career's
 * squads: the global market is an external registry, and the page should be exercised with players
 * whose names, clubs and prices are longer than anything the generated career happens to hold.
 */

const CLUBS = [
  "Club Atlético Rosario Central",
  "Borussia Mönchengladbach",
  "Olympique de Marseille",
  "Sporting Clube de Portugal",
  "Galatasaray Spor Kulübü",
  "Associazione Calcio Fiorentina",
  "Real Sociedad de Fútbol",
  "Feyenoord Rotterdam",
  "Tottenham Hotspur",
  "Club Brugge KV",
];

const NATIONS = ["ARG", "GER", "FRA", "POR", "TUR", "ITA", "ESP", "NED", "ENG", "BEL"];

const NAMES = [
  "Maximiliano Fernández de la Torre",
  "Aleksandar Petrović-Đorđević",
  "Jean-Baptiste Mbemba",
  "Oluwaseun Adebayo Okonkwo",
  "Søren Vestergaard Christensen",
  "N'Golo Kanté-Diallo",
  "Vasilios Papadopoulos",
  "Takumi Yamamoto",
  "Mateo Rodríguez",
  "Dmitri Sokolov",
  "Facundo Almeida",
  "Lucas Bianchi",
  "Amadou Traoré",
  "Emiliano Rossi",
  "Ibrahim Hassan",
  "Rafael da Silva",
  "Kacper Nowak",
  "Luka Modrić",
  "Noah Andersson",
  "Ayo Balogun",
  "Tomás Echeverría",
  "Nikola Jovanović",
  "Youssef El Amrani",
  "Kwame Mensah",
  "Finn Van der Berg",
  "Andrés Felipe Cárdenas",
  "Miroslav Horváth",
  "Shinji Takahashi",
  "Pedro Henrique Nascimento",
  "Erik Lindqvist",
];

const POSITIONS = [
  { position: "Goalkeeper", group: "goalkeeper" },
  { position: "Defender", group: "defender" },
  { position: "Midfielder", group: "midfielder" },
  { position: "Forward", group: "forward" },
] as const;

const ATTRIBUTE_VALUES = [
  "pace",
  "stamina",
  "strength",
  "agility",
  "aerial",
  "passing",
  "shooting",
  "tackling",
  "dribbling",
  "defending",
  "positioning",
  "vision",
  "decisions",
  "composure",
  "aggression",
  "teamwork",
  "leadership",
  "handling",
  "reflexes",
] as const;

const CONTRACT_END = "2033-06-30";

/** Long enough for five pages, so the paging controls have something to page through. */
export const MARKET_TOTAL = NAMES.length;

export const marketPlayers: RegistrySummary[] = NAMES.map((name, index) => {
  const position = POSITIONS[index % POSITIONS.length];
  const freeAgent = index % 5 === 4;
  return {
    id: `market-player-${index + 1}`,
    name,
    age: 18 + (index % 17),
    nationality: NATIONS[index % NATIONS.length],
    position: position.position,
    positionGroup: position.group,
    ovr: 88 - index,
    potential: 92 - (index % 9),
    club: freeAgent ? null : CLUBS[index % CLUBS.length],
    ownership: freeAgent ? "free_agent" : "contracted",
    contractEnd: freeAgent ? null : CONTRACT_END,
    marketValue: 1_500_000 + index * 750_000,
    weeklyWage: 9_000 + index * 750,
    askingPrice: freeAgent ? 0 : 2_000_000 + index * 1_000_000,
    status: "active",
  };
});

function compare(
  left: RegistrySummary,
  right: RegistrySummary,
  sort: RegistrySearchRequest["sort"],
): number {
  switch (sort) {
    case "name":
      return left.name.localeCompare(right.name);
    case "age":
      return (left.age ?? 0) - (right.age ?? 0);
    case "value":
      return left.marketValue - right.marketValue;
    case "club":
      return (left.club ?? "").localeCompare(right.club ?? "");
    default:
      return left.ovr - right.ovr;
  }
}

function matches(player: RegistrySummary, request: RegistrySearchRequest): boolean {
  const text = request.text.trim().toLowerCase();
  if (
    text &&
    ![player.name, player.club ?? "", player.nationality ?? ""].some((value) =>
      value.toLowerCase().includes(text),
    )
  ) {
    return false;
  }
  if (request.nation && player.nationality !== request.nation) return false;
  if (request.club && !(player.club ?? "").toLowerCase().includes(request.club.toLowerCase())) {
    return false;
  }
  if (request.position !== "any" && player.positionGroup !== request.position) return false;
  if (request.availability === "free_agent" && player.ownership !== "free_agent") return false;
  if (request.availability === "contracted" && player.ownership !== "contracted") return false;
  if (request.minAge !== null && (player.age ?? 0) < request.minAge) return false;
  if (request.maxAge !== null && (player.age ?? 99) > request.maxAge) return false;
  return true;
}

export function marketSearchPage(request: RegistrySearchRequest): RegistryPage {
  const pageSize = request.pageSize > 0 ? request.pageSize : 25;
  const found = marketPlayers
    .filter((player) => matches(player, request))
    .sort((left, right) => comparable(request.sort, left, right, request.descending));
  const pages = Math.max(1, Math.ceil(found.length / pageSize));
  const page = Math.min(Math.max(1, request.page), pages);
  return {
    items: found.slice((page - 1) * pageSize, page * pageSize),
    total: found.length,
    page,
    pageSize,
    counts: {
      all: marketPlayers.length,
      freeAgent: marketPlayers.filter((player) => player.ownership === "free_agent").length,
      contracted: marketPlayers.filter((player) => player.ownership === "contracted").length,
      incomplete: 0,
    },
  };
}

function comparable(
  sort: RegistrySearchRequest["sort"],
  left: RegistrySummary,
  right: RegistrySummary,
  descending: boolean,
): number {
  const order = compare(left, right, sort);
  return descending ? -order : order;
}

export function marketPlayerDetail(id: string): RegistryPlayerDetail | null {
  const index = marketPlayers.findIndex((player) => player.id === id);
  if (index < 0) return null;
  const player = marketPlayers[index];
  return {
    id: player.id,
    name: player.name,
    firstName: player.name.split(" ")[0],
    lastName: player.name.split(" ").slice(1).join(" "),
    commonName: null,
    age: player.age,
    birthDate: "2004-03-14",
    nation: player.nationality,
    nationality: player.nationality,
    position: player.position,
    positionGroup: player.positionGroup,
    naturalPosition: player.position,
    alternatePositions: index % 3 === 0 ? ["Midfielder"] : [],
    footedness: index % 4 === 0 ? "Left" : "Right",
    weakFoot: 2 + (index % 4),
    attributes: Object.fromEntries(
      ATTRIBUTE_VALUES.map((attribute, position) => [attribute, 8 + ((index + position) % 12)]),
    ),
    ovr: player.ovr,
    potential: player.potential,
    clubId: player.ownership === "free_agent" ? null : 700 + index,
    clubName: player.club,
    clubNation: player.ownership === "free_agent" ? null : NATIONS[index % NATIONS.length],
    clubReputation: 620,
    ownership: player.ownership,
    contractEnd: player.contractEnd,
    marketValue: player.marketValue,
    weeklyWage: player.weeklyWage,
    valueSource: "market",
    wageSource: "contract",
    askingPrice: player.askingPrice,
    status: player.status,
    exclusionReason: null,
    fmUid: null,
    fmUidStatus: "none",
    sourceRowHash: `page-harness-market-${index + 1}`,
  };
}

/**
 * The fixed answers a player gives in the harness: the first is signable, the seventh refuses and
 * the eighth is blocked by a full squad, so the screen's three outcomes can all be reached.
 */
export function marketNegotiation(
  id: string,
  request: ExternalNegotiationRequest,
): ExternalNegotiationOutcome | null {
  const detail = marketPlayerDetail(id);
  if (!detail) return null;
  const index = marketPlayers.findIndex((player) => player.id === id);
  const freeAgent = detail.ownership === "free_agent";
  const fee = freeAgent ? 0 : (request.fee ?? detail.askingPrice);
  const weeklyWage = request.weeklyWage ?? detail.weeklyWage;
  const contractYears = request.contractYears ?? 3;
  const signingBonus = request.signingBonus ?? 0;
  const refused = index === 6;
  const blocked = index === 7;
  return {
    playerId: detail.id,
    freeAgent,
    status: blocked ? "blocked" : refused ? "player_refused" : "agreed",
    fee,
    minimumFee: freeAgent ? 0 : Math.floor(detail.askingPrice * 0.9),
    clubDecision: "accepted",
    weeklyWage,
    contractYears,
    signingBonus,
    contractEnd: CONTRACT_END,
    playerVerdict: refused ? "refuses" : "accepts",
    playerScore: refused ? 20 : 78,
    mainObstacle: refused ? "wage" : null,
    canAffordFee: true,
    wagePolicyAllows: true,
    canAffordBonus: true,
    windowOpen: !blocked,
    blocker: blocked ? "be.error.freeAgents.squadFull?count=30&max=30&release=1" : null,
    fromClub: detail.clubName,
  };
}

/** The generated career's type is narrower than `GameStateData`; the harness only echoes it back. */
export function marketFinalize(id: string, request: ExternalNegotiationRequest) {
  const outcome = marketNegotiation(id, request);
  if (!outcome) return null;
  return {
    game: gameState,
    outcome: {
      playerId: outcome.playerId,
      freeAgent: outcome.freeAgent,
      fee: outcome.fee,
      weeklyWage: outcome.weeklyWage,
      contractEnd: outcome.contractEnd,
      fromClub: outcome.fromClub,
    },
  };
}
