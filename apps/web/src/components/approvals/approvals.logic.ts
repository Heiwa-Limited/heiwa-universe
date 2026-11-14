import type { NativeAction, NativeProposal, NativeSource } from "@t3tools/contracts";

const HEADINGS: Record<NativeAction["app"], Partial<Record<NativeAction["operation"], string>>> = {
  calendar: {
    create: "New calendar event",
    update: "Change calendar event",
    trash: "Delete calendar event",
  },
  reminders: { create: "New reminder", update: "Change reminder", trash: "Delete reminder" },
  notes: { create: "New note", update: "Change note", append: "Add to note", trash: "Delete note" },
  mail: { create: "Mail draft", trash: "Move mail to Trash" },
};

/** What the change does, in the words a person would use for it. */
export function proposalHeading(action: NativeAction): string {
  if (action.operation === "complete") {
    if (action.app === "mail") return action.completed ? "Mark mail read" : "Mark mail unread";
    return action.completed ? "Complete reminder" : "Reopen reminder";
  }
  return HEADINGS[action.app][action.operation] ?? `${action.operation} in ${action.app}`;
}

export function proposalSummary({ action, before }: NativeProposal): string {
  const subject = action.title ?? before?.title;
  return subject === undefined ? proposalHeading(action) : `${proposalHeading(action)}: ${subject}`;
}

/** The event's time as the user's calendar would show it. All-day events end at the next midnight. */
export function proposalWhen(
  action: Pick<NativeAction, "start" | "end" | "allDay">,
  options: { readonly locale?: string; readonly timeZone?: string; readonly hour12?: boolean } = {},
): string | null {
  const start = action.start === undefined ? Number.NaN : Date.parse(action.start);
  const end = action.end === undefined ? Number.NaN : Date.parse(action.end);
  if (Number.isNaN(start) && Number.isNaN(end)) return null;
  const format = new Intl.DateTimeFormat(options.locale, {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
    ...(action.allDay ? {} : { hour: "numeric", minute: "2-digit", hour12: options.hour12 }),
    timeZone: options.timeZone,
  });
  if (Number.isNaN(start) || Number.isNaN(end))
    return format.format(Number.isNaN(start) ? end : start);
  return format.formatRange(start, action.allDay ? Math.max(start, end - 1) : end);
}

export interface ProposalGroup {
  readonly key: string;
  readonly source: NativeSource | null;
  readonly proposals: ReadonlyArray<NativeProposal>;
}

/** Changes that answer the same item are reviewed together; the rest stand alone. */
export function groupBySource(
  proposals: ReadonlyArray<NativeProposal>,
): ReadonlyArray<ProposalGroup> {
  const groups = new Map<string, { source: NativeSource | null; proposals: NativeProposal[] }>();
  for (const proposal of proposals) {
    const { source } = proposal;
    const key = source === null ? proposal.id : `${source.app}\n${source.resourceId}\n${source.id}`;
    const group = groups.get(key);
    if (group) group.proposals.push(proposal);
    else groups.set(key, { source, proposals: [proposal] });
  }
  return [...groups].map(([key, group]) => ({ key, ...group }));
}
