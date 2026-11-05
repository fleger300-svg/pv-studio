import type { Language } from '@pv/locales';
import { createI18n, isLanguage } from './config';

export { isLanguage };

const STORAGE_KEY = 'pv.language';

function initialLanguage(): Language {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored && isLanguage(stored)) return stored;
  const nav = navigator.language.slice(0, 2).toLowerCase();
  return isLanguage(nav) ? nav : 'fr';
}

const i18n = createI18n(initialLanguage());
document.documentElement.lang = i18n.language;

/** Switches the UI language instantly (no reload) and remembers the choice. */
export async function setLanguage(lang: Language): Promise<void> {
  await i18n.changeLanguage(lang);
  localStorage.setItem(STORAGE_KEY, lang);
  document.documentElement.lang = lang;
}

export default i18n;
