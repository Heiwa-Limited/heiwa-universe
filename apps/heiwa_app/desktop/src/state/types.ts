/** Shared shapes for data the runtime serves to more than one surface. */

export type CalendarEvent = {
  id?: string;
  title?: string;
  start?: string;
  end?: string;
  date?: string;
  kind?: string;
  status?: string;
  source?: string;
  note?: string;
};

export type InboxItem = {
  title?: string;
  detail?: string;
  occurred_at?: string;
  kind?: string;
  source?: string;
  [key: string]: unknown;
};

/** Per-surface descriptor shown on Home and the feature windows. */
export type SubApp = {
  id: string;
  title: string;
  agent: string;
  server: string;
  state: string;
  pinnedPane: string;
  skills: string[];
  tools: string[];
  personalization: string[];
};
