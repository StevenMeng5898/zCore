use super::*;
use crate::object::*;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Signaling and mailbox primitive
///
/// ## SYNOPSIS
///
/// Ports allow threads to wait for packets to be delivered from various
/// events. These events include explicit queueing on the port,
/// asynchronous waits on other handles bound to the port, and
/// asynchronous message delivery from IPC transports.
pub struct Port {
    base: KObjectBase,
    inner: Mutex<PortInner>,
}

impl_kobject!(Port);

#[derive(Default)]
struct PortInner {
    queue: Vec<PortPacket>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PortPacket {
    pub key: u64,
    pub status: ZxError,
    pub data: PortPacketPayload,
}

#[non_exhaustive]
#[derive(Debug, Eq, PartialEq)]
pub enum PortPacketPayload {
    Signal(Signal),
    User([u8; 32]),
}

impl Port {
    /// Create a new `Port`.
    pub fn new() -> Arc<Self> {
        Arc::new(Port {
            base: KObjectBase::default(),
            inner: Mutex::default(),
        })
    }

    /// Push a `packet` into the port.
    pub fn push(&self, packet: PortPacket) {
        let mut inner = self.inner.lock();
        inner.queue.push(packet);
        drop(inner);
        self.base.signal_set(Signal::READABLE);
    }

    /// Asynchronous wait until at least one packet is available, then take out all packets.
    pub async fn wait_async(self: &Arc<Self>) -> Vec<PortPacket> {
        (self.clone() as Arc<dyn KernelObject>)
            .wait_signal_async(Signal::READABLE)
            .await;
        let mut inner = self.inner.lock();
        self.base.signal_clear(Signal::READABLE);
        core::mem::take(&mut inner.queue)
    }

    /// Get the number of packets in queue.
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.inner.lock().queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[async_std::test]
    async fn wait_async() {
        let port = Port::new();
        let object = DummyObject::new() as Arc<dyn KernelObject>;
        object.send_signal_to_port_async(Signal::READABLE, &port, 1);

        async_std::task::spawn({
            let port = port.clone();
            let object = object.clone();
            async move {
                object.signal_set(Signal::READABLE);
                async_std::task::sleep(Duration::from_millis(1)).await;

                port.push(PortPacket {
                    key: 2,
                    status: ZxError::OK,
                    data: PortPacketPayload::Signal(Signal::WRITABLE),
                });
            }
        });

        let packets = port.wait_async().await;
        assert_eq!(
            packets,
            [PortPacket {
                key: 1,
                status: ZxError::OK,
                data: PortPacketPayload::Signal(Signal::READABLE),
            }]
        );

        let packets = port.wait_async().await;
        assert_eq!(
            packets,
            [PortPacket {
                key: 2,
                status: ZxError::OK,
                data: PortPacketPayload::Signal(Signal::WRITABLE),
            }]
        );
    }
}
