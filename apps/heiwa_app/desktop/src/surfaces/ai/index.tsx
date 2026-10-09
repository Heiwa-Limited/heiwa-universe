import { providerPresence, providersFromSnapshot } from "../../runtime";
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
    const presence = providerPresence(providersFromSnapshot(app.runtime.health()));
    const routing =
      app.operator.status() === "submitting" ||
      snapshot.turns.some((turn) => turn.status === "running");
    return {
      title: "AI Console",
      lines: [
        `${snapshot.messages.length} messages`,
        `${presence.connected} providers present, ${presence.withRuns} with recorded runs`,
        routing ? "route in flight" : app.operator.status(),
      ],
    };
  },
};
