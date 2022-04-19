extern crate linux_embedded_hal as hal;
extern crate sx127x_lora;

use std::result::Result;

// RANDOM

use rand::{rngs::StdRng, Rng, SeedableRng};
use rand_core::OsRng;

// EDHOC

use oscore::edhoc::{
    api::{Msg1Receiver, Msg3Receiver},
    error::{OwnError, OwnOrPeerError},
    PartyR,
};

use x25519_dalek_ng::{PublicKey, StaticSecret};

// Ratchet

use twoRatchet::AS::ASRatchet;

// LORA MODULE

use sx127x_lora::LoRa;

const LORA_CS_PIN: u8 = 8;
const LORA_RESET_PIN: u8 = 22;
const FREQUENCY: i64 = 915;

// HAL

use rppal::gpio::{Gpio, OutputPin};
use rppal::hal::Delay;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};

// JSON AND FILES

use std::collections::HashMap;
use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Data {
    data: HashMap<String, Device>,
    deveui: Vec<Vec<u8>>,
    appeui: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Device {
    key: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct StaticKeys {
    as_static_material: [u8; 32],
    ed_keys: Vec<EdKeys>, //ed_static_pk_material: [u8; 32]
}

#[derive(Serialize, Deserialize, Debug)]
struct EdKeys {
    kid: Vec<u8>,
    ed_static_material: [u8; 32],
}

static mut FCNTDOWN: u16 = 0;

fn main() {
    lora_recieve();
}

/// Get the content from a text file
///     
/// # Arguments
///
/// * `path` - A string for where the file are located
fn load_file(path: String) -> String {
    fs::read_to_string(path).expect("Unable to read file")
}

fn setup_sx127x(bandwidth: i64, spreadfactor: u8) -> LoRa<Spi, OutputPin, OutputPin> {
    let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 8_000_000, Mode::Mode0).unwrap();

    let gpio = Gpio::new().unwrap();

    let cs = gpio.get(LORA_CS_PIN).unwrap().into_output();
    let reset = gpio.get(LORA_RESET_PIN).unwrap().into_output();

    let mut lora = sx127x_lora::LoRa::new(spi, cs, reset, FREQUENCY, &mut Delay).unwrap();
    let _ = lora.set_signal_bandwidth(bandwidth);
    let _ = lora.set_spreading_factor(spreadfactor);
    lora
}

/// Convert a files content to a StaticKeys struct
///
/// # Arguments
///
/// *  `path` - A string for where the file are located
fn load_static_keys(path: String) -> StaticKeys {
    let static_data = load_file(path);
    let static_keys: StaticKeys = serde_json::from_str(&static_data).unwrap();
    static_keys
}

/// Starting the server application.
/// This function handles all the logic behind listening & recieving messages.
fn lora_recieve() {
    // load keys

    let enc_keys: StaticKeys = load_static_keys("./keys.json".to_string());

    let mut lora = setup_sx127x(125000, 7);
    // Creating two hashmaps, outside the loop to ensure they are no overwritten on each iteration
    // We do this to make the server function more advanced such it can handle multiple clients at a time
    // and access the correct data based on the clients devaddr.
    let mut msg3_receivers: HashMap<[u8; 4], PartyR<Msg3Receiver>> = HashMap::new();
    let mut lora_ratchets: HashMap<[u8; 4], ASRatchet<OsRng>> = HashMap::new();
    let mut ratchet_recieved: HashMap<[u8; 4], u16> = HashMap::new();
    loop {
        let poll = lora.poll_irq(None, &mut Delay); //30 Second timeout
        match poll {
            Ok(size) => {
                println!("Recieved packet with size: {:?}", size);
                let buffer = lora.read_packet().unwrap(); // Received buffer. NOTE: 255 bytes are always returned
                match buffer[0] {
                    0 => {
                        println!("Recieved m type 0");
                        let rtn = handle_m_type_zero(
                            buffer,
                            msg3_receivers,
                            lora,
                            enc_keys.as_static_material,
                        );
                        msg3_receivers = rtn.msg3_receivers;
                        lora = rtn.lora;
                    }
                    2 => {
                        println!("Recieved m type 2");
                        let rtn = handle_m_type_two(
                            buffer,
                            msg3_receivers,
                            lora_ratchets,
                            lora,
                            ratchet_recieved,
                        );
                        msg3_receivers = rtn.msg3_receivers;
                        lora_ratchets = rtn.lora_ratchets;
                        lora = rtn.lora;
                        ratchet_recieved = rtn.ratchet_recieved;
                    }
                    5 => {
                        println!("Recieved m type 5");
                        let incoming = &buffer;
                        let rtn = handle_ratchet_message(
                            incoming.to_vec(),
                            lora,
                            lora_ratchets,
                            ratchet_recieved,
                        );
                        lora = rtn.lora;
                        lora_ratchets = rtn.lora_ratchets;
                        ratchet_recieved = rtn.ratchet_recieved;
                    }
                    7 => {
                        println!("Recieved m type 7");
                        let incoming = &buffer;
                        let rtn = handle_ratchet_message(
                            incoming.to_vec(),
                            lora,
                            lora_ratchets,
                            ratchet_recieved,
                        );
                        lora = rtn.lora;
                        lora_ratchets = rtn.lora_ratchets;
                        ratchet_recieved = rtn.ratchet_recieved;
                    }
                    _ => {
                        println!("Recieved m type _");
                    }
                }
            }
            Err(_) => println!("Timeout"),
        }
    }
}

struct Msg2 {
    msg: Vec<u8>,
    msg3_receiver: PartyR<Msg3Receiver>,
    devaddr: [u8; 4],
}

fn handle_first_gen_second_message(
    msg: Vec<u8>,
    msg1_receiver: PartyR<Msg1Receiver>,
) -> Result<Msg2, OwnOrPeerError> {
    let (msg2_sender, _ad_r, _ad_i) = match msg1_receiver.handle_message_1(msg) {
        Err(OwnError(b)) => {
            return Err(OwnOrPeerError::OwnError(b));
        }
        Ok(val) => val,
    };

    let (msg2_bytes, msg3_receiver) = match msg2_sender.generate_message_2() {
        Err(OwnOrPeerError::PeerError(s)) => {
            panic!("Received error msg: {}", s)
        }
        Err(OwnOrPeerError::OwnError(b)) => {
            return Err(OwnOrPeerError::OwnError(b));
        }
        Ok(val) => val,
    };

    // generate dev id, make sure its unique!
    // TODO: Make sure dev_addr is unique!
    let devaddr: [u8; 4] = rand::random();
    let msg = prepare_message(msg2_bytes, 1, devaddr, false);

    Ok(Msg2 {
        msg,
        msg3_receiver,
        devaddr,
    })
}

struct Msg4 {
    msg4_bytes: Vec<u8>,
    as_sck: Vec<u8>,
    as_rck: Vec<u8>,
    as_master: Vec<u8>,
}

fn handle_third_gen_fourth_message(
    msg: Vec<u8>,
    msg3_receiver: PartyR<Msg3Receiver>,
) -> Result<Msg4, OwnOrPeerError> {
    let (msg3verifier, ed_kid) = match msg3_receiver.unpack_message_3_return_kid(msg) {
        //.handle_message_3(msg) {
        Err(OwnOrPeerError::PeerError(s)) => {
            panic!("Error during  {}", s)
        }
        Err(OwnOrPeerError::OwnError(b)) => {
            panic!("Send these bytes: {}", hexstring(&b))
        }
        Ok(val) => val,
    };

    let enc_keys: StaticKeys = load_static_keys("./keys.json".to_string());
    let mut opt_ed_static_pub: Option<PublicKey> = None;
    for each in enc_keys.ed_keys {
        if each.kid.to_vec() == ed_kid {
            opt_ed_static_pub = Some(PublicKey::from(each.ed_static_material));
        }
    }

    // find ed_static_pub kommer fra lookup
    match opt_ed_static_pub {
        Some(ed_static_pub) => {
            let (msg4_sender, as_sck, as_rck, as_master) =
                match msg3verifier.verify_message_3(ed_static_pub.as_bytes().as_ref()) {
                    Err(OwnOrPeerError::PeerError(s)) => {
                        panic!("Error during  {}", s)
                    }
                    Err(OwnOrPeerError::OwnError(b)) => {
                        panic!("Send these bytes: {}", hexstring(&b))
                    }
                    Ok(val) => val,
                };

            // send message 4

            let msg4_bytes = // fjern den der len imorgen
    match msg4_sender.generate_message_4() {
        Err(OwnOrPeerError::PeerError(s)) => {
            panic!("Received error msg: {}", s)
        }
        Err(OwnOrPeerError::OwnError(b)) => {
            //stream.write(&b)?;// in this case, return this errormessage
            return Err(OwnOrPeerError::OwnError(b))
        }

        Ok(val) => val,
    };

            Ok(Msg4 {
                msg4_bytes,
                as_sck,
                as_rck,
                as_master,
            })
        }
        None => panic!("Missing kid value"),
    }
}

struct RatchetMessage {
    lora: LoRa<Spi, OutputPin, OutputPin>,
    lora_ratchets: HashMap<[u8; 4], ASRatchet<OsRng>>,
    ratchet_recieved: HashMap<[u8; 4], u16>,
}

fn handle_ratchet_message(
    buffer: Vec<u8>,
    mut lora: LoRa<Spi, OutputPin, OutputPin>,
    mut lora_ratchets: HashMap<[u8; 4], ASRatchet<OsRng>>,
    mut ratchet_recieved: HashMap<[u8; 4], u16>,
) -> RatchetMessage {
    let incoming = &buffer;
    let devaddr: [u8; 4] = buffer[14..18].try_into().unwrap();
    let ratchet = lora_ratchets.remove(&devaddr);
    match ratchet {
        Some(mut lora_ratchet) => {
            let mut message_recieved = ratchet_recieved.remove(&devaddr).unwrap();
            message_recieved += 1;
            println!(
                "Recieved #{:?} messages on the following devaddr {:?}",
                message_recieved, devaddr
            );
            ratchet_recieved.insert(devaddr, message_recieved);
            let (newout, sendnew) = match lora_ratchet.receive(incoming.to_vec()) {
                Ok((x, b)) => (x, b),
                Err(x) => {
                    println!("error has happened {:?}", incoming);
                    println!("Error message {:?}", x);
                    lora_ratchets.insert(devaddr, lora_ratchet);
                    return RatchetMessage {
                        lora,
                        lora_ratchets,
                        ratchet_recieved,
                    };
                }
            };
            if !sendnew {
            } else {
                //println!("sending {:?}", newout);
                let (msg_buffer, len) = get_message_lenght(newout);
                //println!("msg 4 {:?}", msg_buffer);
                let transmit = lora.transmit_payload_busy(msg_buffer, len);
                match transmit {
                    Ok(packet_size) => {
                        println!("Sent packet with size: {:?}", packet_size)
                    }
                    Err(_) => println!("Error"),
                }
            }
            lora_ratchets.insert(devaddr, lora_ratchet);
            //n += 1;
            //println!("n {}", n);
        }
        None => println!("No ratchet on this devaddr"),
    }
    RatchetMessage {
        lora,
        lora_ratchets,
        ratchet_recieved,
    }
}

fn prepare_message(msg: Vec<u8>, mtype: u8, devaddr: [u8; 4], first_msg: bool) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&mtype.to_be_bytes());
    unsafe {
        buffer.extend_from_slice(&FCNTDOWN.to_be_bytes());
        FCNTDOWN += 1;
    }
    if !first_msg {
        buffer.extend_from_slice(&devaddr);
    }
    buffer.extend_from_slice(&msg);
    buffer
}

