import { providersFromSnapshot, runtimeVersion } from "../runtime";
import { useApp } from "../state/app";

/**
 * The always-present input. Submitting from any surface routes the turn and
 * switches to AI, which is where the answer streams in.
 */
export function Composer(props: { caption: string }) {
  const app = useApp();
  let input: HTMLTextAreaElement | undefined;

  const connectedProviders = () =>
    providersFromSnapshot(app.runtime.health()).filter(
      (provider) => provider.status === "connected",
    ).length;

  const send = async () => {
    const text = input?.value.trim() ?? "";
    if (!text || !app.operator.ready()) return;
    if (input) {
      input.value = "";
      input.style.height = "auto";
    }
    app.navigate("ai");
    await app.operator.submit(text).catch(() => undefined);
  };

  return (
    <div class="composer-area">
      <div class="composer-wrap">
        <textarea
          ref={input}
          rows="1"
          placeholder="Message Heiwa…"
          aria-label="Message Heiwa"
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void send();
            }
          }}
          onInput={(event) => {
            const el = event.currentTarget;
            el.style.height = "auto";
            el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
          }}
        />
        <button
          class="composer-send"
          disabled={!app.operator.ready()}
          aria-label="Send"
          onClick={() => void send()}
        >
          {app.operator.ready() ? "↑" : "…"}
        </button>
      </div>
      <div class="composer-hint">
        <span class="hint">{props.caption}</span>
        <span class="hint">
          {runtimeVersion(app.runtime.health())} · {connectedProviders()} providers
        </span>
      </div>
    </div>
  );
}
