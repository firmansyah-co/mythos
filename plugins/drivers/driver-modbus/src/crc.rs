//! Standard Modbus RTU CRC-16 calculation.
//!
//! Polynomial: 0xA001 (reversed 0x8005), Initial value: 0xFFFF.

/// Calculate Modbus RTU CRC-16 for the given byte slice.
pub fn calculate_crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if (crc & 0x0001) != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// Append 2-byte CRC16 (little-endian) to payload.
pub fn append_crc16(payload: &mut Vec<u8>) {
    let crc = calculate_crc16(payload);
    payload.push((crc & 0xFF) as u8);
    payload.push(((crc >> 8) & 0xFF) as u8);
}

/// Verify trailing 2-byte CRC16 on a received RTU frame.
pub fn verify_crc16(frame: &[u8]) -> bool {
    if frame.len() < 3 {
        return false;
    }
    let payload = &frame[..frame.len() - 2];
    let expected = calculate_crc16(payload);
    let received = (frame[frame.len() - 2] as u16) | ((frame[frame.len() - 1] as u16) << 8);
    expected == received
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modbus_crc() {
        // Standard Modbus RTU example: [0x01, 0x03, 0x00, 0x00, 0x00, 0x0A] -> CRC is 0xC5CD (or CD C5 on wire)
        let data = [0x01, 0x03, 0x00, 0x00, 0x00, 0x0A];
        let crc = calculate_crc16(&data);
        assert_eq!(crc, 0xCDC5);

        let mut frame = data.to_vec();
        append_crc16(&mut frame);
        assert_eq!(frame, vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC5, 0xCD]);
        assert!(verify_crc16(&frame));

        // Corrupt frame
        frame[2] ^= 0xFF;
        assert!(!verify_crc16(&frame));
    }
}
