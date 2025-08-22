use blockchain_net_utils::PortRange;
use std::net::IpAddr;
use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct BindIpAddrs {
    /// The IP addresses this node may bind to
    /// Index 0 is the primary address
    /// Index 1+ are secondary addresses
    addrs: Vec<IpAddr>,
}

impl BindIpAddrs {
    pub fn new(addrs: Vec<IpAddr>) -> Result<Self, String> {
        if addrs.is_empty() {
            return Err(
                "BindIpAddrs requires at least one IP address (--bind-address)".to_string(),
            );
        }
        if addrs.len() > 1 {
            for &ip in &addrs {
                if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
                    return Err(format!(
                        "Invalid configuration: {ip:?} is not allowed with multiple \
                         --bind-address values (loopback, unspecified, or multicast)"
                    ));
                }
            }
        }

        Ok(Self { addrs })
    }

    #[inline]
    pub fn primary(&self) -> IpAddr {
        self.addrs[0]
    }
}

// Makes BindIpAddrs behave like &[IpAddr]
impl Deref for BindIpAddrs {
    type Target = [IpAddr];

    fn deref(&self) -> &Self::Target {
        &self.addrs
    }
}

// For generic APIs expecting something like AsRef<[IpAddr]>
impl AsRef<[IpAddr]> for BindIpAddrs {
    fn as_ref(&self) -> &[IpAddr] {
        &self.addrs
    }
}

pub struct NodeConfig {
    /// The IP address advertised to the cluster in gossip
    pub advertised_ip: IpAddr,
    /// The gossip port advertised to the cluster
    pub gossip_port: u16,
    pub port_range: PortRange,
    /// Multihoming: The IP addresses the node can bind to
    pub bind_ip_addrs: BindIpAddrs,
}

#[derive(Debug)]
pub struct Sockets {}
