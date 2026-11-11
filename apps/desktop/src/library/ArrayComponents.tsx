import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { ArrayChoices_Serialize, CommandError, PanelArray } from '../bindings';
import { api, unwrap } from '../ipc';
import { runApplied, toCommandError } from '../commands/runCommand';
import { useProjectStore } from '../store/project';
import { useToastStore } from '../store/toast';
import { useLibraryStore } from '../store/library';

/**
 * Module and mounting system of an array, chosen from the library. Systems incompatible
 * with the module or the roof are listed but disabled, with the reason.
 */
export function ArrayComponents({ array }: { array: PanelArray }) {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const version = useLibraryStore((s) => s.version);
  const [choices, setChoices] = useState<ArrayChoices_Serialize | null>(null);
  const moduleId = useId();
  const systemId = useId();

  useEffect(() => {
    let live = true;
    unwrap(api.arrayChoices(array.id))
      .then((c) => {
        if (live) setChoices(c);
      })
      .catch((e: unknown) => {
        useToastStore.getState().push(toCommandError(e));
      });
    return () => {
      live = false;
    };
  }, [array.id, summary, version]);

  const assign = (module: string | null, mounting: string | null) =>
    runApplied('apply', () => unwrap(api.assignComponents(array.id, module, mounting)));
  const reason = (issues: CommandError[]) =>
    issues.map((i) => t(i.key, i.params as Record<string, unknown>)).join(' ');

  if (!choices) return null;
  return (
    <div className="form-grid array-components">
      <div className="pv-field form-field">
        <label htmlFor={moduleId} className="pv-field__label">
          {t('arrays.module')}
        </label>
        <select
          id={moduleId}
          className="form-input"
          value={array.module ?? ''}
          onChange={(e) => {
            const m = e.target.value || null;
            void assign(m, m ? (array.mounting ?? null) : null);
          }}
        >
          <option value="">{t('arrays.genericModule')}</option>
          {choices.modules.map(({ summary: s }) => (
            <option key={s.id} value={s.id}>
              {s.label}
              {s.verified ? '' : ` (${t('library.unverified')})`}
            </option>
          ))}
        </select>
      </div>
      <div className="pv-field form-field">
        <label htmlFor={systemId} className="pv-field__label">
          {t('arrays.mounting')}
        </label>
        <select
          id={systemId}
          className="form-input"
          value={array.mounting ?? ''}
          disabled={!array.module}
          onChange={(e) => {
            void assign(array.module ?? null, e.target.value || null);
          }}
        >
          <option value="">{t('arrays.noMounting')}</option>
          {choices.systems.map(({ summary: s, issues }) => (
            <option key={s.id} value={s.id} disabled={issues.length > 0} title={reason(issues)}>
              {s.label}
              {issues.length > 0 ? ` — ${reason(issues)}` : ''}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
