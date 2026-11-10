import { useTranslation } from 'react-i18next';
import { Button } from '@pv/ui';
import type { StressPreset } from '../bindings';
import { useProjectStore } from '../store/project';

const PRESETS: readonly { preset: StressPreset; key: string }[] = [
  { preset: 'Residential1k', key: 'project.presets.residential' },
  { preset: 'Industrial10k', key: 'project.presets.industrial' },
];

/** Project files (new, open: native dialogs) and the synthetic stress projects. */
export function ProjectPicker() {
  const { t } = useTranslation();
  const loading = useProjectStore((s) => s.loading);
  const current = useProjectStore((s) => s.preset);
  const error = useProjectStore((s) => s.error);
  const loadPreset = useProjectStore((s) => s.loadPreset);
  const newProject = useProjectStore((s) => s.newProject);
  const openProject = useProjectStore((s) => s.openProject);
  const busy = loading !== null;

  return (
    <>
      <section className="project-picker" aria-labelledby="project-files-title">
        <h2 id="project-files-title" className="workflow__title">
          {t('project.files')}
        </h2>
        <Button
          variant="secondary"
          disabled={busy}
          aria-busy={loading === 'new'}
          onClick={() => void newProject()}
        >
          {t('project.new')}
        </Button>
        <Button
          variant="secondary"
          disabled={busy}
          aria-busy={loading === 'open'}
          onClick={() => void openProject()}
        >
          {t('project.open')}
        </Button>
      </section>
      <section className="project-picker" aria-labelledby="project-picker-title">
        <h2 id="project-picker-title" className="workflow__title">
          {t('project.title')}
        </h2>
        {PRESETS.map(({ preset, key }) => (
          <Button
            key={preset}
            variant={current === preset ? 'primary' : 'secondary'}
            disabled={busy}
            aria-busy={loading === preset}
            onClick={() => void loadPreset(preset)}
          >
            {loading === preset ? t('project.loading') : t(key)}
          </Button>
        ))}
        {error && (
          <p role="alert" className="project-picker__error">
            {t(error.key, error.params)}
          </p>
        )}
      </section>
    </>
  );
}
