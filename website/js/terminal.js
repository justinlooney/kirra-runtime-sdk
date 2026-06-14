// ============================================================
// KIRRA — live action_filter.py terminal
// Types real Kirra usage (syntax-highlighted) then "runs" it: the Governor
// evaluating LLM actions against live posture, printing ALLOW / CLAMP / DENY
// with a blinking cursor. Loops, starts on scroll, reduced-motion safe.
// Code tokens reuse the site's c-kw / c-fn / c-str / c-num / c-cm colours.
// ============================================================

// line = { c?: lineColourClass, text?: string }  — single colour
//      | { segs: [ [text, tokenClass], ... ] }   — syntax-coloured code
const LINES = [
  { c: 't-prompt', text: '$ python action_filter.py' },
  { text: '' },
  { segs: [['import ', 'c-kw'], ['kirra', '']] },
  { segs: [['gov = kirra.', ''], ['Governor', 'c-fn'], ['(', ''], ['"verifier.fleet.local"', 'c-str'], [')', '']] },
  { text: '' },
  { c: 'c-cm', text: '# every action is judged against live fleet posture' },
  { segs: [['gov.', ''], ['evaluate', 'c-fn'], ['(', ''], ['"robot-07"', 'c-str'], [', cmd_vel=', ''], ['1.2', 'c-num'], [')', '']] },
  { c: 't-ok', text: '  ✓ ALLOW   Nominal · dispatched' },
  { segs: [['gov.', ''], ['evaluate', 'c-fn'], ['(', ''], ['"robot-07"', 'c-str'], [', cmd_vel=', ''], ['999', 'c-num'], [')', '']] },
  { c: 't-warn', text: '  ⊘ CLAMP   999 → 2.0 m/s · envelope cap' },
  { segs: [['gov.', ''], ['evaluate', 'c-fn'], ['(', ''], ['"robot-07"', 'c-str'], [', ', ''], ['"drive_to_moon"', 'c-str'], [')', '']] },
  { c: 't-bad', text: '  ✕ DENY    unknown action · blocked' },
  { segs: [['gov.', ''], ['evaluate', 'c-fn'], ['(', ''], ['"robot-07"', 'c-str'], [', cmd_vel=', ''], ['1.2', 'c-num'], [')', ''], ['   # Degraded', 'c-cm']] },
  { c: 't-bad', text: '  ✕ DENY    kinetic write blocked' },
  { text: '' },
  { c: 'c-cm', text: "# fail-closed — if trust isn't proven, nothing moves" },
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function startTerminal() {
  const el = document.getElementById('termBody');
  if (!el) return;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const cursor = document.createElement('span');
  cursor.className = 't-cursor';

  function lineSegs(line) { return line.segs || [[line.text || '', '']]; }

  function renderStatic() {
    el.textContent = '';
    for (const line of LINES) {
      const div = document.createElement('div');
      div.className = 't-line ' + (line.c || '');
      for (const [text, cls] of lineSegs(line)) {
        if (cls) { const s = document.createElement('span'); s.className = cls; s.textContent = text; div.appendChild(s); }
        else if (text) div.appendChild(document.createTextNode(text));
      }
      el.appendChild(div);
    }
  }

  async function typeLine(line) {
    const div = document.createElement('div');
    div.className = 't-line ' + (line.c || '');
    el.appendChild(div);
    div.appendChild(cursor);
    const isOut = /t-(ok|warn|bad)/.test(line.c || '');
    if (isOut) await sleep(360); // "executing…"
    for (const [text, cls] of lineSegs(line)) {
      let span = null;
      if (cls) { span = document.createElement('span'); span.className = cls; div.insertBefore(span, cursor); }
      for (const ch of text) {
        const node = document.createTextNode(ch);
        if (span) span.appendChild(node);
        else div.insertBefore(node, cursor);
        await sleep(isOut ? 12 : 18);
      }
    }
    await sleep(80);
  }

  async function run() {
    el.textContent = '';
    for (const line of LINES) await typeLine(line);
    await sleep(2600);
    run();
  }

  if (reduced) { renderStatic(); return; }

  let started = false;
  const io = new IntersectionObserver((entries) => {
    if (entries.some((e) => e.isIntersecting) && !started) {
      started = true;
      io.disconnect();
      run();
    }
  }, { threshold: 0.2 });
  io.observe(el);
}

if (document.readyState !== 'loading') startTerminal();
else document.addEventListener('DOMContentLoaded', startTerminal);
