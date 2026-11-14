import { createFileRoute } from "@tanstack/react-router";
import { StandalonePage, StandalonePageHeader } from "../components/ui/standalone-page";
function PrivacyPage() {
  return (
    <StandalonePage>
      <StandalonePageHeader
        title="Privacy in this build"
        description="Heiwa keeps its workspace on your computer and connects services when you use them."
      />
      <div className="mt-6 space-y-4 text-sm leading-relaxed text-muted-foreground">
        <p>
          Conversations, workspace settings, native selections and workflow receipts are stored in
          your Heiwa profile. Apple app access follows macOS permissions and the calendars, lists or
          folders you select.
        </p>
        <p>
          When you ask an AI provider to work with selected content, that provider receives the
          request and tool results it needs. Provider accounts, usage limits and retention rules
          remain governed by that provider. Local inference stays at the local endpoint you
          configure.
        </p>
        <p>
          Product analytics is off by default. This build has no default T3 analytics recipient.
          Local diagnostics may contain operational details; review them before sharing. Optional
          remote environments, sharing and cloud services can transmit data when you enable them.
        </p>
        <p>
          Heiwa is derived from the MIT-licensed T3 Code foundation. Its license and third-party
          notices remain available in About.
        </p>
      </div>
    </StandalonePage>
  );
}
export const Route = createFileRoute("/privacy")({ component: PrivacyPage });
