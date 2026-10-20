// Headless smoke test against a running dev server:
//   npm run dev   (in another shell)
//   npx tsx e2e/smoke.ts [url]
// Uses the system Chromium; set CHROMIUM to point elsewhere. Screenshots land in .shots/.
// It creates a scratch factory and world and deletes them at the end.
import { chromium, type Page } from 'playwright-core';

const base = process.argv[2] ?? 'http://127.0.0.1:5173/';
const shots = new URL('../.shots/', import.meta.url).pathname;
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium-browser' });
const page = await browser.newPage({ viewport: { width: 1400, height: 1000 }, colorScheme: 'dark' });
const errors: string[] = [];
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
page.on('pageerror', (e) => errors.push(String(e)));

let failures = 0;
const created: string[] = [];
async function step(name: string, fn: () => Promise<void>) {
  try {
    await fn();
    console.log(`ok   ${name}`);
  } catch (e) {
    failures++;
    console.log(`FAIL ${name}: ${(e as Error).message.split('\n')[0]}`);
    await page.screenshot({ path: `${shots}fail-${name.replace(/\W+/g, '-')}.png`, fullPage: true });
  }
}
const shot = (name: string) => page.screenshot({ path: `${shots}${name}.png`, fullPage: true });
const tile = (p: Page, label: string) => p.locator('.tile', { hasText: label }).locator('.value').innerText();
function expect(cond: unknown, msg: string) {
  if (!cond) throw new Error(msg);
}

await page.goto(base);
await page.waitForSelector('text=Factories', { timeout: 15000 });

await step('T9 pasta tab: 1,335 foundations', async () => {
  await page.goto(`${base}#/satisfactory/f/t9-pasta/lines`);
  await page.waitForSelector('text=Foundations');
  const v = await tile(page, 'Foundations');
  expect(v.replace(/,/g, '') === '1335', `foundations tile shows ${v}`);
  await shot('pasta-lines');
});

