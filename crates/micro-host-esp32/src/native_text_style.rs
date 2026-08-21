use std::ffi::c_void;

use micro_ir::{FontFamily, FontWeight, TextStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeFontHandle(pub(crate) *const c_void);

impl Default for NativeFontHandle {
    fn default() -> Self {
        Self(std::ptr::null())
    }
}

// SAFETY: Catalog handles point only to immutable, platform-owned static font assets.
unsafe impl Sync for NativeFontHandle {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeFontKey {
    pub(crate) family: FontFamily,
    pub(crate) size_px: u8,
    pub(crate) weight: FontWeight,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NativeTextStyle {
    pub(crate) font_handle: NativeFontHandle,
    pub(crate) line_height_px: u32,
}

pub(crate) const AVAILABLE_NATIVE_FONTS: &[(NativeFontKey, NativeFontHandle)] = &[];

fn select_native_text_style(
    style: Option<&TextStyle>,
    available_fonts: &[(NativeFontKey, NativeFontHandle)],
) -> Result<NativeTextStyle, String> {
    let Some(style) = style else {
        return Ok(NativeTextStyle::default());
    };
    let key = NativeFontKey {
        family: style.family,
        size_px: style.size_px,
        weight: style.weight,
    };
    available_fonts
        .iter()
        .find_map(|(candidate, font_handle)| {
            (*candidate == key).then_some(NativeTextStyle {
                font_handle: *font_handle,
                line_height_px: u32::from(style.line_height_px),
            })
        })
        .ok_or_else(|| {
            format!(
                "ESP native font unavailable: {:?} {}px {:?}",
                style.family, style.size_px, style.weight
            )
        })
}

pub(crate) fn call_with_native_text_style<T>(
    style: Option<&TextStyle>,
    available_fonts: &[(NativeFontKey, NativeFontHandle)],
    apply: impl FnOnce(NativeTextStyle) -> T,
) -> Result<T, String> {
    let selected = select_native_text_style(style, available_fonts)?;
    Ok(apply(selected))
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use micro_ir::{FontFamily, FontWeight, TextStyle};

    use super::{
        AVAILABLE_NATIVE_FONTS, NativeFontHandle, NativeFontKey, NativeTextStyle,
        call_with_native_text_style,
    };

    #[test]
    fn forwards_selected_font_handle_and_line_height_to_esp_call() {
        static TEST_FONT: u8 = 0;
        const TEST_FONTS: [(NativeFontKey, NativeFontHandle); 1] = [(
            NativeFontKey {
                family: FontFamily::UiSans,
                size_px: 24,
                weight: FontWeight::Bold,
            },
            NativeFontHandle(&raw const TEST_FONT as *const c_void),
        )];
        let style = TextStyle::ui_sans(24, FontWeight::Bold, 32).unwrap();
        let mut applied = None;
        call_with_native_text_style(Some(&style), &TEST_FONTS, |selected| {
            applied = Some(selected);
            0
        })
        .unwrap();
        assert_eq!(
            applied,
            Some(NativeTextStyle {
                font_handle: NativeFontHandle(&raw const TEST_FONT as *const c_void),
                line_height_px: 32,
            })
        );
    }

    #[test]
    fn rejects_styles_while_esp_font_catalog_is_empty() {
        let style = TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap();
        assert_eq!(
            call_with_native_text_style(Some(&style), AVAILABLE_NATIVE_FONTS, |_| 0),
            Err("ESP native font unavailable: UiSans 18px Regular".into())
        );
    }
}
