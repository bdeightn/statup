// A day coloured by a detected outage opens the service's panel on that day's
// outages; an editor erases the outage that was not one, with the one that
// fell at the same moment on another service.
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { expect, test } from "../fixtures.js";

test.skip(({ isMobile }) => isMobile, "the panel does not exist on a phone");

async function createService(page, name) {
    await page.goto("/services/new");
    await page.getByLabel("Nom", { exact: true }).fill(name);
    await page.getByText("Port", { exact: true }).filter({ visible: true }).click();
    await page.getByRole("textbox", { name: "Hôte", exact: true }).fill("127.0.0.1");
    await page.getByRole("textbox", { name: "Port", exact: true }).fill("1");
    await page.getByRole("button", { name: "Créer le service" }).click();
    await expect(page).toHaveURL(/\/services\?saved=/);
}

function openDatabase() {
    return new DatabaseSync(join(process.env.STATUP_BROWSER_DATA, "statup.db"));
}

// Old services, so the strip tracks the day, each with one ended outage of
// 46 minutes; the second starts two minutes after the first.
function recordEndedOutages(names) {
    const db = openDatabase();
    try {
        names.forEach((name, index) => {
            const { id } = db.prepare("SELECT id FROM services WHERE name = ?").get(name);
            db.prepare("UPDATE services SET created_at = datetime('now', '-60 days') WHERE id = ?").run(id);
            db.prepare(
                `INSERT INTO service_outages (service_id, started_at, ended_at, source, reason)
                 VALUES (?, datetime('now', '-3 hours', ?), datetime('now', '-3 hours', ?), 'check', 'name not found')`,
            ).run(id, `+${index * 2} minutes`, `+${index * 2 + 46} minutes`);
        });
    } finally {
        db.close();
    }
}

function outagesLeft(names) {
    const db = openDatabase();
    try {
        const marks = names.map(() => "?").join(", ");
        return db
            .prepare(
                `SELECT COUNT(*) AS n FROM service_outages
                 WHERE service_id IN (SELECT id FROM services WHERE name IN (${marks}))`,
            )
            .get(...names).n;
    } finally {
        db.close();
    }
}

test("erasing a detected outage from the panel erases the one that fell with it", async ({ page }, testInfo) => {
    const first = `Messagerie ${testInfo.project.name}`;
    const second = `Agenda ${testInfo.project.name}`;
    await createService(page, first);
    await createService(page, second);
    recordEndedOutages([first, second]);

    await page.goto("/");
    const row = page.locator(".svc-link", { hasText: first }).filter({ visible: true });
    await row.locator("[data-outage-day]").first().click();

    const panel = page.locator("#drawer-content");
    await expect(panel.getByText("Pannes détectées")).toBeVisible();
    await expect(panel.getByText("46 min")).toBeVisible();
    await expect(panel.getByText("Nom introuvable")).toBeVisible();
    await expect(panel.locator("button[data-outage-day]").first()).toHaveAttribute("aria-pressed", "true");

    await panel.getByRole("button", { name: "Ce n'était pas une panne" }).click();
    await expect(
        panel.getByRole("checkbox", { name: "Effacer aussi chez l'autre service tombé au même moment" }),
    ).toBeChecked();
    await panel.getByRole("button", { name: "Effacer", exact: true }).click();

    await expect(panel.getByText("Pannes détectées")).toHaveCount(0);
    expect(outagesLeft([first, second])).toBe(0);
});
