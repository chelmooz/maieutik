import { test } from '@playwright/test';

// Il n'existe pas encore de page /search ni de formulaire de recherche dans
// l'application. Le test est enregistré en « fixme » : il apparaît dans le
// rapport Playwright sans faire échouer la CI, et devra être activé en même
// temps que la fonctionnalité.
test.fixme('Search page returns results', async ({ page }) => {
  await page.goto('/search');
  await page.fill('input[name="query"]', 'test');
  await page.click('button[type="submit"]');
  await page.getByText('Results').waitFor();
});
