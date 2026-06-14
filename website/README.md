# Kirra SDK — Marketing Site

An Awwwards-style landing page for the Kirra Runtime SDK, built with
**Three.js** (a live WebGL "trust lattice") and **GSAP** (scroll
choreography + micro-interactions).

## What's inside

| File | Role |
|------|------|
| `index.html` | Page structure + content (all copy grounded in the real SDK) |
| `css/style.css` | Design system, posture-driven palette, layout, components |
| `js/scene.js` | Three.js module — the trust-lattice fleet graph + bloom |
| `js/main.js` | GSAP — preloader, cursor, magnetic UI, scroll reveals, posture pin |

## Signature interactions

- **Trust Lattice** — a 3D fleet dependency graph (170 nodes, nearest-neighbour
  edges, traveling "trust packets") rendered with `UnrealBloomPass` for the neon glow.
  It reacts to the pointer (parallax) and rotates continuously.
- **Posture-driven colour** — the pinned **Posture** section scrubs the lattice
  through Kirra's three real states as you scroll:
  `Nominal` (green) → `Degraded` (amber) → `LockedOut` (red), with rising agitation.
- **Preloader** that completes only once the WebGL scene is ready (with a 6s safety net).
- Custom cursor, magnetic buttons, kinetic hero reveal, manifesto word-sweep,
  animated stat counters, copy-to-clipboard install command.

## Running it

It's fully static — serve the folder with anything:

```bash
cd website
python3 -m http.server 8077
# open http://localhost:8077
```

## Dependencies (loaded from CDN at runtime)

- `three@0.160.0` + addons (`EffectComposer`, `RenderPass`, `UnrealBloomPass`, `OutputPass`)
  via an `importmap` from unpkg.
- `gsap@3.12.5` + `ScrollTrigger` via cdnjs.

> An internet connection is required on first load to fetch these from CDN.
> To ship fully offline, vendor the two libraries into `website/vendor/` and
> repoint the `<script>`/`importmap` URLs.

## Accessibility / resilience

- Respects `prefers-reduced-motion` — animations collapse to a static, readable page.
- If WebGL is unavailable the lattice silently no-ops; the site still renders.
- If GSAP fails to load, content is shown un-animated rather than hidden.
