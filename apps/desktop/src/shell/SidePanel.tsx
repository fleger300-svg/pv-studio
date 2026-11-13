import { useTranslation } from 'react-i18next';
import { Panel, Select, Stat } from '@pv/ui';
import { useProjectStore } from '../store/project';
import { useViewStore } from '../store/view';
import { StaticsPanel } from './StaticsPanel';
import { BomPanel } from '../bom/BomPanel';
import type { ColorMode } from '../viewport3d/PanelMeshes';
import { BuildingPanel } from '../panels/BuildingPanel';
import { ProjectSitePanel } from '../panels/ProjectSitePanel';
import { RoofPanel } from '../panels/RoofPanel';
import { ArraysPanel } from '../panels/ArraysPanel';
import { ConnectionPanel } from '../panels/ConnectionPanel';
import { ProductionPanel } from '../panels/ProductionPanel';
import { useWorkflowStore } from '../store/workflow';
import { ErrorBoundary } from './ErrorBoundary';

/**
 * Right-hand panel, by step: the building form, the roof faces, or (layout) selection,
 * display options, statics and bill of materials.
 */
export function SidePanel() {
  const step = useWorkflowStore((s) => s.step);
  if (step === 'building') {
    return (
      <aside className="app__right">
        <ErrorBoundary>
          <ProjectSitePanel />
          <BuildingPanel />
        </ErrorBoundary>
      </aside>
    );
  }
  if (step === 'roof') {
    return (
      <aside className="app__right">
        <ErrorBoundary>
          <RoofPanel />
        </ErrorBoundary>
      </aside>
    );
  }
  if (step === 'connection') {
    return (
      <aside className="app__right">
        <ErrorBoundary>
          <ConnectionPanel />
          <ProductionPanel />
        </ErrorBoundary>
      </aside>
    );
  }
  return <LayoutPanels />;
}

function LayoutPanels() {
  const { t, i18n } = useTranslation();
  const colorMode = useViewStore((s) => s.colorMode);
  const setColorMode = useViewStore((s) => s.setColorMode);
  const count = useViewStore((s) => s.selectionCount);
  const wp = useProjectStore((s) => s.summary?.modulePeakPowerWp ?? 0);
  const kwp = new Intl.NumberFormat(i18n.language, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format((count * wp) / 1000);

  return (
    <aside className="app__right">
      <ErrorBoundary>
        <ArraysPanel />
        <Panel title={t('selection.title')}>
          <Stat label={t('selection.count')} value={t('selection.modules', { count })} />
          <Stat label={t('selection.power')} value={t('selection.kwp', { kwp })} />
        </Panel>
        <Panel title={t('display.title')}>
          <Select<ColorMode>
            label={t('display.colorMode')}
            value={colorMode}
            options={[
              { value: 'default', label: t('display.colorModes.default') },
              { value: 'string', label: t('display.colorModes.string') },
            ]}
            onChange={setColorMode}
          />
        </Panel>
        <StaticsPanel />
        <BomPanel />
      </ErrorBoundary>
    </aside>
  );
}
