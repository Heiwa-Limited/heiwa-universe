import { useNavigate } from "@tanstack/react-router";

import { AppRouteModal, AppRouteModalClose } from "../AppRouteModal";
import { SettingsGroup } from "../settings/SettingsGroup";
import { Button } from "../ui/button";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "../ui/empty";
import { ScrollArea } from "../ui/scroll-area";
import { WorkspacePageContainer } from "../WorkspacePageContainer";
import { ProposalOutcomes, ProposalReviewList } from "./NativeProposals";
import { useApprovals } from "./useApprovals";

const THREAD_STATUS_LABEL: Record<string, string> = {
  approval: "Approval needed",
  input: "Input needed",
  request: "Guest request",
};

export function ApprovalsPage() {
  const { native, threads, count } = useApprovals();
  const navigate = useNavigate();
  const severalMacs = native.filter((source) => source.pending.length > 0).length > 1;
  const outcomes = native.filter((source) => source.recent.length > 0);
  return (
    <AppRouteModal label="Approvals">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex min-h-12 shrink-0 items-center gap-1 border-b ps-5 pe-2">
          <h1 className="min-w-0 flex-1 truncate text-sm font-medium">Approvals</h1>
          <AppRouteModalClose />
        </header>
        <ScrollArea className="min-h-0 flex-1">
          <WorkspacePageContainer>
            {count === 0 ? (
              <Empty>
                <EmptyHeader>
                  <EmptyTitle>Nothing is waiting for you</EmptyTitle>
                  <EmptyDescription>
                    Apple changes an agent prepares, and threads that need an answer, appear here.
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : null}
            {native.map((source) =>
              source.pending.length === 0 ? null : (
                <section key={source.environmentId}>
                  <h2 className="text-sm font-semibold">
                    Apple changes{severalMacs ? ` on ${source.label}` : ""}
                  </h2>
                  <p className="mt-1 mb-4 text-sm text-muted-foreground">
                    Approval applies only to the exact change shown. If the item changed since, ask
                    for a fresh proposal.
                  </p>
                  <ProposalReviewList
                    environmentId={source.environmentId}
                    proposals={source.pending}
                  />
                </section>
              ),
            )}
            {threads.length > 0 ? (
              <section>
                <h2 className="mb-4 text-sm font-semibold">Threads</h2>
                <SettingsGroup>
                  {threads.map(({ thread, status }) => (
                    <div
                      key={`${thread.environmentId}:${thread.id}`}
                      className="flex items-center justify-between gap-4 p-4"
                    >
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium">{thread.title}</p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {THREAD_STATUS_LABEL[status]}
                        </p>
                      </div>
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() =>
                          void navigate({
                            to: "/$environmentId/$threadId",
                            params: { environmentId: thread.environmentId, threadId: thread.id },
                          })
                        }
                      >
                        Open thread
                      </Button>
                    </div>
                  ))}
                </SettingsGroup>
              </section>
            ) : null}
            {outcomes.length > 0 ? (
              <section>
                <h2 className="text-sm font-semibold">Recent outcomes</h2>
                {outcomes.map((source) => (
                  <ProposalOutcomes key={source.environmentId} proposals={source.recent} />
                ))}
              </section>
            ) : null}
          </WorkspacePageContainer>
        </ScrollArea>
      </div>
    </AppRouteModal>
  );
}
