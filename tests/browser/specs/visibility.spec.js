// A service reserved for a group stays out of sight of a reader who is not in
// it and of a visitor, then shows to the reader once the owner adds them to
// the group. The test removes what it made, so the specs after it see the
// instance as before.
import { expect, test } from "../fixtures.js";
import { READER_SESSION } from "../reader.js";

test.skip(({ isMobile }) => isMobile, "the same pages on a phone, checked once");

const GROUP = "Compta";
const SERVICE = "Coffre-fort";
const NOBODY = { cookies: [], origins: [] };

async function homeShowsService(browser, storageState) {
    const context = await browser.newContext({ storageState });
    try {
        const page = await context.newPage();
        await page.goto("/");
        await expect(page.getByText("Intranet", { exact: true }).filter({ visible: true }).first()).toBeVisible();
        return (await page.getByText(SERVICE).filter({ visible: true }).count()) > 0;
    } finally {
        await context.close();
    }
}

async function createGroup(page) {
    await page.goto("/admin/users");
    await page.getByText("Nouveau groupe", { exact: true }).click();
    await page.getByLabel("Nom du groupe").fill(GROUP);
    await page.getByRole("button", { name: "Créer le groupe" }).click();
    await expect(page).toHaveURL(/\/admin\/groups\/\d+\?created=1$/);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(GROUP);
    return new URL(page.url()).pathname;
}

async function createReservedService(page) {
    await page.goto("/services/new");
    await page.getByLabel("Nom", { exact: true }).fill(SERVICE);
    await page.getByText("Restreinte", { exact: true }).click();
    await page.locator("[data-access-field] .grant-add > summary").click();
    await page.getByRole("checkbox", { name: new RegExp(`^${GROUP}`) }).check();
    await page.locator("[data-grant-add]").click();
    await expect(page.locator(`[data-access-field] [data-grant]:not([hidden])`)).toHaveText(new RegExp(GROUP));
    await page.getByRole("button", { name: "Créer le service" }).click();
    await expect(page).toHaveURL(/\/services\?saved=/);
    const href = await page.getByRole("link", { name: SERVICE, exact: true }).first().getAttribute("href");
    return href.match(/\/services\/(\d+)\/edit$/)[1];
}

test("a service reserved for a group shows only to the members of that group", async ({ page, browser }) => {
    const groupUrl = await createGroup(page);
    const serviceId = await createReservedService(page);

    const reader = await browser.newContext({ storageState: READER_SESSION });
    try {
        const answer = await reader.request.get(`/services/${serviceId}/drawer`);
        expect(answer.status(), "the reader is told the service does not exist").toBe(404);
    } finally {
        await reader.close();
    }
    expect(await homeShowsService(browser, READER_SESSION), "reader outside the group").toBe(false);
    expect(await homeShowsService(browser, NOBODY), "visitor").toBe(false);

    await page.goto(groupUrl);
    await page.getByText("Ajouter des membres", { exact: true }).click();
    await page.getByLabel(/Reader/).check();
    await page.getByRole("button", { name: "Ajouter", exact: true }).click();
    await expect(page.getByText("1 membre ajouté.")).toBeVisible();

    expect(await homeShowsService(browser, READER_SESSION), "reader inside the group").toBe(true);
    expect(await homeShowsService(browser, NOBODY), "visitor, still").toBe(false);

    await page.goto(`/services/${serviceId}/edit`);
    await page.getByRole("button", { name: "Supprimer", exact: true }).click();
    await page.getByRole("group", { name: /./ }).getByRole("button", { name: "Supprimer" }).click();
    await expect(page.getByText("Service supprimé.")).toBeVisible();
    await page.goto(groupUrl);
    await page.getByRole("button", { name: "Supprimer", exact: true }).click();
    await page.getByRole("group", { name: /./ }).getByRole("button", { name: "Supprimer" }).click();
    await expect(page.getByText("Groupe supprimé.")).toBeVisible();
});
