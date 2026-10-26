// The key just made: masked, shown on demand. Copying never needs it shown.
(function () {
    "use strict";

    document.querySelectorAll("[data-key-reveal]").forEach((button) => {
        const field = document.getElementById(button.getAttribute("aria-controls"));
        const showLabel = button.textContent;
        button.hidden = false;
        button.addEventListener("click", () => {
            const shown = field.type === "password";
            field.type = shown ? "text" : "password";
            button.textContent = shown ? button.dataset.hideLabel : showLabel;
        });
    });
})();
