import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel, Select, Stat } from '@pv/ui';
import type {
  ComponentDetail_Serialize,
  ComponentFamily,
  ComponentSummary_Serialize,
  Component_Serialize,
  Currency,
  ExtractionResult,
  ModuleElectrical,
  PartKind_Serialize,
} from '../bindings';
import { api, unwrap } from '../ipc';
import { toCommandError } from '../commands/runCommand';
import { useLibraryStore } from '../store/library';
import { useToastStore } from '../store/toast';
import { NumInput, TextInput } from './inputs';
import { ImportDialog } from './ImportDialog';
import { ExtractDialog } from './ExtractDialog';
import { base64ToBytes } from './priceList';
import { SpecSummary } from './SpecSummary';

type Family = ComponentFamily | 'all';

function report(e: unknown) {
  useToastStore.getState().push(toCommandError(e));
}

/** Today, `YYYY-MM-DD` (local date). */
export function today(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return `${String(d.getFullYear())}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** The component library: search, edit (each save is a revision), prices, datasheets. */
export function LibraryView() {
  const { t } = useTranslation();
  const setOpen = useLibraryStore((s) => s.setOpen);
  const selected = useLibraryStore((s) => s.selected);
  const select = useLibraryStore((s) => s.select);
  const version = useLibraryStore((s) => s.version);
  const changed = useLibraryStore((s) => s.changed);
  const [text, setText] = useState('');
  const [family, setFamily] = useState<Family>('all');
  const [archived, setArchived] = useState(false);
  const [rows, setRows] = useState<ComponentSummary_Serialize[]>([]);
  const [draft, setDraft] = useState<Component_Serialize | null>(null);
  const [importing, setImporting] = useState(false);
  const [extracting, setExtracting] = useState(false);
  const [extracted, setExtracted] = useState<ExtractionResult | null>(null);

  useEffect(() => {
    let live = true;
    const query = {
      text: text.trim() === '' ? null : text,
      family: family === 'all' ? null : family,
      includeArchived: archived,
      limit: 500,
    };
    unwrap(api.librarySearch(query))
      .then((r) => {
        if (live) setRows(r);
      })
      .catch(report);
    return () => {
      live = false;
    };
  }, [text, family, archived, version]);

  const startNew = async (f: ComponentFamily) => {
    try {
      select(null);
      setDraft(await unwrap(api.libraryNewComponent(f)));
    } catch (e) {
      report(e);
    }
  };

  return (
    <section className="library" aria-label={t('library.title')}>
      <header className="library__bar">
        <h2 className="library__title">{t('library.title')}</h2>
        <input
          className="form-input library__search"
          type="search"
          placeholder={t('library.search')}
          aria-label={t('library.search')}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
          }}
        />
        <Select<Family>
          label={t('library.family')}
          value={family}
          options={(
            [
              'all',
              'module',
              'inverter',
              'optimizer',
              'battery',
              'mountingSystem',
              'mountingPart',
              'equipment',
            ] as const
          ).map((f) => ({
            value: f,
            label: t(`library.families.${f}`),
          }))}
          onChange={setFamily}
        />
        <label className="form-check">
          <input
            type="checkbox"
            checked={archived}
            onChange={(e) => {
              setArchived(e.target.checked);
            }}
          />
          {t('library.showArchived')}
        </label>
        <div className="topbar__spacer" />
        <Button variant="secondary" onClick={() => void startNew('module')}>
          {t('library.newModule')}
        </Button>
        <Button variant="secondary" onClick={() => void startNew('mountingPart')}>
          {t('library.newPart')}
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            setImporting(true);
          }}
        >
          {t('library.import.open')}
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            setExtracting(true);
          }}
        >
          {t('library.extract.open')}
        </Button>
        <Button
          onClick={() => {
            setOpen(false);
          }}
        >
          {t('library.close')}
        </Button>
      </header>
      <div className="library__body">
        <ul className="library__list" aria-label={t('library.results', { count: rows.length })}>
          {rows.length === 0 && <li className="panel-hint">{t('library.empty')}</li>}
          {rows.map((r) => (
            <li key={r.id}>
              <button
                type="button"
                className={
                  r.id === selected ? 'library__row library__row--selected' : 'library__row'
                }
                aria-pressed={r.id === selected}
                onClick={() => {
                  setDraft(null);
                  select(r.id);
                }}
              >
                <span className="library__row-label">{r.label}</span>
                <span className="library__row-meta">
                  {t(`library.families.${r.family}`)} · {r.manufacturer}
                  {r.articleNo ? ` · ${r.articleNo}` : ''}
                  {r.pmaxW ? ` · ${String(r.pmaxW)} W` : ''}
                </span>
                {!r.verified && <span className="library__badge">{t('library.unverified')}</span>}
                {r.source === 'aiExtracted' && (
                  <span className="library__badge">{t('library.extract.source')}</span>
                )}
                {r.archived && <span className="library__badge">{t('library.archived')}</span>}
              </button>
            </li>
          ))}
        </ul>
        <div className="library__detail">
          {draft ? (
            <ComponentEditor
              key={draft.id}
              initial={draft}
              isNew
              onSaved={(c) => {
                setDraft(null);
                changed();
                select(c.id);
              }}
            />
          ) : selected ? (
            <>
              {extracted?.component.id === selected && (
                <div className="library__report" role="status">
                  <p>{t('library.extract.done')}</p>
                  {extracted.warnings.length > 0 && (
                    <>
                      <strong>{t('library.extract.warnings')}</strong>
                      <ul>
                        {extracted.warnings.map((w) => (
                          <li key={w}>{w}</li>
                        ))}
                      </ul>
                    </>
                  )}
                </div>
              )}
              <ComponentDetailView
                id={selected}
                version={version}
                onDuplicate={(c) => {
                  select(null);
                  setDraft(c);
                }}
              />
            </>
          ) : (
            <p className="panel-hint">{t('library.pick')}</p>
          )}
        </div>
      </div>
      {importing && (
        <ImportDialog
          onClose={() => {
            setImporting(false);
          }}
        />
      )}
      {extracting && (
        <ExtractDialog
          onClose={() => {
            setExtracting(false);
          }}
          onExtracted={(r) => {
            setExtracting(false);
            setExtracted(r);
            setDraft(null);
            changed();
            select(r.component.id);
          }}
        />
      )}
    </section>
  );
}

function ComponentDetailView({
  id,
  version,
  onDuplicate,
}: {
  id: string;
  version: number;
  onDuplicate: (c: Component_Serialize) => void;
}) {
  const { t } = useTranslation();
  const changed = useLibraryStore((s) => s.changed);
  const [detail, setDetail] = useState<ComponentDetail_Serialize | null>(null);
  useEffect(() => {
    let live = true;
    unwrap(api.libraryComponent(id))
      .then((d) => {
        if (live) setDetail(d);
      })
      .catch(report);
    return () => {
      live = false;
    };
  }, [id, version]);
  if (!detail) return null;
  const c = detail.entry.component;
  return (
    <>
      <ComponentEditor
        key={`${c.id}-${String(c.revision)}`}
        initial={c}
        isNew={false}
        onSaved={() => {
          changed();
        }}
      />
      <div className="library__actions">
        <Button
          variant="secondary"
          onClick={() => {
            unwrap(api.libraryDuplicate(c.id)).then(onDuplicate).catch(report);
          }}
        >
          {t('library.duplicate')}
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            unwrap(api.libraryArchive(c.id, !detail.entry.archived)).then(changed).catch(report);
          }}
        >
          {detail.entry.archived ? t('library.restore') : t('library.archive')}
        </Button>
      </div>
      <Datasheet component={c} />
      <PriceTable componentId={c.id} prices={detail.prices} />
      <Panel title={t('library.history')}>
        <ol className="library__history">
          {detail.revisions.map((r) => (
            <li key={r.revision}>
              {t('library.revision', { n: r.revision })} ·{' '}
              {r.changedAt.slice(0, 16).replace('T', ' ')} · {t(`library.authors.${r.author}`)}
              {r.note ? ` · ${r.note}` : ''}
            </li>
          ))}
        </ol>
      </Panel>
    </>
  );
}

const PART_KINDS = [
  'rail',
  'splice',
  'clamp',
  'fixation',
  'climber',
  'ballastBlock',
  'other',
] as const;
type PartKindTag = (typeof PART_KINDS)[number];

function partKindOf(tag: PartKindTag, previous: PartKind_Serialize): PartKind_Serialize {
  if (tag === previous.kind) return previous;
  switch (tag) {
    case 'rail':
      return {
        kind: 'rail',
        stock_length_m: 4.8,
        section: {
          name: '',
          areaCm2: 1,
          iyCm4: 1,
          izCm4: 1,
          wyCm3: 1,
          wzCm3: 1,
          f0Mpa: 200,
          gammaM1: 1.1,
          eMpa: 70000,
          deflectionLimitM: 0.01,
        },
      };
    case 'clamp':
      return { kind: 'clamp', frame_height: { minM: 0.025, maxM: 0.04 } };
    case 'fixation':
      return { kind: 'fixation', capacity: null };
    default:
      return { kind: tag };
  }
}

const NO_ELECTRICAL: ModuleElectrical = {
  pmaxW: 400,
  vocV: 37,
  iscA: 13.5,
  vmppV: 31,
  imppA: 12.9,
  tempCoeffPmaxPctPerK: -0.3,
  tempCoeffVocPctPerK: -0.25,
  tempCoeffIscPctPerK: 0.05,
  maxSystemVoltageV: 1500,
  bifaciality: null,
};

/** Edits a component locally; « Save » stores a new revision (or the first one). */
function ComponentEditor({
  initial,
  isNew,
  onSaved,
}: {
  initial: Component_Serialize;
  isNew: boolean;
  onSaved: (c: Component_Serialize) => void;
}) {
  const { t } = useTranslation();
  const [c, setC] = useState<Component_Serialize>(initial);
  const [note, setNote] = useState('');
  const [saving, setSaving] = useState(false);
  const dirty = useMemo(() => JSON.stringify(c) !== JSON.stringify(initial), [c, initial]);
  const set = (patch: Partial<Component_Serialize>) => {
    setC({ ...c, ...patch });
  };
  const spec = c.spec;
  const setSpec = (patch: Record<string, unknown>) => {
    setC({ ...c, spec: { ...spec, ...patch } });
  };
  const num = (v: number | null, fallback = 0) => v ?? fallback;

  const save = async () => {
    setSaving(true);
    try {
      onSaved(await unwrap(api.librarySave(c, note)));
      setNote('');
    } catch (e) {
      report(e);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Panel
      title={
        isNew
          ? t('library.newTitle', { family: t(`library.families.${spec.family}`) })
          : t('library.editTitle', { label: initial.label, n: initial.revision })
      }
    >
      <div className="form-grid">
        <TextInput
          label={t('library.fields.manufacturer')}
          value={c.manufacturer}
          required
          onChange={(manufacturer) => {
            set({ manufacturer });
          }}
        />
        <TextInput
          label={t('library.fields.model')}
          value={c.model}
          onChange={(model) => {
            set({ model });
          }}
        />
        <TextInput
          label={t('library.fields.articleNo')}
          value={c.articleNo}
          onChange={(articleNo) => {
            set({ articleNo });
          }}
        />
        <TextInput
          label={t('library.fields.label')}
          value={c.label}
          required
          onChange={(label) => {
            set({ label });
          }}
        />
      </div>
      {spec.family === 'module' && (
        <>
          <div className="form-grid">
            <NumInput
              label={t('library.fields.lengthM')}
              unit="m"
              value={spec.lengthM}
              min={0.1}
              max={5}
              onChange={(v) => {
                setSpec({ lengthM: num(v, spec.lengthM) });
              }}
            />
            <NumInput
              label={t('library.fields.widthM')}
              unit="m"
              value={spec.widthM}
              min={0.1}
              max={5}
              onChange={(v) => {
                setSpec({ widthM: num(v, spec.widthM) });
              }}
            />
            <NumInput
              label={t('library.fields.thicknessM')}
              unit="m"
              value={spec.thicknessM}
              min={0.001}
              max={0.2}
              onChange={(v) => {
                setSpec({ thicknessM: num(v, spec.thicknessM) });
              }}
            />
            <NumInput
              label={t('library.fields.massKg')}
              unit="kg"
              value={spec.massKg}
              decimals={2}
              min={0.1}
              max={200}
              onChange={(v) => {
                setSpec({ massKg: num(v, spec.massKg) });
              }}
            />
          </div>
          <label className="form-check">
            <input
              type="checkbox"
              checked={!!spec.electrical}
              onChange={(e) => {
                setSpec({ electrical: e.target.checked ? NO_ELECTRICAL : null });
              }}
            />
            {t('library.fields.electrical')}
          </label>
          {spec.electrical && (
            <ElectricalFields
              value={spec.electrical}
              onChange={(electrical) => {
                setSpec({ electrical });
              }}
            />
          )}
        </>
      )}
      {spec.family === 'mountingPart' && (
        <div className="form-grid">
          <NumInput
            label={t('library.fields.unitMassKg')}
            unit="kg"
            value={spec.unitMassKg}
            decimals={4}
            min={0}
            max={1000}
            onChange={(v) => {
              setSpec({ unitMassKg: num(v) });
            }}
          />
          <NumInput
            label={t('library.fields.packQty')}
            value={spec.packQty}
            decimals={0}
            min={1}
            max={100000}
            onChange={(v) => {
              setSpec({ packQty: Math.round(num(v, 1)) });
            }}
          />
          <Select<PartKindTag>
            label={t('library.fields.kind')}
            value={spec.kind.kind}
            options={PART_KINDS.map((k) => ({ value: k, label: t(`library.partKinds.${k}`) }))}
            onChange={(k) => {
              setSpec({ kind: partKindOf(k, spec.kind) });
            }}
          />
          {spec.kind.kind === 'rail' && (
            <NumInput
              label={t('library.fields.stock_length_m')}
              unit="m"
              value={spec.kind.stock_length_m}
              min={0.1}
              max={20}
              onChange={(v) => {
                if (spec.kind.kind === 'rail')
                  setSpec({ kind: { ...spec.kind, stock_length_m: num(v, 1) } });
              }}
            />
          )}
          {spec.kind.kind === 'clamp' && (
            <>
              <NumInput
                label={t('library.fields.frameMin')}
                unit="mm"
                value={spec.kind.frame_height.minM * 1000}
                decimals={0}
                min={0}
                max={200}
                onChange={(v) => {
                  if (spec.kind.kind === 'clamp')
                    setSpec({
                      kind: {
                        kind: 'clamp',
                        frame_height: { ...spec.kind.frame_height, minM: num(v) / 1000 },
                      },
                    });
                }}
              />
              <NumInput
                label={t('library.fields.frameMax')}
                unit="mm"
                value={spec.kind.frame_height.maxM * 1000}
                decimals={0}
                min={1}
                max={200}
                onChange={(v) => {
                  if (spec.kind.kind === 'clamp')
                    setSpec({
                      kind: {
                        kind: 'clamp',
                        frame_height: { ...spec.kind.frame_height, maxM: num(v, 1) / 1000 },
                      },
                    });
                }}
              />
            </>
          )}
        </div>
      )}
      {spec.family === 'mountingSystem' && (
        <>
          <div className="form-grid">
            <Stat
              label={t('library.fields.structure')}
              value={t(`library.structures.${spec.structure.kind}`)}
            />
            <NumInput
              label={t('library.fields.systemMass')}
              unit="kg"
              value={spec.statics.systemMassPerModuleKg}
              decimals={2}
              min={0}
              max={500}
              onChange={(v) => {
                setSpec({ statics: { ...spec.statics, systemMassPerModuleKg: num(v) } });
              }}
            />
            <NumInput
              label={t('library.fields.loadedArea')}
              unit="m²"
              value={spec.statics.loadedAreaM2}
              decimals={1}
              min={0.1}
              max={100}
              onChange={(v) => {
                setSpec({ statics: { ...spec.statics, loadedAreaM2: num(v, 10) } });
              }}
            />
          </div>
          <p className="panel-hint">{t('library.systemParts', { count: spec.parts.length })}</p>
        </>
      )}
      <SpecSummary spec={spec} />
      <label className="form-check">
        <input
          type="checkbox"
          checked={c.verified}
          onChange={(e) => {
            set({ verified: e.target.checked });
          }}
        />
        {t('library.fields.verified')}
      </label>
      <div className="library__actions">
        <TextInput label={t('library.note')} value={note} onChange={setNote} />
        <Button disabled={!dirty || saving} onClick={() => void save()}>
          {isNew ? t('library.create') : t('library.save')}
        </Button>
      </div>
    </Panel>
  );
}

function ElectricalFields({
  value: e,
  onChange,
}: {
  value: ModuleElectrical;
  onChange: (e: ModuleElectrical) => void;
}) {
  const { t } = useTranslation();
  const field = (
    key: keyof ModuleElectrical,
    unit: string,
    min: number,
    max: number,
    decimals = 2,
  ) => (
    <NumInput
      key={key}
      label={t(`library.electrical.${key}`)}
      unit={unit}
      value={e[key] ?? null}
      decimals={decimals}
      min={min}
      max={max}
      onChange={(v) => {
        onChange({ ...e, [key]: key === 'bifaciality' ? v : (v ?? e[key]) });
      }}
    />
  );
  return (
    <div className="form-grid">
      {field('pmaxW', 'W', 1, 2000, 0)}
      {field('vocV', 'V', 1, 200)}
      {field('iscA', 'A', 0.1, 50)}
      {field('vmppV', 'V', 1, 200)}
      {field('imppA', 'A', 0.1, 50)}
      {field('tempCoeffPmaxPctPerK', '%/K', -2, 2, 3)}
      {field('tempCoeffVocPctPerK', '%/K', -2, 2, 3)}
      {field('tempCoeffIscPctPerK', '%/K', -2, 2, 3)}
      {field('maxSystemVoltageV', 'V', 100, 2000, 0)}
      {field('bifaciality', '', 0, 1)}
    </div>
  );
}

function Datasheet({ component }: { component: Component_Serialize }) {
  const { t } = useTranslation();
  const changed = useLibraryStore((s) => s.changed);
  const [url, setUrl] = useState<string | null>(null);
  useEffect(
    () => () => {
      if (url) URL.revokeObjectURL(url);
    },
    [url],
  );
  const show = async () => {
    if (!component.datasheet) return;
    try {
      const b64 = await unwrap(api.libraryDatasheet(component.datasheet));
      if (!b64) return;
      setUrl(URL.createObjectURL(new Blob([base64ToBytes(b64)], { type: 'application/pdf' })));
    } catch (e) {
      report(e);
    }
  };
  return (
    <Panel title={t('library.datasheet.title')}>
      <div className="library__actions">
        <Button
          variant="secondary"
          onClick={() => {
            unwrap(api.libraryAttachDatasheet(component.id, null))
              .then((c) => {
                if (c) changed();
              })
              .catch(report);
          }}
        >
          {t('library.datasheet.attach')}
        </Button>
        {component.datasheet && (
          <Button variant="secondary" onClick={() => void show()}>
            {t('library.datasheet.show')}
          </Button>
        )}
        {!component.datasheet && <span className="panel-hint">{t('library.datasheet.none')}</span>}
      </div>
      {url && <iframe className="library__pdf" title={t('library.datasheet.title')} src={url} />}
    </Panel>
  );
}

function PriceTable({
  componentId,
  prices,
}: {
  componentId: string;
  prices: ComponentDetail_Serialize['prices'];
}) {
  const { t, i18n } = useTranslation();
  const changed = useLibraryStore((s) => s.changed);
  const [supplier, setSupplier] = useState(prices.at(-1)?.supplier ?? '');
  const [amount, setAmount] = useState<number | null>(null);
  const [currency, setCurrency] = useState<Currency>('CHF');
  const [from, setFrom] = useState(today());
  const money = (cents: number, cur: string) =>
    new Intl.NumberFormat(i18n.language, { style: 'currency', currency: cur }).format(cents / 100);
  const add = async () => {
    if (amount === null || supplier.trim() === '') return;
    try {
      await unwrap(
        api.libraryAddPrice({
          component: componentId,
          supplier: supplier.trim(),
          listPriceCents: Math.round(amount * 100),
          currency,
          validFrom: from,
          validTo: null,
        }),
      );
      setAmount(null);
      changed();
    } catch (e) {
      report(e);
    }
  };
  return (
    <Panel title={t('library.prices.title')}>
      {prices.length === 0 ? (
        <p className="panel-hint">{t('library.prices.none')}</p>
      ) : (
        <table className="library__table">
          <thead>
            <tr>
              <th>{t('library.prices.supplier')}</th>
              <th>{t('library.prices.listPrice')}</th>
              <th>{t('library.prices.from')}</th>
              <th>{t('library.prices.to')}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {prices.map((p) => (
              <tr key={`${p.supplier}-${p.validFrom}`}>
                <td>{p.supplier}</td>
                <td>{money(p.listPriceCents, p.currency)}</td>
                <td>{p.validFrom}</td>
                <td>{p.validTo ?? '—'}</td>
                <td>
                  <button
                    type="button"
                    className="library__link"
                    onClick={() => {
                      unwrap(api.librarySetPreferredSupplier(p.supplier))
                        .then(changed)
                        .catch(report);
                    }}
                  >
                    {t('library.prices.prefer')}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="form-grid">
        <TextInput label={t('library.prices.supplier')} value={supplier} onChange={setSupplier} />
        <NumInput
          label={t('library.prices.listPrice')}
          unit={currency}
          value={amount}
          decimals={2}
          min={0}
          max={1e9}
          onChange={setAmount}
        />
        <Select<Currency>
          label={t('library.prices.currency')}
          value={currency}
          options={(['CHF', 'EUR'] as const).map((c) => ({ value: c, label: c }))}
          onChange={setCurrency}
        />
        <label className="pv-field form-field">
          <span className="pv-field__label">{t('library.prices.from')}</span>
          <input
            type="date"
            className="form-input"
            value={from}
            onChange={(e) => {
              setFrom(e.target.value);
            }}
          />
        </label>
      </div>
      <Button
        variant="secondary"
        disabled={amount === null || supplier.trim() === ''}
        onClick={() => void add()}
      >
        {t('library.prices.add')}
      </Button>
    </Panel>
  );
}
