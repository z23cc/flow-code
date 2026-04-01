# Prompt Antipatterns

Avoid these when composing prompts for Codex/GPT review tasks.

## 1. Vague task framing

Bad:
```
Take a look at this code and tell me what you think.
```

Good:
```xml
<task>
Review this change for material correctness risks and regression hazards.
Focus area: auth middleware changes.
</task>
```

## 2. Missing output contract

Bad:
```
Investigate and report back.
```

Good:
```xml
<compact_output_contract>
Return: root cause (1 sentence), evidence, smallest fix, verification command.
</compact_output_contract>
```

**Why it matters**: Without an output contract, models produce verbose, unstructured responses that require manual parsing.

## 3. Asking for both review AND fix

Bad:
```
Review this code and fix any issues you find.
```

Good:
```
Review this code. Report findings with severity and file/line.
Do NOT fix issues — report them for the author to address.
```

**Why it matters**: Review + fix in one prompt creates conflicts. The model skips reporting findings it can "just fix," losing the audit trail.

## 4. Style feedback in adversarial reviews

Bad:
```
Check for code quality issues including naming, formatting, and style.
```

Good:
```xml
<finding_bar>
Report only material findings: correctness, security, data integrity.
Do NOT include style, naming, or formatting feedback.
</finding_bar>
```

**Why it matters**: Style findings dilute serious issues. Adversarial reviews should focus on "what can go wrong," not "what looks ugly."

## 5. No grounding rules → hallucinated findings

Bad:
```
Find all possible security vulnerabilities.
```

Good:
```xml
<grounding_rules>
Every finding must reference a specific file and line from the provided diff.
Do not invent code paths or runtime behavior not present in the context.
</grounding_rules>
```

**Why it matters**: Without grounding, models generate plausible-sounding but fictional vulnerabilities.

## 6. Raising reasoning effort before tightening prompt

Bad approach:
```
Set effort to xhigh and hope the model figures it out.
```

Good approach:
```
1. Tighten the prompt contract first
2. Add verification and grounding rules
3. Only raise effort if the task genuinely needs more reasoning depth
```

**Why it matters**: Higher effort costs more tokens and time. A well-structured prompt at `high` effort outperforms a vague prompt at `xhigh`.

## 7. Restarting from scratch instead of resuming

Bad:
```
Review this again from the beginning.
```

Good:
```
Continue the review. The author fixed findings 1 and 3.
Re-check those specific areas and verify the fixes are correct.
```

**Why it matters**: Restarting loses the model's context about what it already found. Resuming is cheaper and catches fix-introduced regressions.

## 8. Embedding implementation in review prompts

Bad:
```
Here's how the auth should work: [50 lines of pseudocode].
Now review whether the implementation matches.
```

Good:
```xml
<task>
Review the auth implementation against the acceptance criteria in the spec.
</task>
```

**Why it matters**: Implementation details in review prompts bias the reviewer toward the author's mental model instead of finding independent issues.
