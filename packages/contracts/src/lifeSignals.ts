import * as Schema from "effect/Schema";

/**
 * Life signals: the owner's local day as metadata (names, ages, counts, on-device triage labels),
 * never message bodies. Served by this Mac's feeder (`~/heiwa-root/life.py`, loopback only) and
 * read by agents only after the owner turns `lifeSignalsEnabled` on.
 */
const Name = Schema.String.check(Schema.isMaxLength(200));
const Hours = Schema.Number;
const Count = Schema.Int;

export const LifeSignalsInput = Schema.Struct({
  view: Schema.Literals(["loops", "day"]),
  date: Schema.optional(Schema.String.check(Schema.isPattern(/^\d{4}-\d{2}-\d{2}$/))),
});
export type LifeSignalsInput = typeof LifeSignalsInput.Type;

export const LifeLoops = Schema.Struct({
  view: Schema.Literal("loops"),
  asOf: Count,
  bodies: Schema.Literal(false),
  messages: Schema.Array(
    Schema.Struct({ who: Name, ageHours: Hours, triage: Schema.NullOr(Name) }),
  ).check(Schema.isMaxLength(25)),
  mail: Schema.Array(
    Schema.Struct({
      who: Name,
      subject: Schema.String.check(Schema.isMaxLength(120)),
      ageHours: Hours,
      flagged: Schema.Boolean,
    }),
  ).check(Schema.isMaxLength(25)),
});

const Tally = <K extends string>(key: K) =>
  Schema.Array(
    Schema.Struct({ [key]: Name, count: Count } as { [P in K]: typeof Name } & {
      count: typeof Count;
    }),
  ).check(Schema.isMaxLength(10));

export const LifeDay = Schema.Struct({
  view: Schema.Literal("day"),
  date: Schema.String,
  asOf: Count,
  bodies: Schema.Literal(false),
  messages: Schema.Struct({ in: Count, out: Count, topSenders: Tally("who") }),
  notifications: Tally("app"),
  focus: Schema.Array(Schema.Struct({ app: Name, hours: Hours })).check(Schema.isMaxLength(10)),
  ai: Schema.Array(
    Schema.Struct({ provider: Name, input: Count, output: Count, calls: Count }),
  ).check(Schema.isMaxLength(10)),
  openDecisions: Count,
  collectorAgeMinutes: Schema.NullOr(Count),
});

export const LifeSignalsResult = Schema.Union([LifeLoops, LifeDay]);
export type LifeSignalsResult = typeof LifeSignalsResult.Type;
