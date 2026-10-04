use core::net::{IpAddr, SocketAddr};
use embassy_net::{IpEndpoint, udp::UdpSocket};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::transport::network::{Address, NetworkReceive, NetworkSend};
#[derive(Clone, Copy)]
pub struct MatterUdp<'s, 'd>(pub &'s UdpSocket<'d>);
impl NetworkSend for MatterUdp<'_, '_> {
    async fn send_to(&mut self, data: &[u8], address: Address) -> Result<(), Error> {
        let socket = address.udp().ok_or(ErrorCode::NoNetworkInterface)?;
        self.0.send_to(data, IpEndpoint::new(socket.ip().into(), socket.port())).await.map_err(|_| ErrorCode::NoNetworkInterface.into())
    }
}
impl NetworkReceive for MatterUdp<'_, '_> {
    async fn wait_available(&mut self) -> Result<(), Error> {
        self.0.wait_recv_ready().await;
        Ok(())
    }
    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, Address), Error> {
        let (size, metadata) = self.0.recv_from(buffer).await.map_err(|_| ErrorCode::InvalidData)?;
        let ip: IpAddr = metadata.endpoint.addr.into();
        Ok((size, Address::Udp(SocketAddr::new(ip, metadata.endpoint.port))))
    }
}
