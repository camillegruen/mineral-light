#![no_std]
#![no_main]

use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    delay::Delay,
    gpio::{Input, InputConfig, Pull},
    main,
    rmt::Rmt,
    time::Rate,
};
use esp_hal_smartled::{buffer_size, color_order, RmtSmartLeds, WS2812B_TIMING};
use esp_println::println;
use smart_leds::{brightness, SmartLedsWrite, RGB8};

esp_bootloader_esp_idf::esp_app_desc!();

// Print the panic message instead of freezing silently.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

// ---------------------------------------------------------------------------
// LED strip
// ---------------------------------------------------------------------------
const NUM_LEDS: usize = 50;
const MAX_BRIGHTNESS: u8 = 80; // 0..=255, keep low when powered from USB
const OFF: RGB8 = RGB8::new(0, 0, 0);

// --- Morpho butterfly (blues) ---
const MORPHO_BLUE: RGB8 = RGB8::new(3, 153, 196); // #0399C4
const BUTTERFLY_BLUE: RGB8 = RGB8::new(32, 153, 187); // #2099BB
const LIBRA_BLUE: RGB8 = RGB8::new(79, 205, 255); // #4FCDFF
const STRUCTURAL_BLUE: RGB8 = RGB8::new(25, 2, 199); // #1902C7

// --- Monarch butterfly (oranges) ---
const DARK_ORANGE: RGB8 = RGB8::new(254, 141, 3); // #FE8D03
const AMBER_ORANGE: RGB8 = RGB8::new(235, 148, 46); // #EB942E
const CLASSIC_ORANGE: RGB8 = RGB8::new(239, 160, 104); // #EFA068
const BURNT_ORANGE: RGB8 = RGB8::new(219, 113, 9); // #DB7109

// --- Purples ---
const DEEP_VIOLET: RGB8 = RGB8::new(94, 23, 168); // #5E17A8
const ROYAL_PURPLE: RGB8 = RGB8::new(140, 42, 196); // #8C2AC4
const ORCHID: RGB8 = RGB8::new(186, 85, 211); // #BA55D3
const LAVENDER: RGB8 = RGB8::new(200, 150, 240); // #C896F0

// --- Whites (by color temperature) ---
const WARM_WHITE: RGB8 = RGB8::new(255, 180, 107); // #FFB46B
const SOFT_WHITE: RGB8 = RGB8::new(255, 214, 170); // #FFD6AA
const NEUTRAL_WHITE: RGB8 = RGB8::new(255, 241, 224); // #FFF1E0
const COOL_WHITE: RGB8 = RGB8::new(201, 226, 255); // #C9E2FF

const PALETTE: [RGB8; 16] = [
    MORPHO_BLUE,
    BUTTERFLY_BLUE,
    LIBRA_BLUE,
    STRUCTURAL_BLUE,
    DARK_ORANGE,
    AMBER_ORANGE,
    CLASSIC_ORANGE,
    BURNT_ORANGE,
    DEEP_VIOLET,
    ROYAL_PURPLE,
    ORCHID,
    LAVENDER,
    WARM_WHITE,
    SOFT_WHITE,
    NEUTRAL_WHITE,
    COOL_WHITE,
];

const FADE_STEPS: u8 = 64;
const STEP_MS: u32 = 25;
const HOLD_MS: u32 = 800;

