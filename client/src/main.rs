extern crate linux_embedded_hal as hal;
extern crate sx127x_lora;

use sx127x_lora::LoRa;

use rppal::gpio::{Gpio, OutputPin};
use rppal::hal::Delay;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};

mod filehandling;
mod edhoc;
mod generics;
mod ratchet;

const LORA_CS_PIN: u8 = 8;
const LORA_RESET_PIN: u8 = 22;
const FREQUENCY: i64 = 915;

fn main() {
    let config: filehandling::Config = filehandling::load_config("./config.json".to_string());
    let enc_keys: filehandling::StaticKeys = filehandling::load_static_keys("./keys.json".to_string());
    let lora = &mut setup_sx127x(125000, 7);
    let rtn = edhoc::handshake(lora, enc_keys, config.deveui, config.appeui, config).unwrap();
    ratchet::run(lora, rtn, config.dhr_const, config);
}

fn setup_sx127x(bandwidth: i64, spreadfactor: u8) -> LoRa<Spi, OutputPin, OutputPin> {
    let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 8_000_000, Mode::Mode0).unwrap();

    let gpio = Gpio::new().unwrap();

    let cs = gpio.get(LORA_CS_PIN).unwrap().into_output();
    let reset = gpio.get(LORA_RESET_PIN).unwrap().into_output();

    let mut lora = sx127x_lora::LoRa::new(spi, cs, reset, FREQUENCY, &mut Delay).unwrap();
    //let _ = lora.set_signal_bandwidth()
    let _ = lora.set_signal_bandwidth(bandwidth);
    let _ = lora.set_spreading_factor(spreadfactor);
    lora
}