use russh::client::{self, Msg, Session};
use russh::keys::PublicKey;
use russh::Channel;
use tokio::sync::mpsc;
use tracing::debug;

use crate::tunnel::ForwardedChannel;

/// SSH client handler that implements the russh callback interface.
///
/// For MVP, this handler accepts all server host keys.
/// TODO: Implement known_hosts checking.
///
/// Supports remote port forwarding by forwarding incoming channels
/// through an mpsc channel.
#[derive(Debug)]
pub struct ShellyHandler {
    /// Sender for forwarded TCP/IP channels (remote port forwarding).
    /// If None, forwarded channels are rejected.
    pub(crate) forwarded_tx: Option<mpsc::UnboundedSender<ForwardedChannel>>,
}

impl ShellyHandler {
    /// Create a new handler without remote forwarding support.
    pub fn new() -> Self {
        Self {
            forwarded_tx: None,
        }
    }

    /// Create a new handler with remote forwarding support.
    /// Returns the handler and a receiver for forwarded channels.
    pub fn with_forwarding() -> (Self, mpsc::UnboundedReceiver<ForwardedChannel>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                forwarded_tx: Some(tx),
            },
            rx,
        )
    }
}

impl Default for ShellyHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl client::Handler for ShellyHandler {
    type Error = ShellyHandlerError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        // TODO: check against known_hosts file
        debug!(
            key_type = %server_public_key.algorithm(),
            "accepting server host key (known_hosts check not yet implemented)"
        );
        Ok(true)
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(
            connected = %connected_address,
            connected_port,
            originator = %originator_address,
            originator_port,
            "forwarded channel opened by server"
        );

        if let Some(ref tx) = self.forwarded_tx {
            let fwd = ForwardedChannel {
                channel,
                connected_address: connected_address.to_string(),
                connected_port,
                originator_address: originator_address.to_string(),
                originator_port,
            };
            if tx.send(fwd).is_err() {
                debug!("forwarded channel receiver dropped, channel will be closed");
            }
        } else {
            debug!("no forwarding receiver configured, dropping channel");
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellyHandlerError {
    #[error("SSH protocol error: {0}")]
    Russh(#[from] russh::Error),
}
