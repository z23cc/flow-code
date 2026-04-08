# Accessibility Checklist (WCAG 2.1 AA)

Quick reference for `flow-code-frontend-ui` and code review. Every user-facing change should pass these checks.

## Keyboard Navigation

- [ ] All interactive elements reachable via Tab
- [ ] Focus order follows visual reading order
- [ ] Focus indicator visible on all focused elements
- [ ] No keyboard traps (can Tab in AND out of every component)
- [ ] Custom widgets handle Enter, Space, Escape, Arrow keys as expected
- [ ] Skip-to-content link on pages with navigation

## Screen Readers

- [ ] All images have `alt` text (decorative images use `alt=""`)
- [ ] Form inputs have associated `<label>` or `aria-label`
- [ ] Icon-only buttons have `aria-label`
- [ ] Page has one `<h1>`, heading levels don't skip (h1→h2→h3)
- [ ] Landmark roles present: `<main>`, `<nav>`, `<header>`, `<footer>`
- [ ] Dynamic content changes announced via `aria-live` regions
- [ ] Tables have `<th>` headers with `scope` attributes

## Color & Contrast

- [ ] Text contrast ratio >= 4.5:1 (normal text), >= 3:1 (large text/UI components)
- [ ] Color is NOT the sole indicator of state (add icons, text, or patterns)
- [ ] UI is usable in high-contrast mode
- [ ] Links distinguishable from body text without relying on color alone

## Forms

- [ ] Error messages associated with the field (via `aria-describedby` or `aria-errormessage`)
- [ ] Required fields marked with `aria-required="true"` (not just `*`)
- [ ] Autocomplete attributes on common fields (name, email, address)
- [ ] Form validation errors summarized and focusable

## Motion & Media

- [ ] Animations respect `prefers-reduced-motion`
- [ ] No content flashes more than 3 times per second
- [ ] Video has captions; audio has transcripts
- [ ] Auto-playing media has pause/stop control

## Responsive & Touch

- [ ] Touch targets >= 44x44px
- [ ] Content readable at 200% zoom without horizontal scroll
- [ ] Works in both portrait and landscape orientation
- [ ] Pinch-to-zoom not disabled (`user-scalable=no` removed)

## Testing Tools

```bash
# Browser extensions
- axe DevTools (Deque)
- WAVE (WebAIM)
- Lighthouse accessibility audit

# CLI
npx axe-core <url>
npx pa11y <url>
```

## Quick Test

1. Unplug mouse, navigate with keyboard only
2. Turn on VoiceOver (Mac: Cmd+F5) / NVDA (Windows), listen to the page
3. Zoom browser to 200%, check layout
4. Run axe DevTools scan, fix all Critical/Serious issues
