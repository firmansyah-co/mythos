//! Modbus TCP and RTU frame encoders, decoders, and PDU handlers.

use crate::crc::{append_crc16, verify_crc16};
use mythos_plugin_sdk::PluginError;

/// Supported Modbus Function Codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FunctionCode {
    ReadCoils = 0x01,
    ReadDiscreteInputs = 0x02,
    ReadHoldingRegisters = 0x03,
    ReadInputRegisters = 0x04,
    WriteSingleCoil = 0x05,
    WriteSingleRegister = 0x06,
    WriteMultipleRegisters = 0x10,
}

impl FunctionCode {
    pub fn from_u8(val: u8) -> Result<Self, PluginError> {
        match val {
            0x01 => Ok(Self::ReadCoils),
            0x02 => Ok(Self::ReadDiscreteInputs),
            0x03 => Ok(Self::ReadHoldingRegisters),
            0x04 => Ok(Self::ReadInputRegisters),
            0x05 => Ok(Self::WriteSingleCoil),
            0x06 => Ok(Self::WriteSingleRegister),
            0x10 => Ok(Self::WriteMultipleRegisters),
            other => Err(PluginError::InvalidConfiguration(format!(
                "Unsupported Modbus function code 0x{:02X}",
                other
            ))),
        }
    }
}

/// Modbus Request descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModbusRequest {
    ReadHoldingRegisters {
        unit_id: u8,
        start_addr: u16,
        quantity: u16,
    },
    ReadInputRegisters {
        unit_id: u8,
        start_addr: u16,
        quantity: u16,
    },
    WriteSingleRegister {
        unit_id: u8,
        addr: u16,
        value: u16,
    },
}

