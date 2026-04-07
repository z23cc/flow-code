---
name: flow-code-qa
description: "Use when testing web apps visually, finding UI bugs, verifying deployed changes, or running QA passes. Triggers on 'qa', 'test the site', 'check the UI', 'verify deployment'."
tier: 4
user-invocable: true
---

# QA Testing

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.

Systematic visual QA testing with browser automation. Tests page health, navigation, forms, responsive layout, and accessibility, then reports findings with screenshots and repro steps.

## Input

Full request: $ARGUMENTS

**Format:** `/flow-code:qa [url] [--fix] [--viewport <size>] [--scope <pages>]`

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| url | no | Auto-detect from project config | Target URL to test |
| --fix | no | off | After reporting, iterate fixing found issues in source code |
| --viewport | no | all (mobile, tablet, desktop) | Specific viewport to test |
| --scope | no | all pages | Comma-separated list of paths to test |

## Browser Setup

Follow browser setup from `skills/browser/SKILL.md`. Verify agent-browser is installed:

```bash
command -v agent-browser >/dev/null 2>&1 && agent-browser --version || echo "MISSING: npm i -g agent-browser && agent-browser install"
```

## URL Detection

Determine the target URL:
1. **Explicit argument** -- use it directly
2. **Project config** -- check for `package.json` scripts (dev/start), `Procfile`, `fly.toml`, `vercel.json`, `.env` with `BASE_URL`
3. **Localhost fallback** -- `http://localhost:3000` (common default)

```bash
# Open the target URL
agent-browser open "$TARGET_URL" && agent-browser wait --load networkidle
```

## QA Checklist

Run each check sequentially. Take screenshots as evidence at each step.

### 1. Page Load Health

```bash
# Screenshot initial load
agent-browser screenshot --full /tmp/qa-page-load.png

# Check for console errors
agent-browser errors

# Check page title exists
agent-browser get title
```

- Verify page renders (not blank, no error page)
- Check console for JS errors or failed network requests
- Confirm page title is set and meaningful

### 2. Navigation

```bash
agent-browser snapshot -i
```

- Click each major navigation link
- Verify destination loads without errors
- Check for broken links (404 pages)
- Verify back/forward browser navigation works

For each link found:
```bash
agent-browser click @eN
agent-browser wait --load networkidle
agent-browser screenshot /tmp/qa-nav-N.png
agent-browser errors
agent-browser back
```

### 3. Forms

For each form on the page:
```bash
agent-browser snapshot -i
```

- Test with valid input (happy path)
- Test with empty required fields (validation)
- Test with invalid input (error states)
- Verify success/error messages display correctly
- Check form does not submit with invalid data

### 4. Responsive Layout

Test at three viewports:

```bash
# Mobile (375x667)
agent-browser eval 'window.resizeTo(375, 667)'
agent-browser wait 500
agent-browser screenshot --full /tmp/qa-mobile.png

# Tablet (768x1024)
agent-browser eval 'window.resizeTo(768, 1024)'
agent-browser wait 500
agent-browser screenshot --full /tmp/qa-tablet.png

# Desktop (1440x900)
agent-browser eval 'window.resizeTo(1440, 900)'
agent-browser wait 500
agent-browser screenshot --full /tmp/qa-desktop.png
```

- Check layout does not break at each size
- Verify mobile menu works (if present)
- Check text is readable at all sizes
- Verify no horizontal scrollbar on mobile

### 5. Accessibility

```bash
# Check for images without alt text
agent-browser eval --stdin <<'EVALEOF'
JSON.stringify(
  Array.from(document.querySelectorAll("img"))
    .filter(i => !i.alt)
    .map(i => ({ src: i.src.split("/").pop(), width: i.width }))
)
EVALEOF

# Check for form inputs without labels
agent-browser eval --stdin <<'EVALEOF'
JSON.stringify(
  Array.from(document.querySelectorAll("input, select, textarea"))
    .filter(el => !el.labels?.length && !el.getAttribute("aria-label"))
    .map(el => ({ type: el.type, name: el.name, id: el.id }))
)
EVALEOF
```

- Check color contrast (text readable against backgrounds)
- Verify all interactive elements are keyboard-accessible
- Check for missing alt text on images
- Verify form inputs have associated labels

## Report Format

After completing all checks, compile a structured report:

```
## QA Report: [URL]
Date: [timestamp]

### Health Score: N/10

### Issues Found

#### P0 - Critical (blocks usage)
- [Issue]: [description]
  - Screenshot: [path]
  - Repro: [steps]

#### P1 - Major (significant UX impact)
- [Issue]: [description]
  - Screenshot: [path]
  - Repro: [steps]

#### P2 - Minor (cosmetic or edge case)
- [Issue]: [description]
  - Screenshot: [path]
  - Repro: [steps]

#### P3 - Nitpick (polish)
- [Issue]: [description]
  - Screenshot: [path]
  - Repro: [steps]

### Health Score Breakdown
- Page load: N/10
- Navigation: N/10
- Forms: N/10
- Responsive: N/10
- Accessibility: N/10

### Screenshots
[List of all captured screenshots with descriptions]
```

**Health score formula:**
- Start at 10
- P0 issue: -3 each
- P1 issue: -2 each
- P2 issue: -1 each
- P3 issue: -0.5 each
- Minimum score: 0

## Fix Mode

When `--fix` is passed:

1. For each issue found (P0 first, then P1, P2, P3):
   a. Identify the source file and code causing the issue
   b. Fix the issue in the source code
   c. Commit the fix atomically: `git add -A && git commit -m "fix: [description]"`
   d. Reload the page and re-verify the fix
   e. Take before/after screenshots as evidence

2. Re-run the full QA checklist after all fixes
3. Report final health score vs initial score

## Cleanup

Always close the browser session when done:

```bash
agent-browser close
```
