import { test } from '@playwright/test';

test('UI Monkey Test - Click Storm', async ({ page }) => {
  await page.goto('/');
  
  for (let i = 0; i < 100; i++) {
    const x = Math.floor(Math.random() * 1280);
    const y = Math.floor(Math.random() * 720);
    await page.mouse.click(x, y);
  }
});

test('UI Monkey Test - Keyboard Random', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.type('abcdefghijklmnopqrstuvwxyz');
});
