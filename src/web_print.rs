//! The browser print surface for an already rendered document.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
let closePrintFrame;

export function cancelQuietPrint() {
  closePrintFrame?.();
}

export function printQuietDocument(html) {
  cancelQuietPrint();
  return new Promise((resolve, reject) => {
    const frame = document.createElement("iframe");
    frame.setAttribute("sandbox", "allow-same-origin allow-modals");
    frame.setAttribute("aria-hidden", "true");
    frame.tabIndex = -1;
    Object.assign(frame.style, {position: "fixed", top: "0", left: "-9999px", width: "668px", height: "1000px", border: "0"});
    const close = () => {
      clearTimeout(timeout);
      frame.remove();
      if (closePrintFrame === close) closePrintFrame = undefined;
      resolve();
    };
    const fail = error => {
      reject(error);
      close();
    };
    const timeout = setTimeout(() => fail(new Error("Print frame timed out")), 20000);
    closePrintFrame = close;
    frame.addEventListener("load", async () => {
      try {
        const page = frame.contentDocument;
        const target = frame.contentWindow;
        if (!page?.querySelector("h1.title") || !target) throw new Error("Missing print document");
        await page.fonts.ready;
        if (closePrintFrame !== close) return;
        clearTimeout(timeout);
        target.addEventListener("afterprint", close, {once: true});
        target.print();
        resolve();
      } catch (error) {
        if (closePrintFrame === close) fail(error);
      }
    }, {once: true});
    frame.srcdoc = html;
    document.body.append(frame);
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = cancelQuietPrint)]
    pub(crate) fn cancel();
    #[wasm_bindgen(js_name = printQuietDocument)]
    fn print_document(html: &str) -> js_sys::Promise;
}

pub(crate) fn print(html: &str) -> impl Future<Output = Result<(), &'static str>> + use<> {
    // Install the frame now so a subsequent account or route change can cancel it.
    let printing = JsFuture::from(print_document(html));
    async move {
        printing.await.map(|_| ()).map_err(|_| crate::settings::tr(
            "无法准备打印，请重试。",
            "Unable to prepare the document for printing.",
        ))
    }
}
