// A long list narrows as a name is typed in its search box: the items of
// the same [data-filter-scope] whose text does not hold it hide. The box
// takes the focus when its panel opens, and Enter in it sends nothing.
(function () {
    "use strict";

    const box = "[data-filter-input]";

    document.addEventListener("input", (event) => {
        const field = event.target;
        if (!(field instanceof HTMLInputElement) || !field.matches(box)) return;
        const scope = field.closest("[data-filter-scope]");
        const wanted = field.value.trim().toLocaleLowerCase();
        let shown = 0;
        for (const item of scope.querySelectorAll("[data-filter-text]")) {
            const match = item.dataset.filterText.toLocaleLowerCase().includes(wanted);
            item.hidden = !match;
            if (match) shown += 1;
        }
        const none = scope.querySelector("[data-filter-none]");
        if (none) none.hidden = shown > 0;
    });

    document.addEventListener("keydown", (event) => {
        const field = event.target;
        if (event.key === "Enter" && field instanceof HTMLInputElement && field.matches(box)) {
            event.preventDefault();
        }
    });

    // A toggle event does not bubble: listen while it goes down.
    document.addEventListener(
        "toggle",
        (event) => {
            const panel = event.target;
            if (!(panel instanceof HTMLDetailsElement) || !panel.open) return;
            if (!panel.matches("[data-filter-focus]")) return;
            panel.querySelector(box)?.focus();
        },
        true,
    );
})();
