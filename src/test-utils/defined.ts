/**
 * Narrow a value a fixture is known to have, failing the test with a message that names what
 * was missing. Replaces `value!` in tests: the assertion is the same, but a wrong fixture says
 * so instead of throwing "cannot read properties of undefined" further down.
 */
export function defined<T>(value: T | null | undefined, what = "value"): T {
  if (value === null || value === undefined) {
    throw new Error(`Expected ${what} to be defined in this fixture`);
  }
  return value;
}
