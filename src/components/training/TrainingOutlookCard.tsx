import { useTranslation } from "react-i18next";

import type { TrainingOutlookData } from "../../services/trainingService";
import { Card, CardBody, CardHeader } from "../ui";
import { formatEffect } from "./trainingOutlook.helpers";

interface TrainingOutlookCardProps {
  outlook: TrainingOutlookData;
  /** The intensity stored on the Training tab, to say what a taper eased it from. */
  plannedIntensity: string;
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-3 text-body">
      <dt className="text-fg-muted">{label}</dt>
      <dd className="font-heading font-bold tabular-nums text-fg">{value}</dd>
    </div>
  );
}

/**
 * What the training will really do: today's session after the taper, the week's fixtures, what
 * the coaching staff and the medical department are worth, and who sets the plan. All of it is
 * the backend's reading; nothing here decides a rule.
 */
export default function TrainingOutlookCard({
  outlook,
  plannedIntensity,
}: TrainingOutlookCardProps) {
  const { t } = useTranslation();
  const intensity = t(`training.intensities.${outlook.session_intensity}.label`);

  return (
    <Card>
      <CardHeader>{t("training.outlook.title")}</CardHeader>
      <CardBody>
        <div className="flex flex-col gap-3">
          <p className="text-body text-fg">
            {outlook.is_training_day
              ? t("training.outlook.session", { intensity })
              : t("training.outlook.restDay")}
          </p>
          {outlook.is_training_day && outlook.taper === "nearMatch" ? (
            <p className="text-caption text-warning">
              {t("training.outlook.taperNearMatch", {
                planned: t(`training.intensities.${plannedIntensity}.label`),
              })}
            </p>
          ) : null}
          {outlook.is_training_day && outlook.taper === "congested" ? (
            <p className="text-caption text-warning">
              {t("training.outlook.taperCongested", { count: outlook.fixtures_this_week })}
            </p>
          ) : null}
          <dl className="flex flex-col gap-1">
            <Row
              label={t("training.outlook.fixtures")}
              value={String(outlook.fixtures_this_week)}
            />
            <Row
              label={t("training.outlook.starters")}
              value={`${Math.round(outlook.starters_condition)}%`}
            />
            <Row
              label={t("training.outlook.coaching")}
              value={formatEffect(outlook.coaching_effect)}
            />
            <Row
              label={t("training.outlook.recovery")}
              value={formatEffect(outlook.recovery_effect)}
            />
          </dl>
          <p className="text-caption text-fg-muted">
            {outlook.delegated_to
              ? t("training.outlook.setByStaff", {
                  name: outlook.delegated_to.name,
                  post: t(`staff.posts.${outlook.delegated_to.post}`),
                  rating: outlook.delegated_to.overall,
                })
              : t("training.outlook.setByYou")}
          </p>
          {outlook.proposal_waiting ? (
            <p className="text-caption text-info">{t("training.outlook.proposalWaiting")}</p>
          ) : null}
        </div>
      </CardBody>
    </Card>
  );
}
