// heiwa-desk spike: Deno front-end over the herdr multiplexer.
// Web mode:     deno task desk           -> http://127.0.0.1:7480
// Desktop mode: deno desktop prototypes/heiwa-desk/main.ts
// Same entrypoint for both — `deno desktop` points its webview at Deno.serve.

import { focusPane, getHerd, readPane, runInPane, sendToPane, splitPane } from "./herdr.ts";
import { ansiToHtml } from "./ansi.ts";

const LOCAL_ORIGINS = new Set([
  "http://127.0.0.1:5174",
  "http://127.0.0.1:1420",
  "http://localhost:5174",
  "http://localhost:1420",
  "tauri://localhost",
  "http://tauri.localhost",
]);

function corsHeaders(req: Request, extra: HeadersInit = {}): Headers {
  const headers = new Headers(extra);
  const origin = req.headers.get("origin");
  if (origin && LOCAL_ORIGINS.has(origin)) {
    headers.set("access-control-allow-origin", origin);
    headers.set("vary", "origin");
  }
  headers.set("access-control-allow-methods", "GET, POST, OPTIONS");
  headers.set("access-control-allow-headers", "content-type");
  return headers;
}

const PAGE = /*html*/ `<!doctype html>
<meta charset="utf-8">
<title>Heiwa Desk</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body { margin:0; display:flex; height:100vh; background:#121417; color:#d0d0d0;
         font:13px/1.45 ui-monospace, SF Mono, Menlo, monospace; }
  #side { width:280px; border-right:1px solid #2a2e35; padding:10px; overflow-y:auto; flex-shrink:0; }
  #side h1 { font-size:13px; margin:2px 0 10px; color:#e5c07b; letter-spacing:.08em; }
  .pane-btn { display:block; width:100%; text-align:left; background:none; border:1px solid #2a2e35;
              border-radius:6px; color:inherit; padding:7px 9px; margin-bottom:6px; cursor:pointer; font:inherit; }
  .pane-btn:hover { border-color:#4a5160; }
  .pane-btn.active { border-color:#61afef; background:#1a2029; }
  .pane-btn .id { color:#5c6370; }
  .badge { float:right; padding:0 6px; border-radius:8px; font-size:11px; }
  .working { background:#4d3d12; color:#f0cf8f; } .blocked { background:#4d1a1f; color:#ff7a85; }
  .idle { background:#1d3a24; color:#a9d48a; } .done { background:#173042; color:#74bdf7; }
  .unknown { background:#2a2e35; color:#8a919e; }
  #main { flex:1; display:flex; flex-direction:column; min-width:0; }
  #term { flex:1; overflow-y:auto; padding:12px 14px; white-space:pre-wrap; word-break:break-all; }
  #bar { display:flex; gap:8px; padding:10px; border-top:1px solid #2a2e35; }
  #bar input { flex:1; background:#1a1d22; border:1px solid #2a2e35; border-radius:6px;
               color:inherit; padding:8px 10px; font:inherit; }
  #bar button, #bar select { background:#1a2029; border:1px solid #2a2e35; border-radius:6px;
               color:inherit; padding:8px 12px; cursor:pointer; font:inherit; }
  #hint { padding:4px 14px 8px; color:#5c6370; font-size:11px; }
</style>
<div id="side"><h1>HEIWA · HERD</h1><div id="list">loading…</div></div>
<div id="main">
  <div id="term">select a pane</div>
  <div id="hint">keys 1–9 switch panes · Enter sends · mode: <b>send</b> = prompt text, <b>run</b> = shell command</div>
  <div id="bar">
    <select id="mode"><option value="send">send</option><option value="run">run</option></select>
    <input id="input" placeholder="prompt or command for selected pane…">
    <button id="go">→</button>
  </div>
</div>
<script>
let herd = [], selected = null, timer = null;

async function refreshHerd() {
  herd = await (await fetch("/api/herd")).json();
  const list = document.getElementById("list");
  list.innerHTML = "";
  herd.forEach((h, i) => {
    const b = document.createElement("button");
    b.className = "pane-btn" + (h.pane === selected ? " active" : "");
    b.innerHTML = '<span class="badge ' + h.state + '">' + h.state + "</span>" +
      "<b>" + (i + 1) + "</b> " + h.agent + ' <span class="id">' + h.pane + "</span><br>" +
      '<span class="id">' + h.workspace + " · " + h.cwd.replace(/^\\/Users\\/[^/]+/, "~") + "</span>";
    b.onclick = () => select(h.pane);
    list.appendChild(b);
  });
  if (!selected && herd.length) select(herd[0].pane);
}

async function refreshPane() {
  if (!selected) return;
  const html = await (await fetch("/api/pane/" + encodeURIComponent(selected))).text();
  const t = document.getElementById("term");
  const stick = t.scrollTop + t.clientHeight >= t.scrollHeight - 8;
  t.innerHTML = html || '<span style="color:#5c6370">— empty —</span>';
  if (stick) t.scrollTop = t.scrollHeight;
}

function select(pane) {
  selected = pane;
  refreshHerd();
  refreshPane();
  clearInterval(timer);
  timer = setInterval(refreshPane, 1500);
}

async function submit() {
  const input = document.getElementById("input");
  const mode = document.getElementById("mode").value;
  if (!selected || !input.value) return;
  await fetch("/api/pane/" + encodeURIComponent(selected) + "/" + mode, {
    method: "POST",
    body: input.value,
  });
  input.value = "";
  setTimeout(refreshPane, 350);
}

document.getElementById("go").onclick = submit;
document.getElementById("input").addEventListener("keydown", (e) => {
  if (e.key === "Enter") submit();
});
document.addEventListener("keydown", (e) => {
  if (e.target.tagName === "INPUT") return;
  const n = parseInt(e.key, 10);
  if (n >= 1 && n <= herd.length) select(herd[n - 1].pane);
});

refreshHerd();
setInterval(refreshHerd, 2000);
</script>`;

