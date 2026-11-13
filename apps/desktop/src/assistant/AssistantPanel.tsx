import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { create } from 'zustand';
import { Button } from '@pv/ui';
import { toCommandError } from '../commands/runCommand';
import { api, unwrap } from '../ipc';
import { useSettingsStore } from '../store/settings';
import { useToastStore } from '../store/toast';

interface Message {
  role: 'user' | 'assistant';
  text: string;
  actions?: string[];
}

interface AssistantState {
  open: boolean;
  messages: Message[];
  costUsd: number;
  setOpen: (open: boolean) => void;
  push: (m: Message, cost?: number) => void;
  reset: () => void;
}

/** The assistant's conversation (UI copy; the full one, with tool calls, stays in Rust). */
export const useAssistantStore = create<AssistantState>()((set) => ({
  open: false,
  messages: [],
  costUsd: 0,
  setOpen: (open) => {
    set({ open });
  },
  push: (m, cost = 0) => {
    set((s) => ({ messages: [...s.messages, m], costUsd: s.costUsd + cost }));
  },
  reset: () => {
    set({ messages: [], costUsd: 0 });
  },
}));

/**
 * Floating assistant: technical questions answered from the project's computed values, and
 * building on request (buildings, layout, mounting system, inverters, strings). Each change
 * is one undoable step authored by the copilot. Uses the user's own API key.
 */
export function AssistantPanel() {
  const { t, i18n } = useTranslation();
  const { open, messages, costUsd, setOpen, push, reset } = useAssistantStore();
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [configured, setConfigured] = useState<boolean | null>(null);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    unwrap(api.apiKeyStatus())
      .then((s) => {
        setConfigured(s.configured);
      })
      .catch(() => {
        setConfigured(false);
      });
  }, [open]);
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' });
  }, [messages]);

  if (!open) return null;
  const fmt = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 3 });

  const ask = async () => {
    const question = text.trim();
    if (!question || busy) return;
    setText('');
    push({ role: 'user', text: question });
    setBusy(true);
    try {
      const r = await unwrap(api.assistantAsk(question, i18n.language));
      push({ role: 'assistant', text: r.text, actions: r.actions }, r.costUsd);
    } catch (e) {
      useToastStore.getState().push(toCommandError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="assistant" aria-label={t('assistant.title')}>
      <header className="assistant__header">
        <h2 className="assistant__title">{t('assistant.title')}</h2>
        <span className="panel-hint">{t('assistant.cost', { cost: fmt.format(costUsd) })}</span>
        <button
          type="button"
          className="link-button"
          onClick={() => {
            reset();
            void unwrap(api.assistantReset());
          }}
        >
          {t('assistant.new')}
        </button>
        <button
          type="button"
          className="toast__close"
          aria-label={t('assistant.close')}
          onClick={() => {
            setOpen(false);
          }}
        >
          ×
        </button>
      </header>
      <div className="assistant__messages">
        {messages.length === 0 && <p className="panel-hint">{t('assistant.intro')}</p>}
        {messages.map((m, i) => (
          <div key={i} className={`assistant__msg assistant__msg--${m.role}`}>
            <p>{m.text}</p>
            {m.actions && m.actions.length > 0 && (
              <small className="panel-hint">
                {t('assistant.actions', { list: m.actions.join(', ') })}
              </small>
            )}
          </div>
        ))}
        {busy && <p className="panel-hint">{t('assistant.thinking')}</p>}
        <div ref={end} />
      </div>
      {configured === false ? (
        <div className="assistant__nokey">
          <p className="panel-hint">{t('assistant.noKey')}</p>
          <Button
            variant="secondary"
            onClick={() => {
              useSettingsStore.getState().setOpen(true);
            }}
          >
            {t('settings.open')}
          </Button>
        </div>
      ) : (
        <form
          className="assistant__form"
          onSubmit={(e) => {
            e.preventDefault();
            void ask();
          }}
        >
          <textarea
            className="form-input"
            rows={2}
            value={text}
            placeholder={t('assistant.placeholder')}
            aria-label={t('assistant.placeholder')}
            onChange={(e) => {
              setText(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                void ask();
              }
            }}
          />
          <Button disabled={busy || text.trim() === ''}>{t('assistant.send')}</Button>
        </form>
      )}
      <p className="assistant__privacy">{t('assistant.privacy')}</p>
    </section>
  );
}
