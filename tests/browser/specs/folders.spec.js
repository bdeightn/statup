// A service put in a folder follows the folder's visibility: once the folder
// is opened to one reader only, that reader sees the folder's title and its
// service on the dashboard, and a visitor sees neither. The test removes
// what it made, so the specs after it see the instance as before.
import { expect, test } from "../fixtures.js";
import { READER_SESSION } from "../reader.js";

test.skip(({ isMobile }) => isMobile, "the same pages on a phone, checked once");

const FOLDER = "Paie";
const SERVICE = "Logiciel de paie";
const NOBODY = { cookies: [], origins: [] };

async function dashboardShows(browser, storageState) {
    const context = await browser.newContext({ storageState });
    try {
        const page = await context.newPage();
        await page.goto("/");
        await expect(page.getByText("Intranet", { exact: true }).filter({ visible: true }).first()).toBeVisible();
        const folder = page.locator(".svc-folder", { hasText: FOLDER }).filter({ visible: true });
        const service = page.getByText(SERVICE, { exact: true }).filter({ visible: true });
        return { folder: (await folder.count()) > 0, service: (await service.count()) > 0 };
    } finally {
        await context.close();
    }
}

async function createService(page) {
    await page.goto("/services/new");
    await page.getByLabel("Nom", { exact: true }).fill(SERVICE);
    await page.getByRole("button", { name: "Créer le service" }).click();
    await expect(page).toHaveURL(/\/services\?saved=/);
    const href = await page.getByRole("link", { name: SERVICE, exact: true }).first().getAttribute("href");
    return href.match(/\/services\/(\d+)\/edit$/)[1];
}

async function createFolder(page) {
    await page.goto("/services");
    await page.locator("summary", { hasText: "Nouveau" }).click();
    await page.getByRole("button", { name: "Dossier", exact: true }).click();
    await page.getByLabel("Nom du dossier").fill(FOLDER);
    await page.getByRole("button", { name: "Créer le dossier" }).click();
    await expect(page).toHaveURL(/\/services\/folders\/\d+/);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(FOLDER);
    return new URL(page.url()).pathname;
}

async function addService(page) {
    await page.getByText("Ajouter des services", { exact: true }).click();
    await page.getByRole("checkbox", { name: new RegExp(`^${SERVICE}`) }).check();
    await page.getByRole("button", { name: "Ajouter", exact: true }).click();
    await expect(page.getByText("1 service ajouté. Il hérite de la visibilité du dossier.")).toBeVisible();
}

async function openToReaderOnly(page) {
    await page.locator(".folder-menu > summary").click();
    await page.getByText("Restreinte", { exact: true }).click();
    await page.locator("[data-access-field] .grant-add > summary").click();
    await page.getByRole("checkbox", { name: /^Reader/ }).check();
    await page.locator("[data-grant-add]").click();
    await expect(page.locator("[data-access-field] [data-grant]:not([hidden])")).toHaveText(/Reader/);
    await page.locator(".folder-panel").getByRole("button", { name: "Enregistrer" }).click();
    await expect(page.getByText("Dossier enregistré.")).toBeVisible();
}

async function deleteAll(page, folderUrl, serviceId) {
    await page.goto(folderUrl);
    await page.getByRole("button", { name: "Supprimer", exact: true }).click();
    await page.getByRole("group", { name: /./ }).getByRole("button", { name: "Supprimer" }).click();
    await expect(page.getByText("Dossier supprimé.")).toBeVisible();
    await page.goto(`/services/${serviceId}/edit`);
    await page.getByRole("button", { name: "Supprimer", exact: true }).click();
    await page.getByRole("group", { name: /./ }).getByRole("button", { name: "Supprimer" }).click();
    await expect(page.getByText("Service supprimé.")).toBeVisible();
}

test("a folder opened to one reader shows its title and its service to that reader only", async ({ page, browser }) => {
    const serviceId = await createService(page);
    const folderUrl = await createFolder(page);
    await addService(page);

    const everyone = { folder: true, service: true };
    expect(await dashboardShows(browser, NOBODY), "visitor, public folder").toEqual(everyone);

    await openToReaderOnly(page);
    expect(await dashboardShows(browser, READER_SESSION), "the reader it is open to").toEqual(everyone);
    expect(await dashboardShows(browser, NOBODY), "visitor").toEqual({ folder: false, service: false });

    await deleteAll(page, folderUrl, serviceId);
});
