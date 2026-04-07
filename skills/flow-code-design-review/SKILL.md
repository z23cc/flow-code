---
name: flow-code-design-review
description: "Use when auditing visual design consistency, spacing, typography, color usage, or UI polish. Triggers on 'design review', 'visual audit', 'check design', 'UI polish'."
tier: 4
user-invocable: true
---

# Design Review

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.

Visual design consistency audit with browser automation. Evaluates spacing, typography, color, component consistency, alignment, and detects "AI slop" patterns. Produces a dimensional scorecard and fixes issues iteratively.

## Input

Full request: $ARGUMENTS

**Format:** `/flow-code:design-review [url] [--pages <paths>] [--fix] [--baseline <dir>]`

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| url | no | Auto-detect from project config | Target URL to audit |
| --pages | no | all discoverable pages | Comma-separated paths to audit |
| --fix | no | on (default behavior) | Fix issues found in source code |
| --no-fix | no | off | Report only, do not fix |
| --baseline | no | none | Directory of baseline screenshots for comparison |

## Browser Setup

Follow browser setup from `skills/browser/SKILL.md`. Verify agent-browser is installed:

```bash
command -v agent-browser >/dev/null 2>&1 && agent-browser --version || echo "MISSING: npm i -g agent-browser && agent-browser install"
```

## URL Detection

Determine the target URL:
1. **Explicit argument** -- use it directly
2. **Project config** -- check for `package.json` scripts (dev/start), `Procfile`, `fly.toml`, `vercel.json`, `.env` with `BASE_URL`
3. **Localhost fallback** -- `http://localhost:3000`

## Step 1: Baseline Screenshots

Capture screenshots of all key pages before making any changes:

```bash
agent-browser open "$TARGET_URL" && agent-browser wait --load networkidle

# Full-page screenshot of each key page
agent-browser screenshot --full /tmp/design-baseline-home.png
```

Navigate to each page listed in `--pages` (or discover from navigation links):
```bash
agent-browser snapshot -i
# For each nav link:
agent-browser click @eN
agent-browser wait --load networkidle
agent-browser screenshot --full /tmp/design-baseline-PAGE.png
agent-browser back
```

## Step 2: Visual Audit Checklist

For each page, evaluate the following dimensions.

### 2.1 Spacing Consistency

```bash
# Extract computed styles for spacing analysis
agent-browser eval --stdin <<'EVALEOF'
JSON.stringify({
  margins: Array.from(new Set(
    Array.from(document.querySelectorAll("section, article, div > h1, div > h2, div > h3, .card, .container > *"))
      .map(el => getComputedStyle(el).marginBottom)
      .filter(v => v !== "0px")
  )),
  paddings: Array.from(new Set(
    Array.from(document.querySelectorAll("section, .card, .container, main, header, footer"))
      .map(el => getComputedStyle(el).padding)
  )),
  gaps: Array.from(new Set(
    Array.from(document.querySelectorAll("[style*=gap], .flex, .grid, [class*=flex], [class*=grid]"))
      .map(el => getComputedStyle(el).gap)
      .filter(v => v !== "normal")
  ))
})
EVALEOF
```

Check for:
- Consistent vertical rhythm (margins between sections)
- Consistent padding within containers
- Consistent gap values in flex/grid layouts
- No mixed spacing systems (e.g., some 16px, some 15px, some 1rem)

### 2.2 Typography Hierarchy

```bash
agent-browser eval --stdin <<'EVALEOF'
JSON.stringify(
  ["h1","h2","h3","h4","p","a","button","label","span"].map(tag => {
    const el = document.querySelector(tag);
    if (!el) return null;
    const s = getComputedStyle(el);
    return { tag, fontSize: s.fontSize, fontWeight: s.fontWeight, lineHeight: s.lineHeight, fontFamily: s.fontFamily.split(",")[0] };
  }).filter(Boolean)
)
EVALEOF
```

Check for:
- Clear size hierarchy (h1 > h2 > h3 > p)
- Consistent font families (no more than 2-3 families)
- Appropriate line heights (1.4-1.6 for body, 1.1-1.3 for headings)
- Font weights used purposefully (not random)

### 2.3 Color Usage

