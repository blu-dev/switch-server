#[macro_use]
extern crate log;

use std::io::{
    Read,
    Write,
    Seek,
    self
};

use std::convert::TryFrom;

pub mod decode;
pub mod encode;

pub use decode::*;
pub use encode::*;

pub mod packets;

pub mod server;

const PORT: u16 = 18050;

#[derive(Clone)]
pub enum Packet {
    Message(packets::Message),
    Connection(packets::ConnectionAcquired),
    Disconnection(packets::Disconnection),
    Custom(packets::Custom)
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum PacketType {
    Message = 0x0,
    Connection = 0x1,
    Disconnection = 0x2,
    Custom = 0x3
}

impl TryFrom<u8> for PacketType {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        // Check for approved packet values and return IO error if it's invalid
        match value {
            0 => Ok(Self::Message),
            1 => Ok(Self::Connection),
            2 => Ok(Self::Disconnection),
            3 => Ok(Self::Custom),
            _ => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Input value '{}' is out of range of PacketType", value)))
        }
    }
}

impl Encode for PacketType {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        (*self as u8).encode(writer)
    }
}

impl Decode for PacketType {
    fn decode<R>(reader: &mut R) -> io::Result<Self>
    where R: Read + Seek
    {
        // Guarantee that it is an approved packet type before returning it
        let val = u8::decode(reader)?;
        PacketType::try_from(val)
    }
}

impl Packet {
    pub fn from_custom(custom: &impl Encode) -> io::Result<Self> {
        let mut cursor = io::Cursor::new(vec![]);
        custom.encode(&mut cursor)?;
        let bytes = cursor.into_inner();
        Ok(Self::Custom(packets::Custom(bytes)))
    }

    pub fn into_custom<D: Decode>(self) -> io::Result<D> {
        let data = match self {
            Self::Custom(packets::Custom(data)) => data,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Packet is not a custom packet!"))
        };
        let mut cursor = io::Cursor::new(data);
        D::decode(&mut cursor)
    }
}

impl Encode for Packet {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        // Write the packet type marker and then write the packet
        match self {
            Self::Message(m) => {
                PacketType::Message.encode(writer)?;
                m.encode(writer)
            },
            Self::Connection(c) => {
                PacketType::Connection.encode(writer)?;
                c.encode(writer)
            },
            Self::Disconnection(d) => {
                PacketType::Disconnection.encode(writer)?;
                d.encode(writer)
            },
            Self::Custom(c) => {
                PacketType::Custom.encode(writer)?;
                c.encode(writer)
            }
        }
    }
}

impl Decode for Packet {
    fn decode<R>(reader: &mut R) -> io::Result<Self>
    where R: Read + Seek
    {
        // Parse the packet kind and create a packet out of it
        match PacketType::decode(reader)? {
            PacketType::Message => packets::Message::decode(reader).map(Self::Message),
            PacketType::Connection => packets::ConnectionAcquired::decode(reader).map(Self::Connection),
            PacketType::Disconnection => packets::Disconnection::decode(reader).map(Self::Disconnection),
            PacketType::Custom => packets::Custom::decode(reader).map(Self::Custom)
        }
    }
}

#[test]
fn packet_msg_test() {
    use io::SeekFrom;
    let message = packets::Message::new("Hello world!");
    let packet = Packet::Message(message);
    let mut rw = io::Cursor::new(vec![]);
    packet.encode(&mut rw).unwrap();
    rw.seek(SeekFrom::Start(0)).unwrap();
    let packet2 = Packet::decode(&mut rw).unwrap();
    match packet2 {
        Packet::Message(msg) => assert_eq!(msg.0, "Hello world!"),
        _ => panic!("Received packet was invalid type!")
    }
}

#[test]
fn packet_connection_test() {
    use io::SeekFrom;

    let message = packets::ConnectionAcquired::default();
    let packet = Packet::Connection(message);
    let mut rw = io::Cursor::new(vec![]);
    packet.encode(&mut rw).unwrap();
    rw.seek(SeekFrom::Start(0)).unwrap();
    let packet2 = Packet::decode(&mut rw).unwrap();
    match packet2 {
        Packet::Connection(c) => assert!(c.is_valid()),
        _ => panic!("Received packet was invalid type!")
    }
}

#[test]
fn packet_disconnection_test() {
    use io::SeekFrom;

    let message = packets::Disconnection::default();
    let packet = Packet::Disconnection(message);
    let mut rw = io::Cursor::new(vec![]);
    packet.encode(&mut rw).unwrap();
    rw.seek(SeekFrom::Start(0)).unwrap();
    let packet2 = Packet::decode(&mut rw).unwrap();
    match packet2 {
        Packet::Disconnection(d) => assert!(d.is_valid()),
        _ => panic!("Received packet was invalid type!")
    }
}