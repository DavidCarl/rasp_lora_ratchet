extern crate linux_embedded_hal as hal;
extern crate sx127x_lora;

use std::fmt;
use std::{error::Error as stdError, result::Result};

use std::{thread, time};

// RANDOM

use rand::{rngs::StdRng, Rng, SeedableRng};
use rand_core::OsRng;
use x25519_dalek_ng::{PublicKey, StaticSecret};

// EDHOC

use oscore::edhoc::{
    api::{Msg1Sender, Msg2Receiver, Msg4ReceiveVerify},
    error::{OwnError, OwnOrPeerError},
    PartyI,
};

// Ratchet

use twoRatchet::ED::EDRatchet;

// LORA MODULE

use sx127x_lora::LoRa;
use sx127x_lora::RadioMode;

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

const SUITE_I: u8 = 3;
const METHOD_TYPE_I: u8 = 0;

#[derive(Serialize, Deserialize, Debug)]
struct StaticKeys {
    ed_static_material: [u8; 32],
    as_keys: Vec<AsKeys>,
}

#[derive(Serialize, Deserialize, Debug)]
struct AsKeys {
    kid: [u8; 32],
    as_static_material: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct Config {
    deveui: [u8; 8],
    appeui: [u8; 8],
    dhr_const: u16,
    rx1_delay: u64,
    rx1_duration: i32,
    rx2_delay: u64,
    rx2_duration: i32,
}

static mut FCNTUP: u16 = 0;
static mut MESSAGENUMBER: u16 = 2;
static mut DEVADDR: [u8; 4] = [0, 0, 0, 0];

#[derive(Debug)]
struct MyError(String);

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There is an error: {}", self.0)
    }
}

impl stdError for MyError {}

struct MessageStruct {
    _m: u8,
    _fcntdown: [u8; 2],
    devaddr: [u8; 4],
    msg: Vec<u8>,
}

struct RatchetKeys {
    ed_sck: Vec<u8>,
    ed_rck: Vec<u8>,
    ed_rk: Vec<u8>,
    devaddr: Vec<u8>,
}

fn main() {
    let config: Config = load_config("./config.json".to_string());
    let enc_keys: StaticKeys = load_static_keys("./keys.json".to_string());
    let lora = &mut setup_sx127x(125000, 7);
    let rtn = edhoc_handshake(lora, enc_keys, config.deveui, config.appeui, config).unwrap();
    lora_ratchet(lora, rtn, config.dhr_const, config);
}

fn load_file(path: String) -> String {
    fs::read_to_string(path).expect("Unable to read file")
}

fn load_static_keys(path: String) -> StaticKeys {
    let static_data = load_file(path);
    let static_keys: StaticKeys = serde_json::from_str(&static_data).unwrap();
    static_keys
}

fn load_config(path: String) -> Config {
    let config_data = load_file(path);
    let config: Config = serde_json::from_str(&config_data).unwrap();
    config
}

fn lora_ratchet(
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

    ratchet_message(lora, ed_ratchet, dhr_const, 1, config, ratchetkeys.devaddr);
}

fn ratchet_message(
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
                unsafe {
                    MESSAGENUMBER += 1;
                }
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
                    unsafe {
                        MESSAGENUMBER += 1;
                    }
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

        unsafe {
            println!("Message sent: {:?}", MESSAGENUMBER);
        }
        let lora_set_mode = lora.set_mode(RadioMode::Sleep);
        match lora_set_mode {
            Ok(_) => println!("Set to sleep mode success"),
            Err(_) => println!("Set to sleep mode failed"),
        }
        thread::sleep(time::Duration::from_millis(10000));
    }
}

