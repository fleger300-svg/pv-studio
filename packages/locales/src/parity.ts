/** A locale file: nested objects whose leaves are ICU message strings. */
export interface LocaleTree {
  [key: string]: string | LocaleTree;
}

export interface ParityIssue {
  locale: string;
  kind: 'missing' | 'extra' | 'notString' | 'forbiddenChar';
  key: string;
}

/** Flattens a nested locale tree into dotted keys (`a.b.c`). */
export function flattenKeys(tree: LocaleTree, prefix = ''): Map<string, unknown> {
  const out = new Map<string, unknown>();
  for (const [k, v] of Object.entries(tree)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (typeof v === 'object') {
      for (const [ck, cv] of flattenKeys(v, key)) out.set(ck, cv);
    } else {
      out.set(key, v);
    }
  }
  return out;
}

/**
 * Checks that every locale has exactly the same keys as the reference locale, that all
 * leaves are strings, and that Swiss German (`de`) never uses "ß".
 */
export function checkParity(locales: Record<string, LocaleTree>, reference = 'en'): ParityIssue[] {
  const ref = locales[reference];
  if (!ref) throw new Error(`reference locale "${reference}" not provided`);
  const refKeys = new Set(flattenKeys(ref).keys());
  const issues: ParityIssue[] = [];
  for (const [locale, tree] of Object.entries(locales)) {
    const flat = flattenKeys(tree);
    for (const key of refKeys) {
      if (!flat.has(key)) issues.push({ locale, kind: 'missing', key });
    }
    for (const [key, value] of flat) {
      if (!refKeys.has(key)) issues.push({ locale, kind: 'extra', key });
      if (typeof value !== 'string') issues.push({ locale, kind: 'notString', key });
      else if (locale === 'de' && value.includes('ß'))
        issues.push({ locale, kind: 'forbiddenChar', key });
    }
  }
  return issues;
}
