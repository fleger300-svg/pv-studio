import fr from '../fr.json';
import en from '../en.json';
import de from '../de.json';

export const SUPPORTED_LANGUAGES = ['fr', 'en', 'de'] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];

export const resources = {
  fr: { translation: fr },
  en: { translation: en },
  de: { translation: de },
} as const;

export { checkParity, flattenKeys } from './parity.ts';
export type { LocaleTree, ParityIssue } from './parity.ts';
