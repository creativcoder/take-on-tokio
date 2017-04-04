use std::net::SocketAddr;
use std::thread;
use std::io;
use std::mem;

use tokio_core::reactor::{Core, Handle};
use tokio_core::net::TcpStream;
use tokio_io::AsyncRead;
use tokio_io::codec::Framed;

use futures::sync::mpsc::{self, Receiver, Sender};
use futures::{Future, Sink, Stream, Poll, StartSend, Async};

use error::Error;
use codec::LineCodec;

use std::time::Instant;
use std::time::Duration;

use futures::IntoFuture;
use tokio_timer::Timer;

use futures::task::park;

use tokio_timer::Interval;

pub struct Connection;

use futures::future::{ok, loop_fn, FutureResult, Loop};

impl Connection {
    pub fn start(addr: String) -> Result<Sender<String>, Error> {
        let (command_tx, command_rx) = mpsc::channel::<String>(1000);
        thread::spawn(move || { Self::run(&addr, command_rx); });
        Ok(command_tx)
    }

    fn run(addr: &str, command_rx: Receiver<String>) -> Result<(), Error> {
        let addr: SocketAddr = addr.parse()?;
        let mut reactor = Core::new()?;

        let tcp = TcpStream::connect(&addr, &reactor.handle());
        let handle = reactor.handle();
        let addr = addr.clone();

        // `client` consists of cascades of different futures running in interleaved manner.
        let client = tcp.map_err(|_| Error::Line).and_then(|connection| {
            let framed = connection.framed(LineCodec);
            let mqtt_stream = LineStream::new(framed, handle, addr);

            let (network_sender, network_receiver) = mqtt_stream.split();
            // let network_sender_c = network_sender.clone();

            // Future which responds to messages by the client
            let receiver_future = network_receiver.for_each(|msg| {
                match msg.as_str() {
                    "PING" => {
                        println!("PING_RESPONSE : {:?}", msg);
                    }
                    _ => {
                        println!("Command {:?}", msg);
                    }
                }
                Ok(())
            }).map_err(|e| Error::Io(e));

            // Future which takes messages from client and forwards them to the tcp connection Sink
            let client_to_tcp = command_rx.map_err(|_| Error::Line)
                .and_then(|p| Ok(p))
                .forward(network_sender);


            receiver_future.join(client_to_tcp).map_err(|e| Error::Line)
        });


        let _ = reactor.run(client);
        println!("@@@@@@@@@@@@@@@@@@@@");
        Ok(())
    }
}

pub struct LineStream {
    inner: Framed<TcpStream, LineCodec>,
    last_ping: Instant,
    handle: Handle,
    addr: SocketAddr,
    ping_timer: Interval
}

impl LineStream {
    fn new(inner: Framed<TcpStream, LineCodec>, handle: Handle, addr: SocketAddr) -> Self {
        LineStream {
            inner: inner,
            last_ping: Instant::now(),
            handle: handle,
            addr: addr,
            ping_timer: Timer::default().interval(Duration::from_millis(5000))
        }
    }
}

impl Stream for LineStream {
    type Item = String;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match try_ready!(self.inner.poll()) {
                Some(m) => {
                    // send pings to the server
                    match self.ping_timer.poll() {
                        Ok(Async::Ready(m)) => {
                            let res = self.inner.start_send("PING".to_string())?;
                            assert!(res.is_ready());
                            self.inner.poll_complete()?;
                        },
                        Ok(Async::NotReady) => {}
                        Err(timer_err) => {println!("{:?}", timer_err);}
                    }
                    
                    return Ok(Async::Ready(Some(m)));
                }
                None => {
                    println!("Disconnected.");

                    let tcp = TcpStream::connect(&self.addr, &self.handle);
                    let mut a = tcp.and_then(|conn|{
                        let framed = conn.framed(LineCodec);
                        mem::replace(&mut self.inner, framed);
                        Ok(())
                    });

                    match a.poll() {
                        Ok(Async::Ready(m)) => {
                            println!("Connected again");
                        }
                        Ok(Async::NotReady) => {
                            println!("NotReady");
                        }
                        Err(_) => {
                            println!("Some errror");
                        }
                    }
                    // let mut t = TcpStream::connect(&self.addr, &self.handle).and_then(|conn|{
                    //     let framed = conn.framed(LineCodec);

                    // });
                    // self.handle.spawn(ok::<(), ()>(()));
                    // let stream = match t.poll() {
                    //     Ok(Async::Ready(t)) => {
                    //         println!("Got connection");
                    //         t
                    //     },
                    //     Ok(Async::NotReady) => {
                    //         println!("NotReady");
                    //         return Ok(Async::NotReady);
                    //     },
                    //     _ => {
                    //         // continue;
                    //         println!("Err");
                    //         return Ok(Async::NotReady);
                    //     }
                    // };
                    // let stream = match ::std::net::TcpStream::connect(&self.addr) {
                    //     Ok(s) => s,
                    //     Err(_) => continue
                    // };

                    // let stream = TcpStream::from_stream(stream, &self.handle);
                    // let stream = try_ready!(stream.into_future().poll());

                    //println!("Connected");
                    //let framed = stream.framed(LineCodec);
                    //mem::replace(&mut self.inner, framed);
                }
            }
        }
    }
}

impl Sink for LineStream {
    type SinkItem = String;
    type SinkError = Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        Ok(self.inner.start_send(item)?)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(self.inner.poll_complete()?)
    }
}