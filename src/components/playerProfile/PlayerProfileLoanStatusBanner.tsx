import { ArrowRightLeft, BadgeEuro, CalendarDays, Percent } from "lucide-react";
import type { TOptions } from "i18next";

import { formatDate } from "../../lib/dateFormatting";
import { formatExactMoney } from "../../lib/helpers";
import type { ActiveLoanData, TeamData } from "../../store/gameStore";
import { Card, CardBody } from "../ui";
import { getPlayerTeamName } from "./PlayerProfile.helpers";
import PlayerProfileOwnerToClub from "./PlayerProfileOwnerToClub";

type TranslateFn = (key: string, options?: TOptions) => string;

interface PlayerProfileLoanStatusBannerProps {
  loan: ActiveLoanData;
  teams: TeamData[];
  language: string;
  t: TranslateFn;
}

/** Terms the importer configured, or that nobody recorded the source of, are not contract facts. */
function termsNote(loan: ActiveLoanData): "estimated" | "unrecorded" | null {
  switch (loan.terms_source) {
    case "estimated":
      return "estimated";
    case "agreed":
    case "sourced":
      return null;
    default:
      return "unrecorded";
  }
}

export default function PlayerProfileLoanStatusBanner({
  loan,
  teams,
  language,
  t,
}: PlayerProfileLoanStatusBannerProps) {
  const labels = {
    freeAgent: t("common.freeAgent"),
    unknown: t("common.unknown"),
  };
  const parentTeamName = getPlayerTeamName(teams, loan.parent_team_id, labels);
  const loanTeamName = teams.some((team) => team.id === loan.loan_team_id)
    ? getPlayerTeamName(teams, loan.loan_team_id, labels)
    : (loan.loan_team_name ?? getPlayerTeamName(teams, loan.loan_team_id, labels));
  const note = termsNote(loan);
  const estimated = loan.terms_source === "estimated";

  return (
    <Card accent="primary" className="mb-5">
      <CardBody>
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary-500/10">
              <ArrowRightLeft className="h-5 w-5 text-primary-500" />
            </div>
            <div>
              <p className="font-heading text-xs font-bold uppercase tracking-wider text-gray-500 dark:text-gray-400">
                {t("playerProfile.loanStatus")}
              </p>
              <PlayerProfileOwnerToClub ownerName={parentTeamName} clubName={loanTeamName} t={t} />
            </div>
          </div>
          <div className="grid gap-2 text-xs text-gray-500 dark:text-gray-400 sm:grid-cols-3 sm:text-right">
            <span className="inline-flex items-center gap-1 sm:justify-end">
              <CalendarDays className="h-3.5 w-3.5" />
              {t(estimated ? "playerProfile.loanUntilEstimate" : "playerProfile.loanUntil", {
                date: formatDate(loan.end_date, language),
              })}
            </span>
            <span className="inline-flex items-center gap-1 sm:justify-end">
              <Percent className="h-3.5 w-3.5" />
              {t(
                estimated ? "playerProfile.loanWageSplitEstimate" : "playerProfile.loanWageSplit",
                { percent: loan.wage_contribution_pct },
              )}
            </span>
            {loan.buy_option_fee ? (
              <span className="inline-flex items-center gap-1 sm:justify-end">
                <BadgeEuro className="h-3.5 w-3.5" />
                {t("playerProfile.loanBuyOption", {
                  fee: formatExactMoney(loan.buy_option_fee),
                })}
              </span>
            ) : null}
          </div>
        </div>
        {note ? (
          <p className="mt-3 text-body text-fg-muted">
            {t(
              note === "estimated"
                ? "playerProfile.loanTermsEstimatedNote"
                : "playerProfile.loanTermsUnrecordedNote",
            )}
          </p>
        ) : null}
      </CardBody>
    </Card>
  );
}
