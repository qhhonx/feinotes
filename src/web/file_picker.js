// A picker must open during the original browser user activation.
const pending = new WeakMap();

export function pickBrowserFiles(accept, directory, existing) {
    return new Promise((resolve, reject) => {
        const input = existing ?? document.createElement("input");
        input.type = "file";
        if (directory && !("webkitdirectory" in input)) {
            reject(new Error("Folder selection is unavailable"));
            return;
        }

        pending.get(input)?.();
        input.value = "";
        input.accept = accept;
        input.multiple = true;
        input.toggleAttribute("webkitdirectory", directory);
        input.hidden = true;
        if (!existing) document.body.append(input);

        const clean = () => {
            pending.delete(input);
            input.removeEventListener("change", change);
            input.removeEventListener("cancel", cancel);
            input.value = "";
            if (!existing) input.remove();
        };
        const change = () => {
            const files = Array.from(input.files);
            clean();
            resolve(files);
        };
        const cancel = () => {
            clean();
            resolve([]);
        };
        input.addEventListener("change", change);
        input.addEventListener("cancel", cancel);
        pending.set(input, cancel);
        try {
            if (typeof input.showPicker !== "function") {
                input.click();
            } else {
                try { input.showPicker(); }
                catch (error) {
                    if (error.name !== "NotAllowedError") throw error;
                    input.click();
                }
            }
        } catch (error) {
            clean();
            reject(error);
        }
    });
}
