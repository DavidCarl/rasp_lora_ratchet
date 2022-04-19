use std::{thread, time};

use rand_core::OsRng;

use twoRatchet::ED::EDRatchet;

use sx127x_lora::LoRa;
use sx127x_lora::RadioMode;

use rppal::gpio::OutputPin;
use rppal::spi::Spi;

use crate::{
    filehandling::{Config},
    generics::{get_message_lenght, recieve_window},
    edhoc::{RatchetKeys}
};

pub fn run(
    lora: &mut LoRa<Spi, OutputPin, OutputPin>,
    ratchetkeys: RatchetKeys,
    dhr_const: u16,
    config: Config,
) {
    let ed_ratchet = EDRatchet::new(
        ratchetkeys.ed_rk.try_into().unwrap(),
        ratchetkeys.ed_rck.try_into().unwrap(),
        ratchetkeys.ed_sck.try_into().unwrap(),
        ratchetkeys.devaddr.clone().try_into().unwrap(),
        OsRng,
    );

    thread::sleep(time::Duration::from_millis(5000));

    message(lora, ed_ratchet, dhr_const, 1, config, ratchetkeys.devaddr);
}

fn message(
    lora: &mut LoRa<Spi, OutputPin, OutputPin>,
    mut ed_ratchet: EDRatchet<OsRng>,
    dhr_const: u16,
    n: i32,
    config: Config,
    devaddr: Vec<u8>,
) {
    loop {
        println!("{:?}", n);
        let random_message: [u8; 8] = rand::random();
        let uplink = ed_ratchet.ratchet_encrypt_payload(&random_message, &devaddr);
        let (msg_uplink, len_uplink) = get_message_lenght(uplink);
        let transmit = lora.transmit_payload_busy(msg_uplink, len_uplink);
        match transmit {
            Ok(packet_size) => {
                /*unsafe {
                    MESSAGENUMBER += 1;
                }*/
                println!("Uplink message {:?}", n);
                println!("Sent packet with size: {:?}", packet_size)
            }
            Err(_) => println!("Error uplink"),
        }
        let incoming = recieve_window(lora, config);
        if !incoming.is_empty() {
            match ed_ratchet.receive(incoming.to_vec()) {
                Ok(x) => match x {
                    Some(y) => {
                        println!("receiving message from server {:?}", y)
                    }
                    None => println!("test"),
                },
                Err(x) => {
                    println!("{:?}", x)
                }
            };
        }
        if ed_ratchet.fcnt_up >= dhr_const {
            //println!("BEFORE: fcnt_up {:?} dh_id {:?}", ed_ratchet.fcnt_up, ed_ratchet.dh_id);
            let dhr_req = ed_ratchet.initiate_ratch(); //ed_initiate_ratch();
            let (msg_dhr_req, len_dhr_req) = get_message_lenght(dhr_req);
            println!("DHR_REQ payload print {:?}", &msg_dhr_req);
            let transmit = lora.transmit_payload_busy(msg_dhr_req, len_dhr_req);
            match transmit {
                Ok(packet_size) => {
                    /*unsafe {
                        MESSAGENUMBER += 1;
                    }*/
                    println!("Sent packet with size: {:?}", packet_size);
                    let incoming = recieve_window(lora, config);
                    if !incoming.is_empty() {
                        match ed_ratchet.receive(incoming.to_vec()) {
                            Ok(x) => match x {
                                Some(y) => {
                                    println!("receiving message from server {:?}", y)
                                }
                                None => println!("test"),
                            },
                            Err(x) => {
                                println!("{:?}", x)
                            }
                        };
                    }
                    //lora = res.lora;
                }
                Err(er) => println!("Error {:?}, {:?}", n, er),
            }
            //println!("AFTER: fcnt_up {:?} dh_id {:?}", ed_ratchet.fcnt_up, ed_ratchet.dh_id);
        }

        /*unsafe {
            println!("Message sent: {:?}", MESSAGENUMBER);
        }*/
        let lora_set_mode = lora.set_mode(RadioMode::Sleep);
        match lora_set_mode {
            Ok(_) => println!("Set to sleep mode success"),
            Err(_) => println!("Set to sleep mode failed"),
        }
        thread::sleep(time::Duration::from_millis(10000));
    }
}