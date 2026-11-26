import { useTranslation } from 'react-i18next';
import { SHADING_STOPS, roofLossAt, useShadingStore } from '../store/shading';
import { useViewStore } from '../store/view';

/**
 * Overlay of the shading colour mode: the colour scale with the project's means, the switch
 * of the roof map, and the shading of the module or roof point under the pointer.
 */
export function ShadingLegend() {
  const { t, i18n } = useTranslation();
  const mode = useViewStore((s) => s.colorMode);
  const { mean, busy, error, hover, byModule, roof, roofBusy, showRoof, setShowRoof } =
    useShadingStore();
  if (mode !== 'shading') return null;
  const pct = (v: number) =>
    `${new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(v * 100)} %`;
  const last = SHADING_STOPS[SHADING_STOPS.length - 1]?.at ?? 1;
  const gradient = SHADING_STOPS.map((s) => `${s.color} ${String((s.at / last) * 100)}%`).join(
    ', ',
  );
  const module = hover?.id != null ? byModule.get(hover.id) : undefined;
  const roofLoss =
    hover && !module && showRoof ? roofLossAt(roof, hover.face, hover.u, hover.v) : null;
  return (
    <>
      <div className="shading-legend" role="group" aria-label={t('shading.title')}>
        <strong className="shading-legend__title">{t('shading.title')}</strong>
        <div
          className="shading-legend__bar"
          style={{ background: `linear-gradient(90deg, ${gradient})` }}
        />
        <div className="shading-legend__ticks">
          {SHADING_STOPS.map((s) => (
            <span key={s.at}>{s.at === last ? `≥ ${pct(s.at)}` : pct(s.at)}</span>
          ))}
        </div>
        {busy && !mean && <p className="shading-legend__status">{t('shading.computing')}</p>}
        {error && <p className="shading-legend__status">{t(error.key, error.params)}</p>}
        {mean && (
          <p className="shading-legend__status">
            {t('shading.mean', {
              total: pct(mean.horizon + mean.near),
              horizon: pct(mean.horizon),
              near: pct(mean.near),
            })}
            {busy && ' …'}
          </p>
        )}
        <label className="shading-legend__toggle">
          <input
            type="checkbox"
            checked={showRoof}
            onChange={(e) => {
              setShowRoof(e.currentTarget.checked);
            }}
          />
          {t('shading.roofMap')}
          {showRoof && roofBusy && ` — ${t('shading.computing')}`}
        </label>
        <p className="shading-legend__note">{t('shading.note')}</p>
      </div>
      {hover && module && (
        <div className="shading-tip" style={{ left: hover.x + 14, top: hover.y + 14 }}>
          <strong>{pct(module.horizon + module.near)}</strong>
          <span>{t('shading.horizon', { value: pct(module.horizon) })}</span>
          <span>{t('shading.near', { value: pct(module.near) })}</span>
        </div>
      )}
      {hover && roofLoss !== null && (
        <div className="shading-tip" style={{ left: hover.x + 14, top: hover.y + 14 }}>
          <span>{t('shading.roofPoint', { value: pct(roofLoss) })}</span>
        </div>
      )}
    </>
  );
}
