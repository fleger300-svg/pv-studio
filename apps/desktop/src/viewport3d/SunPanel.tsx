import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@pv/ui';
import { useProjectStore } from '../store/project';
import { useViewStore } from '../store/view';
import { dateOfDay, daylight, sunAtLocal } from './sun';

/** Days of the solstices and the equinox, as quick choices. */
const PRESETS = [
  { key: 'winter', day: 355 },
  { key: 'equinox', day: 80 },
  { key: 'summer', day: 172 },
] as const;
/** Animation: minutes of the day per step, and step duration [ms]. */
const PLAY_STEP_MIN = 10;
const PLAY_STEP_MS = 120;

const clock = (minute: number) =>
  `${String(Math.floor(minute / 60)).padStart(2, '0')}:${String(minute % 60).padStart(2, '0')}`;

/**
 * Real sun of the 3-D view (toolbar): date and Swiss legal time, the day played as an
 * animation, the sun's position. The shadows follow.
 */
export function SunPanel() {
  const on = useViewStore((s) => s.sun !== null);
  const located = useProjectStore((s) => Boolean(s.summary?.location));
  // Mounted only while the sun is on: its animation state starts afresh each time.
  return on && located ? <SunControls /> : null;
}

function SunControls() {
  const { t, i18n } = useTranslation();
  const sun = useViewStore((s) => s.sun);
  const setSun = useViewStore((s) => s.setSun);
  const location = useProjectStore((s) => s.summary?.location ?? null);
  const [playing, setPlaying] = useState(false);
  const day = sun?.day ?? 1;
  const light = useMemo(
    () => (location ? daylight(location.latDeg, location.lonDeg, day) : null),
    [location, day],
  );

  // Play the day from sunrise to sunset, in a loop.
  useEffect(() => {
    if (!playing || !light) return;
    const id = setInterval(() => {
      const s = useViewStore.getState().sun;
      if (!s) return;
      const next = s.minute + PLAY_STEP_MIN;
      useViewStore
        .getState()
        .setSun({ ...s, minute: next > light.set || next < light.rise ? light.rise : next });
    }, PLAY_STEP_MS);
    return () => {
      clearInterval(id);
    };
  }, [playing, light]);
  if (!sun || !location) return null;
  const p = sunAtLocal(location.latDeg, location.lonDeg, sun.day, sun.minute);
  const date = new Intl.DateTimeFormat(i18n.language, {
    day: 'numeric',
    month: 'long',
    timeZone: 'UTC',
  }).format(dateOfDay(sun.day));
  const deg = (v: number) =>
    `${new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 }).format(v)}°`;
  return (
    <div className="sun-panel" role="group" aria-label={t('sun.title')}>
      <strong className="sun-panel__title">{t('sun.title')}</strong>
      <label className="sun-panel__row">
        <span>
          {t('sun.date')} : <b>{date}</b>
        </span>
        <input
          type="range"
          min={1}
          max={365}
          value={sun.day}
          aria-label={t('sun.date')}
          onChange={(e) => {
            setSun({ ...sun, day: Number(e.currentTarget.value) });
          }}
        />
      </label>
      <div className="sun-panel__presets">
        {PRESETS.map((x) => (
          <Button
            key={x.key}
            variant="secondary"
            onClick={() => {
              setSun({ ...sun, day: x.day });
            }}
          >
            {t(`sun.presets.${x.key}`)}
          </Button>
        ))}
      </div>
      <label className="sun-panel__row">
        <span>
          {t('sun.time')} : <b>{clock(sun.minute)}</b>
        </span>
        <input
          type="range"
          min={0}
          max={1435}
          step={5}
          value={sun.minute}
          aria-label={t('sun.time')}
          onChange={(e) => {
            setSun({ ...sun, minute: Number(e.currentTarget.value) });
          }}
        />
      </label>
      <div className="sun-panel__presets">
        <Button
          variant="secondary"
          disabled={!light}
          onClick={() => {
            setPlaying((x) => !x);
          }}
        >
          {playing ? t('sun.pause') : t('sun.play')}
        </Button>
      </div>
      <p className="sun-panel__status">
        {p.elevationDeg > 0
          ? t('sun.position', { azimuth: deg(p.azimuthDeg), elevation: deg(p.elevationDeg) })
          : t('sun.night')}
      </p>
      {light && (
        <p className="sun-panel__note">
          {t('sun.daylight', { rise: clock(light.rise), set: clock(light.set) })}
        </p>
      )}
      <p className="sun-panel__note">{t('sun.note')}</p>
    </div>
  );
}
