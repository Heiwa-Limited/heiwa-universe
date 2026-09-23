/**
 * HTTP adapters for the System One wire protocol.
 *
 * Both providers speak the same body — `{state, model, questions}` in,
 * `{model, answers, usage}` out — so they share one implementation and differ
 * only in URL, model id, and headers.
 *
 * ## Provenance of these constants
 *
 * The TypeSafe route (`POST /v1/systemone`, bearer auth, the documented 401 /
 * 422 / 429 / 529 error codes) comes from TypeSafe's own API reference and is
 * safe to rely on.
 *
 * The OpenRouter route (`/api/alpha/decisions`, model id `typesafe/jev-latest`)
 * is assembled from community integrations rather than OpenRouter's own API
 * reference, which was not reachable when this was written. Treat it as
 * UNVERIFIED against a live endpoint: both `path` and `model` are constructor
 * options precisely so a live check can correct them without a code change.
 * The alpha path also implies it may move.
 *
 * ## What these adapters deliberately do not do
 *
 * No validation, no retry, no telemetry, no timing. `client.ts` owns all of
 * that so the behaviour is identical across providers. An adapter's only
 * discretion is classifying an HTTP status into the error taxonomy.
 */

import {
  err,
  errorKindForStatus,
  ok,
  type Result,
  system1Error,
} from "../errors.ts";
import type { Adapter, AdapterRequest } from "./types.ts";

export type HttpAdapterOptions = {
  readonly apiKey: string;
  /** Origin only; the path is appended. Overridable for staging and tests. */
  readonly baseUrl?: string;
  /** Route path. Overridable because the OpenRouter route is alpha. */
  readonly path?: string;
  /** Model id sent in the body, overriding the caller's. */
  readonly model?: string;
  readonly headers?: Readonly<Record<string, string>>;
  /** Injectable for testing; defaults to global fetch. */
  readonly fetchImpl?: typeof fetch;
};

export const TYPESAFE_BASE_URL = "https://api.typesafe.ai";
export const TYPESAFE_PATH = "/v1/systemone";

/** UNVERIFIED against live OpenRouter docs — see the module note above. */
export const OPENROUTER_BASE_URL = "https://openrouter.ai";
export const OPENROUTER_PATH = "/api/alpha/decisions";
export const OPENROUTER_MODEL = "typesafe/jev-latest";

function httpAdapter(
  name: string,
  defaults: Required<Pick<HttpAdapterOptions, "baseUrl" | "path">>,
) {
  return (options: HttpAdapterOptions): Adapter => {
    const baseUrl = options.baseUrl ?? defaults.baseUrl;
    const path = options.path ?? defaults.path;
    const doFetch = options.fetchImpl ?? fetch;
    const url = `${baseUrl.replace(/\/$/, "")}${path}`;

    return {
      name,
      async evaluate(
        req: AdapterRequest,
        signal: AbortSignal,
      ): Promise<Result<unknown>> {
        if (!options.apiKey) {
          return err(
            system1Error(
              "not_configured",
              `adapter "${name}" has no API key configured`,
              {
                adapter: name,
              },
            ),
          );
        }

        // `model` override lets one question set run against a provider that
        // names the same model differently, without rebuilding the body.
        const body = options.model
          ? { ...req.body, model: options.model }
          : req.body;

        let response: Response;
        try {
          response = await doFetch(url, {
            method: "POST",
            signal,
            // Never follow: a 307/308 re-sends this body to an unclassified origin.
            redirect: "error",
            headers: {
              authorization: `Bearer ${options.apiKey}`,
              "content-type": "application/json",
              accept: "application/json",
              ...options.headers,
            },
            body: JSON.stringify(body),
          });
        } catch (cause) {
          // An abort is the caller's budget expiring, not a network fault;
          // classifying it correctly is what lets the gate tell "too slow"
          // apart from "unreachable".
          if (signal.aborted) {
            return err(
              system1Error(
                "timeout",
                `adapter "${name}" aborted on the latency budget`,
                {
                  adapter: name,
                  cause,
                },
              ),
            );
          }
          return err(
            system1Error(
              "transport",
              `adapter "${name}" could not reach ${url}: ${String(cause)}`,
              {
                adapter: name,
                cause,
              },
            ),
          );
        }

        if (!response.ok) {
          const detail = await safeText(response);
          return err(
            system1Error(
              errorKindForStatus(response.status),
              `adapter "${name}" returned HTTP ${response.status}${detail ? `: ${detail}` : ""}`,
              { adapter: name, status: response.status },
            ),
          );
        }

        try {
          return ok(await response.json());
        } catch (cause) {
          // A 200 carrying HTML is a proxy or a captive portal, not an answer.
          return err(
            system1Error(
              "schema_violation",
              `adapter "${name}" returned a non-JSON 200 body`,
              {
                adapter: name,
                status: response.status,
                cause,
              },
            ),
          );
        }
      },
    };
  };
}

/** Native TypeSafe `/v1/systemone`. */
export const typesafeAdapter = httpAdapter("typesafe", {
  baseUrl: TYPESAFE_BASE_URL,
  path: TYPESAFE_PATH,
});

const openRouterBase = httpAdapter("openrouter", {
  baseUrl: OPENROUTER_BASE_URL,
  path: OPENROUTER_PATH,
});

/** Jev via OpenRouter. See the UNVERIFIED note on the route constants. */
export function openRouterAdapter(options: HttpAdapterOptions): Adapter {
  return openRouterBase({ model: OPENROUTER_MODEL, ...options });
}

async function safeText(response: Response): Promise<string> {
  try {
    return (await response.text()).slice(0, 300);
  } catch {
    return "";
  }
}
