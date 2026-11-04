import { test, expect } from '@playwright/test';

test('FileList loads and displays files', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('h1')).toContainText('Fichiers');
  await expect(page.locator('ul')).toBeVisible();
});
