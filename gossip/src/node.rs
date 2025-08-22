use crate::cluster_info::{NodeConfig, Sockets};
use crate::contact_info::ContactInfo;
use blockchain_net_utils::sockets::bind_gossip_port_in_range;
use log::{info, trace};
use solana_pubkey::Pubkey;
use solana_time_utils::timestamp;
use std::net::SocketAddr;

#[derive(Debug)]
pub struct Node {
    pub info: ContactInfo,
    pub sockets: Sockets,
}

impl Node {
    pub fn new_with_external_ip(pubkey: &Pubkey, config: NodeConfig) -> Node {
        let NodeConfig {
            advertised_ip,
            gossip_port,
            port_range,
            bind_ip_addrs,
        } = config;

        let bind_ip_addr = bind_ip_addrs.primary();

        let gossip_addr = SocketAddr::new(advertised_ip, gossip_port);
        //let (gossip_port, (gossip, ip_echo)) =
        bind_gossip_port_in_range(&gossip_addr, port_range, bind_ip_addr);

        let info = ContactInfo::new(
            *pubkey,
            timestamp(), // wallclock
        );

        trace!("new ContactInfo: {info:?}");
        let sockets = Sockets {};

        info!("Bound all network sockets as follows: {:#?}", &sockets);
        Node { info, sockets }
    }
}
