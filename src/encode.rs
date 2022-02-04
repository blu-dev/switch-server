use std::io::{
    Write,
    Seek,
    self
};

pub trait Encode {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()> where W: Write + Seek;
}

impl Encode for &str {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        rmp::encode::write_str(writer, self)
            .map_err(Into::<io::Error>::into)
    }
}

impl Encode for String {
    fn encode<W>(&self, writer: &mut W) -> io::Result<()>
    where W: Write + Seek
    {
        self.as_str().encode(writer)   
    }
}

macro_rules! impl_encode {
    ($t:ty, $fn:ident) => {
        impl Encode for $t {
            fn encode<W>(&self, writer: &mut W) -> io::Result<()>
            where W: Write + Seek
            {
                rmp::encode::$fn(writer, *self)
                    .map_err(Into::<io::Error>::into)
            }
        }
    };
}

impl_encode!(i8, write_i8);
impl_encode!(u8, write_u8);
impl_encode!(i16, write_i16);
impl_encode!(u16, write_u16);
impl_encode!(i32, write_i32);
impl_encode!(u32, write_u32);
impl_encode!(i64, write_i64);
impl_encode!(u64, write_u64);
impl_encode!(f32, write_f32);
impl_encode!(f64, write_f64);