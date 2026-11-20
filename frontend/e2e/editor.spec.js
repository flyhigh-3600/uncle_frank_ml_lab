import { test, expect } from "@playwright/test";

test("backend health and editor are available", async ({ page, request }) => {
  const health = await request.get("/api/health");
  expect(health.ok()).toBeTruthy();
  expect(await health.json()).toMatchObject({ lab: expect.any(String), version: expect.any(String) });

  await page.goto("/");
  await expect(page).toHaveTitle(/Frank's Plan Editor/);
  await expect(page.getByRole("textbox", { name: "Plan source" })).toBeVisible();
  await expect(page.locator(".node")).toHaveCount(4);
});

test("source text can be edited and run reports API output/events", async ({ page }) => {
  await page.goto("/");
  const source = page.getByRole("textbox", { name: "Plan source" });
  await expect(source).toHaveValue(/input prompt/);
  await source.fill('input prompt = "hello from Playwright";\noutput reply;\npipe prompt.text -> reply.text;');
  await page.getByRole("button", { name: /Run session/i }).click();
  await expect(page.locator("#session-state")).toHaveText(/RUNNING|COMPLETE/);
  await expect(page.locator("#terminal")).toContainText("output");
  await expect(page.locator("#terminal")).toContainText("reply: hello from a session");
  await expect(page.locator("#terminal")).toContainText("eventRunFinished");
});

test("backend diagnostics show validation or the documented unavailable fallback", async ({ page }) => {
  await page.goto("/");
  const backend = page.locator("#backend-status");
  await expect(backend).toContainText(/Backend connected|Backend unavailable/);
  const diagnostics = page.locator("#wire-status");
  await expect(diagnostics).toContainText(/Backend demo graph:|Backend graph validation unavailable/);
});

test("portal opens the layer source", async ({ page }) => {
  await page.goto("/");
  await page.locator("#portal-open").click();
  await expect(page.locator("#crumb-file")).toHaveText("layer_000.ufp");
  await expect(page.getByRole("textbox", { name: "Plan source" })).toHaveValue(/layer|input|output/i);
});

test(".txt notes remain inert and cannot be validated or run", async ({ page }) => {
  await page.goto("/");
  await page.locator('.file[data-file="notes.txt"]').click();
  const source = page.getByRole("textbox", { name: "Plan source" });
  await expect(source).toHaveAttribute("readonly", "");
  await expect(page.locator("#file-kind")).toContainText("INERT TXT");
  await expect(page.locator("#validate")).toBeDisabled();
  await expect(page.locator("#run")).toBeDisabled();
});
