import { useTranslation } from 'react-i18next';
import { TopBar } from './shell/TopBar';
import { WorkflowSteps } from './shell/WorkflowSteps';

export function App() {
  const { t } = useTranslation();
  return (
    <div className="app">
      <TopBar projectName={null} />
      <aside className="app__left">
        <WorkflowSteps />
      </aside>
      <main className="app__viewport" aria-label={t('viewport.label')}>
        <p className="viewport-empty">{t('viewport.empty')}</p>
      </main>
    </div>
  );
}
