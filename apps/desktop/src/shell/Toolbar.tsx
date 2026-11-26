import { useTranslation } from 'react-i18next';
import { IconButton } from '@pv/ui';
import { useProjectStore } from '../store/project';
import { useViewStore, type Tool } from '../store/view';
import { usePerfStore } from '../perf/perfStore';
import { redo, undo } from '../commands/shortcuts';
import { dayOfYear, daylight, swissUtcOffset } from '../viewport3d/sun';
import {
  IconBox,
  IconDelete,
  IconFit,
  IconGauge,
  IconMove,
  IconRedo,
  IconSelect,
  IconSun,
  IconUndo,
} from './icons';

/** The sun of today at the current Swiss time (5 min), or at noon when it is down. */
function sunNow(latDeg: number, lonDeg: number) {
  const now = new Date();
  const day = dayOfYear(now);
  const utc = now.getUTCHours() * 60 + now.getUTCMinutes();
  const local = (utc + swissUtcOffset(day, utc + 60) * 60) % 1440;
  const minute = Math.round(local / 5) * 5;
  const light = daylight(latDeg, lonDeg, day);
  const up = light !== null && minute >= light.rise && minute <= light.set;
  return { day, minute: up ? minute : 12 * 60 };
}

const TOOLS: readonly { tool: Tool; icon: () => React.JSX.Element }[] = [
  { tool: 'select', icon: IconSelect },
  { tool: 'box', icon: IconBox },
  { tool: 'move', icon: IconMove },
];

/**
 * Floating viewport toolbar: module tools (layout step only) and history (everywhere).
 */
export function Toolbar({ modules }: { modules: boolean }) {
  const { t } = useTranslation();
  const tool = useViewStore((s) => s.tool);
  const setTool = useViewStore((s) => s.setTool);
  const editor = useViewStore((s) => s.editor);
  const selectionCount = useViewStore((s) => s.selectionCount);
  const canUndo = useProjectStore((s) => s.counters.canUndo);
  const canRedo = useProjectStore((s) => s.counters.canRedo);
  const hudVisible = usePerfStore((s) => s.hudVisible);
  const toggleHud = usePerfStore((s) => s.toggleHud);
  const requestFrame = useViewStore((s) => s.requestFrame);
  const sun = useViewStore((s) => s.sun);
  const setSun = useViewStore((s) => s.setSun);
  const location = useProjectStore((s) => s.summary?.location ?? null);

  return (
    <div className="toolbar" role="toolbar" aria-label={t('tools.label')}>
      {modules &&
        TOOLS.map(({ tool: id, icon: Icon }) => (
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
      {modules && (
        <>
          <span className="toolbar__sep" aria-hidden="true" />
          <IconButton
            label={t('tools.delete')}
            icon={<IconDelete />}
            disabled={!editor || selectionCount === 0}
            onClick={() => void editor?.removeSelected()}
          />
        </>
      )}
      <IconButton
        label={t('tools.undo')}
        icon={<IconUndo />}
        disabled={!canUndo}
        onClick={() => void undo()}
      />
      <IconButton
        label={t('tools.redo')}
        icon={<IconRedo />}
        disabled={!canRedo}
        onClick={() => void redo()}
      />
      <span className="toolbar__sep" aria-hidden="true" />
      <IconButton label={t('tools.frame')} icon={<IconFit />} onClick={requestFrame} />
      <IconButton
        label={location ? t('tools.sun') : t('tools.sunNoLocation')}
        icon={<IconSun />}
        active={sun !== null}
        disabled={!location}
        onClick={() => {
          setSun(sun || !location ? null : sunNow(location.latDeg, location.lonDeg));
        }}
      />
      <IconButton
        label={t('hud.toggle')}
        icon={<IconGauge />}
        active={hudVisible}
        onClick={toggleHud}
      />
    </div>
  );
}
