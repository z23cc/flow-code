---
name: flow-code-prompt-eng
description: "Internal guidance for composing Codex/GPT review and task prompts. Loaded automatically by worker agents and review skills when building prompts for cross-model review."
tier: 2
user-invocable: false
---

# Prompt Engineering for Cross-Model Review

Use this skill when composing prompts for Codex, GPT, or any external model via `flowctl codex *` commands or `/flow-code:impl-review`.

## Core Rules

1. **One clear task per prompt.** Split unrelated asks into separate runs.
2. **Tell the model what done looks like.** Explicit output contract, not implied.
3. **Add grounding rules where unsupported guesses would hurt.** Reviews and research need citation/evidence anchoring.
4. **Tighten the prompt before raising effort.** A well-structured prompt at `high` beats a vague one at `xhigh`.
5. **Use XML tags consistently.** Stable tag names from the block library.

## Prompt Assembly Checklist

1. Define the exact task and scope in `<task>`.
2. Choose the smallest output contract that makes the answer actionable.
3. Decide: should the model keep going by default, or stop for missing details?
4. Add verification, grounding, and safety blocks only where the task needs them.
5. Remove redundant instructions before sending.

## Block Selection by Task Type

| Task type | Required blocks | Optional blocks |
|-----------|----------------|-----------------|
| **Adversarial review** | `task`, `operating_stance`, `attack_surface`, `calibration_rules`, `structured_output_contract`, `final_check`, `grounding_rules` | `finding_bar`, `review_method` |
| **Security review** | `task`, `attack_surface` (security-specific), `calibration_rules`, `structured_output_contract`, `grounding_rules` | `final_check` |
| **Performance review** | `task`, `attack_surface` (perf-specific), `compact_output_contract` | `calibration_rules` |
| **Diagnosis/debugging** | `task`, `compact_output_contract`, `verification_loop` | `grounding_rules` |
| **Implementation** | `task`, `completeness_contract`, `action_safety`, `verification_loop` | — |
| **Research** | `task`, `citation_rules`, `compact_output_contract` | `grounding_rules` |

## How to Compose

1. Start with a recipe from `prompts/recipes.md` if one fits.
2. Swap in blocks from `prompts/blocks.md` for customization.
3. Check `prompts/antipatterns.md` to verify you're not making a known mistake.
4. Interpolate placeholders: `{{diff_summary}}`, `{{diff_content}}`, `{{embedded_files}}`, `{{focus_block}}`.

## Working Rules

- Prefer explicit contracts over vague nudges.
- Use stable XML tag names from `prompts/blocks.md`.
- Do NOT raise reasoning effort first — tighten prompt and verification before escalating.
- Keep claims anchored to observed evidence. If something is a hypothesis, say so.
- Prefer fewer, stronger findings over many weak ones.

## Files

- `prompts/blocks.md` — 14 reusable XML blocks
- `prompts/recipes.md` — Ready-to-use templates (adversarial, security, performance, diagnosis, implementation)
- `prompts/antipatterns.md` — 8 common mistakes
- `prompts/adversarial-review.md` — Active adversarial review template (used by `flowctl codex adversarial`)
