import { useTranslation } from 'react-i18next';
import { TopBar } from './shell/TopBar';
import { WorkflowSteps } from './shell/WorkflowSteps';
import { ProjectPicker } from './shell/ProjectPicker';
import { SidePanel } from './shell/SidePanel';
import { useProjectStore } from './store/project';
import { Viewport } from './viewport3d/Viewport';

export function App() {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const modules = useProjectStore((s) => s.modules);
  return (
    <div className="app">
      <TopBar projectName={summary?.name ?? null} />
      <aside className="app__left">
        <WorkflowSteps />
        <ProjectPicker />
      </aside>
      <main className="app__viewport" aria-label={t('viewport.label')}>
        {summary && modules ? (
          <Viewport summary={summary} modules={modules} />
        ) : (
          <p className="viewport-empty">{t('viewport.empty')}</p>
        )}
      </main>
      <SidePanel />
    </div>
  );
}
