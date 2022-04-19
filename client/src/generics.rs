use sx127x_lora::LoRa;
use std::{thread, time};

use rppal::gpio::OutputPin;
use rppal::hal::Delay;
use rppal::spi::Spi;

use crate::filehandling::Config;

static mut FCNTUP: u16 = 0;

pub struct MessageStruct {
    pub _m: u8,
    pub _fcntdown: [u8; 2],
    pub devaddr: [u8; 4],
    pub msg: Vec<u8>,
}

pub fn remove_message(ogmsg: Vec<u8>) -> MessageStruct {
    MessageStruct {
        _m: ogmsg[0],
        _fcntdown: ogmsg[1..3].try_into().unwrap(),
        devaddr: ogmsg[3..7].try_into().unwrap(),
        msg: ogmsg[7..].try_into().unwrap(),
    }
}

pub fn prepare_message(msg: Vec<u8>, mtype: u8, first_msg: bool, devaddr: [u8; 4]) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&mtype.to_be_bytes());
    unsafe {
        buffer.extend_from_slice(&FCNTUP.to_be_bytes());
        FCNTUP += 1;
    }
    if !first_msg {
        buffer.extend_from_slice(&devaddr);
    }
    buffer.extend_from_slice(&msg);
    buffer
}

pub fn get_message_lenght(message: Vec<u8>) -> ([u8; 255], usize) {
    let mut buffer = [0; 255];
    for (i, byte) in message.iter().enumerate() {
        buffer[i] = *byte;
    }
    (buffer, message.len())
}

pub fn recieve_window(lora: &mut LoRa<Spi, OutputPin, OutputPin>, config: Config) -> Vec<u8> {
    //Result<ReceiveWindow, Box<dyn stdError>> {
    thread::sleep(time::Duration::from_millis(config.rx1_delay));
    let poll = lora.poll_irq(Some(config.rx1_duration), &mut Delay);
    match poll {
        Ok(size) => {
            let buffer = lora.read_packet().unwrap();
            println!("Recieved packet with size: {:?}", size);
            buffer
        }
        Err(_) => {
            thread::sleep(time::Duration::from_millis(config.rx1_delay));
            let poll = lora.poll_irq(Some(config.rx1_duration), &mut Delay);
            match poll {
                Ok(size) => {
                    let buffer = lora.read_packet().unwrap();
                    println!("Recieved packet with size: {:?}", size);
                    buffer
                }
                Err(_) => Vec::new(),
            }
        }
    }
}