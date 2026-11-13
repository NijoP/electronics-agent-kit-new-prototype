//! The single hardware/firmware pin map for the ESP32-C3 dashboard board.
//!
//! This file is the only place where GPIO numbers, module pads and schematic net names are
//! defined. The EAK design driver (`validation/esp32-dashboard/design`) labels the
//! schematic from this table, the ESP32-C3 firmware configures its peripherals from it, and
//! `design/tests/pinmap.rs` re-derives the connectivity from the saved native EAK project and
//! checks that it agrees with this table and with the module's datasheet pin names.
//!
//! Module pin numbers and GPIO functions: Espressif "ESP32-C3-WROOM-02 & WROOM-02U
//! Datasheet" v1.7, Table 3-1 "Pin Definitions"
//! (<https://documentation.espressif.com/esp32-c3-wroom-02_datasheet_en.pdf>).

/// How a signal is used electrically (drives the interface/voltage agreement checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// Open-drain I2C line with an external pull-up to 3.3 V.
    I2cOpenDrain,
    /// Push-pull digital output.
    DigitalOut,
    /// Digital input with an external pull-up (active low).
    DigitalInPullUp,
    /// ADC input (ADC1 channel).
    AnalogIn,
    /// USB 2.0 full-speed data line of the on-chip USB Serial/JTAG controller.
    UsbData,
    /// Strapping pin held high by an external pull-up; not used by the application.
    StrapPullUp,
    /// Chip enable (reset) input with an RC delay.
    ChipEnable,
}

/// One assignment: firmware GPIO <-> module pad <-> schematic net.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinAssignment {
    /// Human-readable signal name.
    pub signal: &'static str,
    /// ESP32-C3 GPIO number (`None` for the EN pin, which is not a GPIO).
    pub gpio: Option<u8>,
    /// Pin number on the ESP32-C3-WROOM-02 module (datasheet Table 3-1).
    pub module_pin: &'static str,
    /// Module pin name as printed in datasheet Table 3-1.
    pub module_pin_name: &'static str,
    /// Net name in the EAK schematic.
    pub net: &'static str,
    pub usage: Use,
}

pub const I2C_SDA: PinAssignment = PinAssignment {
    signal: "I2C SDA (sensor + display bus)",
    gpio: Some(4),
    module_pin: "3",
    module_pin_name: "IO4",
    net: "I2C_SDA",
    usage: Use::I2cOpenDrain,
};
pub const I2C_SCL: PinAssignment = PinAssignment {
    signal: "I2C SCL (sensor + display bus)",
    gpio: Some(5),
    module_pin: "4",
    module_pin_name: "IO5",
    net: "I2C_SCL",
    usage: Use::I2cOpenDrain,
};
pub const STATUS_LED: PinAssignment = PinAssignment {
    signal: "Status LED (active high)",
    gpio: Some(10),
    module_pin: "10",
    module_pin_name: "IO10",
    net: "LED_DRV",
    usage: Use::DigitalOut,
};
pub const USER_BUTTON: PinAssignment = PinAssignment {
    signal: "BOOT / user button (active low, strapping GPIO9)",
    gpio: Some(9),
    module_pin: "8",
    module_pin_name: "IO9",
    net: "BOOT_BTN",
    usage: Use::DigitalInPullUp,
};
pub const VBUS_SENSE: PinAssignment = PinAssignment {
    signal: "USB VBUS sense (ADC1_CH3 via 100k/47k divider)",
    gpio: Some(3),
    module_pin: "15",
    module_pin_name: "IO3",
    net: "VBUS_SENSE",
    usage: Use::AnalogIn,
};
pub const USB_DM: PinAssignment = PinAssignment {
    signal: "USB D- (USB Serial/JTAG)",
    gpio: Some(18),
    module_pin: "13",
    module_pin_name: "IO18",
    net: "USB_DM",
    usage: Use::UsbData,
};
pub const USB_DP: PinAssignment = PinAssignment {
    signal: "USB D+ (USB Serial/JTAG)",
    gpio: Some(19),
    module_pin: "14",
    module_pin_name: "IO19",
    net: "USB_DP",
    usage: Use::UsbData,
};
pub const STRAP_GPIO2: PinAssignment = PinAssignment {
    signal: "Strapping GPIO2 (pulled up per datasheet Table 4-3 note 2)",
    gpio: Some(2),
    module_pin: "16",
    module_pin_name: "IO2",
    net: "STRAP_IO2",
    usage: Use::StrapPullUp,
};
pub const STRAP_GPIO8: PinAssignment = PinAssignment {
    signal: "Strapping GPIO8 (must be 1 for joint download boot, Table 4-3)",
    gpio: Some(8),
    module_pin: "7",
    module_pin_name: "IO8",
    net: "STRAP_IO8",
    usage: Use::StrapPullUp,
};
pub const CHIP_EN: PinAssignment = PinAssignment {
    signal: "Chip enable / reset (10k + 1uF RC, datasheet section 8 note)",
    gpio: None,
    module_pin: "2",
    module_pin_name: "EN",
    net: "EN",
    usage: Use::ChipEnable,
};

