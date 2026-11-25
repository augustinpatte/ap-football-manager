import type { GameStateData } from "./types";

type Stamp = Pick<GameStateData, "career_token" | "state_revision">;

/**
 * True when `incoming` is an older picture of the game than the one `shown`, and must not replace it.
 *
 * The backend stamps every game it sends with the career it belongs to (`career_token`, never
 * reused inside a process and increasing with each career installed) and the revision of the state
 * it shows (`state_revision`, advanced under the game lock on every write). Two answers can leave
 * the backend in one order and reach the screen in the other; the stamps say which is newer.
 *
 * - A different career is ordered by its token alone: revisions of two careers are never compared
 *   (a new career restarts its count), and a lower token is a career that has been replaced.
 * - Inside one career a lower revision is older; the same revision is the same state (or the
 *   screen's own edit of it) and is accepted.
 * - A game with no stamp (an older backend, a hand-built fixture) carries no order and is accepted.
 */
export function isStaleSnapshot(shown: Stamp | null | undefined, incoming: Stamp): boolean {
  if (!shown) return false;
  const { career_token: shownCareer, state_revision: shownRevision } = shown;
  const { career_token: incomingCareer, state_revision: incomingRevision } = incoming;
  if (shownCareer === undefined || incomingCareer === undefined) return false;
  if (incomingCareer !== shownCareer) return incomingCareer < shownCareer;
  if (shownRevision === undefined || incomingRevision === undefined) return false;
  return incomingRevision < shownRevision;
}
