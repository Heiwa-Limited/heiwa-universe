import { providersFromSnapshot } from "../../runtime";
import { Conversation } from "../../shell/Conversation";
import type { SurfaceModule } from "../types";

export const aiSurface: SurfaceModule = {
  id: "ai",
  label: "AI",
  glyph: "✦",
  caption: "conversation",
  Component: Conversation,
  preview: (app) => {
    const snapshot = app.operator.snapshot();
    const connected = providersFromSnapshot(app.runtime.health()).filter(
      (provider) => provider.status === "connected",
    ).length;
    const routing =
      app.operator.status() === "submitting" ||
      snapshot.turns.some((turn) => turn.status === "running");
    return {
      title: "AI Console",
      lines: [
        `${snapshot.messages.length} messages`,
        `${connected} connected providers`,
        routing ? "route in flight" : app.operator.status(),
      ],
    };
  },
};
