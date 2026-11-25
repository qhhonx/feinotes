//! The browser save action used by application exports.

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function downloadQuietFile(name, bytes, mime) {
  const url = URL.createObjectURL(new Blob([bytes], {type: mime}));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  try { anchor.click(); }
  finally { setTimeout(() => URL.revokeObjectURL(url), 1000); }
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = downloadQuietFile)]
    pub(crate) fn download_file(name: &str, bytes: &[u8], mime: &str) -> Result<(), wasm_bindgen::JsValue>;
}

/// A file attachment's display name may have been written on another OS.
pub(crate) fn download_attachment(name: &str, bytes: &[u8]) -> Result<(), wasm_bindgen::JsValue> {
    let filename = name.rsplit(['/', '\\']).next().filter(|name| !matches!(*name, "" | "." | "..")).unwrap_or("Attachment");
    download_file(filename, bytes, "application/octet-stream")
}
