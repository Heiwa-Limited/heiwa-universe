import { FeatureWindow } from "../shared/FeatureWindow";
import type { SurfaceModule } from "../types";

export const mailSurface: SurfaceModule = {
  id: "mail",
  label: "Mail",
  glyph: "✉",
  caption: "mail window",
  Component: () => (
    <FeatureWindow
      id="mail"
      pending="Reads land on the L3 connector plane. Sends stay approval-gated with a receipt."
    />
  ),
  preview: (app) => ({
    title: "Mail",
    lines: [`${app.runtime.inbox().length} inbox items`, "draft/write gated"],
  }),
  refresh: (app) => app.runtime.loadInbox(),
};
