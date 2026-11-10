import { useTranslation } from 'react-i18next';
import { useToastStore } from '../store/toast';

/** Translated error toasts (back-end errors arrive as i18n key + params). */
export function Toasts() {
  const { t, i18n } = useTranslation();
  // Technical field names (`tiltDeg`…) are shown with their label (`fields.tiltDeg`).
  const params = (p: Record<string, unknown>) => {
    const field = p.field;
    return typeof field === 'string' && i18n.exists(`fields.${field}`)
      ? { ...p, field: t(`fields.${field}`) }
      : p;
  };
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  return (
    <div className="toasts" role="alert" aria-live="assertive">
      {toasts.map((toast) => (
        <div key={toast.id} className="toast">
          <span>{t(toast.message.key, params(toast.message.params))}</span>
          <button
            type="button"
            className="toast__close"
            aria-label={t('toast.close')}
            onClick={() => {
              dismiss(toast.id);
            }}
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
