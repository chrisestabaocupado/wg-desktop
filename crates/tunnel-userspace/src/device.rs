use std::collections::VecDeque;

use smoltcp::{
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    time::Instant,
};

const MAX_PACKETS: usize = 256;

/// L3 packets remain in memory. No TUN adapter or OS routes are created.
pub(crate) struct MemoryDevice {
    incoming: VecDeque<Vec<u8>>,
    pub(crate) outgoing: VecDeque<Vec<u8>>,
    mtu: usize,
}

impl MemoryDevice {
    pub(crate) fn new(mtu: u16) -> Self {
        Self {
            incoming: VecDeque::new(),
            outgoing: VecDeque::new(),
            mtu: usize::from(mtu),
        }
    }

    pub(crate) fn receive_packet(&mut self, packet: &[u8]) {
        if self.incoming.len() < MAX_PACKETS && packet.len() <= self.mtu {
            self.incoming.push_back(packet.to_vec());
        }
    }
}

pub(crate) struct Receive(Vec<u8>);
pub(crate) struct Transmit<'a>(&'a mut VecDeque<Vec<u8>>);

impl RxToken for Receive {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.0)
    }
}

impl TxToken for Transmit<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut packet = vec![0; len];
        let result = f(&mut packet);
        self.0.push_back(packet);
        result
    }
}

impl Device for MemoryDevice {
    type RxToken<'a> = Receive;
    type TxToken<'a> = Transmit<'a>;

    fn receive(&mut self, _: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if self.outgoing.len() >= MAX_PACKETS {
            return None;
        }
        let packet = self.incoming.pop_front()?;
        Some((Receive(packet), Transmit(&mut self.outgoing)))
    }

    fn transmit(&mut self, _: Instant) -> Option<Self::TxToken<'_>> {
        (self.outgoing.len() < MAX_PACKETS).then_some(Transmit(&mut self.outgoing))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = self.mtu;
        caps.max_burst_size = Some(64);
        caps
    }
}