let factoryId = '';
await step('new target factory: Alclad 10/min', async () => {
  await page.goto(`${base}#/satisfactory`);
  await page.getByRole('button', { name: 'New factory' }).click();
  await page.waitForURL(/\/f\//);
  await page.waitForSelector('text=Targets');
  factoryId = decodeURIComponent(page.url().split('/f/')[1].split('/')[0]);
  await page.locator('input[aria-label="Factory name"]').fill('smoke test');
  const picker = page.getByPlaceholder('What do you want to make?');
  await picker.fill('alclad');
  await page.locator('[role=listbox] button', { hasText: 'Alclad Aluminum Sheet' }).first().click();
  await page.locator('input[aria-label="Rate per minute"]').fill('10');
  await page.waitForTimeout(300);
  const machines = await tile(page, 'Machines');
  expect(Number(machines) > 0.5 && Number(machines) < 2, `machines ${machines}`);
  await shot('alclad-flow');
});

await step('click imports one consumer; shift-click imports all; undo restores', async () => {
  const before = await tile(page, 'Imports');
  const toggle = page.locator('article', { hasText: 'Alclad Aluminum Sheet' }).locator('.io-row', { hasText: 'Copper Ingot' }).locator('button.toggle');
  await toggle.click();
  await page.waitForTimeout(200);
  expect((await toggle.innerText()) === 'imported', 'toggle did not switch on');
  const balance = await page.locator('.side', { hasText: 'Imports' }).innerText();
  expect(balance.includes('Copper Ingot'), 'Copper Ingot not listed as import');
  await page.locator('body').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('Control+z');
  await page.waitForTimeout(200);
  expect((await tile(page, 'Imports')) === before, 'undo did not restore imports');
  await toggle.click({ modifiers: ['Shift'] });
  await page.waitForTimeout(200);
  expect((await toggle.innerText()) === 'imported', 'shift-click did not import');
});

await step('balance, buildings and lines views render', async () => {
  for (const v of ['balance', 'buildings', 'lines']) {
    await page.goto(`${base}#/satisfactory/f/${encodeURIComponent(factoryId)}/${v}`);
    await page.waitForSelector('table');
    await shot(`alclad-${v}`);
  }
});

await step('saved to disk', async () => {
  const res = await page.request.get(`${base}api/satisfactory/factories/${encodeURIComponent(factoryId)}`);
  const doc = await res.json();
  expect(doc.name === 'smoke test' && doc.targets?.[0]?.rate === 10, `saved doc ${JSON.stringify(doc).slice(0, 120)}`);
});

await step('share link opens read-only copy', async () => {
  await page.goto(`${base}#/satisfactory/f/${encodeURIComponent(factoryId)}/flow`);
  await page.waitForSelector('text=Targets');
  // Headless clipboards are unreliable; the app falls back to a prompt holding the link.
  let link = '';
  page.once('dialog', async (d) => {
    link = d.defaultValue();
    await d.dismiss();
  });
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.getByRole('button', { name: 'Share link' }).click();
  await page.waitForTimeout(300);
  if (!link) link = await page.evaluate(() => navigator.clipboard.readText());
  expect(link.includes('#/share/'), `clipboard ${link.slice(0, 60)}`);
  await page.goto(link);
  await page.waitForSelector('text=shared link');
  expect((await page.locator('input[aria-label="Factory name"]').inputValue()) === 'smoke test', 'shared name');
});

await step('world combines factories', async () => {
  await page.goto(`${base}#/satisfactory`);
  await page.getByRole('button', { name: 'New world' }).click();
  await page.waitForSelector('text=Factories in this world');
  const add = async (name: string) => {
    await page.getByPlaceholder('Add a factory…').fill(name);
    await page.locator('[role=listbox] button', { hasText: name }).first().click();
  };
  await add('pasta');
  await add('smoke test');
  await page.waitForTimeout(300);
  await shot('world');
  const f = await tile(page, 'Foundations');
  expect(Number(f.replace(/,/g, '')) > 1335, `world foundations ${f}`);
  created.push(`satisfactory/worlds/${page.url().split('/w/')[1]}`);
});

await step('factorio and dsp load and solve', async () => {
  for (const [game, item] of [['factorio', 'Electronic circuit'], ['dsp', 'Graphene']] as const) {
    await page.goto(`${base}#/${game}`);
    await page.getByRole('button', { name: 'New factory' }).click();
    await page.waitForURL(/\/f\//);
    await page.waitForSelector('text=Targets');
    const id = decodeURIComponent(page.url().split('/f/')[1].split('/')[0]);
    await page.getByPlaceholder('What do you want to make?').fill(item);
    await page.locator('[role=listbox] button').first().click();
    await page.waitForTimeout(300);
    const m = await tile(page, 'Machines');
    expect(Number(m.replace(/,/g, '')) > 0, `${game} machines ${m}`);
    await shot(`${game}-flow`);
    created.push(`${game}/factories/${encodeURIComponent(id)}`);
  }
});

await step('manual build: add, edit and delete a line', async () => {
  await page.goto(`${base}#/satisfactory`);
  await page.getByRole('button', { name: 'New build (manual)' }).click();
  await page.waitForURL(/\/f\//);
  const id = decodeURIComponent(page.url().split('/f/')[1].split('/')[0]);
  await page.getByPlaceholder('Add a recipe line…').fill('iron plate');
  await page.locator('[role=listbox] button', { hasText: 'Iron Plate' }).first().click();
  const count = page.locator('input[aria-label="Machines"]').first();
  await count.fill('3');
  await page.waitForTimeout(200);
  expect((await tile(page, 'Machines')) === '3', `machines ${await tile(page, 'Machines')}`);
  const imports = await tile(page, 'Imports');
  expect(imports === '1', `imports ${imports}`);
  await page.getByRole('button', { name: 'Delete line' }).click();
  await page.waitForTimeout(200);
  expect((await tile(page, 'Machines')) === '0', 'line not deleted');
  created.push(`satisfactory/factories/${encodeURIComponent(id)}`);
});

await step('settings page', async () => {
  await page.goto(`${base}#/satisfactory/settings`);
  await page.waitForSelector('text=Unlocked alternates');
  await shot('settings');
});

if (factoryId) created.push(`satisfactory/factories/${encodeURIComponent(factoryId)}`);
await page.goto('about:blank');
for (const path of created) await page.request.delete(`${base}api/${path}`);
console.log(errors.length ? `console errors:\n  ${errors.join('\n  ')}` : 'no console errors');
await browser.close();
process.exit(failures || errors.length ? 1 : 0);
