use crate::PortRange;
#[cfg(test)]
use log::warn;
use socket2::{Domain, SockAddr, Socket, Type};
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};

#[derive(Clone, Copy, Debug, Default)]
pub struct SocketConfiguration {
    reuseport: bool, // controls SO_REUSEPORT, this is not intended to be set explicitly
    recv_buffer_size: Option<usize>,
    send_buffer_size: Option<usize>,
    non_blocking: bool,
}

/// binds both a UdpSocket and a TcpListener on the same port
pub fn bind_common_with_config(
    ip_addr: IpAddr,
    port: u16,
    config: SocketConfiguration,
) -> io::Result<(UdpSocket, TcpListener)> {
    let sock = udp_socket_with_config(config)?;

    let addr = SocketAddr::new(ip_addr, port);
    let sock_addr = SockAddr::from(addr);
    sock.bind(&sock_addr)
        .and_then(|_| TcpListener::bind(addr).map(|listener| (sock.into(), listener)))
}

pub fn bind_gossip_port_in_range(
    gossip_addr: &SocketAddr,
    port_range: PortRange,
    bind_ip_addr: IpAddr,
) -> (u16, (UdpSocket, TcpListener)) {
    let config = SocketConfiguration::default();
    if gossip_addr.port() != 0 {
        (
            gossip_addr.port(),
            bind_common_with_config(bind_ip_addr, gossip_addr.port(), config).unwrap_or_else(|e| {
                panic!("gossip_addr bind_to port {}: {}", gossip_addr.port(), e)
            }),
        )
    } else {
        bind_common_in_range_with_config(bind_ip_addr, port_range, config).expect("Failed to bind")
    }
}

/// Find a port in the given range with a socket config that is available for both TCP and UDP
pub fn bind_common_in_range_with_config(
    ip_addr: IpAddr,
    range: PortRange,
    config: SocketConfiguration,
) -> io::Result<(u16, (UdpSocket, TcpListener))> {
    for port in range.0..range.1 {
        if let Ok((sock, listener)) = bind_common_with_config(ip_addr, port, config) {
            return Result::Ok((sock.local_addr().unwrap().port(), (sock, listener)));
        }
    }

    Err(io::Error::other(format!(
        "No available TCP/UDP ports in {range:?}"
    )))
}

/// True on platforms that support advanced socket configuration
pub(crate) const PLATFORM_SUPPORTS_SOCKET_CONFIGS: bool =
    cfg!(not(any(windows, target_os = "ios")));

/// Sets SO_REUSEPORT on platforms that support it.
#[cfg(not(any(windows, target_os = "ios")))]
fn set_reuse_port<T>(socket: &T) -> io::Result<()>
where
    T: std::os::fd::AsFd,
{
    use nix::sys::socket::{setsockopt, sockopt::ReusePort};
    setsockopt(socket, ReusePort, &true).map_err(io::Error::from)
}

pub(crate) fn udp_socket_with_config(config: SocketConfiguration) -> io::Result<Socket> {
    let SocketConfiguration {
        reuseport,
        recv_buffer_size,
        send_buffer_size,
        non_blocking,
    } = config;
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
    if PLATFORM_SUPPORTS_SOCKET_CONFIGS {
        // Set buffer sizes
        if let Some(recv_buffer_size) = recv_buffer_size {
            sock.set_recv_buffer_size(recv_buffer_size)?;
        }
        if let Some(send_buffer_size) = send_buffer_size {
            sock.set_send_buffer_size(send_buffer_size)?;
        }

        if reuseport {
            set_reuse_port(&sock)?;
        }
    }
    sock.set_nonblocking(non_blocking)?;
    Ok(sock)
}

#[cfg(test)]
pub(crate) fn bind_to(ip_addr: IpAddr, port: u16) -> io::Result<UdpSocket> {
    let config = SocketConfiguration {
        ..Default::default()
    };
    bind_to_with_config(ip_addr, port, config)
}

#[cfg(test)]
fn bind_to_with_config(
    ip_addr: IpAddr,
    port: u16,
    config: SocketConfiguration,
) -> io::Result<UdpSocket> {
    let sock = udp_socket_with_config(config)?;

    let addr = SocketAddr::new(ip_addr, port);

    sock.bind(&SockAddr::from(addr)).map(|_| sock.into())
}

#[cfg(test)]
pub(crate) fn bind_in_range(ip_addr: IpAddr, range: PortRange) -> io::Result<(u16, UdpSocket)> {
    let config = SocketConfiguration::default();
    bind_in_range_with_config(ip_addr, range, config)
}

#[cfg(test)]
pub(crate) fn bind_in_range_with_config(
    ip_addr: IpAddr,
    range: PortRange,
    config: SocketConfiguration,
) -> io::Result<(u16, UdpSocket)> {
    let socket = udp_socket_with_config(config)?;

    for port in range.0..range.1 {
        let addr = SocketAddr::new(ip_addr, port);

        if socket.bind(&SockAddr::from(addr)).is_ok() {
            let udp_socket: UdpSocket = socket.into();
            return Ok((udp_socket.local_addr()?.port(), udp_socket));
        }
    }

    Err(io::Error::other(format!(
        "No available UDP ports in {range:?}"
    )))
}

