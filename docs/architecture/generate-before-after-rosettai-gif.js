const { mkdirSync, writeFileSync } = require("node:fs");

const outputDir = process.argv[2];
if (!outputDir) throw new Error("Usage: node generate-before-after-rosettai-gif.js <frames-dir>");
mkdirSync(outputDir, { recursive: true });

const width = 1200;
const height = 675;
const frames = 72;
const legacy = [".github/", ".cursor/", ".antigravity/", ".claude/", ".opencode/", ".codex/"];
const legacyY = [205, 245, 285, 325, 365, 405];

const clamp = (value, min = 0, max = 1) => Math.max(min, Math.min(max, value));
const ease = (value) => {
  const t = clamp(value);
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
};
const lerp = (from, to, t) => from + (to - from) * t;

function folder(x, y, color = "#aeb4bd", accent = false) {
  const fill = accent ? "url(#accent)" : color;
  return `<g transform="translate(${x} ${y})">
    <path d="M0 10a8 8 0 0 1 8-8h19l9 10h35a8 8 0 0 1 8 8v34a8 8 0 0 1-8 8H8a8 8 0 0 1-8-8z" fill="${fill}"/>
    <path d="M0 20h79v34a8 8 0 0 1-8 8H8a8 8 0 0 1-8-8z" fill="${fill}" opacity=".88"/>
  </g>`;
}

function fileIcon(x, y) {
  return `<g transform="translate(${x} ${y})" fill="none" stroke="#aeb4bd" stroke-width="5">
    <path d="M5 2h29l15 15v48H5z"/><path d="M34 2v16h15M15 34h24M15 47h24"/>
  </g>`;
}

function row(x, y, label, type = "folder", accent = false) {
  const icon = type === "file" ? fileIcon(x, y - 28) : folder(x, y - 29, "#aeb4bd", accent);
  return `${icon}<text x="${x + 98}" y="${y + 16}" class="code${accent ? " accentText" : ""}">${label}</text>`;
}

function frame(index) {
  const t = index / (frames - 1);
  const migration = clamp((t - 0.16) / 0.62);
  const arrowGlow = 0.18 + 0.38 * Math.sin(Math.PI * migration);
  const moving = legacy.map((label, i) => {
    const local = ease((migration - i * 0.085) / 0.58);
    const x = lerp(150, 828, local);
    const y = lerp(legacyY[i], 250, local) - Math.sin(Math.PI * local) * (55 + i * 4);
    const scale = lerp(1, 0.58, local);
    const opacity = 1 - ease((local - 0.72) / 0.28);
    return `<g opacity="${opacity.toFixed(3)}" transform="translate(${x.toFixed(2)} ${y.toFixed(2)}) scale(${scale.toFixed(3)}) translate(-150 ${-legacyY[i]})">
      ${folder(150, legacyY[i] - 29)}
    </g>`;
  }).join("\n");

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <defs>
    <linearGradient id="accent" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#ff765c"/><stop offset=".5" stop-color="#ef4f96"/><stop offset="1" stop-color="#713df0"/></linearGradient>
    <filter id="shadow"><feDropShadow dx="0" dy="5" stdDeviation="8" flood-color="#172033" flood-opacity=".12"/></filter>
    <filter id="glow"><feDropShadow dx="0" dy="0" stdDeviation="${8 + arrowGlow * 12}" flood-color="#b653df" flood-opacity="${arrowGlow}"/></filter>
    <style>
      .title{font:700 43px Inter,Arial,sans-serif;fill:#172033}.code{font:600 25px ui-monospace,SFMono-Regular,Menlo,monospace;fill:#1d2638}.accentText{fill:#5d32d8}.muted{fill:#697386;font:500 18px Inter,Arial,sans-serif}
    </style>
  </defs>
  <rect width="1200" height="675" fill="#f7f7f4"/>
  <text x="265" y="72" text-anchor="middle" class="title">Before RosettAI</text>
  <text x="935" y="72" text-anchor="middle" class="title">With RosettAI</text>
  <rect x="42" y="105" width="500" height="520" rx="24" fill="#fff" stroke="#dde1e8" stroke-width="2" filter="url(#shadow)"/>
  <rect x="658" y="105" width="500" height="520" rx="24" fill="#fff" stroke="#d8d5f0" stroke-width="2" filter="url(#shadow)"/>
  ${folder(86, 132)}<text x="184" y="180" class="code" font-size="32">repo/</text>
  ${folder(702, 132)}<text x="800" y="180" class="code" font-size="32">repo/</text>
  <path d="M564 305h43v-35l55 67-55 67v-35h-43z" fill="url(#accent)" filter="url(#glow)"/>
  <g opacity="${(1 - ease(migration / 0.92)).toFixed(3)}">${legacy.map((label, i) => row(150, legacyY[i], label)).join("")}</g>
  ${moving}
  ${row(150, 450, "src/")}${row(150, 495, "README.md", "file")}${row(150, 540, "package.json", "file")}${row(150, 585, "…/")}
  ${row(766, 255, ".rosettai/", "folder", true)}
  ${row(766, 355, "src/")}${row(766, 430, "README.md", "file")}${row(766, 505, "package.json", "file")}${row(766, 580, "…/")}
  </svg>`;
}

for (let index = 0; index < frames; index += 1) {
  writeFileSync(`${outputDir}/frame-${String(index).padStart(3, "0")}.svg`, frame(index));
}
