import * as Schema from "effect/Schema";

/**
 * Decisions: questions only the owner can answer, prepared on this Mac by a local
 * engine (`~/heiwa-root/asks.py`) from Messages, Mail, Calendar, Reminders and
 * holdings. Each one is an A2UI v1.0 surface, so every client renders the same
 * card. Answers that would change an Apple app become Heiwa Apple-change proposals.
 */

/** A2UI v1.0 basic-catalog component: an id, a component type, and its properties. */
export const A2uiComponent = Schema.Record(Schema.String, Schema.Unknown);
export type A2uiComponent = typeof A2uiComponent.Type;

/** A2UI v1.0 `createSurface` message (https://a2ui.org/specification/v1.0-a2ui/). */
export const A2uiSurface = Schema.Struct({
  version: Schema.Literal("1.0"),
  createSurface: Schema.Struct({
    surfaceId: Schema.String.check(Schema.isMaxLength(200)),
    catalogId: Schema.optional(Schema.String.check(Schema.isMaxLength(500))),
    components: Schema.Array(A2uiComponent).check(Schema.isMaxLength(200)),
  }),
});
export type A2uiSurface = typeof A2uiSurface.Type;

const AskId = Schema.String.check(Schema.isPattern(/^[0-9a-f]{8}$/));

export const Decision = Schema.Struct({
  id: AskId,
  kind: Schema.String.check(Schema.isMaxLength(64)),
  title: Schema.String.check(Schema.isMaxLength(500)),
  surface: A2uiSurface,
});
export type Decision = typeof Decision.Type;

export const DecisionsStatus = Schema.Struct({
  /** False when the local engine is not running on this Mac; `detail` says why. */
  available: Schema.Boolean,
  open: Schema.Array(Decision),
  /** Answers whose real-world change still waits for approval in the engine. */
  pendingApproval: Schema.Number,
  detail: Schema.NullOr(Schema.String),
});
export type DecisionsStatus = typeof DecisionsStatus.Type;

/** The A2UI `answer` event a Button sends: its context carries askId, choice and signature. */
export const DecisionAnswerInput = Schema.Struct({
  askId: AskId,
  choice: Schema.String.check(Schema.isPattern(/^[a-z_]{1,40}$/)),
  sig: Schema.String.check(Schema.isPattern(/^[0-9a-f]{24}$/)),
});
export type DecisionAnswerInput = typeof DecisionAnswerInput.Type;

export const DecisionAnswerResult = Schema.Struct({
  ok: Schema.Boolean,
  detail: Schema.String,
});
export type DecisionAnswerResult = typeof DecisionAnswerResult.Type;
