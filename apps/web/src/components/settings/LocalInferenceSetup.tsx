import { useState } from "react";
import { AuthProvidersManageScope, type EnvironmentId } from "@t3tools/contracts";
import { CpuIcon } from "lucide-react";
import { connectLocalInference } from "../../state/localInference";
import { useAtomCommand } from "../../state/use-atom-command";
import { readEnvironmentScope } from "../../state/session";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { SettingsGroup } from "./SettingsGroup";
export function LocalInferenceSetup({ environmentId }: { environmentId: EnvironmentId }) {
  const connect = useAtomCommand(connectLocalInference);
  const [endpoint, setEndpoint] = useState("http://127.0.0.1:11434");
  const [busy, setBusy] = useState(false);
  const [connectedModel, setConnectedModel] = useState<string | null>(null);
  const run = async () => {
    if (busy || !readEnvironmentScope(environmentId, AuthProvidersManageScope)) return;
    setBusy(true);
    // React Compiler skips functions with try/finally, so release the busy state on the promise.
    const result = await connect({ environmentId, input: { endpoint } }).finally(() =>
      setBusy(false),
    );
    if (result._tag === "Success") setConnectedModel(result.value.model);
  };
  return (
    <SettingsGroup className="mb-6 p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="flex items-center gap-2 text-sm font-medium">
            <CpuIcon className="size-4" />
            Run on this Mac
          </h3>
          <p className="mt-2 max-w-lg text-sm text-muted-foreground">
            Use installed Ollama models with Heiwa’s file, process and Apple workflow tools. No
            cloud account is required.
          </p>
        </div>
        <Button
          size="sm"
          variant="outline"
          disabled={busy || !readEnvironmentScope(environmentId, AuthProvidersManageScope)}
          onClick={() => void run()}
        >
          {busy ? "Connecting…" : "Connect Ollama"}
        </Button>
      </div>
      <details className="mt-3 text-xs text-muted-foreground">
        <summary className="cursor-pointer">Local endpoint</summary>
        <Input
          className="mt-2"
          aria-label="Local Ollama endpoint"
          value={endpoint}
          onChange={(event) => setEndpoint(event.target.value)}
          disabled={busy}
        />
      </details>
      {connectedModel ? (
        <p role="status" className="mt-3 text-sm">
          Configured {connectedModel}. Choose Ollama · local in a chat to start working.
        </p>
      ) : null}
    </SettingsGroup>
  );
}
