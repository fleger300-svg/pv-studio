import { describe, expect, it } from 'vitest';
import { createI18n, isLanguage } from './config';

describe('i18n config', () => {
  it('formats ICU messages in every language', () => {
    expect(createI18n('fr').t('workflow.step', { index: 3, label: 'Calepinage' })).toBe(
      '3. Calepinage',
    );
    expect(createI18n('de').t('workflow.layout')).toBe('Belegung');
    expect(createI18n('en').t('workflow.layout')).toBe('Layout');
  });

  it('switches language on the same instance without reload', async () => {
    const i18n = createI18n('en');
    await i18n.changeLanguage('fr');
    expect(i18n.t('workflow.roof')).toBe('Toiture');
  });

  it('recognises supported languages only', () => {
    expect(isLanguage('de')).toBe(true);
    expect(isLanguage('it')).toBe(false);
  });
});
