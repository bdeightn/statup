// An admin adds a notification destination: the form says where to find the
// address of the chosen tool, a test reaches the tool before anything is
// saved, and the destination is listed afterwards. Without a mail server
// (this instance has none), the Alerts page says nobody can subscribe by email.
import { createServer } from "node:http";

import { expect, test } from "../fixtures.js";
import { READER_SESSION } from "../reader.js";

// A tool on this machine that accepts every post and keeps what it got.
function startTool() {
    const received = [];
    const server = createServer((request, response) => {
        let body = "";
        request.on("data", (chunk) => {
            body += chunk;
        });
        request.on("end", () => {
            received.push({ method: request.method, url: request.url, body });
            response.writeHead(200);
            response.end("ok");
        });
    });
    return new Promise((resolve) => {
        server.listen(0, "127.0.0.1", () => resolve({ server, received, port: server.address().port }));
    });
}

test("an admin adds a destination, tests it, and finds it listed", async ({ page }, testInfo) => {
    const tool = await startTool();
    try {
        const name = `Tableau n8n ${testInfo.project.name}`;

        await page.goto("/admin/settings");
        await page.getByRole("link", { name: "Gérer les alertes" }).click();
        await expect(page).toHaveURL(/\/admin\/notifications$/);

        const form = page.locator("form[data-destination-form]");
        await expect(form, "the add form is closed on arrival").toBeHidden();
        await page.getByText("Nouveau canal").click();
        await page.getByRole("combobox", { name: "Type" }).click();
        await page.getByRole("option", { name: "Webhook" }).click();
        await expect(form.getByText("Statup y envoie un JSON")).toBeVisible();

        await form.getByLabel("Nom", { exact: true }).fill(name);
        await form.getByLabel("URL du webhook").fill(`http://127.0.0.1:${tool.port}/hook`);
        await form.getByRole("button", { name: "Envoyer un test" }).click();
        await expect(form.getByText("Message de test envoyé.")).toBeVisible();
        expect(tool.received).toHaveLength(1);
        expect(tool.received[0].method).toBe("POST");
        expect(tool.received[0].body).toContain('"happening":"test"');

        await form.getByRole("button", { name: "Ajouter le canal", exact: true }).click();
        await expect(page).toHaveURL(/\/admin\/notifications\?added=/);
        const row = page.getByRole("listitem").filter({ hasText: name });
        await expect(row.getByText("Aucun envoi", { exact: true })).toBeVisible();
    } finally {
        tool.server.close();
    }
});

test("without a mail server the Alerts page says nobody can subscribe by email", async ({ page }, testInfo) => {
    await page.goto("/admin/notifications");

    const subscribers = page.getByRole("region", { name: "Abonnés par e-mail" });
    await expect(subscribers.locator(".card-head .count")).toHaveText("0");
    await expect(subscribers.getByText("Abonnement impossible sans serveur SMTP.")).toBeVisible();
    await expect(subscribers.getByRole("link", { name: "Configurer le serveur SMTP" })).toBeVisible();
    await expect(page.locator('input[name="open"]')).toHaveCount(0);

    const first = await subscribers.boundingBox();
    const second = await page.getByRole("region", { name: "URL de la page" }).boundingBox();
    if (testInfo.project.name === "phone") {
        expect(second.y, "stacked: the address card comes below").toBeGreaterThanOrEqual(first.y + first.height);
    } else {
        expect(second.y, "side by side: same top").toBeCloseTo(first.y, 0);
        expect(second.x, "side by side: different left").toBeGreaterThan(first.x);
    }
});

test("the subscriber list is empty and leads back to Alerts", async ({ page }) => {
    await page.goto("/admin/notifications/subscribers");
    await expect(page.getByText("Aucun abonné.", { exact: true })).toBeVisible();

    await page.locator("a.back-link", { hasText: "Alertes" }).click();
    await expect(page).toHaveURL(/\/admin\/notifications$/);
});

test("a reader is refused the subscriber list", async ({ browser }) => {
    const reader = await browser.newContext({ storageState: READER_SESSION });
    try {
        const answer = await reader.request.get("/admin/notifications/subscribers");
        expect(answer.status()).toBe(403);
    } finally {
        await reader.close();
    }
});
