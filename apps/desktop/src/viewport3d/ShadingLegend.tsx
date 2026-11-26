import { useTranslation } from 'react-i18next';
import { SHADING_STOPS, useShadingStore } from '../store/shading';
import { useViewStore } from '../store/view';

/**
 * Overlay of the shading colour mode: the colour scale with the project's means, and the
 * shading of the module under the pointer.
 */
export function ShadingLegend() {
  const { t, i18n } = useTranslation();
  const mode = useViewStore((s) => s.colorMode);
  const { mean, busy, error, hover, byModule } = useShadingStore();
  if (mode !== 'shading') return null;
  const pct = (v: number) =>
    `${new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(v * 100)} %`;
  const last = SHADING_STOPS[SHADING_STOPS.length - 1]?.at ?? 1;
  const gradient = SHADING_STOPS.map((s) => `${s.color} ${String((s.at / last) * 100)}%`).join(
    ', ',
  );
  const hovered = hover ? byModule.get(hover.id) : undefined;
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
        <p className="shading-legend__note">{t('shading.note')}</p>
      </div>
      {hover && hovered && (
        <div className="shading-tip" style={{ left: hover.x + 14, top: hover.y + 14 }}>
          <strong>{pct(hovered.horizon + hovered.near)}</strong>
          <span>{t('shading.horizon', { value: pct(hovered.horizon) })}</span>
          <span>{t('shading.near', { value: pct(hovered.near) })}</span>
        </div>
      )}
    </>
  );
}
