// C5 (datasheet extraction by the copilot) in the running app (French UI), without calling
// the real API: a local server stands for the Messages API and answers with the recorded
// synthetic response (testdata/copilot/lr7_response.json), after checking the request.
// The component is created unverified (copilot revision), then reviewed and verified by the
// user (a revision of their own).
//
//   $env:ANTHROPIC_BASE_URL = "http://127.0.0.1:8787"; $env:ANTHROPIC_API_KEY = "e2e-key"
//   $env:PV_LIBRARY_PATH = "<new file>"; $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
//   (start the server first:) node apps/desktop/e2e/c5-extraction.mjs <output directory>   (then) pnpm tauri dev
//
// The scenario starts its server, then waits for the app on the debugging port.
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { check, connect, waitFor } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const recorded = JSON.parse(
  fs.readFileSync(new URL('../../../testdata/copilot/lr7_response.json', import.meta.url)),
);
const requests = [];
const server = http.createServer((req, res) => {
  let body = '';
  req.on('data', (c) => (body += c));
  req.on('end', () => {
    requests.push({ url: req.url, headers: req.headers, body: JSON.parse(body) });
    res.writeHead(recorded.status, { 'content-type': 'application/json' });
    res.end(JSON.stringify(recorded.body));
  });
});
await new Promise((resolve) => server.listen(8787, '127.0.0.1', resolve));

// Wait for the app (it may be started after this script).
for (let i = 0; ; i++) {
  try {
    await fetch('http://127.0.0.1:9222/json/version');
    break;
  } catch {
    if (i > 300) throw new Error('app not started');
    await new Promise((r) => setTimeout(r, 1000));
  }
}
const { browser, page } = await connect();
await page.reload();
await page.waitForTimeout(1500);

// A "datasheet" (the stand-in server does not read it).
const pdf = path.join(out, 'LR7-54HVH-datasheet.pdf');
fs.writeFileSync(pdf, '%PDF-1.4\n% synthetic test file\n%%EOF\n');

await page.getByRole('button', { name: 'Bibliothèque', exact: true }).click();
await page.getByRole('button', { name: 'Extraire d’une fiche technique (IA)…' }).click();
await page.getByText('Le PDF est envoyé à Anthropic').waitFor();
check(true, 'the user is told the PDF is sent to Anthropic');
await page.getByLabel('Variante (p. ex. LR7-54HVH-490M), facultatif').fill('LR7-54HVH-490M');
await page.evaluate((p) => window.__pvExtractFile(p), pdf);
await page.getByText('Composant créé à partir de la fiche').waitFor({ timeout: 60000 });

const r = requests.at(-1);
check(
  r.url === '/v1/messages' &&
    r.headers['x-api-key'] === 'e2e-key' &&
    r.headers['anthropic-version'] === '2026-10-21' &&
    r.headers['anthropic-beta'] === 'server-side-fallback-2026-07-01',
  'request: endpoint and headers',
);
const content = r.body.messages[0].content;
check(
  r.body.model === 'claude-opus-5-5' &&
    r.body.output_config.format.type === 'json_schema' &&
    content[0].source.data === fs.readFileSync(pdf).toString('base64') &&
    content[1].text.includes('Variant hint: LR7-54HVH-490M'),
  'request: model, structured output, the PDF and the variant',
);
await page.getByText('Fiche multi-puissances').waitFor();
check(true, 'the points to check are shown');
await page.screenshot({ path: `${out}/c5-extracted.png` });

// Unverified, extracted by AI, copilot revision.
const row = page.locator('.library__row--selected');
await row.getByText('non vérifié').waitFor();
await row.getByText('extrait par IA').waitFor();
await page.getByText(/Révision 1 · .* · copilote/).waitFor();
check(true, 'created unverified, as revision 1 by the copilot');

// The user checks the values and verifies them: a revision of their own.
await page.getByLabel('Données vérifiées sur la fiche technique').check();
await page.getByLabel('Note de modification').fill('Relu sur la fiche');
await page.getByRole('button', { name: 'Enregistrer une nouvelle révision' }).click();
await page.getByText(/Révision 2 · .* · utilisateur · Relu sur la fiche/).waitFor();
await waitFor(page, async () => (await row.getByText('non vérifié').count()) === 0, 'verified');
check(true, 'verified by the user in revision 2');
await page.getByRole('button', { name: 'Afficher' }).waitFor();
check(true, 'the datasheet is attached');

await browser.close();
server.close();
console.log('C5 scenario passed');
