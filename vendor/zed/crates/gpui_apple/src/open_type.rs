use core_foundation::{
    array::{
        CFArray, CFArrayAppendValue, CFArrayCreateMutable, CFMutableArrayRef, kCFTypeArrayCallBacks,
    },
    base::{CFRelease, TCFType, kCFAllocatorDefault},
    dictionary::{
        CFDictionaryCreate, kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
    },
    number::CFNumber,
    string::CFString,
};
use core_foundation_sys::locale::CFLocaleCopyPreferredLanguages;
use core_text::{
    font::{CTFont, cascade_list_for_languages},
    font_descriptor::{
        CTFontDescriptor, CTFontDescriptorCreateWithAttributes,
        CTFontDescriptorCreateWithNameAndSize, kCTFontCascadeListAttribute,
        kCTFontFeatureSettingsAttribute,
    },
};
use font_kit::font::Font as FontKitFont;
use gpui::{FontFallbacks, FontFeatures};
use objc2_core_text::{kCTFontOpenTypeFeatureTag, kCTFontOpenTypeFeatureValue};

pub fn apply_features_and_fallbacks(
    font: &mut FontKitFont,
    features: &FontFeatures,
    fallbacks: Option<&FontFallbacks>,
) -> anyhow::Result<()> {
    unsafe {
        let mut keys = vec![kCTFontFeatureSettingsAttribute];
        let mut values = vec![generate_feature_array(features)];
        if let Some(fallbacks) = fallbacks
            && !fallbacks.fallback_list().is_empty()
        {
            keys.push(kCTFontCascadeListAttribute);
            values.push(generate_fallback_array(fallbacks, font));
        }
        let attrs = CFDictionaryCreate(
            kCFAllocatorDefault,
            keys.as_ptr() as _,
            values.as_ptr() as _,
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );

        for value in &values {
            CFRelease(*value as _);
        }

        let new_descriptor = CTFontDescriptorCreateWithAttributes(attrs);
        CFRelease(attrs as _);
        let new_descriptor = CTFontDescriptor::wrap_under_create_rule(new_descriptor);
        // font-kit uses the older core-text wrappers. Borrow the same CF objects
        // for the generated API, then transfer its create-rule ownership back.
        let native_font = font.native_font();
        let generated_font = &*native_font
            .as_concrete_TypeRef()
            .cast::<objc2_core_text::CTFont>();
        let descriptor = &*new_descriptor
            .as_concrete_TypeRef()
            .cast::<objc2_core_text::CTFontDescriptor>();
        let new_font = generated_font.copy_with_attributes(0.0, std::ptr::null(), Some(descriptor));
        let new_font = CTFont::wrap_under_create_rule(
            objc2_core_foundation::CFRetained::into_raw(new_font)
                .as_ptr()
                .cast(),
        );
        *font = font_kit::font::Font::from_native_font(&new_font);

        Ok(())
    }
}

fn generate_feature_array(features: &FontFeatures) -> CFMutableArrayRef {
    unsafe {
        let feature_array = CFArrayCreateMutable(kCFAllocatorDefault, 0, &kCFTypeArrayCallBacks);
        for (tag, value) in features.tag_value_list() {
            let keys = [
                kCTFontOpenTypeFeatureTag as *const _,
                kCTFontOpenTypeFeatureValue as *const _,
            ];
            let tag = CFString::new(tag);
            let value = CFNumber::from(*value as i32);
            let values = [tag.as_CFTypeRef(), value.as_CFTypeRef()];
            let dict = CFDictionaryCreate(
                kCFAllocatorDefault,
                &keys as *const _ as _,
                &values as *const _ as _,
                2,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            CFArrayAppendValue(feature_array, dict as _);
            CFRelease(dict as _);
        }
        feature_array
    }
}

fn generate_fallback_array(fallbacks: &FontFallbacks, font: &mut FontKitFont) -> CFMutableArrayRef {
    unsafe {
        let fallback_array = CFArrayCreateMutable(kCFAllocatorDefault, 0, &kCFTypeArrayCallBacks);
        for user_fallback in fallbacks.fallback_list() {
            // quiet.md: look the fallback up by name, as GPUI 0.2 did. A
            // family-name + traits descriptor resolves "PingFang SC" to the
            // family in PingFang.ttc instead of the system's UI copy, whose
            // outlines the app's CJK text was tuned with (see also
            // fallback-font-identity).
            let name = CFString::from(user_fallback.as_str());
            let fallback_desc = CTFontDescriptorCreateWithNameAndSize(name.as_concrete_TypeRef(), 0.0);
            CFArrayAppendValue(fallback_array, fallback_desc as _);
            CFRelease(fallback_desc as _);
        }

        append_system_fallbacks(fallback_array, &font.native_font());
        fallback_array
    }
}

fn append_system_fallbacks(fallback_array: CFMutableArrayRef, font: &CTFont) {
    unsafe {
        let preferred_languages: CFArray<CFString> =
            CFArray::wrap_under_create_rule(CFLocaleCopyPreferredLanguages());

        let default_fallbacks = cascade_list_for_languages(font, &preferred_languages);

        for desc in default_fallbacks
            .iter()
            .filter(|desc| desc.font_path().is_some())
        {
            CFArrayAppendValue(fallback_array, desc.as_concrete_TypeRef() as _);
        }
    }
}
