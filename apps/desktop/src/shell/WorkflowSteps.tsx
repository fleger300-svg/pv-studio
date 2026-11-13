import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../store/project';
import { STEPS, useWorkflowStore } from '../store/workflow';

/** Workflow steps: building, roof, layout, connection. */
export function WorkflowSteps() {
  const { t } = useTranslation();
  const step = useWorkflowStore((s) => s.step);
  const setStep = useWorkflowStore((s) => s.setStep);
  const open = useProjectStore((s) => s.summary !== null);
  return (
    <nav className="workflow" aria-label={t('workflow.title')}>
      <h2 className="workflow__title">{t('workflow.title')}</h2>
      <ol className="workflow__list">
        {STEPS.map((key, i) => (
          <li key={key}>
            <button
              type="button"
              className="workflow__step"
              disabled={!open}
              aria-current={open && step === key ? 'step' : undefined}
              onClick={() => {
                setStep(key);
              }}
            >
              {t('workflow.step', { index: i + 1, label: t(`workflow.${key}`) })}
            </button>
          </li>
        ))}
      </ol>
    </nav>
  );
}
