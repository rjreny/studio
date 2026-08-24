# Studio Film Library — Design Contract

## Visual direction

Letterboxd structure, Apple TV polish, centered canvas, poster-led hierarchy.

## Material system (three surfaces only)

| Surface | Use |
|---------|-----|
| Canvas | Neutral page background |
| Solid | Cards, settings, stats, readable content |
| Glass | Sticky nav, filters, command palette, detail action rail |

## Shell

```css
.shell {
  width: min(1200px, calc(100vw - 48px));
  margin-inline: auto;
}
```

960px minimum: no horizontal scroll; content centered with responsive gutters.

## Ratings

- Filled stars: accent color per star.
- Empty stars: muted outline, not container accent.
- Compact mode always shows numeric score.

## Film detail sections (in order)

1. Film identity  
2. Your history  
3. Friends  
4. About the film  
5. TMDB community (labeled)  
6. Cast & crew  
7. Similar films  

## Accessibility

Reduced motion and reduced transparency preserve hierarchy with solid fallbacks.
