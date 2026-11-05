import { useTranslation } from 'react-i18next';
import { TopBar } from './shell/TopBar';
import { WorkflowSteps } from './shell/WorkflowSteps';
import { ProjectPicker } from './shell/ProjectPicker';
import { useProjectStore } from './store/project';

export function App() {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  return (
    <div className="app">
      <TopBar projectName={summary?.name ?? null} />
      <aside className="app__left">
        <WorkflowSteps />
        <ProjectPicker />
      </aside>
      <main className="app__viewport" aria-label={t('viewport.label')}>
        {summary ? (
          <p className="viewport-empty">
            {t('project.loaded', { count: summary.moduleCount, arrays: summary.arrays.length })}
          </p>
        ) : (
          <p className="viewport-empty">{t('viewport.empty')}</p>
        )}
      </main>
    </div>
  );
}
