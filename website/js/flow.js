// ============================================================
// KIRRA — live command flow (hero)
// Planner proposes commands → Kirra Governor judges each one →
// only proven/safe commands reach the Actuator. Out-of-envelope
// commands are clamped; unknown / illegitimate ones are denied
// and never pass. A looping, scripted demo of the verdict path.
// ============================================================

const COMMANDS = [
  { cmd: 'read_telemetry',  verdict: 'allow' },
  { cmd: 'cmd_vel 1.2 m/s', verdict: 'allow' },
  { cmd: 'cmd_vel 999 m/s', verdict: 'clamp', out: 'cmd_vel 2.0 m/s', reason: 'CLAMP · envelope' },
  { cmd: 'steer +4°',       verdict: 'allow' },
  { cmd: 'drive_to_moon()', verdict: 'deny',  reason: 'DENY · unknown action' },
  { cmd: 'throttle 90%',    verdict: 'deny',  reason: 'DENY · fleet degraded' },
];

function startFlow() {
  const gsap = window.gsap;
  const flow = document.getElementById('flow');
  const src  = document.getElementById('flowSrc');
  const gov  = document.getElementById('flowGov');
  const act  = document.getElementById('flowAct');
  const verdictEl = document.getElementById('flowVerdict');
  const actMeta   = document.getElementById('flowActMeta');
  if (!flow || !src || !gov || !act) return;

  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (!gsap || reduced) {
    // Static but still informative if animation can't run.
    if (verdictEl) verdictEl.textContent = 'gates every command';
    if (actMeta)   actMeta.textContent = 'only proven commands execute';
    return;
  }

  const centerY = (node) => node.offsetTop + node.offsetHeight / 2;

  function flashVerdict(c) {
    gov.classList.remove('is-allow', 'is-clamp', 'is-deny');
    const cls = c.verdict === 'allow' ? 'is-allow' : c.verdict === 'clamp' ? 'is-clamp' : 'is-deny';
    gov.classList.add(cls);
    verdictEl.textContent = c.verdict === 'allow' ? 'ALLOW' : (c.reason || c.verdict.toUpperCase());
    gsap.delayedCall(1.0, () => gov.classList.remove(cls));
  }

  function execActuator(label) {
    act.classList.add('is-exec');
    actMeta.textContent = '▶ ' + label;
    gsap.delayedCall(1.1, () => act.classList.remove('is-exec'));
  }

  function spawn(c) {
    const chip = document.createElement('div');
    chip.className = 'flow__chip';
    chip.textContent = c.cmd;
    flow.appendChild(chip);

    const startY = centerY(src);
    const govY = centerY(gov);
    const actY = centerY(act);
    chip.style.top = (startY - chip.offsetHeight / 2) + 'px';

    const tl = gsap.timeline({ onComplete: () => chip.remove() });
    // Planner → Governor
    tl.fromTo(chip, { y: 0, opacity: 0, scale: 0.92 },
                    { y: govY - startY, opacity: 1, scale: 1, duration: 1.05, ease: 'power1.inOut' });
    // verdict at the governor
    tl.add(() => { flashVerdict(c); chip.classList.add('flow__chip--' + c.verdict); });
    tl.to(chip, { duration: 0.55 }); // hold so the verdict reads

    if (c.verdict === 'deny') {
      // denied — dissolves at the governor, never reaches the actuator
      tl.to(chip, { opacity: 0, scale: 0.8, x: 18, duration: 0.4, ease: 'power1.in' });
    } else {
      if (c.verdict === 'clamp') {
        // clamped to the safe envelope, then forwarded
        tl.add(() => {
          chip.textContent = c.out;
          chip.classList.remove('flow__chip--clamp');
          chip.classList.add('flow__chip--allow');
        });
      }
      // Governor → Actuator
      tl.to(chip, { y: actY - startY, duration: 0.95, ease: 'power1.inOut' });
      tl.add(() => execActuator(c.verdict === 'clamp' ? c.out : c.cmd));
      tl.to(chip, { opacity: 0, scale: 0.85, duration: 0.4 });
    }
  }

  let i = 0;
  function loop() {
    spawn(COMMANDS[i % COMMANDS.length]);
    i += 1;
    gsap.delayedCall(2.4, loop);
  }
  gsap.delayedCall(1.4, loop); // let the hero intro reveal settle first
}

function boot() {
  if (!window.gsap) return void setTimeout(boot, 60);
  if (document.readyState !== 'loading') startFlow();
  else document.addEventListener('DOMContentLoaded', startFlow);
}
boot();
