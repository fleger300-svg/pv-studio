import { createInstance, type i18n as I18n } from 'i18next';
import ICU from 'i18next-icu';
import { initReactI18next } from 'react-i18next';
import { resources, SUPPORTED_LANGUAGES, type Language } from '@pv/locales';

export function isLanguage(value: string): value is Language {
  return (SUPPORTED_LANGUAGES as readonly string[]).includes(value);
}

/** Creates a synchronously initialised i18next instance with ICU message format. */
export function createI18n(lng: Language): I18n {
  const instance = createInstance();
  void instance
    .use(ICU)
    .use(initReactI18next)
    .init({
      resources,
      lng,
      fallbackLng: 'en',
      supportedLngs: [...SUPPORTED_LANGUAGES],
      interpolation: { escapeValue: false },
      initAsync: false,
    });
  return instance;
}