Deno.serve({ hostname: "127.0.0.1", port: 7480 }, async (req) => {
  const url = new URL(req.url);
  const paneMatch = url.pathname.match(/^\/api\/pane\/([^/]+)(?:\/(send|run|focus|split))?$/);
  try {
    if (req.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: corsHeaders(req) });
    }
    if (url.pathname === "/") {
      return new Response(PAGE, {
        headers: corsHeaders(req, { "content-type": "text/html; charset=utf-8" }),
      });
    }
    if (url.pathname === "/api/herd") {
      return Response.json(await getHerd(), { headers: corsHeaders(req) });
    }
    if (paneMatch && !paneMatch[2]) {
      const paneText = await readPane(decodeURIComponent(paneMatch[1]));
      if (url.searchParams.get("format") === "text") {
        return new Response(paneText, {
          headers: corsHeaders(req, { "content-type": "text/plain; charset=utf-8" }),
        });
      }
      return new Response(ansiToHtml(paneText), {
        headers: corsHeaders(req, { "content-type": "text/html; charset=utf-8" }),
      });
    }
    if (paneMatch && req.method === "POST") {
      const body = await req.text();
      const pane = decodeURIComponent(paneMatch[1]);
      if (paneMatch[2] === "send") await sendToPane(pane, body);
      else if (paneMatch[2] === "run") await runInPane(pane, body);
      else if (paneMatch[2] === "focus") await focusPane(pane);
      else if (paneMatch[2] === "split") {
        const payload = JSON.parse(body || "{}") as { direction?: "right" | "down"; cwd?: string };
        await splitPane(pane, payload.direction === "down" ? "down" : "right", payload.cwd);
      }
      return Response.json({ ok: true }, { headers: corsHeaders(req) });
    }
    return new Response("not found", { status: 404, headers: corsHeaders(req) });
  } catch (err) {
    return new Response(String(err), { status: 500, headers: corsHeaders(req) });
  }
});

await new Promise(() => {});