struct TypeZero {
    msg3_receivers: HashMap<[u8; 4], PartyR<Msg3Receiver>>,
    lora: LoRa<Spi, OutputPin, OutputPin>,
}

fn handle_m_type_zero(
    buffer: Vec<u8>,
    mut msg3_receivers: HashMap<[u8; 4], PartyR<Msg3Receiver>>,
    mut lora: LoRa<Spi, OutputPin, OutputPin>,
    as_static_material: [u8; 32],
) -> TypeZero {
    let msg = unpack_edhoc_first_message(buffer);

    let as_static_priv = StaticSecret::from(as_static_material);
    let as_static_pub = PublicKey::from(&as_static_priv);

    let as_kid = [0xA3].to_vec();
    let mut r: StdRng = StdRng::from_entropy();
    let as_ephemeral_keying = r.gen::<[u8; 32]>();

    let msg1_receiver = PartyR::new(as_ephemeral_keying, as_static_priv, as_static_pub, as_kid);
    let res = handle_first_gen_second_message(msg.to_vec(), msg1_receiver);
    match res {
        Ok(rtn) => {
            msg3_receivers.insert(rtn.devaddr, rtn.msg3_receiver);
            let (msg_buffer, len) = get_message_lenght(rtn.msg);
            let transmit = lora.transmit_payload_busy(msg_buffer, len);
            match transmit {
                Ok(packet_size) => {
                    println!("Sent packet with size: {:?}", packet_size)
                }
                Err(_) => println!("Error"),
            }
        }
        Err(error) => match error {
            OwnOrPeerError::OwnError(x) => {
                let (msg_buffer, len) = get_message_lenght(x);
                let transmit = lora.transmit_payload_busy(msg_buffer, len);
                match transmit {
                    Ok(packet_size) => {
                        println!("Sent packet with size: {:?} OwnError", packet_size)
                    }
                    Err(_) => println!("Error"),
                }
            }
            OwnOrPeerError::PeerError(x) => {
                println!("Error in m_type_zero {:?}", x)
            }
        },
    }
    TypeZero {
        msg3_receivers,
        lora,
    }
}

