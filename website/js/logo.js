// ============================================================
// KIRRA — dynamic logo
// The K as a live node-graph: dim edges with bright pulses of "data"
// travelling between nodes, a pulsing governor at the junction. One SVG,
// injected into every .klogo slot (nav, footer, preloader). Motion is
// pure CSS (see style.css); respects prefers-reduced-motion.
// ============================================================

const EDGES = [
  'M200 140 L200 210', // spine
  'M200 210 L200 280',
  'M200 280 L200 350',
  'M200 210 L230 240', // spine → governor
  'M200 280 L230 240',
  'M230 240 L260 190', // governor → upper arm
  'M260 190 L320 150',
  'M230 240 L260 270', // governor → lower arm
  'M260 270 L320 330',
];

const NODES = [
  [200, 140], [200, 210], [200, 280], [200, 350],
  [260, 190], [320, 150], [260, 270], [320, 330],
];
const CYAN = new Set(['320,150', '320,330']); // arm tips

function buildSVG() {
  const edges = EDGES.map((d) => `<path d="${d}"/>`).join('');
  const pulses = EDGES.map((d, i) => `<path d="${d}" pathLength="100" style="animation-delay:${(i * 0.24).toFixed(2)}s"/>`).join('');
  const nodes = NODES.map(([x, y]) => `<circle cx="${x}" cy="${y}" r="9"${CYAN.has(x + ',' + y) ? ' class="kl-cyan"' : ''}/>`).join('');
  return (
    '<svg class="kl" viewBox="140 110 260 260" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">' +
    `<g class="kl-edges" fill="none">${edges}</g>` +
    `<g class="kl-pulses" fill="none">${pulses}</g>` +
    `<g class="kl-nodes" fill="#34f5a6">${nodes}</g>` +
    '<circle class="kl-gov" cx="230" cy="240" r="13" fill="#34f5a6"/>' +
    '<circle class="kl-govcore" cx="230" cy="240" r="5.5" fill="#eafff6"/>' +
    '</svg>'
  );
}

function inject() {
  const svg = buildSVG();
  document.querySelectorAll('.klogo').forEach((el) => {
    if (!el.firstChild) el.innerHTML = svg;
  });
}

if (document.readyState !== 'loading') inject();
else document.addEventListener('DOMContentLoaded', inject);