fn edhoc_handshake(
    lora: &mut LoRa<Spi, OutputPin, OutputPin>,
    enc_keys: StaticKeys,
    deveui: [u8; 8],
    appeui: [u8; 8],
    config: Config,
) -> Result<RatchetKeys, Box<dyn stdError>> {
    let ed_kid = [0xA2].to_vec();
    let ed_static_priv = StaticSecret::from(enc_keys.ed_static_material);
    let ed_static_pub = PublicKey::from(&ed_static_priv);
    //let as_static_pub = PublicKey::from(enc_keys.as_static_material);
    // create ehpemeral key material
    let mut r: StdRng = StdRng::from_entropy();
    let ed_ephemeral_keying = r.gen::<[u8; 32]>();

    let msg1_sender = PartyI::new(
        deveui.to_vec(),
        appeui.to_vec(),
        ed_ephemeral_keying,
        ed_static_priv,
        ed_static_pub,
        ed_kid,
    );
    let (payload1, msg2_reciever) = edhoc_first_message(msg1_sender);
    let (msg, len) = get_message_lenght(payload1);

    let transmit = lora.transmit_payload_busy(msg, len);
    match transmit {
        Ok(packet_size) => println!("Sent packet with size: {:?}", packet_size),
        Err(_) => println!("Error"),
    }

    let incoming = recieve_window(lora, config);
    match incoming[0] {
        1 => match edhoc_third_message(incoming.to_vec(), msg2_reciever) {
            Ok((msg3, msg4_reciever)) => {
                let (msg, len) = get_message_lenght(msg3);
                let transmit = lora.transmit_payload_busy(msg, len);
                match transmit {
                    Ok(packet_size) => {
                        println!("Sent packet with size: {:?}", packet_size)
                    }
                    Err(_) => println!("Error"),
                }
                let incoming = recieve_window(lora, config);
                match incoming[0] {
                    3 => {
                        //let (ed_sck, ed_rck, ed_rk, devaddr) =
                        //    handle_message_fourth(incoming, msg4_reciever);
                        let rtn = handle_message_fourth(incoming, msg4_reciever);
                        match rtn {
                            Ok(values) => {
                                let ratchet_keys = RatchetKeys {
                                    ed_sck: values.ed_sck,
                                    ed_rck: values.ed_rck,
                                    ed_rk: values.ed_rk,
                                    devaddr: values.devaddr,
                                };
                                Ok(ratchet_keys)
                            }
                            Err(OwnOrPeerError::OwnError(x)) => {
                                println!("Got my own error {:?}", x);
                                Err(Box::new(MyError("Own error in m_type 3".to_string())))
                            }
                            Err(OwnOrPeerError::PeerError(x)) => {
                                println!("Got peer error {:?}", x);
                                Err(Box::new(MyError("Peer error in m_type 3".to_string())))
                            }
                        }
                    }
                    _ => Err(Box::new(MyError(
                        "Wrong order, got some other message than mtype 3".to_string(),
                    ))),
                }
            }
            Err(OwnOrPeerError::PeerError(x)) => Err(Box::new(MyError(x))),
            Err(OwnOrPeerError::OwnError(_)) => Err(Box::new(MyError("Own error".to_string()))),
        },
        _ => Err(Box::new(MyError(
            "Recieved nothing in our allocated time span".to_string(),
        ))),
    }
}

fn edhoc_first_message(msg1_sender: PartyI<Msg1Sender>) -> (Vec<u8>, PartyI<Msg2Receiver>) {
    let (msg1_bytes, msg2_receiver) =
    // If an error happens here, we just abort. No need to send a message,
    // since the protocol hasn't started yet.
    msg1_sender.generate_message_1(METHOD_TYPE_I, SUITE_I).unwrap();

    let payload1 = prepare_message(msg1_bytes, 0, true);
    (payload1, msg2_receiver)
}

