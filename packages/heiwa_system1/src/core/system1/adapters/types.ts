/**
 * The adapter seam.
 *
 * An adapter's only job is to get a JSON body from somewhere and hand it back.
 * It does not validate, decode, gate, time, or retry — the client owns all of
 * that, so those behaviours are identical no matter which provider answered.
 * That is what makes a fallback adapter trustworthy: swapping Jev for a
 * structured-output LLM cannot change the resilience posture, only the
 * latency and the calibration quality.
 *
 * An adapter MAY throw; the client catches it. Returning a typed failure is
 * preferred because it lets the adapter classify the HTTP status correctly.
 */

import type { Result } from "../errors.ts";
import type { QuestionWire } from "../primitives.ts";

/** The `/v1/systemone` request body, exactly as documented by TypeSafe. */
export type SystemOneRequestBody = {
  readonly state: unknown;
  readonly model: string;
  readonly questions: Readonly<Record<string, QuestionWire>>;
};

export type AdapterRequest = {
  readonly body: SystemOneRequestBody;
};

export type Adapter = {
  /** Stable name used for telemetry attribution and fallback logging. */
  readonly name: string;
  /**
   * Resolve to the raw, *unvalidated* response body. The signal carries the
   * client's latency budget; an adapter that ignores it will be abandoned but
   * may keep a socket open, so honouring it is expected.
   */
  evaluate(req: AdapterRequest, signal: AbortSignal): Promise<Result<unknown>>;
};
