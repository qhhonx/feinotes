export function notify(type, data) {
  window.dispatchEvent(new CustomEvent('feinotes', {detail:{type,data}}));
}

export function notifyDownload(name, bytes, mime) {
  notify('download-binary', {name, bytes, mime});
}

export async function copyBoardPng(bytes, deniedMessage) {
  try {
    const image = new Blob([bytes], {type:'image/png'});
    await navigator.clipboard.write([new ClipboardItem({'image/png':image})]);
  } catch {
    notify('board-clipboard-feedback', deniedMessage);
    return;
  }
  notify('board-clipboard-feedback', '');
}

const docxXml = 'application/vnd.feinotes.docxxml+xml';
const richClipboardType = 'web ' + docxXml;
const clipboardLimit = 32 * 1024 * 1024;

export async function writeDocumentClipboard(text, xml, cut) {
  const clipboard = navigator.clipboard;
  const rich = new Blob([xml], {type:docxXml});
  if (rich.size > clipboardLimit || typeof ClipboardItem === 'undefined' || !ClipboardItem.supports?.(richClipboardType)) {
    if (cut) throw new DOMException('The clipboard cannot preserve this selection.', 'NotSupportedError');
    await clipboard.writeText(text);
    return;
  }
  await clipboard.write([new ClipboardItem({
    'text/plain': new Blob([text], {type:'text/plain'}),
    [richClipboardType]: rich,
  })]);
}

export async function readDocumentClipboard(plain) {
  const clipboard = navigator.clipboard;
  if (plain) return {text: await clipboard.readText(), images: []};
  const items = await clipboard.read();
  const result = {images: []};
  let imageBudget = clipboardLimit;
  for (const item of items) {
    for (const [key, type] of [['text', 'text/plain'], ['html', 'text/html'], ['xml', richClipboardType]]) {
      if (result[key] !== undefined || !item.types.includes(type)) continue;
      const blob = await item.getType(type);
      if (blob.size <= clipboardLimit) result[key] = await blob.text();
    }
    if (item.types.includes('image/png')) {
      const blob = await item.getType('image/png');
      if (blob.size && blob.size <= imageBudget) {
        imageBudget -= blob.size;
        result.images.push(new Uint8Array(await blob.arrayBuffer()));
      }
    }
  }
  return result;
}
