/** Public surface of the orchestrator. */

export type {
  BatchGate,
  GateBand,
  GateDecision,
  GateReason,
  GateThresholds,
  QuestionPolicy,
} from "./gate.ts";
export {
  ambiguityMargin,
  DEFAULT_THRESHOLDS,
  gateAnswer,
  gateBatch,
  noulThresholds,
} from "./gate.ts";
export type {
  DecisionContext,
  PipelineOptions,
  PipelineOutcome,
  PipelineRoute,
  QuarantineRecord,
} from "./pipeline.ts";
export { Pipeline } from "./pipeline.ts";
export type {
  ChunkFailure,
  SpeculativeInput,
  SpeculativeResult,
} from "./speculative.ts";
export { chunkQuestions, speculate } from "./speculative.ts";
