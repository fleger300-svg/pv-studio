// CLI: `pnpm i18n:check`. Exits with code 1 on any missing/extra key or forbidden character.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkParity, type LocaleTree } from '../src/parity.ts';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const load = (lang: string): LocaleTree =>
  JSON.parse(readFileSync(join(root, `${lang}.json`), 'utf8')) as LocaleTree;

const issues = checkParity({ en: load('en'), fr: load('fr'), de: load('de') });
if (issues.length > 0) {
  for (const i of issues) console.error(`[${i.locale}] ${i.kind}: ${i.key}`);
  console.error(`i18n check failed: ${issues.length} issue(s)`);
  process.exit(1);
}
console.log('i18n check passed: fr, en, de have identical keys');
