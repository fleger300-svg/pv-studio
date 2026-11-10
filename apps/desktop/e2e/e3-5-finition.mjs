// E3.5 (polish) in the running app (French UI): a project started from nothing gets its
// statics (site entered in the building panel, covering, array), a chimney drawn in 3D,
// the 3D reframe button, dark theme, an accessibility audit of every step, keyboard use of
// the splitter.
//
//   node apps/desktop/e2e/e3-5-finition.mjs <output directory>
import path from 'node:path';
import { check, connect, summary, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);
const fill = async (label, value) => {
  const input = page.getByLabel(label, { exact: true });
  await input.fill(value);
  await input.press('Enter');
};

/** Fields without a label, buttons without a name, positive tab indexes. */
const audit = () =>
  page.evaluate(() => {
    const problems = [];
    const name = (el) =>
      (el.getAttribute('aria-label') ?? '').trim() ||
      (el.labels && [...el.labels].map((l) => l.textContent.trim()).join(' ')) ||
      (el.getAttribute('aria-labelledby') &&
        document.getElementById(el.getAttribute('aria-labelledby'))?.textContent.trim()) ||
      '';
    for (const el of document.querySelectorAll('input, select, textarea')) {
      if (!name(el)) problems.push(`unlabelled ${el.tagName.toLowerCase()} ${el.className}`);
    }
    for (const el of document.querySelectorAll('button, [role="button"], [role="switch"]')) {
      if (!name(el) && !el.textContent.trim()) problems.push(`unnamed button ${el.className}`);
    }
    for (const el of document.querySelectorAll('[tabindex]')) {
      if (Number(el.getAttribute('tabindex')) > 0) problems.push(`tabindex>0 ${el.className}`);
    }
    return problems;
  });
const audited = async (step) => {
  const problems = await audit();
  check(
    problems.length === 0,
    `accessibility (${step}): ${problems.length ? problems.join('; ') : 'ok'}`,
  );
};

await page.reload();
await page.getByRole('button', { name: /Résidentiel/ }).click();
await waitFor(
  page,
  () => hook(() => window.__pv?.project.getState().summary?.buildings.length),
  'preset',
);
await audited('layout, stress project');
const file = path.join(out, `e3-5-${Date.now()}.pvproj`);
await hook((p) => window.__pv.project.getState().newProject(p), file);
await waitFor(
  page,
  () => hook(() => window.__pv.project.getState().summary?.buildings.length === 0),
  'new',
);
await hook(() => window.__pv.plan.getState().setRatio(0.5));
await page.locator('.plan-view canvas').first().waitFor();
await page.getByText('Site vide : tracez un bâtiment sur le plan.').waitFor();
check(true, 'empty site: hint in the 3D view');

// A 14 × 9 m house, hipped (the engine has the hipped pressure coefficients).
const box = await page.locator('.plan-view').boundingBox();
const screen = async ([e, n]) => {
  const t = await hook(() => window.__pv.plan.getState().transform);
  return [box.x + t.x + e * t.scale, box.y + t.y - n * t.scale];
};
await page.keyboard.press('r');
const [x0, y0] = await screen([-7, -4.5]);
const [x1, y1] = await screen([7, 4.5]);
await page.mouse.move(x0, y0);
await page.mouse.down();
await page.mouse.move(x1, y1, { steps: 10 });
await page.mouse.up();
await waitFor(page, async () => (await summary(page)).buildings.length === 1, 'house');
await page.keyboard.press('v');
await page.getByLabel('Type de toit').selectOption('hipped');
await waitFor(page, async () => (await summary(page)).buildings[0].roofKind === 'hipped', 'hipped');