struct TypeTwo {
    msg3_receivers: HashMap<[u8; 4], PartyR<Msg3Receiver>>,
    lora_ratchets: HashMap<[u8; 4], ASRatchet<OsRng>>,
    lora: LoRa<Spi, OutputPin, OutputPin>,
    ratchet_recieved: HashMap<[u8; 4], u16>,
}

fn handle_m_type_two(
    buffer: Vec<u8>,
    mut msg3_receivers: HashMap<[u8; 4], PartyR<Msg3Receiver>>,
    mut lora_ratchets: HashMap<[u8; 4], ASRatchet<OsRng>>,
    mut lora: LoRa<Spi, OutputPin, OutputPin>,
    mut ratchet_recieved: HashMap<[u8; 4], u16>,
) -> TypeTwo {
    let (msg, devaddr) = unpack_edhoc_message(buffer);
    let msg3rec = msg3_receivers.remove(&devaddr).unwrap();
    //let ed_static_pub = PublicKey::from(ed_static_pk_material);

    let payload = handle_third_gen_fourth_message(msg.to_vec(), msg3rec);
    match payload {
        Ok(msg4) => {
            let msg = prepare_message(msg4.msg4_bytes, 3, devaddr, false);
            let (msg_buffer, len) = get_message_lenght(msg);
            let transmit = lora.transmit_payload_busy(msg_buffer, len);
            match transmit {
                Ok(packet_size) => {
                    println!("Sent packet with size: {:?}", packet_size)
                }
                Err(_) => println!("Error"),
            }
            //Create ratchet
            let as_ratchet = ASRatchet::new(
                msg4.as_master.try_into().unwrap(),
                msg4.as_rck.try_into().unwrap(),
                msg4.as_sck.try_into().unwrap(),
                devaddr,
                OsRng,
            );
            lora_ratchets.insert(devaddr, as_ratchet);
            ratchet_recieved.insert(devaddr, 2);
        }
        Err(error) => match error {
            OwnOrPeerError::OwnError(x) => {
                let (msg_buffer, len) = get_message_lenght(x);
                let transmit = lora.transmit_payload_busy(msg_buffer, len);
                match transmit {
                    Ok(packet_size) => {
                        println!("Sent packet with size: {:?} OwnError", packet_size)
                    }
                    Err(_) => println!("Error"),
                }
            }
            OwnOrPeerError::PeerError(x) => {
                println!("Error in m_type_two {:?}", x)
            }
        },
    }
    TypeTwo {
        msg3_receivers,
        lora_ratchets,
        lora,
        ratchet_recieved,
    }
}

