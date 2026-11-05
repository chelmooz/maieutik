import { test } from '@playwright/test';

test('home screenshot', async ({ page }) => {
  await page.goto('/');
  await page.screenshot({ path: '../docs/screenshots/home.png' });
});

test('chat screenshot', async ({ page }) => {
  await page.goto('/chat');
  await page.screenshot({ path: '../docs/screenshots/chat.png' });
});

test('search screenshot', async ({ page }) => {
  await page.goto('/?q=test');
  await page.screenshot({ path: '../docs/screenshots/search-results.png' });
});
