import { useTranslation } from 'react-i18next';
import { Panel, Select } from '@pv/ui';
import { useViewStore } from '../store/view';
import type { ColorMode } from '../viewport3d/PanelMeshes';

/** Right-hand panel: display options for the layout step. */
export function SidePanel() {
  const { t } = useTranslation();
  const colorMode = useViewStore((s) => s.colorMode);
  const setColorMode = useViewStore((s) => s.setColorMode);

  return (
    <aside className="app__right">
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
