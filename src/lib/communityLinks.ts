/**
 * The project's public links, in one place.
 *
 * These were inline constants in `MainMenu.tsx` while it was the only screen that opened them.
 * The bug report needs the repository too, and a URL that disagrees with itself between two
 * screens is the kind of thing nobody notices until someone lands somewhere unexpected.
 *
 * There is deliberately no Discord invite here: the main menu no longer links to the upstream
 * community (see the distribution guard in `MainMenu.test.tsx`), so a constant for it would be
 * dead code.
 */
export const GITHUB_REPO_URL = "https://github.com/openfootmanager/openfootmanager";
