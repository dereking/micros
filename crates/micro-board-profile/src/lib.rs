use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardProfile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub chip_family: String,
    #[serde(default)]
    pub board_revision: Option<String>,
    pub hardware: HardwareClass,
    pub display: RgbDisplay,
    pub touch: Touch,
    pub expander: ExpanderSignals,
    pub backlight: Backlight,
    pub resources: ResourcePolicy,
    pub driver_catalog: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareClass {
    pub flash_mb: u16,
    pub psram_mb: u16,
    pub psram_mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbDisplay {
    pub driver: String,
    pub width: u16,
    pub height: u16,
    pub color_format: String,
    pub pixel_clock_hz: u32,
    pub pclk_active_negative: bool,
    pub hsync: i16,
    pub vsync: i16,
    pub de: i16,
    pub pclk: i16,
    pub data: Vec<i16>,
    pub timing: RgbTiming,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbTiming {
    pub h_pulse: u16,
    pub h_back: u16,
    pub h_front: u16,
    pub v_pulse: u16,
    pub v_back: u16,
    pub v_front: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Touch {
    pub driver: String,
    pub bus: u8,
    pub sda: i16,
    pub scl: i16,
    pub irq: i16,
    pub reset_expander: u8,
    #[serde(rename = "swapXY")]
    pub swap_xy: bool,
    pub mirror_x: bool,
    pub mirror_y: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpanderSignals {
    pub driver: String,
    pub bus: u8,
    pub sda: i16,
    pub scl: i16,
    pub touch_reset: u8,
    pub backlight_enable: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backlight {
    pub kind: String,
    pub enable_expander: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePolicy {
    pub framebuffers: u8,
    pub bounce_buffer_lines: u16,
    pub prefer_psram: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverCatalog {
    pub id: String,
    display_drivers: BTreeSet<String>,
    touch_drivers: BTreeSet<String>,
    expander_drivers: BTreeSet<String>,
}

impl DriverCatalog {
    pub fn esp32s3_v1() -> Self {
        Self {
            id: "esp32s3-v1".to_owned(),
            display_drivers: BTreeSet::from(["esp-lcd-rgb".to_owned()]),
            touch_drivers: BTreeSet::from(["gt911".to_owned()]),
            expander_drivers: BTreeSet::from(["ch422g".to_owned()]),
        }
    }
}

#[derive(Debug)]
pub enum ProfileError {
    Parse(serde_json::Error),
    Validation(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "board profile JSON parse error: {error}"),
            Self::Validation(message) => {
                write!(formatter, "board profile validation error: {message}")
            }
        }
    }
}

impl Error for ProfileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Validation(_) => None,
        }
    }
}

impl BoardProfile {
    pub fn from_json(json: &str) -> Result<Self, ProfileError> {
        serde_json::from_str(json).map_err(ProfileError::Parse)
    }

    pub fn validate(&self, catalog: &DriverCatalog) -> Result<(), ProfileError> {
        self.validate_identity_and_catalog(catalog)?;
        self.validate_hardware()?;
        self.validate_drivers(catalog)?;
        self.validate_display()?;
        self.validate_resources()?;
        self.validate_gpio_assignments()?;
        self.validate_shared_signals()?;
        Ok(())
    }

    fn validate_identity_and_catalog(&self, catalog: &DriverCatalog) -> Result<(), ProfileError> {
        ensure(
            self.schema_version == 1,
            format!(
                "unsupported schema version {}; expected 1",
                self.schema_version
            ),
        )?;
        ensure(
            self.driver_catalog == catalog.id,
            format!(
                "unknown driver catalog {:?}; expected {:?}",
                self.driver_catalog, catalog.id
            ),
        )?;
        ensure(
            self.chip_family == "esp32s3",
            format!("unsupported chip family {:?}", self.chip_family),
        )
    }

    fn validate_hardware(&self) -> Result<(), ProfileError> {
        ensure(
            self.hardware.flash_mb == 8,
            format!(
                "flash size mismatch: {} MB; expected 8 MB",
                self.hardware.flash_mb
            ),
        )?;
        ensure(
            self.hardware.psram_mb == 8,
            format!(
                "PSRAM size mismatch: {} MB; expected 8 MB",
                self.hardware.psram_mb
            ),
        )?;
        ensure(
            self.hardware.psram_mode == "octal",
            format!(
                "PSRAM mode {:?} is unsupported; expected octal",
                self.hardware.psram_mode
            ),
        )
    }

    fn validate_drivers(&self, catalog: &DriverCatalog) -> Result<(), ProfileError> {
        ensure(
            catalog.display_drivers.contains(&self.display.driver),
            format!("unavailable display driver {:?}", self.display.driver),
        )?;
        ensure(
            catalog.touch_drivers.contains(&self.touch.driver),
            format!("unavailable touch driver {:?}", self.touch.driver),
        )?;
        ensure(
            catalog.expander_drivers.contains(&self.expander.driver),
            format!("unavailable expander driver {:?}", self.expander.driver),
        )
    }

    fn validate_display(&self) -> Result<(), ProfileError> {
        ensure(
            (1..=800).contains(&self.display.width),
            format!("display width {} is outside 1..=800", self.display.width),
        )?;
        ensure(
            (1..=480).contains(&self.display.height),
            format!("display height {} is outside 1..=480", self.display.height),
        )?;
        ensure(
            self.display.color_format == "rgb565",
            format!(
                "unsupported color format {:?}; expected rgb565",
                self.display.color_format
            ),
        )?;
        ensure(
            (1..=21_000_000).contains(&self.display.pixel_clock_hz),
            format!(
                "pixel clock {} Hz is outside the safe range 1..=21000000 Hz",
                self.display.pixel_clock_hz
            ),
        )?;
        ensure(
            self.display.data.len() == 16,
            format!(
                "rgb565 requires 16 RGB data pins; found {}",
                self.display.data.len()
            ),
        )?;

        let timing = &self.display.timing;
        let fields = [
            ("hPulse", timing.h_pulse),
            ("hBack", timing.h_back),
            ("hFront", timing.h_front),
            ("vPulse", timing.v_pulse),
            ("vBack", timing.v_back),
            ("vFront", timing.v_front),
        ];
        for (name, value) in fields {
            ensure(value > 0, format!("display timing {name} must be non-zero"))?;
        }
        Ok(())
    }

    fn validate_resources(&self) -> Result<(), ProfileError> {
        ensure(
            (1..=2).contains(&self.resources.framebuffers),
            format!(
                "framebuffers {} is outside 1..=2",
                self.resources.framebuffers
            ),
        )?;
        ensure(
            (1..=80).contains(&self.resources.bounce_buffer_lines),
            format!(
                "bounce buffer lines {} is outside 1..=80",
                self.resources.bounce_buffer_lines
            ),
        )?;
        ensure(
            self.display.width != 800 || self.display.height != 480 || self.resources.prefer_psram,
            "PSRAM must be preferred for an 800x480 display".to_owned(),
        )
    }

    fn validate_shared_signals(&self) -> Result<(), ProfileError> {
        ensure(
            self.touch.bus == self.expander.bus,
            format!(
                "I2C bus mismatch: touch uses {} but expander uses {}",
                self.touch.bus, self.expander.bus
            ),
        )?;
        ensure(
            self.touch.sda == self.expander.sda && self.touch.scl == self.expander.scl,
            format!(
                "I2C pin mismatch: touch uses SDA{}/SCL{} but expander uses SDA{}/SCL{}",
                self.touch.sda, self.touch.scl, self.expander.sda, self.expander.scl
            ),
        )?;
        ensure(
            self.touch.reset_expander == self.expander.touch_reset,
            format!(
                "touch reset signal mismatch: touch uses {} but expander declares {}",
                self.touch.reset_expander, self.expander.touch_reset
            ),
        )?;
        ensure(
            self.backlight.enable_expander == self.expander.backlight_enable,
            format!(
                "backlight signal mismatch: backlight uses {} but expander declares {}",
                self.backlight.enable_expander, self.expander.backlight_enable
            ),
        )?;
        ensure(
            self.expander.touch_reset != self.expander.backlight_enable,
            "expander touch reset and backlight signals must be distinct".to_owned(),
        )?;
        ensure(
            self.backlight.kind == "binary",
            format!("unsupported backlight kind {:?}", self.backlight.kind),
        )
    }

    fn validate_gpio_assignments(&self) -> Result<(), ProfileError> {
        let mut assignments = BTreeMap::new();
        let fixed = [
            ("display.hsync", self.display.hsync),
            ("display.vsync", self.display.vsync),
            ("display.de", self.display.de),
            ("display.pclk", self.display.pclk),
        ];
        for (name, pin) in fixed {
            register_gpio(&mut assignments, name.to_owned(), pin)?;
        }
        for (index, pin) in self.display.data.iter().copied().enumerate() {
            register_gpio(&mut assignments, format!("display.data[{index}]"), pin)?;
        }
        register_gpio(&mut assignments, "touch.sda".to_owned(), self.touch.sda)?;
        register_gpio(&mut assignments, "touch.scl".to_owned(), self.touch.scl)?;
        register_gpio(&mut assignments, "touch.irq".to_owned(), self.touch.irq)?;
        Ok(())
    }
}

fn ensure(condition: bool, message: String) -> Result<(), ProfileError> {
    if condition {
        Ok(())
    } else {
        Err(ProfileError::Validation(message))
    }
}

fn register_gpio(
    assignments: &mut BTreeMap<i16, String>,
    name: String,
    pin: i16,
) -> Result<(), ProfileError> {
    ensure(
        (0..=48).contains(&pin) && !(26..=37).contains(&pin),
        format!("GPIO {pin} assigned to {name} is unavailable on ESP32-S3 N8R8"),
    )?;
    if let Some(existing) = assignments.insert(pin, name.clone()) {
        return Err(ProfileError::Validation(format!(
            "GPIO {pin} is assigned to both {existing} and {name}"
        )));
    }
    Ok(())
}
