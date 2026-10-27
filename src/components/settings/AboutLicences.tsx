import { useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { formatAppVersion } from "../../lib/appVersion";
import { PRODUCT_NAME, UPSTREAM_PROJECT_NAME, UPSTREAM_REPO_URL } from "../../lib/product";
import licenceText from "../../../LICENSE.md?raw";

/**
 * The About screen and the open-source notices the GPL-3.0 asks us to keep visible: what this
 * program is, what it is based on, that it is modified, that it carries no warranty, where the
 * upstream source is, and the full licence text.
 */
export function AboutLicences() {
  const { t } = useTranslation();
  const [showLicence, setShowLicence] = useState(false);
  const paragraph = "text-xs text-gray-600 dark:text-gray-300 leading-relaxed";

  return (
    <div className="flex flex-col gap-3">
      <div>
        <p className="text-sm font-medium text-gray-800 dark:text-gray-200">{PRODUCT_NAME}</p>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{formatAppVersion()}</p>
      </div>
      <p className={paragraph}>{t("about.summary")}</p>
      <p className={paragraph}>{t("about.basedOn", { upstream: UPSTREAM_PROJECT_NAME })}</p>
      <p className={paragraph}>{t("about.modifications")}</p>
      <p className={paragraph}>{t("about.noWarranty")}</p>
      <p className={paragraph}>{t("about.thirdParty")}</p>
      <p className={paragraph}>{t("about.fm24Notice")}</p>
      <p className={paragraph}>{t("about.privacy")}</p>
      <div className="flex gap-2 flex-wrap">
        <button
          type="button"
          onClick={() => {
            void openUrl(UPSTREAM_REPO_URL);
          }}
          className="px-3 py-1.5 rounded-lg bg-primary-500/10 text-primary-600 dark:text-primary-400 hover:bg-primary-500/20 text-xs font-heading font-bold uppercase tracking-wider transition-colors"
        >
          {t("about.upstreamSource")}
        </button>
        <button
          type="button"
          aria-expanded={showLicence}
          onClick={() => setShowLicence((shown) => !shown)}
          className="px-3 py-1.5 rounded-lg bg-gray-100 dark:bg-navy-700 text-gray-700 dark:text-gray-200 hover:bg-gray-200 dark:hover:bg-navy-600 text-xs font-heading font-bold uppercase tracking-wider transition-colors"
        >
          {showLicence ? t("about.hideLicence") : t("about.showLicence")}
        </button>
      </div>
      {showLicence && (
        <pre className="max-h-72 overflow-auto rounded-lg bg-gray-50 dark:bg-navy-900 p-3 text-[11px] text-gray-700 dark:text-gray-300 whitespace-pre-wrap">
          {licenceText}
        </pre>
      )}
    </div>
  );
}
