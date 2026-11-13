import { useTranslation } from 'react-i18next';
import { Button, Select, Toggle } from '@pv/ui';
import { useLibraryStore } from '../store/library';
import { useSettingsStore } from '../store/settings';
import { SUPPORTED_LANGUAGES, type Language } from '@pv/locales';
import { isLanguage, setLanguage } from '../i18n';
import { useUiStore } from '../store/ui';
import { useStaticsStore } from '../store/statics';
import { useAssistantStore } from '../assistant/AssistantPanel';

interface TopBarProps {
  projectName: string | null;
}

export function TopBar({ projectName }: TopBarProps) {
  const { t, i18n } = useTranslation();
  const theme = useUiStore((s) => s.theme);
  const setTheme = useUiStore((s) => s.setTheme);
  const current: Language = isLanguage(i18n.language) ? i18n.language : 'en';
  const computing = useStaticsStore((s) => s.busy);
  const libraryOpen = useLibraryStore((s) => s.open);
  const setLibraryOpen = useLibraryStore((s) => s.setOpen);
  const settingsOpen = useSettingsStore((s) => s.open);
  const setSettingsOpen = useSettingsStore((s) => s.setOpen);
  const assistantOpen = useAssistantStore((s) => s.open);
  const setAssistantOpen = useAssistantStore((s) => s.setOpen);

  return (
    <header className="topbar">
      <h1 className="topbar__title">{t('app.title')}</h1>
      <span className="topbar__project">{projectName ?? t('app.noProject')}</span>
      <div className="topbar__spacer" />
      {computing && (
        <span className="topbar__badge" role="status">
          {t('statics.computing')}
        </span>
      )}
      <div className="topbar__controls">
        <Button
          variant={assistantOpen ? 'primary' : 'secondary'}
          aria-pressed={assistantOpen}
          onClick={() => {
            setAssistantOpen(!assistantOpen);
          }}
        >
          {t('assistant.title')}
        </Button>
        <Button
          variant={libraryOpen ? 'primary' : 'secondary'}
          aria-pressed={libraryOpen}
          onClick={() => {
            setLibraryOpen(!libraryOpen);
          }}
        >
          {t('library.title')}
        </Button>
        <Button
          variant={settingsOpen ? 'primary' : 'secondary'}
          aria-pressed={settingsOpen}
          onClick={() => {
            setSettingsOpen(!settingsOpen);
          }}
        >
          {t('settings.open')}
        </Button>
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
