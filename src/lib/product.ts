/**
 * What this build is, in one place.
 *
 * AP Football Manager is a modified copy of OpenFoot Manager (GPL-3.0). The upstream names live
 * here, next to the product's own, so the About screen can credit them without a string scattered
 * through the UI.
 */
export const PRODUCT_NAME = "AP Football Manager";

/** The id the FM24 world carries (`metadata.world_id`), which the world screen recognises it by. */
export const FM24_WORLD_ID = "apfm-fm24-2026";

/** The project this one is based on; credited, never contacted. */
export const UPSTREAM_PROJECT_NAME = "OpenFoot Manager";
export const UPSTREAM_REPO_URL = "https://github.com/openfootmanager/openfootmanager";

/**
 * Whether the bug-report screens are offered. They post logs and saves to, or open issues with,
 * the upstream project; AP Football Manager sends nothing to anyone, so they stay off.
 */
export const UPSTREAM_BUG_REPORT_ENABLED = false;
