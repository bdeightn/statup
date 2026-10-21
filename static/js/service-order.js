// Choosing the order on the Services page. The mode gives each row two
// arrows that move it within its folder, and each folder two arrows that
// move it among the folders; the services in no folder stay first. A move
// shows at once, the focus stays on the arrow pressed, and the order is
// saved shortly after the last press. A refused save reloads the page, the
// only way back to a truthful list. With services that have a problem
// listed first, the mode puts back the chosen order, and leaving it reloads
// the page to list them first again.
(function () {
    "use strict";

    const SAVE_DELAY = 600;
    const URLS = { services: "/services/order", folders: "/services/folders/order" };
    const list = document.querySelector("[data-service-order]");
    const toggle = document.querySelector("[data-order-toggle]");
    const done = document.querySelector("[data-order-done]");
    if (!list || !toggle || !done) return;
    const problemsFirst = document.querySelector("[data-problems-first]");

    const { announce, csrfToken } = window.statup;
    const timers = {};
    let sending = Promise.resolve();

    const rows = () => Array.from(list.querySelectorAll("[data-service-id]"));
    const folders = () => Array.from(list.querySelectorAll("[data-folder-id]"));
    const ordering = () => list.classList.contains("is-ordering");

    function post(url, fields) {
        return fetch(url, {
            method: "POST",
            body: new URLSearchParams(fields),
            credentials: "same-origin",
            headers: { "X-CSRF-Token": csrfToken() },
            keepalive: true,
        }).then((response) => {
            if (!response.ok) throw new Error(`not saved: ${response.status}`);
            announce(list.dataset.saved);
        });
    }

    function fieldsOf(kind) {
        return kind === "folders"
            ? folders().map((folder) => ["order", folder.dataset.folderId])
            : rows().map((row) => ["order", row.dataset.serviceId]);
    }

    // One save at a time, so an older order never lands after a newer one;
    // a page being left cannot wait for its turn.
    function save(kind, leaving) {
        delete timers[kind];
        const fields = fieldsOf(kind);
        const send = () => post(URLS[kind], fields);
        const next = leaving ? send() : sending.then(send);
        sending = next.catch(() => window.location.reload());
    }

    function saveSoon(kind) {
        window.clearTimeout(timers[kind]);
        timers[kind] = window.setTimeout(() => save(kind, false), SAVE_DELAY);
    }

    function flush(leaving) {
        for (const kind of Object.keys(timers)) {
            window.clearTimeout(timers[kind]);
            save(kind, leaving);
        }
    }

    // The first of a run cannot go up, the last cannot go down.
    function syncRun(items, selector) {
        items.forEach((item, index) => {
            item.querySelector(`[${selector}="up"]`).disabled = index === 0;
            item.querySelector(`[${selector}="down"]`).disabled = index === items.length - 1;
        });
    }

    function syncEnds() {
        for (const run of list.querySelectorAll(".svc-rows")) {
            syncRun(Array.from(run.querySelectorAll("[data-service-id]")), "data-move");
        }
        syncRun(folders(), "data-move-folder");
    }

    function announceMove(item, siblings, template, name) {
        announce(
            (template || "")
                .replace("{name}", () => name)
                .replace("{index}", String(siblings.indexOf(item) + 1))
                .replace("{total}", String(siblings.length)),
        );
    }

    // A row moves within its folder, a folder after the services in none.
    function move(button, item, isMovable) {
        const up = button.dataset.move === "up" || button.dataset.moveFolder === "up";
        const neighbour = up ? item.previousElementSibling : item.nextElementSibling;
        if (!neighbour || !isMovable(neighbour)) return false;
        neighbour.insertAdjacentElement(up ? "beforebegin" : "afterend", item);
        syncEnds();
        // At the top or the bottom the pressed arrow is disabled; the other
        // one keeps the focus where it was.
        const attribute = button.dataset.move ? "data-move" : "data-move-folder";
        const other = button.parentElement.querySelector(`[${attribute}="${up ? "down" : "up"}"]`);
        (button.disabled ? other : button).focus();
        return true;
    }

    function moveRow(button) {
        const row = button.closest("[data-service-id]");
        if (!move(button, row, (neighbour) => neighbour.hasAttribute("data-service-id"))) return;
        const siblings = Array.from(row.parentElement.children);
        announceMove(row, siblings, list.dataset.moved, row.dataset.serviceName);
        saveSoon("services");
    }

    function moveFolder(button) {
        const folder = button.closest("[data-folder-id]");
        if (!move(button, folder, (neighbour) => neighbour.hasAttribute("data-folder-id"))) return;
        announceMove(folder, folders(), list.dataset.folderMoved, folder.dataset.folderName);
        saveSoon("folders");
    }

    function putBackChosenOrder() {
        for (const run of list.querySelectorAll(".svc-rows")) {
            Array.from(run.children)
                .sort((a, b) => Number(a.dataset.place) - Number(b.dataset.place))
                .forEach((row) => run.appendChild(row));
        }
        syncEnds();
    }

    // As on the dashboard: the mode shows Done and its own option, and hides
    // the buttons that would leave it.
    function reveal(on) {
        document.querySelectorAll("[data-order-only]").forEach((el) => (el.hidden = !on));
        document.querySelectorAll("[data-order-off]").forEach((el) => (el.hidden = on));
    }

    function setMode(on) {
        if (on && problemsFirst?.checked) putBackChosenOrder();
        list.classList.toggle("is-ordering", on);
        reveal(on);
        (on ? done : toggle).focus();
        if (on) return;
        flush(false);
        if (problemsFirst?.checked) sending.then(() => window.location.reload());
    }

    problemsFirst?.addEventListener("change", () => {
        const fields = [["enabled", String(problemsFirst.checked)]];
        sending = sending
            .then(() => post("/services/problems-first", fields))
            .catch(() => window.location.reload());
    });

    toggle.addEventListener("click", () => setMode(true));
    done.addEventListener("click", () => setMode(false));
    list.addEventListener("click", (event) => {
        if (!ordering()) return;
        const row = event.target.closest("[data-move]");
        if (row) moveRow(row);
        const folder = event.target.closest("[data-move-folder]");
        if (folder) moveFolder(folder);
    });
    // Escape closes the mode from its own controls only, not from a menu
    // opened meanwhile.
    document.addEventListener("keydown", (event) => {
        const own = list.contains(document.activeElement) || document.activeElement === done;
        if (event.key !== "Escape" || event.defaultPrevented || !ordering() || !own) return;
        setMode(false);
    });
    window.addEventListener("pagehide", () => flush(true));

    syncEnds();
    reveal(false);
})();
