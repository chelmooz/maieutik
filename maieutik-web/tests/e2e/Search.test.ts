import { test, expect } from '@playwright/test';

test('Search page returns results', async ({ page }) => {
  await page.goto('/search');
  await page.fill('input[name="query"]', 'test');
  await page.click('button[type="submit"]');
  await expect(page.locator('text=Results')).toBeVisible();
});
