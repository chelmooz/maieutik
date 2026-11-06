import { test, expect } from '@playwright/test';

test('Chat sends the message and renders the reply', async ({ page }) => {
  await page.route('**/chat', (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    return route.fulfill({
      json: { message: { role: 'assistant', content: 'Bonjour humain' } },
      headers: { 'access-control-allow-origin': '*' },
    });
  });

  await page.goto('/chat');

  await page.getByPlaceholder('Posez votre question...').fill('Salut');
  await page.getByRole('button', { name: 'Envoyer' }).click();

  await expect(page.getByText('Salut')).toBeVisible();
  await expect(page.getByText('Bonjour humain')).toBeVisible();
});
