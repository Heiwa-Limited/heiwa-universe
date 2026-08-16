import { aiSurface } from "./ai";
import { browserSurface } from "./browser";
import { calendarSurface } from "./calendar";
import { filesSurface } from "./files";
import { financeSurface } from "./finance";
import { homeSurface } from "./home";
import { mailSurface } from "./mail";
import { socialSurface } from "./social";
import { windowsSurface } from "./windows";
import { workersSurface } from "./workers";
import { SURFACE_IDS, type SurfaceId } from "./ids";
import type { SurfaceModule } from "./types";

/**
 * The shell's only knowledge of what surfaces exist. Rail order is this
 * order. Adding a surface is: create the module, add it here.
 */
export const SURFACES: SurfaceModule[] = [
  homeSurface,
  aiSurface,
  windowsSurface,
  calendarSurface,
  mailSurface,
  financeSurface,
  socialSurface,
  workersSurface,
  browserSurface,
  filesSurface,
];

const BY_ID = new Map<SurfaceId, SurfaceModule>(
  SURFACES.map((surface) => [surface.id, surface]),
);

export function surfaceById(id: SurfaceId): SurfaceModule {
  const surface = BY_ID.get(id);
  if (!surface) throw new Error(`unknown surface: ${id}`);
  return surface;
}

/** Guards against the registry drifting out of sync with the id list. */
export function assertRegistryComplete(): void {
  const missing = SURFACE_IDS.filter((id) => !BY_ID.has(id));
  if (missing.length) throw new Error(`surface registry missing: ${missing.join(", ")}`);
}
