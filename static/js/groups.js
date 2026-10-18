// The members offered to a group narrow as a name or an address is typed;
// the search box takes the focus when the panel opens, and Enter in it
// does not send the form.
(function () {
    "use strict";

    const search = "[data-member-search]";

    document.addEventListener("input", (event) => {
        const field = event.target;
        if (!(field instanceof HTMLInputElement) || !field.matches(search)) return;
        const form = field.closest("form");
        const wanted = field.value.trim().toLocaleLowerCase();
        let shown = 0;
        for (const row of form.querySelectorAll("[data-member-name]")) {
            const match = row.dataset.memberName.toLocaleLowerCase().includes(wanted);
            row.hidden = !match;
            if (match) shown += 1;
        }
        form.querySelector("[data-member-none]").hidden = shown > 0;
    });

    document.addEventListener("keydown", (event) => {
        const field = event.target;
        if (event.key === "Enter" && field instanceof HTMLInputElement && field.matches(search)) {
            event.preventDefault();
        }
    });

    // A toggle event does not bubble: listen while it goes down.
    document.addEventListener(
        "toggle",
        (event) => {
            const picker = event.target;
            if (!(picker instanceof HTMLDetailsElement) || !picker.open) return;
            if (!picker.matches("[data-member-picker]")) return;
            picker.querySelector(search)?.focus();
        },
        true,
    );
})();
