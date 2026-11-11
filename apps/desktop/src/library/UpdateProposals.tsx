import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@pv/ui';
import { api, unwrap } from '../ipc';
import { runApplied } from '../commands/runCommand';
import { useLibraryStore } from '../store/library';
import { useProjectStore } from '../store/project';

/**
 * Banner of the project's update proposals: newer library versions (data or prices) of its
 * frozen components. Nothing changes until the user accepts (one command, undoable).
 */
export function UpdateProposals() {
  const { t, i18n } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const proposals = useLibraryStore((s) => s.proposals);
  const refresh = useLibraryStore((s) => s.refreshProposals);
  const [open, setOpen] = useState(false);
  const [chosen, setChosen] = useState<Set<string> | null>(null);

  useEffect(() => {
    if (summary) void refresh();
  }, [summary, refresh]);

  if (!summary || proposals.length === 0) return null;
  const selected = chosen ?? new Set(proposals.map((p) => p.component));
  const money = (cents: number | undefined, currency: string | undefined) =>
    cents === undefined || !currency
      ? '—'
      : new Intl.NumberFormat(i18n.language, { style: 'currency', currency }).format(cents / 100);

  const accept = async () => {
    const ids = proposals.map((p) => p.component).filter((id) => selected.has(id));
    const r = await runApplied('apply', () => unwrap(api.acceptUpdates(ids)));
    if (r) {
      setOpen(false);
      setChosen(null);
      await refresh();
    }
  };

  return (
    <section className="proposals" aria-label={t('library.updates.title')}>
      <div className="proposals__bar">
        <span role="status">{t('library.updates.available', { count: proposals.length })}</span>
        <Button
          variant="secondary"
          onClick={() => {
            setOpen(!open);
          }}
        >
          {open ? t('library.updates.hide') : t('library.updates.show')}
        </Button>
      </div>
      {open && (
        <div className="proposals__list">
          <ul>
            {proposals.map((p) => (
              <li key={p.component}>
                <label className="form-check">
                  <input
                    type="checkbox"
                    checked={selected.has(p.component)}
                    onChange={(e) => {
                      const next = new Set(selected);
                      if (e.target.checked) next.add(p.component);
                      else next.delete(p.component);
                      setChosen(next);
                    }}
                  />
                  <strong>{p.label}</strong>
                  {p.toRevision !== p.fromRevision &&
                    ` · ${t('library.updates.revision', { from: p.fromRevision, to: p.toRevision })}`}
                </label>
                <ul className="proposals__changes">
                  {p.changes.map((c) => (
                    <li key={c.path}>
                      <code>{c.path}</code> : {c.before ?? '—'} → {c.after ?? '—'}
                    </li>
                  ))}
                  {JSON.stringify(p.priceBefore) !== JSON.stringify(p.priceAfter) && (
                    <li>
                      {t('library.updates.price', {
                        before: money(p.priceBefore?.listPriceCents, p.priceBefore?.currency),
                        after: money(p.priceAfter?.listPriceCents, p.priceAfter?.currency),
                      })}
                    </li>
                  )}
                </ul>
              </li>
            ))}
          </ul>
          <Button disabled={selected.size === 0} onClick={() => void accept()}>
            {t('library.updates.accept', { count: selected.size })}
          </Button>
        </div>
      )}
    </section>
  );
}
