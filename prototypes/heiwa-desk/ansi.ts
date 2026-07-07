// Minimal ANSI SGR -> HTML. Covers what agent CLIs actually emit:
// reset, bold, dim, fg 30-37/90-97, bg 40-47, 256-color 38;5;n / 48;5;n.

const BASE = [
  "#1c1c1c",
  "#e06c75",
  "#98c379",
  "#e5c07b",
  "#61afef",
  "#c678dd",
  "#56b6c2",
  "#d0d0d0",
];
const BRIGHT = [
  "#5c6370",
  "#ff7a85",
  "#a9d48a",
  "#f0cf8f",
  "#74bdf7",
  "#d48ae6",
  "#6cc9d6",
  "#ffffff",
];

function xterm256(n: number): string {
  if (n < 8) return BASE[n];
  if (n < 16) return BRIGHT[n - 8];
  if (n < 232) {
    const v = (i: number) => (i === 0 ? 0 : 55 + i * 40);
    const i = n - 16;
    return `rgb(${v(Math.floor(i / 36))},${v(Math.floor((i % 36) / 6))},${v(i % 6)})`;
  }
  const g = 8 + (n - 232) * 10;
  return `rgb(${g},${g},${g})`;
}

const esc = (s: string) =>
  s.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");

export function ansiToHtml(input: string): string {
  let html = "";
  let open = false;
  const style = { fg: "", bg: "", bold: false, dim: false };
  const flushSpan = () => {
    if (open) html += "</span>";
    open = false;
  };
  const openSpan = () => {
    const css: string[] = [];
    if (style.fg) css.push(`color:${style.fg}`);
    if (style.bg) css.push(`background:${style.bg}`);
    if (style.bold) css.push("font-weight:600");
    if (style.dim) css.push("opacity:.6");
    if (css.length) {
      html += `<span style="${css.join(";")}">`;
      open = true;
    }
  };
  // deno-lint-ignore no-control-regex
  const parts = input.split(/\x1b\[([0-9;]*)m/);
  for (let i = 0; i < parts.length; i++) {
    if (i % 2 === 0) {
      html += esc(parts[i]);
      continue;
    }
    flushSpan();
    const codes = parts[i].split(";").map(Number);
    for (let c = 0; c < codes.length; c++) {
      const n = codes[c];
      if (n === 0 || Number.isNaN(n)) {
        Object.assign(style, { fg: "", bg: "", bold: false, dim: false });
      } else if (n === 1) style.bold = true;
      else if (n === 2) style.dim = true;
      else if (n >= 30 && n <= 37) style.fg = BASE[n - 30];
      else if (n >= 90 && n <= 97) style.fg = BRIGHT[n - 90];
      else if (n >= 40 && n <= 47) style.bg = BASE[n - 40];
      else if (n === 38 && codes[c + 1] === 5) {
        style.fg = xterm256(codes[c + 2]);
        c += 2;
      } else if (n === 48 && codes[c + 1] === 5) {
        style.bg = xterm256(codes[c + 2]);
        c += 2;
      } else if (n === 39) style.fg = "";
      else if (n === 49) style.bg = "";
    }
    openSpan();
  }
  flushSpan();
  return html;
}
