import { useTranslation } from 'react-i18next';
import { Select, Toggle } from '@pv/ui';
import { SUPPORTED_LANGUAGES, type Language } from '@pv/locales';
import { isLanguage, setLanguage } from '../i18n';
import { useUiStore } from '../store/ui';

interface TopBarProps {
  projectName: string | null;
}

export function TopBar({ projectName }: TopBarProps) {
  const { t, i18n } = useTranslation();
  const theme = useUiStore((s) => s.theme);
  const setTheme = useUiStore((s) => s.setTheme);
  const current: Language = isLanguage(i18n.language) ? i18n.language : 'en';

  return (
    <header className="topbar">
      <h1 className="topbar__title">{t('app.title')}</h1>
      <span className="topbar__project">{projectName ?? t('app.noProject')}</span>
      <div className="topbar__spacer" />
      <div className="topbar__controls">
        <Select<Language>
          label={t('topbar.language')}
          value={current}
          options={SUPPORTED_LANGUAGES.map((lng) => ({ value: lng, label: t(`languages.${lng}`) }))}
          onChange={(lng) => void setLanguage(lng)}
        />
        <Toggle
          label={t('topbar.theme.label')}
          checked={theme === 'dark'}
          onChange={(dark) => {
            setTheme(dark ? 'dark' : 'light');
          }}
        />
      </div>
    </header>
  );
}
