import { test } from '@playwright/test';

// Même mock que les autres tests : sans API, `FileList` laisse une promesse
// rejetée non gérée qui pollue la sortie sans rien tester.
test.beforeEach(async ({ page }) => {
  await page.route('**/api/files', (route) =>
    route.fulfill({
      json: [],
      headers: { 'access-control-allow-origin': '*' },
    }),
  );
});

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
