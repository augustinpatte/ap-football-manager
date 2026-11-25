import type { LoanOfferData, PlayerData, TransferOfferData } from "../../store/gameStore";

/** The manager's dealings, grouped by what they need from him or what stage they have reached. */
export type ActivitySection =
  | "received"
  | "negotiating"
  | "pendingRegistration"
  | "completed"
  | "activeLoans"
  | "closed";

export interface ActivityEntry {
  /** Unique within the list: the offer id, or the player id for an active loan. */
  key: string;
  kind: "permanent" | "loan";
  /** Which side of the deal the manager's club is on: `buying` includes borrowing. */
  direction: "buying" | "selling";
  playerId: string;
  playerName: string;
  /** The other club of the deal. */
  counterpartyTeamId: string;
  fee: number | null;
  wageSharePct: number | null;
  status: TransferOfferData["status"];
  /** The date the entry is ordered by: when talks closed, else when the offer last moved. */
  date: string;
  registrationDate: string | null;
  failureReason: string | null;
  /** The end date of a loan, when it is one. */
  loanEndDate: string | null;
}

export type TransferActivity = Record<ActivitySection, ActivityEntry[]>;

/** Rejected and withdrawn talks older than this are history, not news. */
const CLOSED_WINDOW_DAYS = 60;
const MAX_CLOSED_ENTRIES = 15;

function daysBetween(from: string, to: string): number {
  return Math.round((Date.parse(to) - Date.parse(from)) / 86_400_000);
}

function emptyActivity(): TransferActivity {
  return {
    received: [],
    negotiating: [],
    pendingRegistration: [],
    completed: [],
    activeLoans: [],
    closed: [],
  };
}

/** Where a persisted offer sits for the manager, given who made it. */
function sectionFor(
  status: TransferOfferData["status"],
  direction: ActivityEntry["direction"],
): ActivitySection {
  switch (status) {
    case "Pending":
      return direction === "selling" ? "received" : "negotiating";
    case "PendingRegistration":
      return "pendingRegistration";
    case "Accepted":
      return "completed";
    case "Rejected":
    case "Withdrawn":
      return "closed";
  }
}

function transferEntries(player: PlayerData, userTeamId: string): ActivityEntry[] {
  return (player.transfer_offers ?? []).flatMap((offer: TransferOfferData) => {
    const buying = offer.from_team_id === userTeamId;
    // An offer from another club belongs to the manager only if the player is, or went from, his
    // club: it is stored on the player, and a club's dealings with a third club are not his.
    const selling =
      !buying && (player.team_id === userTeamId || player.team_id === offer.from_team_id);
    if (!buying && !selling) return [];
    const direction = buying ? "buying" : "selling";
    return [
      {
        key: offer.id,
        kind: "permanent" as const,
        direction,
        playerId: player.id,
        playerName: player.full_name,
        counterpartyTeamId: buying ? (player.team_id ?? offer.from_team_id) : offer.from_team_id,
        fee: offer.fee,
        wageSharePct: null,
        status: offer.status,
        date: offer.closed_on ?? offer.date,
        registrationDate: offer.registration_date ?? null,
        failureReason: offer.registration_failure_reason ?? null,
        loanEndDate: null,
      },
    ];
  });
}

function loanEntries(player: PlayerData, userTeamId: string): ActivityEntry[] {
  const hasActiveLoan = player.active_loan != null;
  return (player.loan_offers ?? []).flatMap((offer: LoanOfferData) => {
    const borrowing = offer.from_team_id === userTeamId;
    const lending = offer.parent_team_id === userTeamId;
    if (!borrowing && !lending) return [];
    // An accepted loan is shown once, from the player's own active-loan record.
    if (offer.status === "Accepted" && hasActiveLoan) return [];
    return [
      {
        key: offer.id,
        kind: "loan" as const,
        direction: borrowing ? ("buying" as const) : ("selling" as const),
        playerId: player.id,
        playerName: player.full_name,
        counterpartyTeamId: borrowing ? offer.parent_team_id : offer.from_team_id,
        fee: offer.buy_option_fee ?? null,
        wageSharePct: offer.wage_contribution_pct,
        status: offer.status,
        date: offer.closed_on ?? offer.date,
        registrationDate: offer.status === "PendingRegistration" ? offer.start_date : null,
        failureReason: null,
        loanEndDate: offer.end_date,
      },
    ];
  });
}

function activeLoanEntry(player: PlayerData, userTeamId: string): ActivityEntry[] {
  const loan = player.active_loan;
  if (!loan) return [];
  const borrowing = loan.loan_team_id === userTeamId;
  if (!borrowing && loan.parent_team_id !== userTeamId) return [];
  return [
    {
      key: `active-${player.id}`,
      kind: "loan",
      direction: borrowing ? "buying" : "selling",
      playerId: player.id,
      playerName: player.full_name,
      counterpartyTeamId: borrowing ? loan.parent_team_id : loan.loan_team_id,
      fee: loan.buy_option_fee ?? null,
      wageSharePct: loan.wage_contribution_pct,
      status: "Accepted",
      date: loan.start_date,
      registrationDate: null,
      failureReason: null,
      loanEndDate: loan.end_date,
    },
  ];
}

/**
 * The manager's transfers and loans as the game has them stored, sorted into the stages he asks
 * about: what waits for his answer, what is under negotiation, what is agreed and waits to
 * register, what has completed, which loans are running, and what fell through. Nothing here is
 * decided: every entry carries the status the backend stored.
 */
export function buildTransferActivity(
  players: PlayerData[],
  userTeamId: string,
  today: string,
): TransferActivity {
  const activity = emptyActivity();
  for (const player of players) {
    const entries = [
      ...transferEntries(player, userTeamId),
      ...loanEntries(player, userTeamId),
      ...activeLoanEntry(player, userTeamId),
    ];
    for (const entry of entries) {
      const section: ActivitySection = entry.key.startsWith("active-")
        ? "activeLoans"
        : sectionFor(entry.status, entry.direction);
      if (section === "closed" && daysBetween(entry.date, today) > CLOSED_WINDOW_DAYS) continue;
      activity[section].push(entry);
    }
  }
  for (const section of Object.keys(activity) as ActivitySection[]) {
    activity[section].sort((a, b) => b.date.localeCompare(a.date) || a.key.localeCompare(b.key));
  }
  activity.closed = activity.closed.slice(0, MAX_CLOSED_ENTRIES);
  return activity;
}