/// Every signal assignment on the module.
pub const ALL: [PinAssignment; 10] = [
    I2C_SDA,
    I2C_SCL,
    STATUS_LED,
    USER_BUTTON,
    VBUS_SENSE,
    USB_DM,
    USB_DP,
    STRAP_GPIO2,
    STRAP_GPIO8,
    CHIP_EN,
];

/// Module pins deliberately left unconnected (marked no-connect in the schematic).
/// IO0/IO1 (32 kHz crystal pins), IO6/IO7 (JTAG), UART0 RXD/TXD (USB is used instead).
pub const UNUSED_MODULE_PINS: [&str; 6] = ["18", "17", "5", "6", "11", "12"];

/// Logic supply of the module and all I2C devices, millivolts.
pub const LOGIC_SUPPLY_MV: u32 = 3300;
/// Nominal USB VBUS, millivolts.
pub const VBUS_NOMINAL_MV: u32 = 5000;
/// I2C bus clock used by the firmware (standard mode).
pub const I2C_FREQ_HZ: u32 = 100_000;
/// I2C pull-up resistors on SDA/SCL, ohms (R8/R9 in the schematic).
pub const I2C_PULLUP_OHM: u32 = 4_700;
/// VBUS divider: top resistor (VBUS side), ohms (R10).
pub const VBUS_DIV_TOP_OHM: u32 = 100_000;
/// VBUS divider: bottom resistor (to GND), ohms (R11).
pub const VBUS_DIV_BOTTOM_OHM: u32 = 47_000;
/// Status LED series resistor, ohms (R7).
pub const LED_SERIES_OHM: u32 = 1_000;

/// Header pinouts (connector pin number -> net), shared with the schematic.
/// J2: SHT40 sensor breakout header.
pub const J2_SENSOR_HEADER: [(&str, &str); 4] = [
    ("1", "+3V3"),
    ("2", "GND"),
    ("3", I2C_SCL.net),
    ("4", I2C_SDA.net),
];
/// J3: 0.96" SSD1306 OLED module header (common GND-VCC-SCL-SDA order).
pub const J3_DISPLAY_HEADER: [(&str, &str); 4] = [
    ("1", "GND"),
    ("2", "+3V3"),
    ("3", I2C_SCL.net),
    ("4", I2C_SDA.net),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpio_and_module_pins_are_unique() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a.module_pin, b.module_pin, "{} / {}", a.signal, b.signal);
                assert_ne!(a.net, b.net);
                if a.gpio.is_some() {
                    assert_ne!(a.gpio, b.gpio);
                }
            }
            assert!(!UNUSED_MODULE_PINS.contains(&a.module_pin));
        }
    }

    #[test]
    fn module_pin_names_match_gpio_numbers() {
        for a in ALL {
            match a.gpio {
                Some(18) | Some(19) | Some(2) | Some(3) | Some(4) | Some(5) | Some(8)
                | Some(9) | Some(10) => {
                    assert_eq!(a.module_pin_name, std::format!("IO{}", a.gpio.unwrap()))
                }
                None => assert_eq!(a.module_pin_name, "EN"),
                Some(g) => panic!("unexpected gpio {g}"),
            }
        }
    }

    #[test]
    fn adc_input_stays_inside_the_11db_range() {
        // VBUS up to 5.25 V (USB 2.0 upper limit) must stay below 2500 mV at the ADC pin.
        let vmax = 5250 * VBUS_DIV_BOTTOM_OHM / (VBUS_DIV_TOP_OHM + VBUS_DIV_BOTTOM_OHM);
        assert!(vmax < 2500, "{vmax} mV");
    }
}
