// An editor imports the services of an Uptime Kuma status page: a wrong
// address is explained, a first import brings the ticked groups only, a second
// one brings what is still missing, and a third finds nothing new. Statup reads
// a stand-in Kuma started here; the test removes what it made.
import { createServer } from "node:http";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { expect, test } from "../fixtures.js";

const DOCS_LINK = "https://statup.dev/fr/docs/monitoring-tools/#uptime-kuma";

function statusPage(p) {
    return {
        config: { slug: "infra", title: "Infra" },
        incidents: [],
        publicGroupList: [
            {
                id: 1,
                name: `Infrastructure ${p}`,
                weight: 1,
                monitorList: [
                    { id: 1, name: `Mail ${p}`, type: "http" },
                    { id: 2, name: `VPN ${p}`, type: "port" },
                ],
            },
            {
                id: 2,
                name: `Tests ${p}`,
                weight: 2,
                monitorList: [{ id: 3, name: `Recette ${p}`, type: "http" }],
            },
        ],
        maintenanceList: [],
    };
}

async function startFakeKuma(p) {
    const server = createServer((request, response) => {
        const found = request.url === "/api/status-page/infra";
        response.writeHead(found ? 200 : 404, { "Content-Type": "application/json" });
        response.end(JSON.stringify(found ? statusPage(p) : { status: "fail", msg: "Status Page Not Found" }));
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    return server;
}

// Services go first: one kept in a folder that follows its visibility cannot
// outlive the folder, the table refuses it. The address kept for the next
// import goes too.
function removeImported(p) {
    const db = new DatabaseSync(join(process.env.STATUP_BROWSER_DATA, "statup.db"));
    try {
        for (const name of [`Mail ${p}`, `VPN ${p}`, `Recette ${p}`]) {
            db.prepare("DELETE FROM services WHERE name = ?").run(name);
        }
        for (const name of [`Infrastructure ${p}`, `Tests ${p}`]) {
            db.prepare("DELETE FROM service_folders WHERE name = ?").run(name);
        }
        db.prepare("DELETE FROM settings WHERE key IN ('kuma_import_address', 'kuma_import_internal_cert')").run();
    } finally {
        db.close();
    }
}

async function openImport(page) {
    await page.goto("/services");
    await page.locator("summary", { hasText: "Nouveau" }).click();
    await page.getByRole("link", { name: "Importer d'Uptime Kuma" }).click();
    await expect(page).toHaveURL(/\/services\/import$/);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Importer d'Uptime Kuma");
}

async function readPage(page, address) {
    await page.getByRole("textbox", { name: "URL de la page d'état" }).fill(address);
    await page.getByRole("button", { name: "Lire la page" }).click();
}

test("an editor imports the services of an Uptime Kuma status page", async ({ page }, testInfo) => {
    const p = testInfo.project.name;
    const infra = `Infrastructure ${p}`;
    const tests = `Tests ${p}`;
    removeImported(p);
    const kuma = await startFakeKuma(p);
    const base = `http://127.0.0.1:${kuma.address().port}/status`;
    const group = (name) => page.getByRole("listitem").filter({ hasText: name });

    try {
        await openImport(page);
        await readPage(page, `${base}/absente`);
        await expect(page.getByRole("alert")).toHaveText("Page d'état introuvable dans Uptime Kuma.");

        await readPage(page, `${base}/infra`);
        await expect(group(infra).getByRole("checkbox")).toBeChecked();
        await expect(group(tests).getByRole("checkbox")).toBeChecked();
        await expect(group(infra)).toContainText("2 à importer");
        await expect(group(tests)).toContainText("1 à importer");
        await page.getByRole("checkbox", { name: tests }).uncheck();
        await page.getByRole("button", { name: "Importer", exact: true }).click();
        await expect(page).toHaveURL(/\/services\?imported=2&folders=1$/);
        const notice = page.getByRole("status").filter({ hasText: "importé" });
        await expect(notice).toContainText("2 services importés. 1 dossier créé.");
        await expect(notice.getByRole("link", { name: "Brancher les alertes d'Uptime Kuma" })).toHaveAttribute(
            "href",
            DOCS_LINK,
        );
        await expect(page.getByText(`Mail ${p}`, { exact: true }).filter({ visible: true }).first()).toBeVisible();
        await expect(page.getByText(`VPN ${p}`, { exact: true }).filter({ visible: true }).first()).toBeVisible();

        await openImport(page);
        await expect(page.getByRole("textbox", { name: "URL de la page d'état" })).toHaveValue(`${base}/infra`);
        await page.getByRole("button", { name: "Lire la page" }).click();
        await expect(page.getByRole("checkbox", { name: infra })).toBeDisabled();
        await expect(group(infra)).toContainText("2 existent déjà");
        await expect(page.getByRole("checkbox", { name: tests })).toBeChecked();
        await expect(group(tests)).toContainText("1 à importer");
        await page.getByRole("button", { name: "Importer", exact: true }).click();
        await expect(page).toHaveURL(/\/services\?imported=1&folders=1$/);
        await expect(page.getByRole("status").filter({ hasText: "importé" })).toContainText(
            "1 service importé. 1 dossier créé.",
        );

        await openImport(page);
        await page.getByRole("button", { name: "Lire la page" }).click();
        await expect(page.getByText("Tous les services de cette page existent déjà.")).toBeVisible();
        await expect(page.getByRole("button", { name: "Importer", exact: true })).toHaveCount(0);
    } finally {
        await new Promise((resolve) => kuma.close(resolve));
        removeImported(p);
    }
});
