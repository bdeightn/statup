// An administrator adds a service and finds the line in the Journal, narrows
// the list by family, exports what is shown, and switches what the journal
// keeps from its menu and back.
import { expect, test } from "../fixtures.js";

test.skip(({ isMobile }) => isMobile, "the same page on a phone, checked once");

test("the journal keeps a new service, filters and exports it", async ({ page }) => {
    const name = "Journal desktop";
    const line = new RegExp(`a ajouté le service «.${name}.»`);

    await page.goto("/services/new");
    await page.getByLabel("Nom", { exact: true }).fill(name);
    await page.getByRole("button", { name: "Créer le service" }).click();
    await expect(page).toHaveURL(/\/services\?saved=/);

    await page.goto("/admin/settings");
    await page.getByRole("link", { name: /Journal/ }).click();
    await expect(page).toHaveURL(/\/admin\/journal$/);
    await expect(page.getByText(line)).toBeVisible();

    await page.goto("/admin/journal?category=system");
    await expect(page.getByText(line)).toHaveCount(0);
    await page.goto("/admin/journal?category=members");
    await expect(page.getByText(line)).toBeVisible();

    const exported = await page.request.get(
        await page.getByRole("link", { name: "Exporter" }).getAttribute("href"),
    );
    expect(exported.status()).toBe(200);
    expect(exported.headers()["content-type"]).toContain("text/csv");
    expect(await exported.text()).toContain(name);
});

test("what the journal keeps is switched from its menu", async ({ page }) => {
    await page.goto("/admin/journal");
    const choose = async (word, summary) => {
        await page.locator(".journal-edit").click();
        await page.locator(".journal-option").filter({ hasText: word }).click();
        await page.getByRole("button", { name: "Enregistrer" }).click();
        await expect(page.getByText("Réglages du journal enregistrés.")).toBeVisible();
        await expect(page.locator(".journal-summary")).toContainText(summary);
    };

    await choose("Détaillé", "Capture détaillée");
    await choose("Essentiel", "Capture essentielle");
});
