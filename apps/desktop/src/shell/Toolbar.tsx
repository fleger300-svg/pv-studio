import { useTranslation } from 'react-i18next';
import { IconButton } from '@pv/ui';
import { useProjectStore } from '../store/project';
import { useViewStore, type Tool } from '../store/view';
import { usePerfStore } from '../perf/perfStore';
import { IconBox, IconDelete, IconGauge, IconMove, IconRedo, IconSelect, IconUndo } from './icons';

const TOOLS: readonly { tool: Tool; icon: () => React.JSX.Element }[] = [
  { tool: 'select', icon: IconSelect },
  { tool: 'box', icon: IconBox },
  { tool: 'move', icon: IconMove },
];

/** Floating viewport toolbar: tools and history. */
export function Toolbar() {
  const { t } = useTranslation();
  const tool = useViewStore((s) => s.tool);
  const setTool = useViewStore((s) => s.setTool);
  const editor = useViewStore((s) => s.editor);
  const selectionCount = useViewStore((s) => s.selectionCount);
  const canUndo = useProjectStore((s) => s.counters.canUndo);
  const canRedo = useProjectStore((s) => s.counters.canRedo);
  const hudVisible = usePerfStore((s) => s.hudVisible);
  const toggleHud = usePerfStore((s) => s.toggleHud);

  return (
    <div className="toolbar" role="toolbar" aria-label={t('tools.label')}>
      {TOOLS.map(({ tool: id, icon: Icon }) => (
        <IconButton
          key={id}
          label={t(`tools.${id}`)}
          icon={<Icon />}
          active={tool === id}
          onClick={() => {
            setTool(id);
          }}
        />
      ))}
      <span className="toolbar__sep" aria-hidden="true" />
      <IconButton
        label={t('tools.delete')}
        icon={<IconDelete />}
        disabled={!editor || selectionCount === 0}
        onClick={() => void editor?.removeSelected()}
      />
      <IconButton
        label={t('tools.undo')}
        icon={<IconUndo />}
        disabled={!editor || !canUndo}
        onClick={() => void editor?.undo()}
      />
      <IconButton
        label={t('tools.redo')}
        icon={<IconRedo />}
        disabled={!editor || !canRedo}
        onClick={() => void editor?.redo()}
      />
      <span className="toolbar__sep" aria-hidden="true" />
      <IconButton
        label={t('hud.toggle')}
        icon={<IconGauge />}
        active={hudVisible}
        onClick={toggleHud}
      />
    </div>
  );
}
