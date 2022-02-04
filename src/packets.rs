use std::io::{
    Read,
    Write,
    Seek,
    self
};
use super::*;

#[derive(Clone)]
pub struct Message(pub String);

impl Message {
    pub fn new<S: Into<String>>(message: S) -> Self {
        Self(message.into())
    }
}

impl Decode for Message {
    fn decode<R>(reader: &mut R) -> io::Result<Self>
    where R: Read + Seek
    {
        String::decode(reader)
            .map(Self)    
    }
}

impl Encode for Message {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        self.0.encode(writer)    
    }
}

#[test]
fn msg_test() {
    use io::SeekFrom;

    let message = Message::new("Hello world!");
    let mut read_writer = io::Cursor::new(vec![]);
    message.encode(&mut read_writer).unwrap();
    read_writer.seek(SeekFrom::Start(0)).unwrap();
    let message2 = Message::decode(&mut read_writer).unwrap();
    assert_eq!(message.0, message2.0);
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct ConnectionAcquired(u32);

impl Default for ConnectionAcquired {
    fn default() -> Self {
        Self(Self::MAGIC)
    }
}

impl ConnectionAcquired {
    const MAGIC: u32 = 0x434F4E4E; // ASCII: CONN

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_valid(self) -> bool {
        self.0 == Self::MAGIC
    }
}

impl Encode for ConnectionAcquired {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        self.0.encode(writer)
    }
}

impl Decode for ConnectionAcquired {
    fn decode<R>(reader: &mut R) -> io::Result<Self>
    where R: Read + Seek
    {
        let val = u32::decode(reader)?;
        if val != Self::MAGIC {
            Err(io::Error::new(io::ErrorKind::InvalidData, "Connection packet received but there was invalid magic!"))
        } else {
            Ok(Self(val))
        }
    }
}

#[test]
fn conn_test() {
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

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Disconnection(u32);

impl Disconnection {
    const MAGIC: u32 = 0x44495354; // ASCII: DISC

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_valid(self) -> bool {
        self.0 == Self::MAGIC
    }
}

impl Default for Disconnection {
    fn default() -> Self {
        Self(Self::MAGIC)
    }
}

impl Encode for Disconnection {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        self.0.encode(writer)    
    }
}

impl Decode for Disconnection {
    fn decode<R>(reader: &mut R) -> io::Result<Self>
    where R: Read + Seek
    {
        let val = u32::decode(reader)?;
        if val != Self::MAGIC {
            Err(io::Error::new(io::ErrorKind::InvalidData, "Disconnection packet received but there was invalid magic!"))
        } else {
            Ok(Self(val))
        }
    }
}

#[test]
fn disc_test() {
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

#[derive(Clone)]
pub struct Custom(pub(crate) Vec<u8>);

impl Encode for Custom {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        let size = self.0.len() as u64;
        size.encode(writer)?;
        writer.write(&self.0).map(|_| ())    
    }
}

impl Decode for Custom {
    fn decode<R>(reader: &mut R) -> io::Result<Self>
    where R: Read + Seek
    {
        let size = u64::decode(reader)? as usize;
        let mut vec = vec![0u8; size];
        reader.read_exact(&mut vec).map(|_| Self(vec))
    }
}