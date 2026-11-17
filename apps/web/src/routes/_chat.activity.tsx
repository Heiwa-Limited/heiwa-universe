import { createFileRoute } from "@tanstack/react-router";

import { ActivityPage } from "../components/activity/ActivityPage";

export const Route = createFileRoute("/_chat/activity")({
  component: ActivityPage,
});
