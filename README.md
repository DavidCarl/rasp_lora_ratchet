# Lora Ratchet on Raspberry Pi

## Prerequisites

* ARM Rust toolchain
* Raspberry Pi (We used 3B+)
* SX1276 modules

For the ARM rust toolchain, we used [cross](https://github.com/cross-rs/cross). This requires Docker, but comes in a single package.

## What is in this repo

* [Client](https://github.com/DavidCarl/rasp_lora_ratchet/tree/main/client)
* [Server](https://github.com/DavidCarl/rasp_lora_ratchet/tree/main/server)


## Run

### Wiring

#! TODO <Insert wiring diagram here>

### Build

Go into eiher the client or server, whatever you want to compile first. 

Change your toolchain to either use a ARM toolchain, or use cross. Here we are using cross for simplicity sake.

`cross build --target arm-unknown-linux-gnueabihf`

This should create a binary in the 

`target/arm-unknown-linux-gnueabihf/debug/rasp_lora_<client or server>`

now transfer this to your raspberry pi.

### Config files

We made config files for the code, all the config files can be found in the respective directories, client & server.

## Modified libraries

We modified several libraries to get this working. This is both 

---

oscore: [original](https://github.com/martindisch/oscore) - [modified](https://github.com/DavidCarl/oscore)

Here we had to update the library to a newer version. 

---

sx127x_lora: [original](https://crates.io/crates/sx127x_lora) - [modified](https://github.com/DavidCarl/sx127x_lora)

Here we modified some ...

---