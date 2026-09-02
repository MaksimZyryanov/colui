import { firefox } from '@playwright/test';

const browser = await firefox.launch({ headless: true });
const failurePage = await browser.newPage();
await failurePage.goto(`${process.argv[2]}/?runtimeFailure`, { waitUntil: 'networkidle' });
await failurePage.getByText('No projects yet').waitFor();
if (!await failurePage.getByText('runtime offline').isVisible()) throw new Error('runtime failure was not visible');
const page = await browser.newPage();
await page.goto(`${process.argv[2]}/?lifecycle`, { waitUntil: 'networkidle' });
await page.getByRole('button', { name: /add project/i }).click();
await page.getByLabel('Display name').fill('Smoke Project');
await page.getByLabel('Compose name').fill('smoke');
await page.getByLabel('Working directory').fill('/tmp/smoke');
await page.getByRole('button', { name: 'Next' }).click();
await page.getByRole('button', { name: 'Save', exact: true }).click();
await page.getByText('Smoke Project').waitFor();
await page.getByRole('button', { name: /edit smoke project/i }).click();
await page.getByLabel('Display name').fill('Edited Project');
await page.getByRole('button', { name: 'Next' }).click();
await page.getByRole('button', { name: 'Save', exact: true }).click();
await page.getByText('Edited Project').waitFor();
await page.getByRole('button', { name: /more actions/i }).click();
await page.getByRole('menuitem', { name: 'Tear down' }).click();
await page.getByRole('button', { name: 'Tear down' }).click();
await page.getByRole('button', { name: /more actions/i }).click();
await page.getByRole('menuitem', { name: 'Apply' }).click();
await page.getByRole('button', { name: 'Stop' }).click();
await page.getByRole('button', { name: 'Restart' }).click();
const { invocations, profileId } = await page.evaluate(() => ({ invocations: window.__COLUI_MOCK_INVOCATIONS ?? [], profileId: window.__COLUI_MOCK_PROFILE_ID }));
for (const command of ['apply_project', 'stop_project', 'tear_down_project', 'restart_project']) {
  const call = invocations.find(invocation => invocation.command === command);
  if (!profileId || !call || JSON.stringify(call.args) !== JSON.stringify({ profileId })) throw new Error(`invalid ${command} payload`);
}
await browser.close();
