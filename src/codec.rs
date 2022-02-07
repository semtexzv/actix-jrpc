use tokio_util::codec::{Encoder, Decoder};

use crate::message::*;

use bytes::{Bytes, BytesMut, Buf, BufMut};
use json::Value;
use serde::Serialize;

pub type IoError = std::io::Error;

#[derive(Debug)]
pub struct LineCodec;

impl Encoder<Bytes> for LineCodec {
    type Error = IoError;

    fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.reserve(item.len() + 1);
        dst.put_slice(&item);
        dst.put_u8(b'\n');
        Ok(())
    }
}

#[derive(Debug)]
pub struct JsonLineCodec;

impl<V: Serialize> Encoder<V> for JsonLineCodec {
    type Error = IoError;

    fn encode(&mut self, item: V, dst: &mut BytesMut) -> Result<(), Self::Error> {
        json::to_writer(dst.writer(), &item)
            .map_err(|e| panic!("{e:?}"))
    }
}

impl Decoder for LineCodec {
    type Item = Bytes;
    type Error = IoError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(i) = src.iter().position(|&b| b == b'\n') {
            let line = src.split_to(i);
            src.split_to(1);
            Ok(Some(line.freeze()))
        } else {
            Ok(None)
        }
    }
}
