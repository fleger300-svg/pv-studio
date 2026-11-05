import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, IconButton, Stat } from '@pv/ui';
import { BackendError } from '../ipc';
import { useViewStore } from '../store/view';
import { runBenchmark, jsHeapMB } from './benchmark';
import { frameRecorder } from './frames';
import { usePerfStore } from './perfStore';
import { percentile, rollingFps } from './stats';

interface Live {
  fps: number;
  p95: number;
  calls: number;
  triangles: number;
  heap: number | null;
}

function sample(): Live {
  const now = performance.now();
  const stamps = frameRecorder.live.filter((t) => t > now - 1000);
  const intervals = stamps.slice(1).map((t, i) => t - (stamps[i] ?? t));
  return {
    fps: rollingFps(frameRecorder.live, now),
    p95: percentile(intervals, 95),
    calls: frameRecorder.calls,
    triangles: frameRecorder.triangles,
    heap: jsHeapMB(),
  };
}

/** F2 toggles the HUD. */
export function useHudShortcut(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'F2') {
        e.preventDefault();
        usePerfStore.getState().toggleHud();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, []);
}

/** Floating performance HUD (labels translated, numbers tabular). */
export function Hud() {
  const { t, i18n } = useTranslation();
  const visible = usePerfStore((s) => s.hudVisible);
  const openMs = usePerfStore((s) => s.openMs);
  const last = usePerfStore((s) => s.lastCommand);
  const bench = usePerfStore((s) => s.bench);
  const toggle = usePerfStore((s) => s.toggleHud);
  const editor = useViewStore((s) => s.editor);
  const [live, setLive] = useState<Live>(sample);

  useEffect(() => {
    if (!visible) return;
    const id = setInterval(() => {
      setLive(sample());
    }, 250);
    return () => {
      clearInterval(id);
    };
  }, [visible]);

  if (!visible) return null;
  const n = (v: number, digits = 0) =>
    new Intl.NumberFormat(i18n.language, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(v);
  const na = t('hud.na');
  const running = bench.state === 'running';

  const start = async () => {
    const perf = usePerfStore.getState();
    try {
      const path = await runBenchmark();
      perf.setBench({ state: 'done', path });
    } catch (e) {
      perf.setBench(
        e instanceof BackendError
          ? { state: 'failed', key: e.key, params: e.params }
          : { state: 'failed', key: 'hud.bench.error', params: { detail: String(e) } },
      );
    }
  };

  return (
    <section className="hud" aria-labelledby="hud-title">
      <header className="hud__header">
        <h2 id="hud-title" className="hud__title">
          {t('hud.title')}
        </h2>
        <IconButton label={t('hud.close')} icon="×" onClick={toggle} />
      </header>
      <div className="hud__stats">
        <Stat label={t('hud.fps')} value={n(live.fps)} />
        <Stat label={t('hud.frameP95')} value={live.fps > 1 ? n(live.p95, 1) : na} />
        <Stat label={t('hud.drawCalls')} value={n(live.calls)} />
        <Stat label={t('hud.triangles')} value={n(live.triangles)} />
        <Stat label={t('hud.modules')} value={editor ? n(editor.panels.instanceCount) : na} />
        <Stat label={t('hud.meshes')} value={editor ? n(editor.panels.meshes.length) : na} />
        <Stat label={t('hud.roundTrip')} value={last ? n(last.roundTripMs, 1) : na} />
        <Stat label={t('hud.rustApply')} value={last ? n(last.rustUs) : na} />
        <Stat label={t('hud.openTime')} value={openMs !== null ? n(openMs) : na} />
        <Stat label={t('hud.heap')} value={live.heap !== null ? n(live.heap, 1) : na} />
      </div>
      <Button variant="primary" disabled={running || !editor} onClick={() => void start()}>
        {running ? t(`hud.bench.steps.${bench.step}`) : t('hud.bench.run')}
      </Button>
      <p className="hud__status" role="status">
        {bench.state === 'done' && t('hud.bench.done', { path: bench.path })}
        {bench.state === 'failed' && t(bench.key, bench.params)}
      </p>
    </section>
  );
}
