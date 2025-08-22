pub mod sockets;

use rand::{rng, Rng};
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use url::Url;

pub type PortRange = (u16, u16);

pub const VALIDATOR_PORT_RANGE_STR: &str = "8000-10000";

pub fn parse_host_port(host_port: &str) -> Result<SocketAddr, String> {
    let addrs: Vec<_> = host_port
        .to_socket_addrs()
        .map_err(|err| format!("Unable to resolve host {host_port}: {err}"))?
        .collect();
    if addrs.is_empty() {
        Err(format!("Unable to resolve host: {host_port}"))
    } else {
        Ok(addrs[0])
    }
}

pub fn parse_host(host: &str) -> Result<IpAddr, String> {
    // First, check if the host syntax is valid. This check is needed because addresses
    // such as `("localhost:1234", 0)` will resolve to IPs on some networks.
    let parsed_url = Url::parse(&format!("http://{host}")).map_err(|e| e.to_string())?;
    if parsed_url.port().is_some() {
        return Err(format!("Expected port in URL: {host}"));
    }

    // Next, check to see if it resolves to an IP address
    let ips: Vec<_> = (host, 0)
        .to_socket_addrs()
        .map_err(|err| err.to_string())?
        .map(|socket_address| socket_address.ip())
        .collect();
    if ips.is_empty() {
        Err(format!("Unable to resolve host: {host}"))
    } else {
        Ok(ips[0])
    }
}

pub fn parse_port_range(port_range: &str) -> Result<PortRange, String> {
    let ports: Vec<&str> = port_range.split('-').collect();
    if ports.len() != 2 {
        return Err(format!("Expected range format: {}", "port1-port2"));
    }

    let start_port = ports[0].parse();
    if start_port.is_err() {
        return Err(format!("Error parsing start port: {}", ports[0]));
    }

    let end_port = ports[1].parse();
    if end_port.is_err() {
        return Err(format!("Error parsing end port: {}", ports[1]));
    }

    let start_port = start_port.unwrap();
    let end_port = end_port.unwrap();
    if end_port < start_port {
        return Err(format!(
            "End port {} is less than start port {}",
            ports[1], ports[0]
        ));
    }

    Ok((start_port, end_port))
}

/// Searches for an open port on a given binding ip_addr in the provided range.
///
/// This will start at a random point in the range provided, and search sequenctially.
/// If it can not find anything, an Error is returned.
///
/// Keep in mind this will not reserve the port for you, only find one that is empty.
pub fn find_available_port_in_range(ip_addr: IpAddr, range: PortRange) -> io::Result<u16> {
    let [port] = find_available_ports_in_range(ip_addr, range)?;
    Ok(port)
}

/// Searches for several ports on a given binding ip_addr in the provided range.
///
/// This will start at a random point in the range provided, and search sequentially.
/// If it can not find anything, an Error is returned.
pub fn find_available_ports_in_range<const N: usize>(
    ip_addr: IpAddr,
    range: PortRange,
) -> io::Result<[u16; N]> {
    let mut result = [0u16; N];
    let range = range.0..range.1;
    let mut next_port_to_try = range
        .clone()
        .cycle() // loop over the end of the range
        .skip(rng().random_range(range.clone()) as usize) // skip to random position
        .take(range.len()) // never take the same value twice
        .peekable();
    let mut num = 0;
    let config = sockets::SocketConfiguration::default();
    while num < N {
        let port_to_try = next_port_to_try.next().unwrap(); // this unwrap never fails since we exit earlier
        let bind = sockets::bind_common_with_config(ip_addr, port_to_try, config);
        match bind {
            Ok(_) => {
                result[num] = port_to_try;
                num = num.saturating_add(1);
            }
            Err(err) => {
                if next_port_to_try.peek().is_none() {
                    return Err(err);
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sockets::bind_to;
    use std::net::Ipv4Addr;

    #[test]
    fn test_parse_host_port() {
        parse_host_port("localhost:1234").unwrap();
        parse_host_port("localhost").unwrap_err();
        parse_host_port("127.0.0.0:1234").unwrap();
        parse_host_port("127.0.0.0").unwrap_err();
    }

    #[test]
    fn test_parse_host() {
        parse_host("localhost:1234").unwrap_err();
        parse_host("localhost").unwrap();
        parse_host("127.0.0.0:1234").unwrap_err();
        parse_host("127.0.0.0").unwrap();
    }

    #[test]
    fn test_find_available_port_in_range() {
        let ip_addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let (pr_s, pr_e) = sockets::tests::localhost_port_range_for_tests();
        assert_eq!(
            find_available_port_in_range(ip_addr, (pr_s, pr_s + 1)).unwrap(),
            pr_s
        );
        let port = find_available_port_in_range(ip_addr, (pr_s, pr_e)).unwrap();
        assert!((pr_s..pr_e).contains(&port));

        let _socket = bind_to(ip_addr, port).unwrap();
        find_available_port_in_range(ip_addr, (port, port + 1)).unwrap_err();
    }

    #[test]
    fn test_parse_port_range() {
        assert_eq!(
            parse_port_range("garbage").unwrap_err(),
            "Expected range format: port1-port2",
            "Should fail for garbage input"
        );
        assert_eq!(
            parse_port_range("1-").unwrap_err(),
            "Error parsing end port: ",
            "Should fail for missing end port"
        );
        assert_eq!(
            parse_port_range("1-2").unwrap(),
            (1, 2),
            "Should parse valid range"
        );
        assert_eq!(
            parse_port_range("1-2-3").unwrap_err(),
            "Expected range format: port1-port2",
            "Should fail for extra parts"
        );
        assert_eq!(
            parse_port_range("2-1").unwrap_err(),
            "End port 1 is less than start port 2",
            "Should fail for inverted range"
        );
    }
}
