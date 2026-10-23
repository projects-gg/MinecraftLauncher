//! A "UDP socket" for quinn whose datagrams travel through a TURN allocation: what quinn sends to a
//! peer's relay address goes out as ChannelData on that peer's channel, and what the peer sends
//! comes back from its relay address. Quinn never sees a real address of anyone.

use std::fmt;
use std::io::{self, IoSliceMut};
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};
use tokio::sync::mpsc;

use super::client::{Datagram, Shared};
use crate::transport::quic::{self, AlwaysWritable, WritablePoller};

pub(crate) struct TurnSocket {
    shared: Arc<Shared>,
    datagrams: Mutex<mpsc::Receiver<Datagram>>,
}

impl TurnSocket {
    /// The socket of an allocation (one per allocation: it takes the datagram queue).
    pub(crate) fn new(shared: Arc<Shared>) -> Option<Self> {
        let datagrams = shared.take_datagrams()?;
        Some(Self {
            shared,
            datagrams: Mutex::new(datagrams),
        })
    }
}

impl fmt::Debug for TurnSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TurnSocket")
    }
}

impl AsyncUdpSocket for TurnSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        match self.shared.writable_socket() {
            Some(socket) => Box::pin(WritablePoller::new(move || {
                let socket = socket.clone();
                Box::pin(async move { socket.writable().await })
            })),
            None => Box::pin(AlwaysWritable),
        }
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        match transmit.destination {
            SocketAddr::V4(to) => self.shared.send_datagram(to, transmit.contents),
            // Relay addresses are IPv4; anything else has no channel.
            SocketAddr::V6(_) => Ok(()),
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let mut datagrams = self
            .datagrams
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let room = bufs.len().min(meta.len());
        let mut count = 0;
        while count < room {
            let datagram = if count == 0 {
                match datagrams.poll_recv(cx) {
                    Poll::Ready(Some(datagram)) => datagram,
                    // Closed: the allocation is gone and its paths close the endpoint.
                    Poll::Ready(None) | Poll::Pending => return Poll::Pending,
                }
            } else {
                match datagrams.try_recv() {
                    Ok(datagram) => datagram,
                    Err(_) => break,
                }
            };
            let buf = &mut bufs[count];
            let len = datagram.data.len().min(buf.len());
            buf[..len].copy_from_slice(&datagram.data[..len]);
            meta[count] = quic::recv_meta(SocketAddr::V4(datagram.from), len);
            count += 1;
        }
        Poll::Ready(Ok(count))
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        // Not a real socket; quinn only needs the family.
        Ok(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))
    }
}
