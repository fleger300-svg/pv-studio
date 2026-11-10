import { useTranslation } from 'react-i18next';
import { usePlanStore } from '../store/plan';

/**
 * Draggable divider between the 2D plan and the 3D view (also moves with the arrow keys).
 */
export function Splitter() {
  const { t } = useTranslation();
  const ratio = usePlanStore((s) => s.ratio);
  const setRatio = usePlanStore((s) => s.setRatio);
  return (
    <div
      className="splitter"
      role="separator"
      aria-orientation="vertical"
      aria-label={t('plan.splitter')}
      aria-valuemin={20}
      aria-valuemax={80}
      aria-valuenow={Math.round(ratio * 100)}
      tabIndex={0}
      onPointerDown={(e) => {
        const host = e.currentTarget.parentElement;
        if (!host) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        const r = host.getBoundingClientRect();
        const move = (ev: PointerEvent) => {
          setRatio((ev.clientX - r.left) / r.width);
        };
        const up = () => {
          window.removeEventListener('pointermove', move);
          window.removeEventListener('pointerup', up);
        };
        window.addEventListener('pointermove', move);
        window.addEventListener('pointerup', up);
      }}
      onKeyDown={(e) => {
        if (e.key === 'ArrowLeft') setRatio(ratio - 0.05);
        else if (e.key === 'ArrowRight') setRatio(ratio + 0.05);
      }}
    />
  );
}
