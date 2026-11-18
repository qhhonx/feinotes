//! The system pasteboard beyond what GPUI reads.

/// Devtools stand-in for the pasteboard: scripted tests
/// must not touch the shared system clipboard.
#[derive(Clone, Default)]
pub struct FakeClipboard {
    pub html: Option<String>,
    pub text: String,
    pub images: Vec<gpui::Image>,
}

pub static FAKE_CLIPBOARD: std::sync::Mutex<Option<FakeClipboard>> = std::sync::Mutex::new(None);

/// Prefer HTML, then decode native RTF/RTFD through AppKit. GPUI reads
/// only plain text and images; Notes and TextEdit often supply RTF instead.
#[cfg(target_os = "macos")]
pub fn clipboard_html() -> Option<String> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    const TYPE: &str = "public.html";
    unsafe {
        let pb: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
        let ty: *mut Object = msg_send![class!(NSString), alloc];
        let ty: *mut Object = msg_send![ty,
            initWithBytes: TYPE.as_ptr()
            length: TYPE.len()
            encoding: 4usize]; // NSUTF8StringEncoding
        let s: *mut Object = msg_send![pb, stringForType: ty];
        let _: () = msg_send![ty, release];
        if !s.is_null() {
            let p: *const std::os::raw::c_char = msg_send![s, UTF8String];
            if !p.is_null() {
                let html = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
                if !html.trim().is_empty() { return Some(html); }
            }
        }
        for (kind, rtfd) in [("public.rtf", false), ("com.apple.flat-rtfd", true)] {
            let ty: *mut Object = msg_send![class!(NSString), alloc];
            let ty: *mut Object = msg_send![ty, initWithBytes: kind.as_ptr() length: kind.len() encoding: 4usize];
            let data: *mut Object = msg_send![pb, dataForType: ty];
            let _: () = msg_send![ty, release];
            if data.is_null() { continue; }
            let len: usize = msg_send![data, length];
            let bytes: *const u8 = msg_send![data, bytes];
            if len == 0 || bytes.is_null() { continue; }
            if let Some(html) = native_rich_html(std::slice::from_raw_parts(bytes, len), rtfd) { return Some(html); }
        }
        None
    }
}

#[cfg(target_os = "macos")]
fn native_rich_html(bytes: &[u8], rtfd: bool) -> Option<String> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    #[repr(C)]
    struct TextRange { location: usize, length: usize }
    unsafe impl objc::Encode for TextRange {
        fn encode() -> objc::Encoding { unsafe { objc::Encoding::from_str("{_NSRange=QQ}") } }
    }
    unsafe {
        let data: *mut Object = msg_send![class!(NSData), dataWithBytes: bytes.as_ptr() length: bytes.len()];
        let text: *mut Object = msg_send![class!(NSAttributedString), alloc];
        let text: *mut Object = if rtfd { msg_send![text, initWithRTFD: data documentAttributes: std::ptr::null_mut::<*mut Object>()] }
            else { msg_send![text, initWithRTF: data documentAttributes: std::ptr::null_mut::<*mut Object>()] };
        if text.is_null() { return None; }
        let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c"DocumentType".as_ptr()];
        let value: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c"NSHTML".as_ptr()];
        let attrs: *mut Object = msg_send![class!(NSDictionary), dictionaryWithObject: value forKey: key];
        let len: usize = msg_send![text, length];
        let html: *mut Object = msg_send![text, dataFromRange: TextRange { location: 0, length: len } documentAttributes: attrs error: std::ptr::null_mut::<*mut Object>()];
        let result = if html.is_null() { None } else {
            let len: usize = msg_send![html, length];
            let ptr: *const u8 = msg_send![html, bytes];
            if ptr.is_null() || len == 0 { None } else { String::from_utf8(std::slice::from_raw_parts(ptr, len).to_vec()).ok() }
        };
        let _: () = msg_send![text, release];
        result.map(|s| s.replacen("<body", "<body data-fei-rich-text=\"rtf\"", 1))
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn native_rtf_import_preserves_run_styles_without_using_pasteboard() {
        let rtf = br"{\rtf1\ansi\deff0{\fonttbl{\f0 Helvetica;}}{\colortbl;\red203\green4\blue36;\red246\green220\blue251;}\f0\fs26 Plain \b Bold\b0  \i Italic\i0  \ul Underline\ul0  \strike Strike\strike0  \cf1 Red\cf0  \cb2 Highlight\cb0\par Unicode \u20013?\u25991?}";
        let html = native_rich_html(rtf, false).expect("AppKit RTF decoding");
        let blocks = crate::paste::html_to_blocks(&html);
        let marks = |word: &str| blocks.iter().find_map(|b| b.text.spans().find(|(r, _)| b.text.text[r.clone()].contains(word)).map(|(_, m)| m.clone())).expect(word);
        assert!(marks("Bold").bold, "{html}");
        assert!(marks("Italic").italic, "{html}");
        assert!(marks("Underline").underline, "{html}");
        assert!(marks("Strike").strike, "{html}");
        assert!(marks("Red").text_color.is_some(), "{html}");
        assert!(marks("Highlight").bg_color.is_some(), "{html}");
        assert!(blocks.iter().any(|b| b.text.text.contains("中文")), "{html}");
    }

    #[test]
    fn native_rtf_import_keeps_existing_heading_and_monospace_formats() {
        let rtf = br"{\rtf1\ansi\deff0{\fonttbl{\f0 Helvetica;}{\f1 Menlo;}}\f0\fs40\b Title\b0\par\fs32\b Heading\b0\par\fs40 Large plain\par\f1\fs26 let x = 1;\par}";
        let html = native_rich_html(rtf, false).expect("AppKit RTF decoding");
        let blocks = crate::paste::html_to_blocks(&html);
        use crate::model::BlockKind;
        assert!(matches!(blocks[0].kind, BlockKind::Heading { level: 1, .. }), "{blocks:?}: {html}");
        assert!(matches!(blocks[1].kind, BlockKind::Heading { level: 2, .. }), "{blocks:?}: {html}");
        assert!(matches!(blocks[2].kind, BlockKind::Text), "{blocks:?}: {html}");
        assert!(matches!(blocks[3].kind, BlockKind::Code { .. }), "{blocks:?}: {html}");
    }
}

#[cfg(target_os = "ios")]
pub fn clipboard_html() -> Option<String> {
    unsafe extern "C" { fn fei_ios_clipboard_html() -> *mut std::ffi::c_char; fn free(value: *mut std::ffi::c_void); }
    let value = unsafe { fei_ios_clipboard_html() };
    if value.is_null() { return None; }
    let html = unsafe { std::ffi::CStr::from_ptr(value) }.to_string_lossy().into_owned();
    unsafe { free(value.cast()); }
    Some(html)
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub fn clipboard_html() -> Option<String> {
    None
}
