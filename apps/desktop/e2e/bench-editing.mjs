// Editing benchmark (run against a RELEASE build, WebView2 with --remote-debugging-port=9222):
// roof preview latency (the per-frame call while a footprint is dragged), roof
// regeneration, automatic layout of ~1 000 and ~10 000 modules, and the end-to-end time of a
// layout setting typed in the arrays panel. Writes `bench-editing.json` to the output
// directory and prints a summary.
//
//   pnpm tauri build --no-bundle
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
//   target/release/pv-studio.exe
//   node apps/desktop/e2e/bench-editing.mjs <output directory>
import fs from 'node:fs';
import path from 'node:path';
import { connect, invoke, summary, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const { browser, page } = await connect();
const stats = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
  return { n: s.length, median: q(0.5), p95: q(0.95), max: s[s.length - 1] };
};
const now = () => performance.now();
const report = {};

const openPreset = async (name, count) => {
  await page.getByRole('button', { name }).click();
  await waitFor(
    page,
    async () => {
      try {
        return (await summary(page)).moduleCount >= count;
      } catch {
        return false; // the project is being regenerated
      }
    },
    'preset',
    120000,
  );
  await page.waitForTimeout(1500);
};

// --- Residential (~1 000 modules) -------------------------------------------------------
await page.reload();
await openPreset(/Résidentiel|Residential/, 1000);
let s = await summary(page);
const barn = s.buildings[1];
const spec = await invoke(page, 'roof_preset', {
  footprint: barn.footprint,
  preset: 'gable',
  pitchDeg: 30,
  overhangM: 0,
});
const building = { ...barn, roof: spec };

// 1. Roof preview while dragging a vertex: one call per frame.
const preview = [];
for (let i = 0; i < 60; i++) {
  const moved = {
    ...building,
    footprint: building.footprint.map(([e, n], k) =>
      k === 2 ? [e + 0.01 * i, n + 0.01 * i] : [e, n],
    ),
  };
  const t0 = now();
  await invoke(page, 'preview_roof', { building: moved });
  preview.push(now() - t0);
}
report.previewRoofMs = stats(preview);

// 2. Roof regeneration as a command (plan + apply + journal).
const t2 = now();
const cmd = await invoke(page, 'plan_roof', { building });
const applied = await invoke(page, 'apply_command', { cmd });
report.roofRegenerationMs = { roundTrip: now() - t2, rust: applied.rustUs / 1000 };

// 3. Layout of a populated face (~170 modules), through the panel: click the face on the
// plan, type a setback, Enter → model reloaded.
await page.getByRole('button', { name: /Bâtiment|Building/ }).click();
await page.locator('.plan-view canvas').first().waitFor();
await page.waitForTimeout(800);
const plan = await page.locator('.plan-view').boundingBox();
await page.mouse.click(plan.x + plan.width / 2, plan.y + plan.height / 2 + 15);
await page.getByRole('button', { name: /Calepinage|Layout/ }).click();
const setback = page.getByLabel(/Retrait des bords|Edge setback/).first();
await setback.waitFor();
const faceArrays = async () => (await summary(page)).arrays.map((a) => a.array.layout.edgeSetbackM);
const panelLayout = [];
for (const v of ['0,45', '0,5', '0,55', '0,6', '0,65']) {
  const before = JSON.stringify(await faceArrays());
  await setback.fill(v);
  const t0 = now();
  await setback.press('Enter');
  await waitFor(page, async () => JSON.stringify(await faceArrays()) !== before, 'layout');
  panelLayout.push(now() - t0);
}
report.panelLayoutResidentialMs = stats(panelLayout);

// --- Industrial (~10 000 modules, east-west) ---------------------------------------------
await openPreset(/Industriel|Industrial/, 9000);
s = await summary(page);
const a = s.arrays[0].array;
const layouts = [];
for (let i = 1; i <= 3; i++) {
  const t0 = now();
  const r = await invoke(page, 'apply_layout', {
    array: { ...a, layout: { ...a.layout, edgeSetbackM: a.layout.edgeSetbackM + 0.1 * i } },
  });
  layouts.push({ roundTrip: now() - t0, rust: r.rustUs / 1000, modules: r.activeModuleCount });
}
report.layoutIndustrialMs = layouts;

fs.writeFileSync(path.join(out, 'bench-editing.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
await browser.close();
