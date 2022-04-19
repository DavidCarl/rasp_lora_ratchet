use rand::{rngs::StdRng, Rng, SeedableRng};
use rand_core::OsRng;

use rppal::gpio::OutputPin;
use rppal::spi::Spi;

use oscore::edhoc::{
    api::{Msg1Receiver, Msg3Receiver},
    error::{OwnError, OwnOrPeerError},
    PartyR,
};

use twoRatchet::AS::ASRatchet;

use x25519_dalek_ng::{PublicKey, StaticSecret};

use sx127x_lora::LoRa;

use std::collections::HashMap;

use crate::{
    filehandler::{load_static_keys, StaticKeys},
    generics::get_message_lenght,
};

/// Pads the message we want to send with relevant data such as the mtype, devaddr and returns the message ready to send.
///     
/// # Arguments
///
/// * `msg` - The message you want to have padded with informatino
/// * `mtype` - The message type usually `0` or `2`
/// * `devaddr` - The dev addresse of the device
/// * `first_msg` - if its the first message being sent
fn prepare_message(msg: Vec<u8>, mtype: u8, devaddr: [u8; 4], first_msg: bool) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&mtype.to_be_bytes());
    if !first_msg {
        buffer.extend_from_slice(&devaddr);
    }
    buffer.extend_from_slice(&msg);
    buffer
}

pub struct TypeZero {
    pub msg3_receivers: HashMap<[u8; 4], PartyR<Msg3Receiver>>,
    pub lora: LoRa<Spi, OutputPin, OutputPin>,
}

/// Handle the zeroth [[0]] message in the EDHOC handshake, initiate the handshake from a AS point of view with a new ED. This function handle all the calls to the different libraries. 
/// It will also transmit the response in this handshake, which is the oneth [[1]] message.
///     
/// # Arguments
///
/// * `buffer` - The incomming message
/// * `msg3_recievers` - A hashmap where the reciever object needs to be stored based on a devaddr
/// * `lora` - A sx127x object, used for broadcasting a response
/// * `as_static_material` - Our static key material. Used to generate our staticSecret
pub fn handle_m_type_zero(
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

pub struct TypeTwo {
    pub msg3_receivers: HashMap<[u8; 4], PartyR<Msg3Receiver>>,
    pub lora_ratchets: HashMap<[u8; 4], ASRatchet<OsRng>>,
    pub lora: LoRa<Spi, OutputPin, OutputPin>,
    pub ratchet_recieved: HashMap<[u8; 4], u16>,
}

/// handle the second [[2]] message in the EDHOC handshake, and transmit the third [[3]] message in the sequence.
///     
/// # Arguments
///
/// * `buffer` - The incomming message
/// * `msg3_receivers` - A hashmap where the reciever object needs to be stored based on a devaddr
/// * `lora_ratchets` - A hashmap where the ratchet object needs to be stored based on a devaddr
/// * `lora` - A sx127x object, used for broadcasting a response
/// * `ratchet_recieved` - A debug hashmap, here we store the amount of messages recieved based on the devaddr
pub fn handle_m_type_two(
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

/// This function removes the framecounter and the m type, and solely returns the message itself.
///     
/// # Arguments
///
/// * `msg` - the message which needs to be handled.
fn unpack_edhoc_first_message(msg: Vec<u8>) -> Vec<u8> {
    let msg = &msg[1..]; // fjerne mtype
    let _framecounter = &msg[0..2]; // gemme framecounter
    let msg = &msg[2..]; // fjerne frame counter
    msg.to_vec()
}

/// This function removes the framecounter and the m type, and returns the message and devaddr.
///     
/// # Arguments
///
/// * `msg` - the message which needs to be handled.
fn unpack_edhoc_message(msg: Vec<u8>) -> (Vec<u8>, [u8; 4]) {
    let msg = &msg[1..]; // fjerne mtype
    let msg = &msg[2..]; // fjerne frame counter
    let devaddr = msg[0..4].try_into().unwrap();
    let msg = &msg[4..];
    (msg.to_vec(), devaddr)
}

struct Msg2 {
    msg: Vec<u8>,
    msg3_receiver: PartyR<Msg3Receiver>,
    devaddr: [u8; 4],
}

/// This function handles the EDHOC logic behind the first [[0]] message. It generates the second message, and the object we need to verify the third [[3]] message later, so we can make
/// sure it comes from the right ED. We also generate a devaddr we use for identifying the devices.
///     
/// # Arguments
///
/// * `msg` - the message which needs to be handled.
/// * `msg1_receiver` - Verifier object, so we can start the whole EDHOC verification.
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
        Err(OwnOrPeerError::PeerError(s)) => return Err(OwnOrPeerError::PeerError(s)),
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

/// This function handles the EDHOC logic behind the third [[3]] message. It extracts a KID value, which makes us able to load a pre known keys from a file. 
/// We then use these informations to get the keys we need to start our LoRaRatchet protocol and send the fourth [[4]] message.
///     
/// # Arguments
///
/// * `msg` - the message which needs to be handled.
/// * `msg3_receiver` - Verifier object, so we can continue the whole EDHOC verification.
fn handle_third_gen_fourth_message(
    msg: Vec<u8>,
    msg3_receiver: PartyR<Msg3Receiver>,
) -> Result<Msg4, OwnOrPeerError> {
    let (msg3verifier, ed_kid) = match msg3_receiver.unpack_message_3_return_kid(msg) {
        //.handle_message_3(msg) {
        Err(OwnOrPeerError::PeerError(s)) => return Err(OwnOrPeerError::PeerError(s)),
        Err(OwnOrPeerError::OwnError(b)) => return Err(OwnOrPeerError::OwnError(b)),
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
                    Err(OwnOrPeerError::PeerError(s)) => return Err(OwnOrPeerError::PeerError(s)),
                    Err(OwnOrPeerError::OwnError(b)) => return Err(OwnOrPeerError::OwnError(b)),
                    Ok(val) => val,
                };
            match msg4_sender.generate_message_4() {
                Err(OwnOrPeerError::PeerError(s)) => Err(OwnOrPeerError::PeerError(s)),
                Err(OwnOrPeerError::OwnError(b)) => Err(OwnOrPeerError::OwnError(b)),
                Ok(msg4_bytes) => Ok(Msg4 {
                    msg4_bytes,
                    as_sck,
                    as_rck,
                    as_master,
                }),
            }
        }
        None => panic!("Missing kid value"),
    }
}