/// Blend from `a` to `b`. t = 0 gives `a`, t = 255 gives `b`.
fn lerp(a: RGB8, b: RGB8, t: u8) -> RGB8 {
    let mix = |x: u8, y: u8| ((x as u16 * (255 - t as u16) + y as u16 * t as u16) / 255) as u8;
    RGB8::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

/// Fade once through the whole palette, then turn the strip off.
fn play_palette<S>(strip: &mut S, delay: &Delay)
where
    S: SmartLedsWrite<Color = RGB8>,
    S::Error: core::fmt::Debug,
{
    for i in 0..PALETTE.len() {
        let from = PALETTE[i];
        let to = PALETTE[(i + 1) % PALETTE.len()];
        for step in 0..=FADE_STEPS {
            let t = (step as u16 * 255 / FADE_STEPS as u16) as u8;
            let color = lerp(from, to, t);
            strip
                .write(brightness([color; NUM_LEDS].into_iter(), MAX_BRIGHTNESS))
                .unwrap();
            delay.delay_millis(STEP_MS);
        }
        delay.delay_millis(HOLD_MS);
    }
    strip.write([OFF; NUM_LEDS]).unwrap();
}

// ---------------------------------------------------------------------------
// IR distance sensor
// ---------------------------------------------------------------------------
const SAMPLES: u32 = 16; // readings averaged per measurement (sensor is noisy)
const MOTION_CM: f32 = 5.0; // change needed to count as motion
const MIN_CM: f32 = 20.0; // sensor's valid range
const MAX_CM: f32 = 150.0;
const TRIGGER_CM: f32 = 50.0; // motion at this distance or closer turns the lights on

/// Convert sensor output voltage to distance using the datasheet curve fit.
fn volts_to_cm(v: f32) -> f32 {
    60.374 * libm::powf(v, -1.16)
}

/// What the motion check found on this reading.
enum Motion {
    Detected(f32), // moved or appeared, with the new distance in cm
    LeftRange,     // object went out of the sensor's range
    None,          // nothing changed
}

/// Compare a new distance with the last reported one.
/// Updates `last_cm` whenever something is reported.
fn check_motion(cm: f32, last_cm: &mut Option<f32>) -> Motion {
    if !(MIN_CM..=MAX_CM).contains(&cm) {
        if last_cm.is_some() {
            *last_cm = None;
            return Motion::LeftRange;
        }
        return Motion::None;
    }

    let moved = match *last_cm {
        None => true,
        Some(prev) => libm::fabsf(cm - prev) > MOTION_CM,
    };

    if moved {
        *last_cm = Some(cm);
        Motion::Detected(cm)
    } else {
        Motion::None
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
#[main]
fn main() -> ! {
    // init() may only be called ONCE.
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    println!("Booting...");

    // --- LED strip on GPIO2 ---
    let rmt_freq = Rate::from_mhz(80);
    let rmt = Rmt::new(peripherals.RMT, rmt_freq).unwrap();
    let mut strip = RmtSmartLeds::<{ buffer_size::<RGB8>(NUM_LEDS) }, _, RGB8, color_order::Rgb>::new(
        WS2812B_TIMING,
        rmt.channel0,
        peripherals.GPIO2,
        rmt_freq,
    )
    .unwrap();
    strip.write([OFF; NUM_LEDS]).unwrap();

    // --- IR sensor on GPIO4 (ADC1 channel 4) ---
    let mut adc_config = AdcConfig::new();
    let mut sensor_pin = adc_config.enable_pin(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    println!("IR distance sensor ready");

    let mut last_cm: Option<f32> = None;

    loop {
        // --- 1. Average several readings to smooth out noise ---
        let mut sum: u32 = 0;
        for _ in 0..SAMPLES {
            let raw: u16 = nb::block!(adc.read_oneshot(&mut sensor_pin)).unwrap();
            sum += raw as u32;
        }
        let raw = sum / SAMPLES;

        // --- 2. Raw 12-bit value (0-4095) -> volts -> centimetres ---
        let volts = raw as f32 * 3.1 / 4095.0;
        let cm = volts_to_cm(volts);

        // Debug: every reading. Delete this line once things work.
        println!("raw={} volts={:.2} cm={:.1}", raw, volts, cm);

        // --- 3. Motion within TRIGGER_CM? Play the lights ---
        match check_motion(cm, &mut last_cm) {
            Motion::Detected(cm) => {
                println!("Motion detected: {:.1} cm", cm);
                if cm <= TRIGGER_CM {
                    println!("Within {:.0} cm - lights on", TRIGGER_CM);
                    play_palette(&mut strip, &delay);
                }
            }
            Motion::LeftRange => println!("Object left range"),
            Motion::None => {}
        }

        delay.delay_millis(50);
    }
}