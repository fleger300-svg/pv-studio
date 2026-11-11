import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel } from '@pv/ui';
import type { ApiKeyStatus_Serialize, KeyCheck } from '../bindings';
import { api, unwrap } from '../ipc';
import { toCommandError } from '../commands/runCommand';
import { useSettingsStore } from '../store/settings';
import { useToastStore } from '../store/toast';

function report(e: unknown) {
  useToastStore.getState().push(toCommandError(e));
}

/** Settings: for now the Anthropic API key (datasheet reading). */
export function SettingsView() {
  const { t } = useTranslation();
  const setOpen = useSettingsStore((s) => s.setOpen);
  return (
    <section className="library settings" aria-label={t('settings.title')}>
      <header className="library__bar">
        <h2 className="library__title">{t('settings.title')}</h2>
        <div className="topbar__spacer" />
        <Button
          onClick={() => {
            setOpen(false);
          }}
        >
          {t('settings.close')}
        </Button>
      </header>
      <div className="settings__body">
        <ApiKeySection />
      </div>
    </section>
  );
}

/**
 * The key goes in once and never comes out: after saving, the field is cleared and only the
 * status (source, last four characters) is shown.
 */
function ApiKeySection() {
  const { t } = useTranslation();
  const id = useId();
  const [status, setStatus] = useState<ApiKeyStatus_Serialize | null>(null);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    unwrap(api.apiKeyStatus()).then(setStatus).catch(report);
  }, []);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setMessage(null);
    try {
      await action();
    } catch (e) {
      report(e);
    } finally {
      setBusy(false);
    }
  };

  const save = () =>
    run(async () => {
      setStatus(await unwrap(api.setApiKey(draft)));
      setDraft('');
      setMessage(t('settings.ai.saved'));
    });
  const remove = () =>
    run(async () => {
      setStatus(await unwrap(api.deleteApiKey()));
      setMessage(t('settings.ai.removed'));
    });
  const test = () =>
    run(async () => {
      const r: KeyCheck = await unwrap(api.testApiKey());
      setMessage(t(`settings.ai.result.${r}`));
    });

  const source = status?.source ? t(`settings.ai.sources.${status.source}`) : '';
  return (
    <Panel title={t('settings.ai.title')}>
      <p className="settings__status" role="status">
        {status?.configured
          ? t('settings.ai.configured', { hint: status.hint ?? '', source })
          : t('settings.ai.notConfigured')}
      </p>
      {status?.source === 'environment' && (
        <p className="panel-hint">{t('settings.ai.envOverride')}</p>
      )}
      {status && !status.storeAvailable && (
        <p className="panel-hint">{t('settings.ai.storeUnavailable')}</p>
      )}
      <div className="pv-field form-field">
        <label htmlFor={id} className="pv-field__label">
          {t('settings.ai.key')}
        </label>
        <input
          id={id}
          className="form-input"
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={t('settings.ai.placeholder')}
          value={draft}
          disabled={busy || !status?.storeAvailable}
          onChange={(e) => {
            setDraft(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && draft.trim()) void save();
          }}
        />
      </div>
      <div className="library__actions">
        <Button disabled={busy || draft.trim() === ''} onClick={() => void save()}>
          {t('settings.ai.save')}
        </Button>
        <Button
          variant="secondary"
          disabled={busy || !status?.configured}
          onClick={() => void test()}
        >
          {busy ? t('settings.ai.testing') : t('settings.ai.test')}
        </Button>
        <Button
          variant="secondary"
          disabled={busy || status?.source !== 'credentialStore'}
          onClick={() => void remove()}
        >
          {t('settings.ai.remove')}
        </Button>
      </div>
      {message && (
        <p className="library__report" role="status">
          {message}
        </p>
      )}
      <p className="panel-hint">{t('settings.ai.privacy')}</p>
      <p className="panel-hint">{t('settings.ai.help')}</p>
    </Panel>
  );
}
