// Real sun of the 3-D view in the running app (French UI): a PV*SOL project is imported, the
// sun is switched on from the toolbar, dates and times are chosen, and the light follows the
// sun (direction checked against the panel's azimuth and elevation); screenshots of a winter
// morning, a winter noon and a summer evening, and the night.
//
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"; pnpm tauri dev
//   node apps/desktop/e2e/sun-shadows.mjs <output directory> <project.pvprj>
import path from 'node:path';
import { check, connect, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const source = process.argv[3];
if (!source) throw new Error('usage: sun-shadows.mjs <output directory> <project.pvprj>');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);

await page.reload();
await page.getByRole('button', { name: 'Importer un projet PV*SOL…' }).waitFor();
await page.getByRole('button', { name: /Résidentiel/ }).click();
await waitFor(page, () => hook(() => window.__pv?.project.getState().summary), 'preset');
const target = path.join(out, `sun-${Date.now()}.pvproj`);
await hook(([s, t]) => window.__pv.project.getState().importPvsol(s, t), [source, target]);
const report = page.getByRole('dialog', { name: 'Import PV*SOL' });
await report.waitFor({ timeout: 120000 });
await report.getByRole('button', { name: 'Fermer' }).click();
await waitFor(page, async () => (await report.count()) === 0, 'report closed');

await page.getByRole('button', { name: 'Ombres du soleil à une date et une heure' }).click();
const panel = page.getByRole('group', { name: 'Soleil' });
await panel.waitFor();
check(true, `sun on: ${(await panel.innerText()).replace(/\s+/g, ' ')}`);

/** Sets a range input as a user would (React listens to `input`). */
async function slide(label, value) {
  await panel.getByRole('slider', { name: label }).evaluate((el, v) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    set.call(el, String(v));
    el.dispatchEvent(new Event('input', { bubbles: true }));
  }, value);
  await page.waitForTimeout(400);
}

/** Direction of the scene's sun light (unit, ENU) and its intensity. */
const lightDir = () =>
  hook(() => {
    let light = null;
    window.__pv.three().scene.traverse((o) => {
      if (o.isDirectionalLight) light = o;
    });
    const d = light.position.clone().sub(light.target.position).normalize();
    return { x: d.x, y: d.y, z: d.z, intensity: light.intensity };
  });

async function shot(name, preset, minute, expectUp) {
  await panel.getByRole('button', { name: preset }).click();
  await slide('Heure', minute);
  const text = (await panel.innerText()).replace(/\s+/g, ' ');
  const d = await lightDir();
  if (expectUp) {
    const m = /Azimut (\d+)°, hauteur (\d+)°/.exec(text);
    check(m !== null, `${name}: ${m?.[0]}`);
    const az = (Math.atan2(d.x, d.y) * 180) / Math.PI;
    const el = (Math.asin(d.z) * 180) / Math.PI;
    check(
      Math.abs(((az + 360) % 360) - Number(m[1])) < 1.5 && Math.abs(el - Number(m[2])) < 1.5,
      `${name}: light from azimuth ${((az + 360) % 360).toFixed(1)}°, elevation ${el.toFixed(1)}°`,
    );
    check(d.intensity > 0, `${name}: sunlight on`);
  } else {
    check(/couché/.test(text) && d.intensity === 0, `${name}: night, no sunlight`);
  }
  await page.screenshot({ path: path.join(out, `sun-${name}.png`) });
}

await shot('winter-morning', '21 déc.', 9 * 60 + 30, true);
await shot('winter-noon', '21 déc.', 12 * 60 + 30, true);
await shot('summer-evening', '21 juin', 19 * 60 + 30, true);
await shot('night', '21 juin', 23 * 60, false);

await page.getByRole('button', { name: 'Ombres du soleil à une date et une heure' }).click();
await waitFor(page, async () => (await panel.count()) === 0, 'sun off');
const d = await lightDir();
check(
  d.intensity > 0 && Math.abs(d.z - Math.sin((40 * Math.PI) / 180)) < 0.01,
  'default light back',
);
await browser.close();
