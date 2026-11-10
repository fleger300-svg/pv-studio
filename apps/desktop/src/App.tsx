import { useTranslation } from 'react-i18next';
import { TopBar } from './shell/TopBar';
import { WorkflowSteps } from './shell/WorkflowSteps';
import { ProjectPicker } from './shell/ProjectPicker';
import { SidePanel } from './shell/SidePanel';
import { Splitter } from './shell/Splitter';
import { Toolbar } from './shell/Toolbar';
import { Toasts } from './shell/Toasts';
import { useShortcuts } from './commands/shortcuts';
import { PlanView } from './editor2d/PlanView';
import { usePlanStore } from './store/plan';
import { useProjectStore } from './store/project';
import { isPlanStep, useWorkflowStore } from './store/workflow';
import { Viewport } from './viewport3d/Viewport';
import { Hud, useHudShortcut } from './perf/Hud';
import { useStaticsSync } from './statics/useStatics';

export function App() {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const modules = useProjectStore((s) => s.modules);
  const step = useWorkflowStore((s) => s.step);
  const ratio = usePlanStore((s) => s.ratio);
  useShortcuts();
  useHudShortcut();
  useStaticsSync();
  const split = isPlanStep(step);
  return (
    <div className="app">
      <TopBar projectName={summary?.name ?? null} />
      <aside className="app__left">
        <WorkflowSteps />
        <ProjectPicker />
      </aside>
      <main
        className={split ? 'app__viewport app__viewport--split' : 'app__viewport'}
        style={{ '--plan-ratio': ratio } as React.CSSProperties}
      >
        {summary && modules ? (
          <>
            {/* The 3D view keeps its place in the tree: switching steps never rebuilds it. */}
            {split && (
              <section className="app__plan" aria-label={t('plan.label')}>
                <PlanView summary={summary} />
              </section>
            )}
            {split && <Splitter />}
            <section className="app__3d" aria-label={t('viewport.label')}>
              <Viewport summary={summary} modules={modules} />
              <Toolbar modules={!split} />
              <Hud />
            </section>
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
