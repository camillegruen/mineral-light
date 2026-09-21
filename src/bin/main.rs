#![no_std]
#![no_main]
 
use esp_hal::{delay::Delay, main, rmt::Rmt, time::Rate, gpio::{Input, InputConfig, Pull}};
use esp_hal_smartled::{buffer_size, color_order, RmtSmartLeds, WS2812B_TIMING};
use smart_leds::{
    brightness, gamma,
    hsv::{hsv2rgb, Hsv},
    SmartLedsWrite, RGB8,
};
 
esp_bootloader_esp_idf::esp_app_desc!();
 
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
 
const NUM_LEDS: usize = 50;
const MAX_BRIGHTNESS: u8 = 80; // 0..=255, keep low when powered from USB

const ON_TIME_MS: u32 = 5 * 60 * 1000; // 5 minutes
const OFF: RGB8 = RGB8::new(0, 0, 0);


// --- Morpho butterfly (blues) ---
const MORPHO_BLUE: RGB8 = RGB8::new(3, 153, 196);     // #0399C4
const BUTTERFLY_BLUE: RGB8 = RGB8::new(32, 153, 187); // #2099BB
const LIBRA_BLUE: RGB8 = RGB8::new(79, 205, 255);     // #4FCDFF
const STRUCTURAL_BLUE: RGB8 = RGB8::new(25, 2, 199);  // #1902C7


// --- Monarch butterfly (oranges) ---
const DARK_ORANGE: RGB8 = RGB8::new(254, 141, 3);     // #FE8D03
const AMBER_ORANGE: RGB8 = RGB8::new(235, 148, 46);   // #EB942E
const CLASSIC_ORANGE: RGB8 = RGB8::new(239, 160, 104); // #EFA068
const BURNT_ORANGE: RGB8 = RGB8::new(219, 113, 9);    // #DB7109

// --- Purples ---
const DEEP_VIOLET: RGB8 = RGB8::new(94, 23, 168);    // #5E17A8
const ROYAL_PURPLE: RGB8 = RGB8::new(140, 42, 196);  // #8C2AC4
const ORCHID: RGB8 = RGB8::new(186, 85, 211);        // #BA55D3
const LAVENDER: RGB8 = RGB8::new(200, 150, 240);     // #C896F0

// --- Whites (by color temperature) ---
const WARM_WHITE: RGB8 = RGB8::new(255, 180, 107);   // #FFB46B
const SOFT_WHITE: RGB8 = RGB8::new(255, 214, 170);   // #FFD6AA
const NEUTRAL_WHITE: RGB8 = RGB8::new(255, 241, 224); // #FFF1E0
const COOL_WHITE: RGB8 = RGB8::new(201, 226, 255);   // #C9E2FF

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


fn lerp(a: RGB8, b: RGB8, t: u8) -> RGB8 {
    let mix = |x: u8, y: u8| {
        ((x as u16 * (255 - t as u16) + y as u16 * t as u16) / 255) as u8
    };
    RGB8::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}


#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();
 
    // The same RMT clock must be given to the RMT driver and to the LED driver.
    let rmt_freq = Rate::from_mhz(80);
    let rmt = Rmt::new(peripherals.RMT, rmt_freq).unwrap();

    let button = Input::new(peripherals.GPIO16, InputConfig::default().with_pull(Pull::Up));

 
    // Type parameters: buffer size, driver mode (inferred), color type, color order.
    // If red and green come out swapped, change color_order::Rgb to color_order::Grb.
    let mut strip = RmtSmartLeds::<{ buffer_size::<RGB8>(NUM_LEDS) }, _, RGB8, color_order::Rgb>::new(
        WS2812B_TIMING,
        rmt.channel0,
        peripherals.GPIO2,
        rmt_freq,
    )
    .unwrap();

    // strip.write([OFF; NUM_LEDS]).unwrap();


// loop {
//     if button.is_low() {
//         // pressed -> lights on
//         strip
//             .write(brightness([PURPLE; NUM_LEDS].into_iter(), MAX_BRIGHTNESS))
//             .unwrap();
//     } else {
//         // released -> lights off
//         strip.write([OFF; NUM_LEDS]).unwrap();
//     }
//     delay.delay_millis(50);
// }
    // let mut colors = [RGB8::default(); NUM_LEDS];

let mut was_pressed = false;

loop {
    let pressed = button.is_low();

    if pressed && !was_pressed {
        delay.delay_millis(30);

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

        while button.is_low() {}
        delay.delay_millis(30);
        was_pressed = false;
        continue;
    }

    was_pressed = pressed;
    delay.delay_millis(10);
}
}