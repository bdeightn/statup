// A visitor without an account reads a page open to everyone, and finds
// where to sign in: in the masthead on a desktop, in the menu on a phone.
// Without a mail server (this instance has none), the subscribe page offers
// the feed and the chat tools only, and still fits the screen.
import { expect, test } from "../fixtures.js";

test.use({ storageState: { cookies: [], origins: [] } });

test("a visitor reads the page without signing in", async ({ page }, testInfo) => {
    await page.goto("/");
    await expect(page).toHaveURL(/\/$/);
    await expect(
        page.getByText("Intranet", { exact: true }).filter({ visible: true }).first(),
    ).toBeVisible();

    if (testInfo.project.name === "phone") {
        await page.getByRole("button", { name: "Menu" }).click();
    }
    await expect(page.getByRole("link", { name: "Connexion" }).filter({ visible: true })).toHaveCount(1);
});

test("without a mail server the subscribe page offers no email form", async ({ page }) => {
    await page.goto("/subscribe");
    await expect(page.getByRole("heading", { name: "Dans Teams, Slack ou par courriel" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Par courriel", exact: true })).toHaveCount(0);
    await expect(page.locator("section.subscribe-band")).toHaveCount(0);
    await expect(page.locator('form[action="/subscribe/email"]')).toHaveCount(0);

    const widths = await page.evaluate(() => ({
        scroll: document.documentElement.scrollWidth,
        client: document.documentElement.clientWidth,
    }));
    expect(widths.scroll, "no horizontal scroll").toBe(widths.client);
});
