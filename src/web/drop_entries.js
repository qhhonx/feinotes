// DataTransfer only grants filesystem entries during the drop callback.
export function captureDropEntries(transfer) {
    const entries = Array.from(transfer.items).filter(item => item.kind === "file")
        .map(item => item.webkitGetAsEntry?.() ?? item.getAsFile()).filter(Boolean);
    return entries.length ? entries : Array.from(transfer.files);
}

export async function readDropEntries(entries, limits, skip, active) {
    const files = new Map();
    let visited = 0;

    async function visit(entry, parent, depth) {
        if (!active()) return;
        if (++visited > limits.entries || depth > limits.depth) throw new Error("Dropped folder is too large");
        if (skip(entry.name, depth, Boolean(entry.isDirectory))) return;
        const path = parent ? parent + "/" + entry.name : entry.name;
        if (entry.isDirectory) {
            const reader = entry.createReader();
            while (active()) {
                const batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
                if (!batch.length) break;
                for (const child of batch) await visit(child, path, depth + 1);
            }
        } else {
            const file = entry instanceof File ? entry : await new Promise((resolve, reject) => entry.file(resolve, reject));
            if (!active()) return;
            if (files.has(path)) throw new Error("duplicate-drop-path");
            files.set(path, file);
        }
    }

    for (const entry of entries) await visit(entry, "", 0);
    return Array.from(files);
}

export function listenQuietFileDrop(destination, maxEntries) {
    const fromCanvas = event =>
        event.target instanceof HTMLCanvasElement && event.dataTransfer?.types.includes("Files");

    document.addEventListener("dragover", event => {
        if (!fromCanvas(event)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = destination(event.offsetX, event.offsetY, null) ? "copy" : "none";
    });
    document.addEventListener("drop", event => {
        if (!fromCanvas(event)) return;
        event.preventDefault();
        const {offsetX: x, offsetY: y, dataTransfer: transfer} = event;
        if (!destination(x, y, null)) return;
        const entries = captureDropEntries(transfer);
        if (!entries.length) return;
        const skip = (name, _, folder) => folder && (
            name === "node_modules" || name === "target" ||
            name.startsWith(".") && name !== ".assets" && name !== ".obsidian"
        );
        const read = () => readDropEntries(entries, {entries: maxEntries, depth: 32}, skip, () => true);
        destination(x, y, read, entries.length);
    });
}
