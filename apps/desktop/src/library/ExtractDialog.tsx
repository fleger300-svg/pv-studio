import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel } from '@pv/ui';
import type { ExtractionResult } from '../bindings';
import { api, unwrap } from '../ipc';
import { toCommandError } from '../commands/runCommand';
import { useSettingsStore } from '../store/settings';
import { useToastStore } from '../store/toast';
import { TextInput } from './inputs';

/**
 * Copilot: a component read from its datasheet by Claude. The user is told the PDF leaves the
 * machine; the result is a new, unverified component (revision 1 by the copilot) that the
 * user checks and marks verified in a revision of their own.
 */
export function ExtractDialog({
  onClose,
  onExtracted,
}: {
  onClose: () => void;
  onExtracted: (r: ExtractionResult) => void;
}) {
  const { t, i18n } = useTranslation();
  const [hint, setHint] = useState('');
  const [busy, setBusy] = useState(false);
  const settingsOpen = useSettingsStore((s) => s.open);
  const openSettings = useSettingsStore((s) => s.setOpen);
  // Whether a key is configured; checked again when the settings close.
  const [configured, setConfigured] = useState<boolean | null>(null);
  useEffect(() => {
    if (settingsOpen) return;
    unwrap(api.apiKeyStatus())
      .then((s) => {
        setConfigured(s.configured);
      })
      .catch(() => {
        setConfigured(false);
      });
  }, [settingsOpen]);

  const run = async (path: string | null = null) => {
    setBusy(true);
    try {
      const r = await unwrap(api.libraryExtractDatasheet(path, hint || null, i18n.language));
      if (r) onExtracted(r);
    } catch (e) {
      useToastStore.getState().push(toCommandError(e));
    } finally {
      setBusy(false);
    }
  };

  // Development builds: automated checks choose the PDF without the native dialog.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    Object.assign(window, { __pvExtractFile: (path: string) => run(path) });
  });

  return (
    <div
      className="library__dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t('library.extract.title')}
    >
      <Panel title={t('library.extract.title')}>
        <p className="panel-hint">{t('library.extract.privacy')}</p>
        {configured === false && (
          <p className="library__report" role="status">
            {t('settings.ai.notConfigured')}
          </p>
        )}
        <TextInput label={t('library.extract.hint')} value={hint} onChange={setHint} />
        <div className="library__actions">
          {configured === false ? (
            <Button
              onClick={() => {
                openSettings(true);
              }}
            >
              {t('library.extract.configure')}
            </Button>
          ) : (
            <Button disabled={busy || configured === null} onClick={() => void run()}>
              {busy ? t('library.extract.running') : t('library.extract.run')}
            </Button>
          )}
          <Button variant="secondary" disabled={busy} onClick={onClose}>
            {t('library.close')}
          </Button>
        </div>
        {busy && (
          <p className="panel-hint" role="status">
            {t('library.extract.running')}
          </p>
        )}
      </Panel>
    </div>
  );
}
