// Who sees a service, in one menu. Its face says the choice; ticking a
// group chooses "Some groups".
(function () {
    "use strict";

    const textOf = (input) => input.nextElementSibling.textContent.trim();

    function summary(picker) {
        const mode = picker.querySelector('input[name="visibility"]:checked');
        if (!mode) return "";
        if (mode.value !== "groups") return textOf(mode);
        const names = [...picker.querySelectorAll('input[name="group_ids"]:checked')].map(textOf);
        if (names.length === 0) return textOf(mode);
        if (names.length <= 2) return names.join(", ");
        return picker.dataset.more.replace("{first}", names[0]).replace("{n}", String(names.length - 1));
    }

    document.addEventListener("change", (event) => {
        const input = event.target;
        if (!(input instanceof HTMLInputElement)) return;
        const picker = input.closest("[data-visibility-picker]");
        if (!picker) return;
        if (input.name === "group_ids" && input.checked) {
            const groups = picker.querySelector('input[name="visibility"][value="groups"]');
            if (groups) groups.checked = true;
        }
        picker.querySelector("[data-visibility-summary]").textContent = summary(picker);
    });
})();
