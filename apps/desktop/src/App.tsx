import { useTranslation } from 'react-i18next';
import { TopBar } from './shell/TopBar';
import { WorkflowSteps } from './shell/WorkflowSteps';
import { ProjectPicker } from './shell/ProjectPicker';
import { SidePanel } from './shell/SidePanel';
import { Toolbar } from './shell/Toolbar';
import { Toasts } from './shell/Toasts';
import { useProjectStore } from './store/project';
import { useViewStore } from './store/view';
import { Viewport } from './viewport3d/Viewport';
import { useEditorShortcuts } from './viewport3d/Interaction';
import { Hud, useHudShortcut } from './perf/Hud';
import { useStaticsSync } from './statics/useStatics';

export function App() {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const modules = useProjectStore((s) => s.modules);
  const editor = useViewStore((s) => s.editor);
  useEditorShortcuts(editor);
  useHudShortcut();
  useStaticsSync();
  return (
    <div className="app">
      <TopBar projectName={summary?.name ?? null} />
      <aside className="app__left">
        <WorkflowSteps />
        <ProjectPicker />
      </aside>
      <main className="app__viewport" aria-label={t('viewport.label')}>
        {summary && modules ? (
          <>
            <Viewport summary={summary} modules={modules} />
            <Toolbar />
            <Hud />
          </>
        ) : (
          <p className="viewport-empty">{t('viewport.empty')}</p>
        )}
        <Toasts />
      </main>
      <SidePanel />
    </div>
  );
}
