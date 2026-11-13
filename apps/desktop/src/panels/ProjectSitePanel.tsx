import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel, Stat } from '@pv/ui';
import type { Place } from '../bindings';
import { runApplied, toCommandError } from '../commands/runCommand';
import { api, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';
import { useToastStore } from '../store/toast';

function report(e: unknown) {
  useToastStore.getState().push(toCommandError(e));
}

/**
 * Project location (address search on geo.admin.ch, terrain altitude) and its typical
 * weather year (nearest MeteoSwiss station). Both are stored in the project: it then
 * recomputes offline.
 */
export function ProjectSitePanel() {
  const { t, i18n } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const [text, setText] = useState('');
  const [places, setPlaces] = useState<Place[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    const timer = setTimeout(() => {
      if (text.trim().length < 3) {
        setPlaces([]);
        return;
      }
      unwrap(api.siteSearch(text))
        .then((p) => {
          if (live) setPlaces(p);
        })
        .catch(report);
    }, 300);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [text]);

  if (!summary) return null;
  const fmt = (v: number, digits = 1) =>
    new Intl.NumberFormat(i18n.language, { maximumFractionDigits: digits }).format(v);
  const loc = summary.location ?? null;
  const weather = summary.weather ?? null;

  const locate = async (place: Place) => {
    setBusy(true);
    try {
      await runApplied('apply', () => unwrap(api.siteLocate(place)));
      setText('');
      setPlaces([]);
    } catch (e) {
      report(e);
    } finally {
      setBusy(false);
    }
  };
  const fetchWeather = async () => {
    setBusy(true);
    try {
      await runApplied('apply', () => unwrap(api.siteFetchWeather()));
    } catch (e) {
      report(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Panel title={t('location.title')}>
      <input
        className="form-input"
        type="search"
        placeholder={t('location.search')}
        aria-label={t('location.search')}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
        }}
      />
      {places.length > 0 && (
        <ul className="face-list">
          {places.map((p) => (
            <li key={`${p.label}-${String(p.latDeg)}`}>
              <button
                type="button"
                className="face-list__item"
                disabled={busy}
                onClick={() => void locate(p)}
              >
                {p.label}
              </button>
            </li>
          ))}
        </ul>
      )}
      {loc ? (
        <>
          <Stat label={t('location.address')} value={loc.label} />
          <Stat
            label={t('location.coordinates')}
            value={`${fmt(loc.eastM, 0)} / ${fmt(loc.northM, 0)}`}
          />
          <Stat label={t('location.altitude')} value={`${fmt(loc.altitudeM, 0)} m`} />
        </>
      ) : (
        <p className="panel-hint">{t('location.noLocation')}</p>
      )}
      {weather ? (
        <>
          <Stat
            label={t('location.irradiation')}
            value={`${fmt(weather.annualGhiKwhM2, 0)} kWh/m²`}
          />
          <Stat label={t('location.meanTemp')} value={`${fmt(weather.meanTempC)} °C`} />
          <p className="panel-hint">{weather.source}</p>
        </>
      ) : (
        loc && <p className="panel-hint">{t('location.noWeather')}</p>
      )}
      <Button variant="secondary" disabled={!loc || busy} onClick={() => void fetchWeather()}>
        {busy
          ? t('location.loading')
          : weather
            ? t('location.refreshWeather')
            : t('location.fetchWeather')}
      </Button>
      <p className="panel-hint">{t('location.attribution')}</p>
    </Panel>
  );
}
