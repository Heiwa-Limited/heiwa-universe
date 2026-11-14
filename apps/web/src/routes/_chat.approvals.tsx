import { createFileRoute } from "@tanstack/react-router";

import { ApprovalsPage } from "../components/approvals/ApprovalsPage";

export const Route = createFileRoute("/_chat/approvals")({
  component: ApprovalsPage,
});
