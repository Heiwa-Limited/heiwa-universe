/**
 * Try adapters in order until one answers.
 *
 * Failover is at the *adapter* level rather than inside the client because
 * the fallback should inherit the same budget, the same decoder, and the same
 * telemetry attribution. From the client's point of view a failover chain is
 * one adapter that happens to have several ways to answer.
 *
 * ## Which failures are worth falling through on
 *
 * By default any failure advances to the next adapter. That is right for
 * outages, throttling, and auth misconfiguration on one provider. It is
 * wrong for `invalid_request`: a question set the provider rejected as
 * malformed will be just as malformed at the next one, so falling through
 * only spends latency and money to collect a second identical rejection.
 * Pass `shouldFailover` to encode that, and note the default is deliberately
 * the permissive one — an operator who wires a fallback usually wants it used.
 *
 * An aborted budget never falls through: there is no time left to spend.
 */

import {
  err,
  type Result,
  type System1Error,
  system1Error,
} from "../errors.ts";
import type { Adapter, AdapterRequest } from "./types.ts";

export type FailoverOptions = {
  /** Return false to stop the chain and surface this error. */
  shouldFailover?(error: System1Error, adapter: Adapter): boolean;
  /** Called for each adapter that is skipped past, for observability. */
  onFailover?(error: System1Error, from: Adapter, to: Adapter): void;
};

export function failover(
  adapters: readonly Adapter[],
  options: FailoverOptions = {},
): Adapter {
  const name =
    adapters.length > 0
      ? `failover(${adapters.map((a) => a.name).join("->")})`
      : "failover(empty)";

  return {
    name,
    async evaluate(
      req: AdapterRequest,
      signal: AbortSignal,
    ): Promise<Result<unknown>> {
      if (adapters.length === 0) {
        return err(
          system1Error("not_configured", "failover() was given no adapters", {
            adapter: name,
          }),
        );
      }

      let last: System1Error = system1Error(
        "not_configured",
        "no adapter was tried",
        { adapter: name },
      );

      for (let i = 0; i < adapters.length; i++) {
        const adapter = adapters[i];
        if (!adapter) continue;

        if (signal.aborted) {
          return err(
            system1Error(
              "timeout",
              "latency budget expired before the failover chain finished",
              {
                adapter: name,
              },
            ),
          );
        }

        let result: Result<unknown>;
        try {
          result = await adapter.evaluate(req, signal);
        } catch (cause) {
          result = err(
            system1Error(
              "transport",
              `adapter "${adapter.name}" threw: ${String(cause)}`,
              {
                adapter: adapter.name,
                cause,
              },
            ),
          );
        }

        if (result.ok) return result;
        last = result.error;

        const next = adapters[i + 1];
        if (!next) break;
        if (options.shouldFailover && !options.shouldFailover(last, adapter))
          break;
        options.onFailover?.(last, adapter, next);
      }

      return err(last);
    },
  };
}
