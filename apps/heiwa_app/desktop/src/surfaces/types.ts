import type { Component } from "solid-js";
import type { AppState } from "../state/app";
import type { SurfaceId } from "./ids";

/** Rail hover card content. */
export type DockPreview = {
  title: string;
  lines: string[];
};

/**
 * SurfaceModule — the contract every surface implements to mount into the
 * shell (L0 interface).
 *
 * The shell knows only this shape. It never imports a surface's internals,
 * and a surface never imports another surface, so a surface can be replaced
 * without touching the shell or its siblings.
 */
export type SurfaceModule = {
  id: SurfaceId;
  /** Rail label and accessible name. */
  label: string;
  /** Rail glyph. */
  glyph: string;
  /** Caption shown under the composer while this surface is active. */
  caption: string;
  /** The surface itself. Reads shared state through `useApp()`. */
  Component: Component;
  /** Rail hover card. Pure: derives from state, never triggers loads. */
  preview: (app: AppState) => DockPreview;
  /** Data this surface needs before it is shown. Optional. */
  refresh?: (app: AppState) => Promise<void>;
};
