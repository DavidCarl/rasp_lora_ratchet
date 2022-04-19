/// Converts a Vector to an array of a fixed size and length of the given Vector.
///
/// # Arguments
///
/// * `message` - The messages which we need to convert and get length
pub fn get_message_lenght(message: Vec<u8>) -> ([u8; 255], usize) {
    let mut buffer = [0; 255];
    for (i, byte) in message.iter().enumerate() {
        buffer[i] = *byte;
    }
    (buffer, message.len())
}
