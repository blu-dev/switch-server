use std::io::{
    Read,
    Seek,
    self, SeekFrom
};

use rmp::decode::{ValueReadError, DecodeStringError};

pub trait Decode: Sized {
    fn decode<R>(reader: &mut R) -> io::Result<Self> where R: Read + Seek;
}

fn map_read_err(e: ValueReadError) -> io::Error {
    match e {
        ValueReadError::InvalidMarkerRead(e) => e,
        ValueReadError::InvalidDataRead(e) => e,
        ValueReadError::TypeMismatch(m) => io::Error::new(io::ErrorKind::InvalidData, format!("Marker type mismatch. Found marker {:?}", m))
    }
}

fn map_dec_string_error(e: DecodeStringError) -> io::Error {
    match e {
        DecodeStringError::InvalidMarkerRead(e) => e,
        DecodeStringError::InvalidDataRead(e) => e,
        DecodeStringError::TypeMismatch(m) => io::Error::new(io::ErrorKind::InvalidData, format!("Marker type mismatch. Found marker {:?}", m)),
        DecodeStringError::BufferSizeTooSmall(needed) => io::Error::new(io::ErrorKind::Other, format!("Buffer for string is too small to accommodate {} bytes needed", needed)),
        DecodeStringError::InvalidUtf8(_, e) => io::Error::new(io::ErrorKind::Other, e.to_string())
    }
}

impl Decode for String {
    fn decode<R>(reader: &mut R) -> io::Result<Self>
    where R: Read + Seek
    {
        let current_pos = reader.stream_position()?;
        let len = rmp::decode::read_str_len(reader).map_err(map_read_err)? as usize;
        let mut buffer = vec![0u8; len];

        reader.seek(SeekFrom::Start(current_pos))?;
        rmp::decode::read_str(reader, &mut buffer)
            .map(|s| s.to_string())
            .map_err(map_dec_string_error)
    }
}

macro_rules! impl_decode {
    ($t:ty, $fn:ident) => {
        impl Decode for $t {
            fn decode<R>(reader: &mut R) -> io::Result<Self>
            where R: Read + Seek
            {
                rmp::decode::$fn(reader)
                    .map_err(map_read_err)
            }
        }
    };
}

impl_decode!(bool, read_bool);
impl_decode!(i8, read_i8);
impl_decode!(u8, read_u8);
impl_decode!(i16, read_i16);
impl_decode!(u16, read_u16);
impl_decode!(i32, read_i32);
impl_decode!(u32, read_u32);
impl_decode!(i64, read_i64);
impl_decode!(u64, read_u64);
impl_decode!(f32, read_f32);
impl_decode!(f64, read_f64);