fn edhoc_third_message(
    msg2: Vec<u8>,
    msg2_receiver: PartyI<Msg2Receiver>,
    //as_static_pub: PublicKey,
) -> Result<(Vec<u8>, PartyI<Msg4ReceiveVerify>), OwnOrPeerError> {
    let msg_struc = remove_message(msg2);
    unsafe {
        DEVADDR = msg_struc.devaddr;
    }

    // read from file, and check what key responds to as_kid
    // Needs to be used when verififying message2 instead of &as_static_pub.as_bytes()
    let (as_kid, _ad_r, msg2_verifier) =
        match msg2_receiver.unpack_message_2_return_kid(msg_struc.msg) {
            Err(OwnOrPeerError::PeerError(s)) => {
                return Err(OwnOrPeerError::PeerError(s));
            }
            Err(OwnOrPeerError::OwnError(b)) => {
                return Err(OwnOrPeerError::OwnError(b));
            }
            Ok(val) => val,
        };

    let enc_keys: StaticKeys = load_static_keys("./keys.json".to_string());
    let mut opt_as_static_pub: Option<PublicKey> = None;
    for each in enc_keys.as_keys {
        if each.kid.to_vec() == as_kid {
            opt_as_static_pub = Some(PublicKey::from(each.as_static_material));
        }
    }

    // I has now received the as_kid, such that the can retrieve the static key of as, and verify the first message
    match opt_as_static_pub {
        Some(as_static_pub) => {
            let msg3_sender =
                match msg2_verifier.verify_message_2(as_static_pub.as_bytes().as_ref()) {
                    Err(OwnError(b)) => {
                        return Err(OwnOrPeerError::OwnError(b));
                    }
                    Ok(val) => val,
                };

            let (msg4_receiver_verifier, msg3_bytes) = match msg3_sender.generate_message_3() {
                Err(OwnError(b)) => {
                    return Err(OwnOrPeerError::OwnError(b));
                }
                Ok(val) => val,
            };

            let payload3 = prepare_message(msg3_bytes, 2, false);
            Ok((payload3, msg4_receiver_verifier))
        }
        None => panic!("No key on KID"),
    }
}

struct FourthMessage {
    ed_sck: Vec<u8>,
    ed_rck: Vec<u8>,
    ed_rk: Vec<u8>,
    devaddr: Vec<u8>,
}

fn handle_message_fourth(
    msg: Vec<u8>,
    msg4_receiver_verifier: PartyI<Msg4ReceiveVerify>,
) -> Result<FourthMessage, oscore::edhoc::error::OwnOrPeerError> {
    let msg_struc = remove_message(msg);
    let out = msg4_receiver_verifier.handle_message_4(msg_struc.msg);
    match out {
        Err(OwnOrPeerError::PeerError(s)) => Err(OwnOrPeerError::PeerError(s)),
        Err(OwnOrPeerError::OwnError(s)) => Err(OwnOrPeerError::OwnError(s)),
        Ok((ed_sck, ed_rck, ed_rk)) => Ok(FourthMessage{ed_sck, ed_rck, ed_rk, devaddr: msg_struc.devaddr.to_vec()}),
    }
}

fn recieve_window(lora: &mut LoRa<Spi, OutputPin, OutputPin>, config: Config) -> Vec<u8> {
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

fn remove_message(ogmsg: Vec<u8>) -> MessageStruct {
    MessageStruct {
        _m: ogmsg[0],
        _fcntdown: ogmsg[1..3].try_into().unwrap(),
        devaddr: ogmsg[3..7].try_into().unwrap(),
        msg: ogmsg[7..].try_into().unwrap(),
    }
}

fn prepare_message(msg: Vec<u8>, mtype: u8, first_msg: bool) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&mtype.to_be_bytes());
    unsafe {
        buffer.extend_from_slice(&FCNTUP.to_be_bytes());
        FCNTUP += 1;
    }
    if !first_msg {
        unsafe {
            buffer.extend_from_slice(&DEVADDR);
        }
    }
    buffer.extend_from_slice(&msg);
    buffer
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

fn get_message_lenght(message: Vec<u8>) -> ([u8; 255], usize) {
    let mut buffer = [0; 255];
    for (i, byte) in message.iter().enumerate() {
        buffer[i] = *byte;
    }
    (buffer, message.len())
}

/*fn hexstring(slice: &[u8]) -> String {
    String::from("0x")
        + &slice
            .iter()
            .map(|n| format!("{:02X}", n))
            .collect::<Vec<String>>()
            .join(", 0x")
}*/
