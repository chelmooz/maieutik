import { test } from '@playwright/test';

// Captures d'écran : artefacts de test (test-results/, ignoré par git), pas
// des baselines de comparaison. Aucune assertion visuelle pour l'instant.
const cors = { 'access-control-allow-origin': '*' };

test('capture home', async ({ page }) => {
  await page.route('**/api/files', (route) =>
    route.fulfill({ json: ['alpha.md', 'beta.md'], headers: cors }),
  );
  await page.goto('/');
  await page.screenshot({
    path: 'test-results/screenshots/home.png',
    fullPage: true,
  });
});

test('capture chat', async ({ page }) => {
  await page.goto('/chat');
  await page.screenshot({
    path: 'test-results/screenshots/chat.png',
    fullPage: true,
  });
});
