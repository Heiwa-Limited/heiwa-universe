/**
 * The System 1 failure taxonomy.
 *
 * Nothing in this engine throws across a module boundary on a runtime
 * condition. Every failure that can be caused by the network, the provider, or
 * a malformed response becomes one of these values, returned inside a
 * `Result`. Callers cannot forget to handle them, because the success value is
 * unreachable until the union is narrowed.
 *
 * Throwing is reserved for *programming* errors caught at construction time —
 * a Choice with one option, a blank instruction — which are bugs in the
 * caller's own stack frame, not conditions to be routed on.
 */

export type System1ErrorKind =
  /** The provider returned a body we cannot trust: bad JSON, wrong shape, or
   *  an answer that violates the question contract we sent. */
  | "schema_violation"
  /** The call exceeded the configured latency budget and was aborted. */
  | "timeout"
  /** Auth rejected (HTTP 401/403). Not retryable without operator action. */
  | "unauthorized"
  /** Rate limited (HTTP 429). Retryable with backoff. */
  | "rate_limited"
  /** Provider overloaded (HTTP 529) or 5xx. Retryable with backoff. */
  | "provider_unavailable"
  /** Request rejected as invalid by the provider (HTTP 422). */
  | "invalid_request"
  /** Socket-level failure: DNS, TLS, connection reset. Retryable. */
  | "transport"
  /** A configured adapter is missing credentials or is not wired. */
  | "not_configured";

export type System1Error = {
  readonly kind: System1ErrorKind;
  readonly message: string;
  /** True when a retry with backoff could plausibly succeed. */
  readonly retryable: boolean;
  /** HTTP status, when the failure came from a response. */
  readonly status?: number;
  /** Provider/adapter that produced the failure, for telemetry attribution. */
  readonly adapter?: string;
  /** Underlying cause, preserved for logs but never rethrown. */
  readonly cause?: unknown;
};

const RETRYABLE: ReadonlySet<System1ErrorKind> = new Set([
  "timeout",
  "rate_limited",
  "provider_unavailable",
  "transport",
]);

export function system1Error(
  kind: System1ErrorKind,
  message: string,
  extra: { status?: number; adapter?: string; cause?: unknown } = {},
): System1Error {
  return { kind, message, retryable: RETRYABLE.has(kind), ...extra };
}

/** Map an HTTP status to the taxonomy, per the documented Jev error codes. */
export function errorKindForStatus(status: number): System1ErrorKind {
  if (status === 401 || status === 403) return "unauthorized";
  if (status === 422) return "invalid_request";
  if (status === 429) return "rate_limited";
  if (status === 529 || status >= 500) return "provider_unavailable";
  return "invalid_request";
}

/** A fallible outcome. The only way to reach `value` is to prove `ok`. */
export type Result<T, E = System1Error> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

export function ok<T>(value: T): Result<T, never> {
  return { ok: true, value };
}

export function err<E>(error: E): Result<never, E> {
  return { ok: false, error };
}
