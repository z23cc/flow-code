---
name: flow-code-i18n
description: "Use when adding multi-language support, locale formatting, RTL layouts, or any internationalization/localization work."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: i18n,l10n,internationalization,localization -->

# Internationalization (i18n)

## Overview

Build software that works across languages, regions, and cultures from day one. i18n is architecture — retrofitting it is 10x harder than building it in. Separate content from code, respect locale conventions, and test with real translations.

## When to Use

- Adding multi-language support to an application
- Formatting dates, numbers, or currencies for different locales
- Supporting RTL (right-to-left) languages
- Extracting hardcoded strings for translation
- Setting up translation workflow (keys, files, tools)

**When NOT to use:**
- Internal tools with a single-locale team
- Backend services with no user-facing strings

## Core Principles

### Separate Content from Code

```typescript
// Bad: hardcoded string
<button>Submit Order</button>

// Good: translation key
<button>{t('order.submit')}</button>
```

### Use ICU Message Format

```
# Simple
greeting = Hello, {name}!

# Plural
items = {count, plural,
  =0 {No items}
  one {1 item}
  other {{count} items}
}

# Select (gender, type, etc.)
greeting = {gender, select,
  male {He joined}
  female {She joined}
  other {They joined}
}
```

### Translation File Structure

```
locales/
  en/
    common.json     # Shared strings
    auth.json       # Login, register
    orders.json     # Order-specific
  zh-CN/
    common.json
    auth.json
    orders.json
  ar/               # RTL language
    common.json
```

**Key naming:** `namespace.section.action` → `order.checkout.submit_button`

## Date, Number, Currency Formatting

**Never format manually. Use `Intl` APIs:**

```typescript
// Date
new Intl.DateTimeFormat('zh-CN', { dateStyle: 'long' }).format(date)
// → "2026年4月8日"

// Number
new Intl.NumberFormat('de-DE').format(1234567.89)
// → "1.234.567,89"

// Currency
new Intl.NumberFormat('ja-JP', { style: 'currency', currency: 'JPY' }).format(1000)
// → "¥1,000"

// Relative time
new Intl.RelativeTimeFormat('en', { numeric: 'auto' }).format(-1, 'day')
// → "yesterday"
```

**Rules:**
- Never concatenate strings for sentences (word order varies by language)
- Never assume date format (MM/DD vs DD/MM)
- Store dates in UTC, display in user's timezone
- Use ISO 8601 for date exchange between systems
- Currencies need locale AND currency code (not just symbol)

## RTL Support

```css
/* Use logical properties (not left/right) */
.card {
  margin-inline-start: 1rem;  /* Good: respects text direction */
  padding-inline-end: 0.5rem;
  text-align: start;
}

/* Don't use physical properties */
.card {
  margin-left: 1rem;          /* Bad: breaks in RTL */
  text-align: left;
}
```

**Rules:**
- Set `dir="auto"` on user-generated content
- Use `start`/`end` instead of `left`/`right` in CSS
- Mirror icons that indicate direction (arrows, not universal symbols)
- Test with actual RTL text (Arabic, Hebrew), not just `dir="rtl"`

## Framework Integration

### React (react-i18next)
```typescript
import { useTranslation } from 'react-i18next';

function OrderButton({ count }: { count: number }) {
  const { t } = useTranslation('orders');
  return <button>{t('checkout.submit', { count })}</button>;
}
```

### Next.js (next-intl / built-in)
```typescript
// Use middleware for locale detection
// app/[locale]/layout.tsx for locale-scoped routes
```

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "We only need English" | Requirements change. Retrofitting i18n is a full rewrite of every UI string. |
| "We'll translate later" | Extracting hardcoded strings from 500 files is a month-long project. Start with keys. |
| "Date formatting is simple" | MM/DD/YYYY is US-only. The rest of the world uses different formats. Use Intl. |
| "RTL is rare" | Arabic and Hebrew serve 400M+ speakers. If you have users there, you need it. |

## Red Flags

- Hardcoded strings in UI components
- String concatenation for sentences (`"Hello " + name + ", you have " + count + " items"`)
- Manual date formatting (`month + "/" + day + "/" + year`)
- Physical CSS properties (`margin-left` instead of `margin-inline-start`)
- Locale info hardcoded or not configurable
- Translation keys that are full English sentences (fragile, hard to maintain)
- Missing plural forms (many languages have >2 plural categories)

## Verification

- [ ] All user-facing strings use translation keys (no hardcoded text)
- [ ] ICU message format for plurals and interpolation
- [ ] Dates, numbers, currencies use `Intl` APIs (not manual formatting)
- [ ] CSS uses logical properties (`inline-start`/`inline-end`, not `left`/`right`)
- [ ] Translation files organized by namespace
- [ ] Tested with at least one non-English locale
- [ ] RTL layout tested if supporting Arabic/Hebrew
