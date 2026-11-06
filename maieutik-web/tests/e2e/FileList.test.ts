import { test, expect } from '@playwright/test';

// Le client vise http://localhost:3000 : la réponse est interceptée ici pour
// que le test ne dépende pas d'un serveur d'API lancé à côté.
const cors = { 'access-control-allow-origin': '*' };

test('FileList renders the files returned by the API', async ({ page }) => {
  await page.route('**/api/files', (route) =>
    route.fulfill({ json: ['alpha.md', 'beta.md'], headers: cors }),
  );

  await page.goto('/');

  await expect(page.getByRole('heading', { name: 'Fichiers' })).toBeVisible();
  await expect(page.getByRole('listitem')).toHaveText(['alpha.md', 'beta.md']);
});
