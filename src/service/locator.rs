use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use futures::{Future, Stream};
use futures::sync::{mpsc, oneshot};

use rmpv::ValueRef;

use {Dispatch, Error, Service};
use dispatch::{Streaming, StreamingDispatch};
use protocol::{self, Flatten, Primitive};

use flatten_err;

#[derive(Debug, Deserialize)]
pub struct GraphNode {
    event: String,
    rx: HashMap<u64, GraphNode>,
}

#[derive(Debug, Deserialize)]
pub struct EventGraph {
    name: String,
    tx: HashMap<u64, GraphNode>,
    rx: HashMap<u64, GraphNode>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveInfo {
    endpoints: Vec<(IpAddr, u16)>,
    version: u64,
    methods: HashMap<u64, EventGraph>,
}

#[derive(Debug)]
pub struct Info {
    endpoints: Vec<SocketAddr>,
    version: u64,
    methods: HashMap<u64, EventGraph>,
}

impl Info {
    pub fn endpoints(&self) -> &[SocketAddr] {
        &self.endpoints
    }
}

///// A single-shot dispatch that implements primitive protocol and emits either value or error.
//#[derive(Debug)]
//pub struct PrimitiveDispatch<T> {
//    tx: oneshot::Sender<Result<T, Error>>,
//}
//
//impl<T> PrimitiveDispatch<T> {
//    pub fn new(tx: oneshot::Sender<Result<T, Error>>) -> Self {
//        Self {
//            tx: tx,
//        }
//    }
//}
//
//impl<T: Deserialize + Send> Dispatch for PrimitiveDispatch<T> {
//    fn process(self: Box<Self>, ty: u64, response: &ValueRef) -> Option<Box<Dispatch>> {
//        let result = protocol::deserialize::<Primitive<T>>(ty, response)
//            .flatten();
//        drop(self.tx.send(result));
//
//        None
//    }
//
//    fn discard(self: Box<Self>, err: &Error) {
//        drop(self.tx.send(Err(err.clone())));
//    }
//}

// TODO: Use `PrimitiveDispatch` instead.
struct ResolveDispatch {
    tx: oneshot::Sender<Result<Info, Error>>,
}

impl Dispatch for ResolveDispatch {
    fn process(self: Box<Self>, ty: u64, response: &ValueRef) -> Option<Box<Dispatch>> {
        let result = protocol::deserialize::<Primitive<ResolveInfo>>(ty, response)
            .flatten()
            .map(|ResolveInfo{endpoints, version, methods}|
        {
            let endpoints = endpoints.into_iter()
                .map(|(ip, port)| SocketAddr::new(ip, port))
                .collect();

            Info {
                endpoints: endpoints,
                version: version,
                methods: methods,
            }
        });

        drop(self.tx.send(result));

        None
    }

    fn discard(self: Box<Self>, err: &Error) {
        drop(self.tx.send(Err(err.clone())));
    }
}

pub type HashRing = Vec<(u64, String)>;

#[derive(Debug)]
pub struct Locator {
    service: Service,
}

impl Locator {
    pub fn new(service: Service) -> Self {
        Locator { service: service }
    }

    pub fn resolve(&self, name: &str) -> impl Future<Item = Info, Error = Error> {
        let (tx, rx) = oneshot::channel();
        let dispatch = ResolveDispatch { tx: tx };

        self.service.call(0, &[name], dispatch);

        rx.then(flatten_err)
    }

    pub fn routing(&self, uuid: &str) ->
        impl Stream<Item = Streaming<HashMap<String, HashRing>>, Error = ()>
    {
        let (tx, rx) = mpsc::unbounded();
        let dispatch = StreamingDispatch::new(tx);
        self.service.call(5, &[uuid], dispatch);
        rx
    }
}
