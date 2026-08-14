/**
 * The ten surfaces, in rail order.
 *
 * Ids match the product names in the roadmap. The pre-Solid shell used
 * `chat` for AI and `agents` for Workers; those ids were internal only (no
 * router, no persistence), so they are unified here rather than carried
 * forward as a second vocabulary.
 */
export const SURFACE_IDS = [
  "home",
  "ai",
  "windows",
  "calendar",
  "mail",
  "finance",
  "social",
  "workers",
  "browser",
  "files",
] as const;

export type SurfaceId = (typeof SURFACE_IDS)[number];
