// Render the latest Substack posts (baked into posts.json at deploy time) as
// cards in the Field Notes section. Same-origin fetch — no CORS, no proxy.
// If posts.json is empty or missing, the section's subscribe CTA stays as-is.

async function loadNotes() {
  const grid = document.getElementById('notesGrid');
  if (!grid) return;
  try {
    const res = await fetch('posts.json', { cache: 'no-cache' });
    if (!res.ok) return;
    const posts = await res.json();
    if (!Array.isArray(posts) || posts.length === 0) return;

    for (const p of posts) {
      if (!p || typeof p.link !== 'string' || !/^https?:\/\//.test(p.link)) continue;
      const card = document.createElement('a');
      card.className = 'note-card';
      card.href = p.link;
      card.target = '_blank';
      card.rel = 'noopener';

      if (p.date) {
        const d = document.createElement('span');
        d.className = 'note-card__date';
        d.textContent = p.date;
        card.appendChild(d);
      }
      const t = document.createElement('span');
      t.className = 'note-card__title';
      t.textContent = p.title || 'Untitled';
      card.appendChild(t);

      if (p.excerpt) {
        const e = document.createElement('span');
        e.className = 'note-card__excerpt';
        e.textContent = p.excerpt;
        card.appendChild(e);
      }
      const more = document.createElement('span');
      more.className = 'note-card__more';
      more.textContent = 'Read on Substack ↗';
      card.appendChild(more);

      grid.appendChild(card);
    }
    grid.classList.add('is-loaded');
    if (window.ScrollTrigger) window.ScrollTrigger.refresh();
  } catch (_) {
    /* keep the subscribe CTA as the fallback */
  }
}

if (document.readyState !== 'loading') loadNotes();
else document.addEventListener('DOMContentLoaded', loadNotes);
