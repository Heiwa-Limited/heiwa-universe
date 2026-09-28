/** Public surface of the System 1 core. */

export type { FailoverOptions } from "./adapters/failover.ts";
export { failover } from "./adapters/failover.ts";
export type { HttpAdapterOptions } from "./adapters/http.ts";
export {
  OPENROUTER_BASE_URL,
  OPENROUTER_MODEL,
  OPENROUTER_PATH,
  openRouterAdapter,
  TYPESAFE_BASE_URL,
  TYPESAFE_PATH,
  typesafeAdapter,
} from "./adapters/http.ts";
export type {
  ProbeCode,
  ProbeFinding,
  ProbeOptions,
  ProbeResult,
} from "./adapters/probe.ts";
export { probeModel } from "./adapters/probe.ts";
export type { StructuredLlmOptions } from "./adapters/structured_llm.ts";
export {
  FALLBACK_CONFIDENCE_CEILING,
  questionsToJsonSchema,
  structuredLlmAdapter,
} from "./adapters/structured_llm.ts";
export type {
  Adapter,
  AdapterRequest,
  SystemOneRequestBody,
} from "./adapters/types.ts";
export type {
  EvaluateInput,
  EvaluateOutput,
  System1ClientOptions,
} from "./client.ts";
export {
  DEFAULT_BUDGET_MS,
  DEFAULT_MODEL,
  MAX_QUESTIONS_PER_REQUEST,
  System1Client,
} from "./client.ts";
export type { ConfidenceSource } from "./confidence.ts";
export {
  confidenceOf,
  confidenceSourceOf,
  noulConfidence,
} from "./confidence.ts";
export type { Result, System1Error, System1ErrorKind } from "./errors.ts";
export { err, errorKindForStatus, ok, system1Error } from "./errors.ts";
export type {
  CallTelemetry,
  QuestionTelemetry,
  TelemetrySink,
} from "./observability.ts";
export { memorySink, noopSink } from "./observability.ts";
export type {
  ChoiceQuestion,
  ChoiceWire,
  NoulQuestion,
  NoulWire,
  PrimitiveKind,
  Question,
  QuestionWire,
  ScoreQuestion,
  ScoreWire,
} from "./primitives.ts";
export { choice, MAX_CHOICE_OPTIONS, noul, score } from "./primitives.ts";
export type { DecodedResponse, TokenUsage } from "./schema.ts";
export { decodeResponse } from "./schema.ts";
export type {
  ChoiceAnswer,
  DecodedAnswer,
  NoulAnswer,
  ScoreAnswer,
} from "./types.ts";
