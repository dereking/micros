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

#[cfg(target_os = "espidf")]
unsafe extern "C" {
    static micro_ui_sans_12: c_void;
    static micro_ui_sans_14: c_void;
    static micro_ui_sans_18: c_void;
    static micro_ui_sans_24: c_void;
    static micro_ui_sans_32: c_void;
}

#[cfg(target_os = "espidf")]
pub(crate) const AVAILABLE_NATIVE_FONTS: &[(NativeFontKey, NativeFontHandle)] = &[
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 12,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_12),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 14,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_14),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 18,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_18),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 24,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_24),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 32,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const micro_ui_sans_32),
    ),
];

#[cfg(test)]
static TEST_FONT_12: u8 = 0;
#[cfg(test)]
static TEST_FONT_14: u8 = 0;
#[cfg(test)]
static TEST_FONT_18: u8 = 0;
#[cfg(test)]
static TEST_FONT_24: u8 = 0;
#[cfg(test)]
static TEST_FONT_32: u8 = 0;

#[cfg(test)]
pub(crate) const AVAILABLE_NATIVE_FONTS: &[(NativeFontKey, NativeFontHandle)] = &[
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 12,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const TEST_FONT_12 as *const c_void),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 14,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const TEST_FONT_14 as *const c_void),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 18,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const TEST_FONT_18 as *const c_void),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 24,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const TEST_FONT_24 as *const c_void),
    ),
    (
        NativeFontKey {
            family: FontFamily::UiSans,
            size_px: 32,
            weight: FontWeight::Regular,
        },
        NativeFontHandle(&raw const TEST_FONT_32 as *const c_void),
    ),
];

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
                weight: FontWeight::Regular,
            },
            NativeFontHandle(&raw const TEST_FONT as *const c_void),
        )];
        let style = TextStyle::ui_sans(24, FontWeight::Regular, 32).unwrap();
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
    fn exposes_every_generated_regular_font() {
        for size in [12, 14, 18, 24, 32] {
            let style = TextStyle::ui_sans(size, FontWeight::Regular, size).unwrap();
            let selected =
                call_with_native_text_style(Some(&style), AVAILABLE_NATIVE_FONTS, |selected| {
                    selected
                })
                .unwrap();
            assert!(!selected.font_handle.0.is_null());
        }
    }
}
