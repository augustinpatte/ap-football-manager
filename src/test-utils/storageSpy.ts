import { vi } from "vitest";
import type { MockInstance } from "vitest";

/**
 * Observes calls to a method of `window.localStorage`, whichever implementation backs it.
 *
 * Under jsdom, `window.localStorage` is a `Storage` whose properties are its *items*: an own
 * property assigned over `getItem` is stored as an item, so `vi.spyOn(window.localStorage, ...)`
 * never replaces the method and never sees a call. The method lives on `Storage.prototype`, which
 * is where the spy has to go. Where the setup shim backs it (a plain object, no `Storage`
 * prototype in its chain), the instance is the right target.
 */
export function spyOnLocalStorage(method: "getItem" | "setItem" | "removeItem"): MockInstance {
  const backedByStorage = Object.getPrototypeOf(window.localStorage) === Storage.prototype;
  return backedByStorage
    ? vi.spyOn(Storage.prototype, method)
    : vi.spyOn(window.localStorage, method);
}
