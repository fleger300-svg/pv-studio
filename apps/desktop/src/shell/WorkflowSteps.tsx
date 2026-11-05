import { useTranslation } from 'react-i18next';

const STEPS = [
  { key: 'building', enabled: false },
  { key: 'roof', enabled: false },
  { key: 'layout', enabled: true },
  { key: 'connection', enabled: false },
] as const;

/** Workflow steps. Only "Layout" is active in the prototype. */
export function WorkflowSteps() {
  const { t } = useTranslation();
  return (
    <nav className="workflow" aria-label={t('workflow.title')}>
      <h2 className="workflow__title">{t('workflow.title')}</h2>
      <ol className="workflow__list">
        {STEPS.map((step, i) => (
          <li key={step.key}>
            <button
              type="button"
              className="workflow__step"
              disabled={!step.enabled}
              aria-current={step.enabled ? 'step' : undefined}
              title={step.enabled ? undefined : t('workflow.unavailable')}
            >
              {t('workflow.step', { index: i + 1, label: t(`workflow.${step.key}`) })}
            </button>
          </li>
        ))}
      </ol>
    </nav>
  );
}
