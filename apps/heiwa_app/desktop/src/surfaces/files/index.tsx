import type { SurfaceModule } from "../types";

function FilesSurface() {
  return (
    <div class="view files-view">
      <div class="view-header">
        <h2>Files</h2>
        <p class="muted">Workspace read model over the local runtime.</p>
      </div>
      <div class="panel">
        <header>
          <span>Workspace tree</span>
          <strong>pending</strong>
        </header>
        <div class="empty-state">
          <p>
            The file tree loads from <code>/api/v1/files/tree</code>.
          </p>
          <p class="quiet">Mutations stay receipt-gated.</p>
        </div>
      </div>
    </div>
  );
}

export const filesSurface: SurfaceModule = {
  id: "files",
  label: "Files",
  glyph: "□",
  caption: "files window",
  Component: FilesSurface,
  preview: () => ({
    title: "Files",
    lines: ["workspace read model", "mutations receipt-gated"],
  }),
};