impl ModbusRequest {
    /// Encode request for Modbus TCP (MBAP header + PDU).
    pub fn encode_tcp(&self, transaction_id: u16) -> Vec<u8> {
        let mut buf = Vec::with_capacity(12);
        // Transaction ID (2 bytes)
        buf.extend_from_slice(&transaction_id.to_be_bytes());
        // Protocol ID = 0x0000 (2 bytes)
        buf.extend_from_slice(&0u16.to_be_bytes());
        // Length field placeholder (2 bytes) - Unit ID (1) + Function (1) + 4 bytes payload = 6
        buf.extend_from_slice(&6u16.to_be_bytes());

        match self {
            Self::ReadHoldingRegisters {
                unit_id,
                start_addr,
                quantity,
            } => {
                buf.push(*unit_id);
                buf.push(FunctionCode::ReadHoldingRegisters as u8);
                buf.extend_from_slice(&start_addr.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
            }
            Self::ReadInputRegisters {
                unit_id,
                start_addr,
                quantity,
            } => {
                buf.push(*unit_id);
                buf.push(FunctionCode::ReadInputRegisters as u8);
                buf.extend_from_slice(&start_addr.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
            }
            Self::WriteSingleRegister {
                unit_id,
                addr,
                value,
            } => {
                buf.push(*unit_id);
                buf.push(FunctionCode::WriteSingleRegister as u8);
                buf.extend_from_slice(&addr.to_be_bytes());
                buf.extend_from_slice(&value.to_be_bytes());
            }
        }
        buf
    }

    /// Encode request for Modbus RTU (Slave + PDU + CRC16).
    pub fn encode_rtu(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8);
        match self {
            Self::ReadHoldingRegisters {
                unit_id,
                start_addr,
                quantity,
            } => {
                buf.push(*unit_id);
                buf.push(FunctionCode::ReadHoldingRegisters as u8);
                buf.extend_from_slice(&start_addr.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
            }
            Self::ReadInputRegisters {
                unit_id,
                start_addr,
                quantity,
            } => {
                buf.push(*unit_id);
                buf.push(FunctionCode::ReadInputRegisters as u8);
                buf.extend_from_slice(&start_addr.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
            }
            Self::WriteSingleRegister {
                unit_id,
                addr,
                value,
            } => {
                buf.push(*unit_id);
                buf.push(FunctionCode::WriteSingleRegister as u8);
                buf.extend_from_slice(&addr.to_be_bytes());
                buf.extend_from_slice(&value.to_be_bytes());
            }
        }
        append_crc16(&mut buf);
        buf
    }
}

/// Modbus Response decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModbusResponse {
    Registers(Vec<u16>),
    WriteAcknowledged {
        addr: u16,
        value: u16,
    },
    Exception {
        function_code: u8,
        exception_code: u8,
    },
}

impl ModbusResponse {
    /// Parse raw response from Modbus TCP (skipping 7-byte MBAP).
    pub fn parse_tcp(bytes: &[u8], expected_tx_id: u16) -> Result<Self, PluginError> {
        if bytes.len() < 8 {
            return Err(PluginError::CommunicationError(
                "Modbus TCP response too short".into(),
            ));
        }

        let tx_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        if tx_id != expected_tx_id {
            return Err(PluginError::CommunicationError(format!(
                "Transaction ID mismatch: expected {}, got {}",
                expected_tx_id, tx_id
            )));
        }

        let pdu = &bytes[7..]; // Strip 7-byte MBAP (TxID: 2, Proto: 2, Len: 2, UnitID: 1)
        Self::parse_pdu(pdu)
    }

    /// Parse raw response from Modbus RTU (verifying CRC).
    pub fn parse_rtu(bytes: &[u8], expected_unit_id: u8) -> Result<Self, PluginError> {
        if bytes.len() < 5 {
            return Err(PluginError::CommunicationError(
                "Modbus RTU response too short".into(),
            ));
        }

        if !verify_crc16(bytes) {
            return Err(PluginError::CommunicationError(
                "Modbus RTU CRC check failed".into(),
            ));
        }

        if bytes[0] != expected_unit_id {
            return Err(PluginError::CommunicationError(format!(
                "Slave ID mismatch: expected {}, got {}",
                expected_unit_id, bytes[0]
            )));
        }

        let pdu = &bytes[1..bytes.len() - 2];
        Self::parse_pdu(pdu)
    }

    fn parse_pdu(pdu: &[u8]) -> Result<Self, PluginError> {
        if pdu.is_empty() {
            return Err(PluginError::CommunicationError("Empty Modbus PDU".into()));
        }

        let func_code = pdu[0];
        if (func_code & 0x80) != 0 {
            // Exception response
            if pdu.len() < 2 {
                return Err(PluginError::CommunicationError(
                    "Malformed exception response".into(),
                ));
            }
            return Ok(Self::Exception {
                function_code: func_code & 0x7F,
                exception_code: pdu[1],
            });
        }

        match func_code {
            0x03 | 0x04 => {
                // Read Registers response: func (1) + byte_count (1) + registers (2 * N)
                if pdu.len() < 2 {
                    return Err(PluginError::CommunicationError(
                        "Truncated register read response".into(),
                    ));
                }
                let byte_count = pdu[1] as usize;
                if pdu.len() < 2 + byte_count {
                    return Err(PluginError::CommunicationError(
                        "Incomplete register data in response".into(),
                    ));
                }
                let reg_data = &pdu[2..2 + byte_count];
                let mut regs = Vec::with_capacity(byte_count / 2);
                for chunk in reg_data.chunks_exact(2) {
                    regs.push(u16::from_be_bytes([chunk[0], chunk[1]]));
                }
                Ok(Self::Registers(regs))
            }
            0x06 => {
                // Write Single Register response: func (1) + addr (2) + value (2)
                if pdu.len() < 5 {
                    return Err(PluginError::CommunicationError(
                        "Truncated write response".into(),
                    ));
                }
                let addr = u16::from_be_bytes([pdu[1], pdu[2]]);
                let val = u16::from_be_bytes([pdu[3], pdu[4]]);
                Ok(Self::WriteAcknowledged { addr, value: val })
            }
            other => Err(PluginError::CommunicationError(format!(
                "Unsupported response function code 0x{:02X}",
                other
            ))),
        }
    }
}
