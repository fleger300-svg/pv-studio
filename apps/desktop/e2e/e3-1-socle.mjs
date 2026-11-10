// E3.1 (foundation) in the running app: workflow steps, the split 2D plan | 3D view (the 3D
// view is never rebuilt when switching steps), shared selection, Delete + Ctrl+Z on a
// building from the plan, the splitter, new project and open (explicit paths: native
// dialogs cannot be driven).
//
//   node apps/desktop/e2e/e3-1-socle.mjs <output directory>
import fs from 'node:fs';
import path from 'node:path';
import { check, connect, invoke, summary, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const { browser, page } = await connect();
const hook = (fn, arg) => page.evaluate(fn, arg);
const step = () => hook(() => window.__pv.workflow.getState().step);

// 1. A preset with arrays opens on the layout step, full-screen 3D with the module tools.
// Start from a fresh page: the scenario must not depend on a previous run.
await page.reload();
await page.getByRole('button', { name: /Résidentiel|Residential/ }).waitFor();
await page.getByRole('button', { name: /Résidentiel|Residential/ }).click();
await waitFor(
  page,
  () => hook(() => window.__pv?.project.getState().summary?.arrays.length),
  'preset',
);
check((await step()) === 'layout', 'a project with arrays opens on the layout step');
check(
  (await page.getByRole('button', { name: /^(Sélection|Select)$/ }).count()) === 1,
  'module tools in the layout step',
);
check((await page.locator('.app__plan').count()) === 0, 'no plan in the layout step');

// 2. Building step: plan | splitter | 3D, the 3D editor kept (not rebuilt).
await hook(() => {
  window.__editorBefore = window.__pv.editor;
});
await page.getByRole('button', { name: /Bâtiment|Building/ }).click();
await page.locator('.app__plan canvas').first().waitFor();
await page.waitForTimeout(800);
check((await step()) === 'building', 'building step selected');
check(await hook(() => window.__pv.editor === window.__editorBefore), '3D editor not rebuilt');
check(
  (await page.getByRole('button', { name: /^(Sélection|Select)$/ }).count()) === 0,
  'no module tools on the plan steps',
);
await page.screenshot({ path: `${out}/e3-1-split.png` });

// 3. A click on the plan (centre, slightly south: Barn 2's south face) selects face and
// building; the plan, the 3D view and the panels share it.
const plan = await page.locator('.plan-view').boundingBox();
await page.mouse.click(plan.x + plan.width / 2, plan.y + plan.height / 2 + 15);
const sel = await waitFor(
  page,
  () =>
    hook(() => {
      const s = window.__pv.selection.getState();
      return s.faceId ? s : null;
    }),
  'selection',
);
const s0 = await summary(page);
const picked = s0.buildings.find((b) => b.id === sel.buildingId);
const face = s0.roofFaces.find((f) => f.face.id === sel.faceId);
check(
  picked?.name === 'Barn 2' && face?.face.name === 'South',
  'plan click selects Barn 2 · South',
);

// 4. Wheel zoom around the cursor, then fit.
const before = await hook(() => window.__pv.plan.getState().transform);
await page.mouse.move(plan.x + plan.width / 3, plan.y + plan.height / 3);
await page.mouse.wheel(0, -400);
const zoomed = await hook(() => window.__pv.plan.getState().transform);
check(
  zoomed.scale > before.scale * 1.5,
  `wheel zooms in (${before.scale.toFixed(2)} → ${zoomed.scale.toFixed(2)} px/m)`,
);
await page.getByRole('button', { name: /Ajuster à la vue|Fit to view/ }).click();
const refit = await hook(() => window.__pv.plan.getState().transform);
check(Math.abs(refit.scale - before.scale) < 1e-9, 'fit restores the fitted view');

// 5. Delete removes the selected building (one command), Ctrl+Z brings it back.
await page.keyboard.press('Delete');
await waitFor(page, async () => (await summary(page)).buildings.length === 2, 'removal');
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/e3-1-deleted.png` });
check(await hook(() => window.__pv.selection.getState().buildingId === null), 'selection pruned');
await page.keyboard.press('Control+z');
await waitFor(page, async () => (await summary(page)).buildings.length === 3, 'undo');
check(true, 'Delete then Ctrl+Z on the plan');

// 6. Splitter: dragged to 30 %, remembered.
const splitter = await page.locator('.splitter').boundingBox();
const main = await page.locator('.app__viewport').boundingBox();
await page.mouse.move(splitter.x + 3, splitter.y + 100);
await page.mouse.down();
await page.mouse.move(main.x + main.width * 0.3, splitter.y + 100, { steps: 5 });
await page.mouse.up();
const ratio = await hook(() => window.__pv.plan.getState().ratio);
check(Math.abs(ratio - 0.3) < 0.02, `splitter at ${(ratio * 100).toFixed(0)} %`);
await page.screenshot({ path: `${out}/e3-1-splitter.png` });

// 7. New empty project: building step, empty plan.
const file = path.join(out, 'e3-1-new.pvproj');
for (const f of [file, `${file}-wal`, `${file}-shm`]) fs.rmSync(f, { force: true });
await hook((p) => window.__pv.project.getState().newProject(p), file);
await waitFor(
  page,
  () => hook(() => window.__pv.project.getState().summary?.buildings.length === 0),
  'new project',
);
await page.locator('.plan-view__empty').waitFor();
check((await step()) === 'building', 'a new project opens on the building step');
check(
  (await page.locator('.topbar__project').innerText()) === 'e3-1-new',
  'project named after its file',
);
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/e3-1-new.png` });

// 8. Creating it again is refused (never overwrite a file).
await hook((p) => window.__pv.project.getState().newProject(p), file);
const error = await hook(() => window.__pv.project.getState().error);
check(error?.key === 'errors.store.alreadyExists', 'an existing file is not replaced');

// 9. Open the residential file again.
const resi = await invoke(page, 'stress_project_path', { preset: 'Residential1k' });
await hook((p) => window.__pv.project.getState().openProject(p), resi);
await waitFor(
  page,
  () => hook(() => window.__pv.project.getState().summary?.buildings.length === 3),
  'open',
);
check((await step()) === 'layout', 'opened project back on the layout step');

await browser.close();
console.log('E3.1 scenario passed');
