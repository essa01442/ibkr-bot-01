use std::os::unix::net::UnixDatagram;
use core_types::OmsCommand;

pub struct BridgeCmdSender {
    socket: UnixDatagram,
    target_path: String,
}

impl BridgeCmdSender {
    pub fn new(target_path: &str) -> std::io::Result<Self> {
        let socket = UnixDatagram::unbound()?;
        // Increase buffer sizes if necessary
        Ok(Self {
            socket,
            target_path: target_path.to_string(),
        })
    }

    pub fn send_command(&self, cmd: &OmsCommand) -> std::io::Result<()> {
        let payload = rmp_serde::to_vec_named(cmd)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.socket.send_to(&payload, &self.target_path)?;
        Ok(())
    }
}