fn unpack_edhoc_first_message(msg: Vec<u8>) -> Vec<u8> {
    let msg = &msg[1..]; // fjerne mtype
    let _framecounter = &msg[0..2]; // gemme framecounter
    let msg = &msg[2..]; // fjerne frame counter
    msg.to_vec()
}

fn unpack_edhoc_message(msg: Vec<u8>) -> (Vec<u8>, [u8; 4]) {
    let msg = &msg[1..]; // fjerne mtype
    let _framecounter = &msg[0..2]; // gemme framecounter
    let msg = &msg[2..]; // fjerne frame counter
    let devaddr = msg[0..4].try_into().unwrap();
    let msg = &msg[4..];
    (msg.to_vec(), devaddr)
}

fn get_message_lenght(message: Vec<u8>) -> ([u8; 255], usize) {
    let mut buffer = [0; 255];
    for (i, byte) in message.iter().enumerate() {
        buffer[i] = *byte;
    }
    (buffer, message.len())
}

fn _convert_id_to_string(id: Vec<u8>) -> String {
    serde_json::to_string(&id).unwrap()
}

fn hexstring(slice: &[u8]) -> String {
    String::from("0x")
        + &slice
            .iter()
            .map(|n| format!("{:02X}", n))
            .collect::<Vec<String>>()
            .join(", 0x")
}
