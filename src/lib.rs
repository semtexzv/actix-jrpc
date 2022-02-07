#![feature(box_syntax)]

use actix::prelude::*;
pub use bytes::Bytes;
use actix::actors::resolver::{Resolver, Connect, ResolverError};
use url::Url;

use std::{
    collections::btree_map::BTreeMap,
    collections::HashMap,
    marker::PhantomData
};
use actix::fut::wrap_future;
use actix::io::WriteHandler;
use tokio::{
    sync::oneshot,
    sync::oneshot::Sender
};
use futures::{
    FutureExt,
    TryFutureExt
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use crate::codec::IoError;

mod codec;
mod message;

type WireWriter = actix::io::FramedWrite<message::Message, tokio::net::tcp::OwnedWriteHalf, codec::JsonLineCodec>;

pub struct JrpcClient {
    last_id: usize,
    tx: WireWriter,
    running: HashMap<u32, Sender<Result<json::Value, json::Value>>>,
}

impl Actor for JrpcClient { type Context = Context<Self>; }

impl JrpcClient {
    pub async fn new(addr: &Url) -> Addr<Self> {
        let resolver = Resolver::from_registry();
        let connect = Connect::host_and_port(addr.host().unwrap().to_string(), addr.port().unwrap());
        let (rx, tx) = match resolver.send(connect).await {
            Ok(Ok(stream)) => {
                stream.into_split()
            }
            e => panic!("{:?}", e),
        };


        Actor::create(|ctx| {
            let rx = tokio_util::codec::FramedRead::new(rx, crate::codec::LineCodec);
            ctx.add_stream(rx);
            let tx = actix::io::FramedWrite::new(tx, crate::codec::JsonLineCodec, ctx);

            JrpcClient {
                last_id: 1,
                tx,
                running: HashMap::new(),
            }
        })
    }
}

impl WriteHandler<std::io::Error> for JrpcClient {}

impl StreamHandler<Result<Bytes, IoError>> for JrpcClient {
    fn handle(&mut self, item: Result<Bytes, IoError>, ctx: &mut Self::Context) {
        let item = item.unwrap();
        let wire = json::from_slice::<message::Message>(&item).unwrap();

        match wire {
            message::Message::Response(res) => {
                if let Some(sender) = self.running.remove(&(res.id.as_i64().unwrap() as u32)) {
                    sender.send(res.result).unwrap();
                }
            }
            _ => {
                panic!("Not implemented")
            }
        }
    }
}

pub trait Callable: Serialize + 'static {
    type Res: DeserializeOwned + 'static;
    fn method(&self) -> &str;
}

pub struct Call<T: Callable>(pub T);

impl<T: Callable> From<T> for Call<T> {
    fn from(v: T) -> Self {
        Call(v)
    }
}

impl<T: Callable> Message for Call<T> where T: Callable {
    type Result = Result<T::Res, json::Value>;
}

impl<T: Callable> Handler<Call<T>> for JrpcClient {
    type Result = actix::Response<Result<T::Res, json::Value>>;

    fn handle(&mut self, msg: Call<T>, ctx: &mut Self::Context) -> Self::Result {
        let msg = msg.0;
        self.last_id += 1;

        let id = self.last_id as u32;
        let (tx, rx) = oneshot::channel();
        let _ = self.running.insert(id, tx);

        let params = json::to_value(&msg).unwrap();

        let netmsg = message::Request {
            id: json::Value::Number(id.into()),
            method: msg.method().to_string(),
            params,
        };
        self.tx.write(netmsg.into());
        let resp = rx.map(|v| v.unwrap())
            .map_ok(|v| json::from_value::<T::Res>(v).unwrap());
        Response::fut(resp)
    }
}