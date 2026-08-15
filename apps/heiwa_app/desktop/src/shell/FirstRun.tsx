import { createSignal, For, Show } from "solid-js";
import type { OnboardingState } from "../state/types";
import "./first-run.css";

/**
 * First run, over the shell rather than inside it.
 *
 * Onboarding is a gate on the whole application, not a place you navigate
 * to — so it is an overlay, and the rail keeps its ten surfaces. It renders
 * whatever `heiwa_identity::onboarding` reports and never decides readiness
 * itself, which is what keeps this panel and `heiwa setup` from disagreeing.
 *
 * Every gap shows the remedy the projection carries. A first-run screen that
 * says "not ready" without saying what to do is the documentation the user is
 * supposed to no longer need.
 */
export function FirstRun(props: {
  state: OnboardingState;
  onEstablishIdentity: (displayName: string) => void | Promise<void>;
  onRecheck: () => void | Promise<void>;
}) {
  const [name, setName] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const identityGap = () =>
    props.state.gaps.find((gap) => gap.step === "identity");

  const submitName = async (event: Event) => {
    event.preventDefault();
    const value = name().trim();
    if (!value || busy()) return;
    setBusy(true);
    try {
      await props.onEstablishIdentity(value);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="first-run" role="dialog" aria-modal="true" aria-labelledby="first-run-title">
      <section class="first-run-panel">
        <h1 id="first-run-title" class="first-run-title">
          Set up Heiwa
        </h1>
        <p class="first-run-lede">
          Heiwa runs on this machine. Nothing below leaves it.
        </p>

        <Show when={identityGap()}>
          <form class="first-run-identity" onSubmit={submitName}>
            <label class="first-run-label" for="first-run-name">
              What should Heiwa call you?
            </label>
            <div class="first-run-row">
              <input
                id="first-run-name"
                class="first-run-input"
                type="text"
                value={name()}
                onInput={(event) => setName(event.currentTarget.value)}
                placeholder="Your name"
                disabled={busy()}
                autofocus
              />
              <button
                class="first-run-button"
                type="submit"
                disabled={busy() || !name().trim()}
              >
                Continue
              </button>
            </div>
          </form>
        </Show>

        <ol class="first-run-gaps">
          <For each={props.state.gaps}>
            {(gap) => (
              <li class="first-run-gap" data-step={gap.step}>
                <span class="first-run-step">{gap.step.replace("_", " ")}</span>
                {/*
                  The provider gap's detail is the account-health text, one
                  line per account. Preserving the line breaks is why this is
                  a pre rather than a paragraph.
                */}
                <pre class="first-run-detail">{gap.detail}</pre>
                <p class="first-run-remedy">{gap.remedy}</p>
              </li>
            )}
          </For>
        </ol>

        <button
          class="first-run-button first-run-recheck"
          type="button"
          onClick={() => void props.onRecheck()}
          disabled={busy()}
        >
          Check again
        </button>
      </section>
    </div>
  );
}
