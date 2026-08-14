import { createSignal } from "solid-js";
import type { SurfaceModule } from "../types";
import "./browser.css";

const HOME_URL = "https://example.com";

/**
 * Interim browsing surface: an iframe, honest about its limits.
 *
 * L4 replaces this with a runtime-owned Chromium sidecar driven over CDP,
 * where navigation and interaction classify onto RiskTier and pass the DREX
 * gate. Until then this frame is sandboxed and performs no agent actions.
 */
function BrowserSurface() {
  const [url, setUrl] = createSignal(HOME_URL);
  const [pending, setPending] = createSignal(HOME_URL);

  const go = () => {
    const next = pending().trim();
    if (!next) return;
    setUrl(/^https?:\/\//i.test(next) ? next : `https://${next}`);
  };

  return (
    <div class="view browser-view">
      <div class="browser-bar">
        <input
          type="text"
          value={pending()}
          aria-label="Address"
          placeholder="Enter URL…"
          onInput={(event) => setPending(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") go();
          }}
        />
        <button class="btn-primary" onClick={go}>
          Go
        </button>
      </div>
      <div class="browser-frame">
        <iframe
          title="Heiwa browser"
          src={url()}
          sandbox="allow-forms allow-same-origin allow-scripts allow-popups"
        />
      </div>
    </div>
  );
}

export const browserSurface: SurfaceModule = {
  id: "browser",
  label: "Browser",
  glyph: "⌕",
  caption: "browser window",
  Component: BrowserSurface,
  preview: () => ({
    title: "Browser",
    lines: ["local view", "actions approval-gated"],
  }),
};
