//! ESP32-C3-WROOM-02 firmware for the EAK environment dashboard board.
//!
//! Every second: measure the SHT40 over I2C (GPIO4 SDA / GPIO5 SCL, 100 kHz), read VBUS
//! through the R10/R11 divider on GPIO3 (ADC1, 11 dB attenuation, curve calibration), sample
//! the BOOT/user button (GPIO9, active low), encode a CRC-protected telemetry frame
//! (`dashboard_core::frame`) and send it over the on-chip USB Serial/JTAG CDC port
//! (GPIO18/GPIO19). The status LED (GPIO10) shows a heartbeat, or stays on during a sensor
//! fault. An optional SSD1306 OLED on header J3 (same I2C bus, address 0x3C) shows the
//! latest values.
//!
//! Pin numbers are taken from `dashboard_core::pins` and checked at compile time below, so
//! this file cannot silently drift from the pin map that is verified against the EAK netlist.

#![no_std]
#![no_main]

use dashboard_core::app::{led_on, Sampler, SAMPLE_PERIOD_MS};
use dashboard_core::display::{self, FrameBuffer, Ssd1306};
use dashboard_core::frame::MAX_FRAME_LEN;
use dashboard_core::pins;
use dashboard_core::sht4x::Sht4x;
use esp_hal::analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::main;
use esp_hal::peripherals::ADC1;
use esp_hal::time::{Duration, Instant, Rate};
use esp_hal::usb::usb_serial_jtag::UsbSerialJtag;

esp_bootloader_esp_idf::esp_app_desc!();

// The peripheral pins used below must be the ones in the single pin map.
const _: () = assert!(matches!(pins::I2C_SDA.gpio, Some(4)));
const _: () = assert!(matches!(pins::I2C_SCL.gpio, Some(5)));
const _: () = assert!(matches!(pins::STATUS_LED.gpio, Some(10)));
const _: () = assert!(matches!(pins::USER_BUTTON.gpio, Some(9)));
const _: () = assert!(matches!(pins::VBUS_SENSE.gpio, Some(3)));
const _: () = assert!(matches!(pins::USB_DM.gpio, Some(18)));
const _: () = assert!(matches!(pins::USB_DP.gpio, Some(19)));

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Let the RTC watchdog / user reset recover the device; never continue with bad state.
    loop {
        core::hint::spin_loop();
    }
}

/// Upper bound on spinning for USB FIFO space per frame, so a disconnected host can never
/// stall sampling (the frame is dropped instead and the next sequence number shows the gap).
const USB_WRITE_BUDGET: u32 = 20_000;

#[main]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    let i2c = I2c::new(
        p.I2C0,
        I2cConfig::default().with_frequency(Rate::from_hz(pins::I2C_FREQ_HZ)),
    )
    .expect("valid I2C config")
    .with_sda(p.GPIO4)
    .with_scl(p.GPIO5);
    let mut sensor = Sht4x::new(i2c);

    let mut led = Output::new(p.GPIO10, Level::Low, OutputConfig::default());
    // External 10 k pull-up R4 is fitted; the internal pull-up is enabled as a backup.
    let button = Input::new(p.GPIO9, InputConfig::default().with_pull(Pull::Up));

    let mut adc_cfg = AdcConfig::new();
    let mut vbus_pin =
        adc_cfg.enable_pin_with_cal::<_, AdcCalCurve<ADC1>>(p.GPIO3, Attenuation::_11dB);
    let mut adc = Adc::new(p.ADC1, adc_cfg);

    let mut usb = UsbSerialJtag::new(p.USB_DEVICE);
    let mut delay = Delay::new();
    let mut sampler = Sampler::new();

    let _ = sensor.soft_reset(&mut delay);
    // Optional local display on J3: if it does not ACK, run without it (USB dashboard only).
    let mut display_ok = Ssd1306::new(sensor.bus()).init().is_ok();
    let mut fb = FrameBuffer::default();
    let mut next = Instant::now();
    loop {
        let uptime_ms = Instant::now().duration_since_epoch().as_millis() as u32;
        let adc_mv = nb::block!(adc.read_oneshot(&mut vbus_pin))
            .ok()
            .map(u32::from);
        let frame = sampler.sample(&mut sensor, &mut delay, adc_mv, button.is_low(), uptime_ms);

        let mut buf = [0u8; MAX_FRAME_LEN];
        if let Ok(len) = frame.encode(&mut buf) {
            let mut budget = USB_WRITE_BUDGET;
            for &b in &buf[..len] {
                while usb.write_byte_nb(b).is_err() {
                    budget = budget.saturating_sub(1);
                    if budget == 0 {
                        break;
                    }
                }
                if budget == 0 {
                    break;
                }
            }
            let _ = usb.flush_tx_nb();
        }
        if display_ok {
            display::render(&mut fb, &frame);
            display_ok = Ssd1306::new(sensor.bus()).flush(&fb).is_ok();
        }
        led.set_level(if led_on(&frame) {
            Level::High
        } else {
            Level::Low
        });

        next += Duration::from_millis(SAMPLE_PERIOD_MS as u64);
        while Instant::now() < next {
            core::hint::spin_loop();
        }
    }
}
