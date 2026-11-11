// C6 (API key settings) in the running app (French UI), against a local stand-in for the
// Anthropic API that accepts one key only. The app must be started WITHOUT ANTHROPIC_API_KEY
// (it would override the stored key) and with a dedicated credential store entry, so that the
// user's real key is never touched:
//
//   $env:ANTHROPIC_BASE_URL = "http://127.0.0.1:8787"; $env:PV_SECRETS_SERVICE = "ch.younergy.pvstudio.e2e"
//   $env:PV_LIBRARY_PATH = "<new file>"; $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
//   node apps/desktop/e2e/c6-cle-api.mjs <output directory>   (then) pnpm tauri dev
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { check, connect, invoke } from './lib.mjs';

const out = path.resolve(process.argv[2] ?? '.');
const VALID = 'sk-ant-e2e-valid-key-for-tests-ab12';
const WRONG = 'sk-ant-e2e-revoked-key-for-tests-0000';
const recorded = JSON.parse(
  fs.readFileSync(new URL('../../../testdata/copilot/lr7_response.json', import.meta.url)),
);
const seen = [];
const server = http.createServer((req, res) => {
  let body = '';
  req.on('data', (c) => (body += c));
  req.on('end', () => {
    const key = req.headers['x-api-key'];
    seen.push({ method: req.method, url: req.url, key });
    if (key !== VALID) {
      res.writeHead(401, { 'content-type': 'application/json' });
      res.end(
        JSON.stringify({
          type: 'error',
          error: { type: 'authentication_error', message: 'invalid x-api-key' },
        }),
      );
      return;
    }
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(
      JSON.stringify(req.method === 'GET' ? { data: [{ id: 'claude-opus-5-5' }] } : recorded.body),
    );
  });
});
await new Promise((resolve) => server.listen(8787, '127.0.0.1', resolve));
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
// The app may still be loading when the debugging port opens: retry until it is ready.
for (let i = 0; ; i++) {
  try {
    await page.reload();
    await page.getByRole('button', { name: 'Paramètres', exact: true }).waitFor({ timeout: 10000 });
    break;
  } catch (e) {
    if (i > 10) throw e;
    await page.waitForTimeout(2000);
  }
}
const status = () => invoke(page, 'api_key_status');
const settings = page.locator('section.settings');
const keyField = page.getByLabel('Clé API Anthropic');

// Start from no key (a previous run may have left one in the test entry).
await invoke(page, 'delete_api_key');
check(!(await status()).configured, 'no key at start');

// The extraction dialog leads to the settings.
await page.getByRole('button', { name: 'Bibliothèque', exact: true }).click();
await page.getByRole('button', { name: 'Extraire d’une fiche technique (IA)…' }).click();
await page.getByRole('button', { name: 'Configurer la clé API…' }).click();
await settings.getByText('Aucune clé configurée').waitFor();
check(true, 'without a key, the extraction dialog opens the settings');

// A malformed key is refused.
await keyField.fill('not-a-key');
await settings.getByRole('button', { name: 'Enregistrer la clé' }).click();
await page.getByText('Ceci ne ressemble pas à une clé API Anthropic').waitFor();
check(!(await status()).configured, 'malformed key refused');

// A revoked key is stored, and the test says so.
await keyField.fill(WRONG);
await settings.getByRole('button', { name: 'Enregistrer la clé' }).click();
await settings.getByText('Clé configurée · …0000 · trousseau du système').waitFor();
await settings.getByRole('button', { name: 'Tester la clé' }).click();
await settings.getByText('Anthropic refuse cette clé').waitFor();
check(true, 'revoked key: the free test says it is refused');

// The valid key replaces it; the field is cleared and the key appears nowhere in the page.
await keyField.fill(VALID);
await settings.getByRole('button', { name: 'Enregistrer la clé' }).click();
await settings.getByText('Clé configurée · …ab12 · trousseau du système').waitFor();
check((await keyField.inputValue()) === '', 'field cleared after saving');
const html = await page.content();
const st = JSON.stringify(await status());
check(
  !html.includes(VALID) && !st.includes(VALID),
  'the key appears neither in the page nor in the status',
);
await settings.getByRole('button', { name: 'Tester la clé' }).click();
await settings.getByText('La clé fonctionne.').waitFor();
const check1 = seen.at(-1);
check(
  check1.method === 'GET' && check1.url === '/v1/models?limit=1' && check1.key === VALID,
  'test: free GET /v1/models with the stored key',
);
await page.screenshot({ path: `${out}/c6-settings.png` });

// Extraction now runs with the stored key.
await settings.getByRole('button', { name: 'Fermer' }).click();
const pdf = path.join(out, 'datasheet.pdf');
fs.writeFileSync(pdf, '%PDF-1.4\n% synthetic test file\n%%EOF\n');
await page.getByRole('button', { name: 'Choisir le PDF et extraire…' }).waitFor();
await page.evaluate((p) => window.__pvExtractFile(p), pdf);
await page.getByText('Composant créé à partir de la fiche').waitFor({ timeout: 60000 });
const call = seen.at(-1);
check(
  call.method === 'POST' && call.url === '/v1/messages' && call.key === VALID,
  'extraction sends the stored key',
);

// Removed: extraction is disabled again.
await page.getByRole('button', { name: 'Paramètres', exact: true }).click();
await settings.getByRole('button', { name: 'Supprimer la clé' }).click();
await settings.getByText('Aucune clé configurée').waitFor();
check(!(await status()).configured, 'key removed from the credential store');
await settings.getByRole('button', { name: 'Fermer' }).click();
await page.getByRole('button', { name: 'Extraire d’une fiche technique (IA)…' }).click();
await page.getByRole('button', { name: 'Configurer la clé API…' }).waitFor();
check(true, 'without a key, extraction is offered the settings again');

await browser.close();
server.close();
console.log('C6 scenario passed');