// Site: altitude and q_p0 are required, the rest has defaults.
await page.getByText('Renseignez l’altitude').waitFor();
await fill('Altitude', '520');
check((await summary(page)).buildings[0].site == null, 'altitude alone does not create the site');
await fill('Pression dynamique de référence q_p0', '1,1');
await waitFor(page, async () => (await summary(page)).buildings[0].site?.altitudeM === 520, 'site');
const site = (await summary(page)).buildings[0].site;
check(
  site.windReferencePressureKnM2 === 1.1 && site.terrain === 'III',
  'site created (520 m, q_p0 1,1, terrain III by default)',
);
await page.getByLabel('Catégorie de terrain').selectOption('II');
await waitFor(
  page,
  async () => (await summary(page)).buildings[0].site?.terrain === 'II',
  'terrain',
);
await audited('building, site');

// Roof: covering and a chimney on the south face.
await page.getByRole('button', { name: /Toiture/ }).click();
const south = (await summary(page)).roofFaces.find((f) => f.face.azimuthDeg === 180).face;
await hook(
  ([f, b]) => window.__pv.selection.getState().selectFace(f, b),
  [south.id, south.buildingId],
);
await page.getByLabel('Couverture').selectOption('standingSeam');
await waitFor(
  page,
  async () => (await summary(page)).roofFaces.some((f) => f.face.supportGrid),
  'covering',
);
await page.getByRole('button', { name: 'Ajouter un obstacle' }).click();
await waitFor(
  page,
  async () => (await summary(page)).roofFaces.some((f) => f.face.outline?.obstacles.length),
  'chimney',
);
await audited('roof, obstacle');

// Layout: an array around the chimney, statics computed.
await page.getByRole('button', { name: /Calepinage/ }).click();
await page.getByRole('button', { name: /Ajouter un champ/ }).click();
await waitFor(page, async () => (await summary(page)).arrays[0]?.moduleCount > 0, 'array');
const statics = page.locator('.pv-panel', { hasText: 'Statique' });
await waitFor(
  page,
  async () => {
    const text = await statics.innerText();
    return /OK|Erreur/.test(text) && !/couverture|données de site/.test(text) ? text : null;
  },
  'statics',
);
const staticsText = await statics.innerText();
console.log(staticsText.replace(/\n+/g, ' | '));
check(/OK/.test(staticsText), 'statics computed on a project started from nothing');
await audited('layout, new project');

// The 3D reframe button moves the camera back onto the project.
const cam = () => hook(() => window.__pv.three().camera.position.toArray());
await page.getByRole('button', { name: 'Recadrer la vue 3D sur le projet' }).waitFor();
const canvas = await page.locator('.app__3d canvas').boundingBox();
await page.mouse.move(canvas.x + canvas.width / 2, canvas.y + canvas.height / 2);
for (let i = 0; i < 8; i++) await page.mouse.wheel(0, 400);
await page.waitForTimeout(500);
const away = await cam();
await page.getByRole('button', { name: 'Recadrer la vue 3D sur le projet' }).click();
await page.waitForTimeout(500);
const back = await cam();
const dist = (p) => Math.hypot(...p);
check(
  dist(back) < dist(away),
  `reframe: camera ${dist(away).toFixed(0)} m → ${dist(back).toFixed(0)} m from the origin`,
);
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/e3-5-layout.png` });

// Dark theme, plan and panels.
await page.getByRole('switch', { name: /Thème sombre/ }).click();
await page.getByRole('button', { name: /Toiture/ }).click();
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/e3-5-dark-roof.png` });
await page.getByRole('button', { name: /Bâtiment/ }).click();
await page.waitForTimeout(500);
await page.screenshot({ path: `${out}/e3-5-dark-building.png` });
await audited('building, dark theme');

// Keyboard: the splitter moves with the arrow keys.
await page.locator('.splitter').focus();
const r0 = await hook(() => window.__pv.plan.getState().ratio);
await page.keyboard.press('ArrowRight');
await page.keyboard.press('ArrowRight');
const r1 = await hook(() => window.__pv.plan.getState().ratio);
check(
  Math.abs(r1 - r0 - 0.1) < 1e-9,
  `splitter by keyboard: ${(r0 * 100).toFixed(0)} % → ${(r1 * 100).toFixed(0)} %`,
);
await page.getByRole('switch', { name: /Thème sombre/ }).click();

await browser.close();
console.log('E3.5 scenario passed');
