import { Building2 } from "lucide-react";
import type { TOptions } from "i18next";

import { Card, CardBody } from "../ui";
import PlayerProfileOwnerToClub from "./PlayerProfileOwnerToClub";

type TranslateFn = (key: string, options?: TOptions) => string;

interface PlayerProfileOutsideOwnerBannerProps {
  ownerName: string;
  teamName: string;
  t: TranslateFn;
}

/**
 * For a player the data has owned by a club outside the simulated leagues: the owner is named as the
 * data states it, but he is the borrower's player in the game and no return is scheduled.
 */
export default function PlayerProfileOutsideOwnerBanner({
  ownerName,
  teamName,
  t,
}: PlayerProfileOutsideOwnerBannerProps) {
  return (
    <Card accent="primary" className="mb-5">
      <CardBody>
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary-500/10">
            <Building2 className="h-5 w-5 text-primary-500" />
          </div>
          <PlayerProfileOwnerToClub ownerName={ownerName} clubName={teamName} t={t} />
        </div>
        <p className="mt-3 text-body text-fg-muted">
          {t("playerProfile.outsideOwnerNote", { owner: ownerName, team: teamName })}
        </p>
      </CardBody>
    </Card>
  );
}