#[cfg(test)]
pub fn bind_more_with_config(
    socket: UdpSocket,
    num: usize,
    mut config: SocketConfiguration,
) -> io::Result<Vec<UdpSocket>> {
    if !PLATFORM_SUPPORTS_SOCKET_CONFIGS {
        if num > 1 {
            warn!(
                "bind_more_with_config() only supports 1 socket on this platform ({num} requested)"
            );
        }
        Ok(vec![socket])
    } else {
        set_reuse_port(&socket)?;
        config.reuseport = true;
        let addr = socket.local_addr()?;
        let ip = addr.ip();
        let port = addr.port();
        std::iter::once(Ok(socket))
            .chain((1..num).map(|_| bind_to_with_config(ip, port, config)))
            .collect()
    }
}

/// binds num sockets to the same port in a range with config
#[cfg(test)]
pub fn multi_bind_in_range_with_config(
    ip_addr: IpAddr,
    range: PortRange,
    config: SocketConfiguration,
    mut num: usize,
) -> io::Result<(u16, Vec<UdpSocket>)> {
    if !PLATFORM_SUPPORTS_SOCKET_CONFIGS && num != 1 {
        // See https://github.com/solana-labs/solana/issues/4607
        warn!(
            "multi_bind_in_range_with_config() only supports 1 socket on this platform ({num} \
             requested)"
        );
        num = 1;
    }
    let (port, socket) = bind_in_range_with_config(ip_addr, range, config)?;
    let sockets = bind_more_with_config(socket, num, config)?;
    Ok((port, sockets))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::ops::Range;
    use std::sync::atomic::{AtomicU16, Ordering};
    use {super::*, std::net::Ipv4Addr};

    // base port for deconflicted allocations
    const BASE_PORT: u16 = 5000;
    // how much to allocate per individual process.
    // we expect to have at most 64 concurrent tests in CI at any moment on a given host.
    const SLICE_PER_PROCESS: u16 = (u16::MAX - BASE_PORT) / 64;
    /// When running under nextest, this will try to provide
    /// a unique slice of port numbers (assuming no other nextest processes
    /// are running on the same host) based on NEXTEST_TEST_GLOBAL_SLOT variable
    /// The port ranges will be reused following nextest logic.
    ///
    /// When running without nextest, this will only bump an atomic and eventually
    /// panic when it runs out of port numbers to assign.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn unique_port_range_for_tests(size: u16) -> Range<u16> {
        static SLICE: AtomicU16 = AtomicU16::new(0);
        let offset = SLICE.fetch_add(size, Ordering::Relaxed);
        let start = offset
            + match std::env::var("NEXTEST_TEST_GLOBAL_SLOT") {
                Ok(slot) => {
                    let slot: u16 = slot.parse().unwrap();
                    assert!(
                offset < SLICE_PER_PROCESS,
                "Overrunning into the port range of another test! Consider using fewer ports \
                     per test."
            );
                    BASE_PORT + slot * SLICE_PER_PROCESS
                }
                Err(_) => BASE_PORT,
            };
        assert!(start < u16::MAX - size, "Ran out of port numbers!");
        start..start + size
    }

    /// Retrieve a free 20-port slice for unit tests
    ///
    /// When running under nextest, this will try to provide
    /// a unique slice of port numbers (assuming no other nextest processes
    /// are running on the same host) based on NEXTEST_TEST_GLOBAL_SLOT variable
    /// The port ranges will be reused following nextest logic.
    ///
    /// When running without nextest, this will only bump an atomic and eventually
    /// panic when it runs out of port numbers to assign.
    pub fn localhost_port_range_for_tests() -> (u16, u16) {
        let pr = unique_port_range_for_tests(20);
        (pr.start, pr.end)
    }

    #[test]
    fn test_bind() {
        let (pr_s, pr_e) = localhost_port_range_for_tests();
        let ip_addr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        let config = SocketConfiguration::default();
        let s = bind_in_range(ip_addr, (pr_s, pr_e)).unwrap();
        assert_eq!(s.0, pr_s, "bind_in_range should use first available port");
        let ip_addr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        let x = bind_to_with_config(ip_addr, pr_s + 1, config).unwrap();
        let y = bind_more_with_config(x, 2, config).unwrap();
        assert_eq!(
            y[0].local_addr().unwrap().port(),
            y[1].local_addr().unwrap().port()
        );
        bind_to_with_config(ip_addr, pr_s, SocketConfiguration::default()).unwrap_err();
        bind_in_range(ip_addr, (pr_s, pr_s + 2)).unwrap_err();

        let (port, v) =
            multi_bind_in_range_with_config(ip_addr, (pr_s + 5, pr_e), config, 10).unwrap();
        for sock in &v {
            assert_eq!(port, sock.local_addr().unwrap().port());
        }
    }
}
