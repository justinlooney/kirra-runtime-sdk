// ============================================================
// KIRRA — logo background knockout + auto-crop
// mix-blend can't reach the lattice behind the fixed nav, so the PNG's dark
// square shows as a hard box. Chroma-key the near-black pixels to transparent
// (soft alpha by luminance), THEN crop to the K's bounding box so it fills the
// frame (the source has lots of padding). Same-origin image → untainted canvas.
// ============================================================

function processLogo(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      try {
        const w = img.naturalWidth, h = img.naturalHeight;
        const c = document.createElement('canvas');
        c.width = w; c.height = h;
        const ctx = c.getContext('2d');
        ctx.drawImage(img, 0, 0);
        const d = ctx.getImageData(0, 0, w, h);
        const p = d.data;

        // knock out the dark background + find content bounds
        let minX = w, minY = h, maxX = 0, maxY = 0;
        for (let y = 0; y < h; y++) {
          for (let x = 0; x < w; x++) {
            const i = (y * w + x) * 4;
            const lum = Math.max(p[i], p[i + 1], p[i + 2]);
            let a = (lum - 10) * 3;
            a = a < 0 ? 0 : a > 255 ? 255 : a;
            p[i + 3] = a;
            if (a > 28) {
              if (x < minX) minX = x; if (x > maxX) maxX = x;
              if (y < minY) minY = y; if (y > maxY) maxY = y;
            }
          }
        }
        ctx.putImageData(d, 0, 0);

        // crop to the K (with a little breathing room)
        if (maxX > minX && maxY > minY) {
          const pad = Math.round(Math.max(maxX - minX, maxY - minY) * 0.07);
          const sx = Math.max(0, minX - pad);
          const sy = Math.max(0, minY - pad);
          const sw = Math.min(w - sx, maxX - minX + pad * 2);
          const sh = Math.min(h - sy, maxY - minY + pad * 2);
          const out = document.createElement('canvas');
          out.width = sw; out.height = sh;
          out.getContext('2d').drawImage(c, sx, sy, sw, sh, 0, 0, sw, sh);
          resolve(out.toDataURL('image/png'));
        } else {
          resolve(c.toDataURL('image/png'));
        }
      } catch (e) { reject(e); }
    };
    img.onerror = reject;
    img.src = src;
  });
}

async function applyMask() {
  const slots = document.querySelectorAll('.nav__logo, .preloader__logo');
  if (!slots.length) return;
  const src = slots[0].getAttribute('src');
  if (!src) return;
  try {
    const url = await processLogo(src);
    slots.forEach((el) => { el.src = url; });
  } catch { /* leave the original PNG on failure */ }
}

if (document.readyState !== 'loading') applyMask();
else document.addEventListener('DOMContentLoaded', applyMask);
