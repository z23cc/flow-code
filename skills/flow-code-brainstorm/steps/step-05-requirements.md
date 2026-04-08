# Step 5: Write Requirements Doc

Based on the chosen/selected approach, write a requirements document.

```bash
# Generate slug from the idea
SLUG=$(echo "$IDEA" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-' | head -c 40)

# Ensure .flow/specs/ exists
mkdir -p .flow/specs

# Write requirements doc
```

Write to `.flow/specs/${SLUG}-requirements.md`:

```markdown
# Requirements: <Title>

## Problem
<1-2 sentences — from user answers (interactive) or code-derived analysis (auto)>

## Users
<Who uses this — from Q1 answer>

## Chosen Approach
<Name and summary of selected approach>

## Requirements
- [ ] <Requirement 1>
- [ ] <Requirement 2>
- [ ] <Requirement 3>
...

## Non-Goals
- <What this explicitly does NOT include>

## Constraints
- <Technical or business constraints identified during brainstorm>

## Evidence
<Auto mode only — key files and patterns that informed decisions>
- `path/to/file.rs:42` — <what it shows>
- `path/to/other.rs` — <pattern found>

## Self-Interview Trace
<Auto mode only — full Q&A pairs for auditability>

### Q: <question 1>
**A:** <answer with code evidence>

### Q: <question 2>
**A:** <answer with code evidence>

...

## Open Questions
- <Anything unresolved that /flow-code:plan should address>
```

**Interactive mode**: omit Evidence and Self-Interview Trace sections.

## Output

After writing the file, tell the user:

```
Requirements written to .flow/specs/<slug>-requirements.md

Next step: Run /flow-code:plan .flow/specs/<slug>-requirements.md
```

**Auto mode additionally**: show a one-paragraph summary of the self-interview findings and the selected approach, so the user can quickly validate without reading the full doc.
