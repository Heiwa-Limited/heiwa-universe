import { FeatureWindow } from "../shared/FeatureWindow";
import type { SurfaceModule } from "../types";

export const socialSurface: SurfaceModule = {
  id: "social",
  label: "Social",
  glyph: "@",
  caption: "social window",
  Component: () => (
    <FeatureWindow
      id="social"
      pending="Ingress arrives with the L3 connector plane. Sends stay approval-gated."
    />
  ),
  preview: () => ({
    title: "Social",
    lines: ["ingress pending", "sends approval-gated"],
  }),
};
