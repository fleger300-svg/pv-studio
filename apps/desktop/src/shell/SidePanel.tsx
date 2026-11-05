import { useTranslation } from 'react-i18next';
import { Panel, Select, Stat } from '@pv/ui';
import { useProjectStore } from '../store/project';
import { useViewStore } from '../store/view';
import type { ColorMode } from '../viewport3d/PanelMeshes';

/** Right-hand panel: selection info and display options for the layout step. */
export function SidePanel() {
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
    </aside>
  );
}
