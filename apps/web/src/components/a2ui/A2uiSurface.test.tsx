import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vite-plus/test";

import { A2uiSurface } from "./A2uiSurface";

const surface = (components: ReadonlyArray<Record<string, unknown>>) => ({
  createSurface: { surfaceId: "ask-abcd1234", components },
});

describe("A2uiSurface", () => {
  it("renders text, caption, buttons with shortcut hints, and unknown components as placeholders", () => {
    const html = renderToStaticMarkup(
      <A2uiSurface
        onAction={() => {}}
        surface={surface([
          { id: "root", component: "Card", child: "col" },
          { id: "col", component: "Column", children: ["t", "c", "x", "row"] },
          { id: "t", component: "Text", text: "Calendar clash" },
          { id: "c", component: "Text", text: "Work · 9:00", variant: "caption" },
          { id: "x", component: "Slider", value: 3, max: 10 },
          { id: "row", component: "Row", children: ["b"] },
          { id: "l", component: "Text", text: "Keep both" },
          { id: "b", component: "Button", child: "l", action: { event: { name: "answer" } } },
        ])}
      />,
    );
    expect(html).toContain("Calendar clash");
    expect(html).toContain("text-muted-foreground");
    expect(html).toContain("Keep both");
    expect(html).toContain("Shortcut: 1");
    expect(html).toContain(">Slider<");
  });

  it("shows why a broken surface cannot render instead of throwing", () => {
    const html = renderToStaticMarkup(
      <A2uiSurface
        onAction={() => {}}
        surface={surface([{ id: "x", component: "Text", text: "hi" }])}
      />,
    );
    expect(html).toContain("This card could not be shown");
  });

  it("refuses plain-http images", () => {
    const html = renderToStaticMarkup(
      <A2uiSurface
        onAction={() => {}}
        surface={surface([
          { id: "root", component: "Column", children: ["a", "b"] },
          { id: "a", component: "Image", url: "http://tracker.example/p.png" },
          { id: "b", component: "Image", url: "https://example.com/ok.png", altText: "chart" },
        ])}
      />,
    );
    expect(html).not.toContain("tracker.example");
    expect(html).toContain('alt="chart"');
  });
});
