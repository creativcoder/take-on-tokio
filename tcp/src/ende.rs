use std::io;
use std::str;

use tokio_core::io::Io;
use tokio_core::io::{Codec, EasyBuf, Framed};
use tokio_proto::pipeline::ClientProto;
use futures::{future, Future, BoxFuture};
use tokio_service::Service;

pub struct LineCodec;

impl Codec for LineCodec {
    type In = String;
    type Out = String;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>> {
        if let Some(i) = buf.as_slice().iter().position(|&b| b == b'\n') {
            // remove the serialized frame from the buffer.
            let line = buf.drain_to(i);

        // Also remove the '\n'
        buf.drain_to(1);

        // Turn this data into a UTF string and return it in a Frame.
        return match str::from_utf8(&line.as_ref()) {
            Ok(s) => Ok(Some(s.to_string())),
            Err(_) => Err(io::Error::new(io::ErrorKind::Other,
                                         "invalid UTF-8")),
            }
        } else {
            Ok(None)
        }
    }

    fn encode(&mut self, msg: String, buf: &mut Vec<u8>)
         -> io::Result<()>
    {
        buf.extend_from_slice(msg.as_bytes());
        buf.push(b'\n');
        Ok(())
    }
}

pub struct LineProto;

impl<T: Io + 'static> ClientProto<T> for LineProto {
    /// For this protocol style, `Request` matches the codec `In` type
    type Request = String;

    /// For this protocol style, `Response` matches the coded `Out` type
    type Response = String;

    /// A bit of boilerplate to hook in the codec:
    type Transport = Framed<T, LineCodec>;
    type BindTransport = Result<Self::Transport, io::Error>;
    fn bind_transport(&self, io: T) -> Self::BindTransport {
        Ok(io.framed(LineCodec))
    }
}

pub struct Echo;

impl Service for Echo {
    // These types must match the corresponding protocol types:
    type Request = String;
    type Response = String;

    // For non-streaming protocols, service errors are always io::Error
    type Error = io::Error;

    // The future for computing the response; box it for simplicity.
    type Future = BoxFuture<Self::Response, Self::Error>;

    // Produce a future for computing a response from a request.
    fn call(&self, req: Self::Request) -> Self::Future {
        // In this case, the response is immediate.
        future::ok(req).boxed()
    }
}