```bash
agent-browser eval --stdin <<'EVALEOF'
JSON.stringify({
  backgrounds: Array.from(new Set(
    Array.from(document.querySelectorAll("*"))
      .map(el => getComputedStyle(el).backgroundColor)
      .filter(v => v !== "rgba(0, 0, 0, 0)" && v !== "transparent")
  )).slice(0, 20),
  textColors: Array.from(new Set(
    Array.from(document.querySelectorAll("p, h1, h2, h3, h4, span, a, li, td, th, label"))
      .map(el => getComputedStyle(el).color)
  )).slice(0, 15)
})
EVALEOF
```

Check for:
- Limited, intentional color palette (not random one-offs)
- Sufficient contrast between text and backgrounds
- Consistent use of brand/accent colors
- No harsh or clashing color combinations

### 2.4 Component Consistency

```bash
agent-browser eval --stdin <<'EVALEOF'
JSON.stringify({
  buttons: Array.from(document.querySelectorAll("button, [type=submit], .btn, [class*=button]"))
    .map(el => {
      const s = getComputedStyle(el);
      return { text: el.textContent.trim().slice(0,30), borderRadius: s.borderRadius, padding: s.padding, fontSize: s.fontSize, bg: s.backgroundColor };
    }),
  inputs: Array.from(document.querySelectorAll("input, select, textarea"))
    .map(el => {
      const s = getComputedStyle(el);
      return { type: el.type, borderRadius: s.borderRadius, padding: s.padding, border: s.border, fontSize: s.fontSize };
    })
})
EVALEOF
```

Check for:
- All buttons share the same border-radius, padding, and font size
- All form inputs share the same styling
- Cards/panels have consistent shadows, borders, and radius
- Icons are consistent in size and style

### 2.5 Alignment

```bash
agent-browser screenshot --annotate /tmp/design-alignment.png
```

Check for:
- Elements align to a consistent grid
- Left edges of content blocks align
- Visual balance between left and right sides
- No awkward off-center elements
- Consistent text alignment within sections

### 2.6 AI Slop Detection

Look for common signs of AI-generated or placeholder content:
- Generic placeholder text ("Lorem ipsum", "Your Company", "Acme Corp")
- Stock phrases ("Revolutionize your workflow", "Cutting-edge solutions")
- Mismatched tone between sections
- Duplicate or near-duplicate content blocks
- Generic stock-photo-style hero sections
- Overly symmetrical three-column feature grids with generic icons

```bash
agent-browser eval --stdin <<'EVALEOF'
JSON.stringify({
  suspiciousText: Array.from(document.querySelectorAll("h1, h2, h3, p, span"))
    .filter(el => {
      const t = el.textContent.toLowerCase();
      return t.includes("lorem ipsum") || t.includes("your company") ||
             t.includes("acme") || t.includes("revolutionize") ||
             t.includes("cutting-edge") || t.includes("game-changing") ||
             t.includes("next-level") || t.includes("placeholder");
    })
    .map(el => ({ tag: el.tagName, text: el.textContent.trim().slice(0, 80) }))
})
EVALEOF
```

## Step 3: Scoring

Rate each dimension 0-10 with explanation of what would make it a 10:

```
## Design Review Scorecard: [URL]
Date: [timestamp]

| Dimension | Score | What would make it 10 |
|-----------|-------|----------------------|
| Spacing consistency | N/10 | [specific improvement] |
| Typography hierarchy | N/10 | [specific improvement] |
| Color usage | N/10 | [specific improvement] |
| Component consistency | N/10 | [specific improvement] |
| Alignment | N/10 | [specific improvement] |
| AI slop | N/10 | [specific improvement] |

### Overall Design Score: N/10

### Issues Found
[List each issue with severity, description, affected element, and suggested fix]
```

**Scoring guide:**
- 10: Professional, polished, nothing to improve
- 8-9: Strong, minor refinements possible
- 6-7: Acceptable, noticeable inconsistencies
- 4-5: Below average, multiple issues
- 0-3: Needs significant rework

## Step 4: Fix Loop

Unless `--no-fix` is passed, iterate through found issues:

1. For each issue (highest impact first):
   a. Identify the source CSS/component file
   b. Fix the issue in source code
   c. Commit atomically: `git add -A && git commit -m "design: [description]"`
   d. Reload the page in the browser
   e. Take an after screenshot
   f. Compare before/after visually

2. After all fixes, re-run the full audit
3. Report before/after scores for each dimension

```bash
# After each fix, verify with before/after diff
agent-browser diff screenshot --baseline /tmp/design-baseline-PAGE.png
```

## Cleanup

Always close the browser session when done:

```bash
agent-browser close
```
