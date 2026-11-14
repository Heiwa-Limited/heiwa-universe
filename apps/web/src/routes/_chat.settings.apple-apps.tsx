import { createFileRoute } from "@tanstack/react-router";
import { AppleAppsSettings } from "../components/settings/AppleAppsSettings";
export const Route = createFileRoute("/_chat/settings/apple-apps")({
  component: AppleAppsSettings,
});
