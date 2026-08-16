import { FeatureWindow } from "../shared/FeatureWindow";
import type { SurfaceModule } from "../types";

export const financeSurface: SurfaceModule = {
  id: "finance",
  label: "Finance",
  glyph: "$",
  caption: "finance window",
  Component: () => (
    <FeatureWindow
      id="finance"
      pending="Read model arrives with the L3 connector plane. Heiwa never moves money."
    />
  ),
  preview: () => ({
    title: "Finance",
    lines: ["read model pending", "writes approval-gated"],
  }),
};
