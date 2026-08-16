import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2022",
    minify: "esbuild",
    sourcemap: true,
  },
  test: {
    // Default stays `node`: the operator seam tests (store.test.ts,
    // client.test.ts) are DOM-free and must keep passing unmodified.
    // Component tests opt into jsdom with a per-file
    // `// @vitest-environment jsdom` docblock.
    environment: "node",
    // vite-plugin-solid must compile JSX for the browser-side runtime under
    // test rather than for SSR.
    server: { deps: { inline: [/solid-js/, /@solidjs/] } },
  },
});
