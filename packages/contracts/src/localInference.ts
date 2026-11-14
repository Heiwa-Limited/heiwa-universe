import * as Schema from "effect/Schema";
import { ProviderInstanceId } from "./providerInstance.ts";
export const LocalInferenceInput = Schema.Struct({
  endpoint: Schema.optional(Schema.String),
  model: Schema.optional(Schema.String),
});
export const LocalInferenceInventory = Schema.Struct({
  endpoint: Schema.String,
  models: Schema.Array(Schema.String),
});
export const LocalInferenceConnection = Schema.Struct({
  instanceId: ProviderInstanceId,
  endpoint: Schema.String,
  model: Schema.String,
  status: Schema.Literal("configured"),
});
export class LocalInferenceError extends Schema.TaggedError<LocalInferenceError>()(
  "LocalInferenceError",
  { detail: Schema.String },
) {
  override get message() {
    return this.detail;
  }
}

const HeiwaLocalProvision = Schema.Struct({ provision: Schema.Literal("heiwa-local") });
export const isHeiwaLocalProvision = Schema.is(HeiwaLocalProvision);
