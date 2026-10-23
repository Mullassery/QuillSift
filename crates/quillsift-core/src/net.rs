//! IP address classification for SSRF protection.
//!
//! These helpers are pure and network-free so they can be reused by the HTTP
//! fetcher and, later, the browser adapter. The policy is simple:
//!
//! - Cloud metadata endpoints are **always** blocked.
//! - Broadcast, multicast and unspecified addresses are **always** blocked.
//! - Loopback, private, link-local, unique-local, shared/CGNAT, benchmark,
//!   documentation and reserved ranges are blocked unless
//!   `allow_private_networks` is enabled (trusted deployments only).
//!
//! IPv4-mapped IPv6 addresses are normalized to IPv4 before classification so
//! `::ffff:127.0.0.1` cannot bypass the loopback check.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// How an address is classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressClass {
    /// A globally routable unicast address.
    Public,
    /// RFC 1918 private address.
    Private,
    /// Loopback (`127/8`, `::1`).
    Loopback,
    /// Link-local (`169.254/16`, `fe80::/10`).
    LinkLocal,
    /// IPv6 unique-local (`fc00::/7`).
    UniqueLocal,
    /// Shared address space / CGNAT (`100.64/10`).
    Shared,
    /// Benchmark (`198.18/15`), documentation (`192.0.2/24`, `2001:db8::/32`).
    Reserved,
    /// Multicast or broadcast.
    Multicast,
    /// The unspecified address (`0.0.0.0`, `::`).
    Unspecified,
    /// A known cloud metadata endpoint.
    Metadata,
}

impl AddressClass {
    /// Whether this class is routable on the public internet.
    pub fn is_public(self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Returns the cloud metadata endpoints that must never be reached.
fn is_cloud_metadata_v4(ip: Ipv4Addr) -> bool {
    const IMDS: [Ipv4Addr; 3] = [
        Ipv4Addr::new(169, 254, 169, 254), // AWS/GCP/Azure IMDS
        Ipv4Addr::new(169, 254, 170, 2),   // AWS ECS task metadata
        Ipv4Addr::new(100, 100, 100, 200), // Alibaba Cloud metadata
    ];
    IMDS.contains(&ip)
}

fn is_cloud_metadata_v6(ip: Ipv6Addr) -> bool {
    // AWS IMDS over IPv6.
    ip == Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254)
}

/// Normalizes IPv4-mapped and IPv4-compatible IPv6 addresses to IPv4.
pub fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                // Deprecated IPv4-compatible (::a.b.c.d), excluding :: and ::1.
                if v6.segments()[0..6] == [0, 0, 0, 0, 0, 0]
                    && (v6.segments()[6] != 0 || v6.segments()[7] > 1)
                {
                    IpAddr::V4(v6.to_ipv4().unwrap_or(Ipv4Addr::UNSPECIFIED))
                } else {
                    IpAddr::V6(v6)
                }
            }
        },
        v4 @ IpAddr::V4(_) => v4,
    }
}

fn v4_in(v: u32, net: u32, prefix: u32) -> bool {
    if prefix == 0 {
        return true;
    }
    let mask = u32::MAX << (32 - prefix);
    (v & mask) == (net & mask)
}

fn v6_in(v: u128, net: u128, prefix: u32) -> bool {
    if prefix == 0 {
        return true;
    }
    let mask = u128::MAX << (128 - prefix);
    (v & mask) == (net & mask)
}

fn classify_v4(ip: Ipv4Addr) -> AddressClass {
    if is_cloud_metadata_v4(ip) {
        return AddressClass::Metadata;
    }
    let v = u32::from(ip);
    let net = |a: u8, b: u8, c: u8, d: u8| u32::from(Ipv4Addr::new(a, b, c, d));
    if ip.is_unspecified() {
        AddressClass::Unspecified
    } else if ip.is_loopback() {
        AddressClass::Loopback
    } else if ip.is_multicast() || ip == Ipv4Addr::BROADCAST {
        AddressClass::Multicast
    } else if ip.is_private() {
        AddressClass::Private
    } else if ip.is_link_local() {
        AddressClass::LinkLocal
    } else if v4_in(v, net(100, 64, 0, 0), 10) {
        AddressClass::Shared
    } else if v4_in(v, net(198, 18, 0, 0), 15)
        || v4_in(v, net(192, 0, 2, 0), 24)
        || v4_in(v, net(198, 51, 100, 0), 24)
        || v4_in(v, net(203, 0, 113, 0), 24)
    {
        // Benchmark (198.18/15) and documentation (TEST-NET) ranges.
        AddressClass::Reserved
    } else if v4_in(v, net(240, 0, 0, 0), 4) {
        // 240/4 includes the broadcast address and other reserved space.
        AddressClass::Reserved
    } else {
        AddressClass::Public
    }
}

fn classify_v6(ip: Ipv6Addr) -> AddressClass {
    if is_cloud_metadata_v6(ip) {
        return AddressClass::Metadata;
    }
    let v = u128::from(ip);
    let net = |s: [u16; 8]| u128::from(Ipv6Addr::from(s));
    if ip.is_unspecified() {
        AddressClass::Unspecified
    } else if ip.is_loopback() {
        AddressClass::Loopback
    } else if ip.is_multicast() {
        AddressClass::Multicast
    } else if v6_in(v, net([0xfc00, 0, 0, 0, 0, 0, 0, 0]), 7) {
        AddressClass::UniqueLocal
    } else if v6_in(v, net([0xfe80, 0, 0, 0, 0, 0, 0, 0]), 10) {
        AddressClass::LinkLocal
    } else if v6_in(v, net([0x2001, 0x0db8, 0, 0, 0, 0, 0, 0]), 32) {
        AddressClass::Reserved
    } else {
        AddressClass::Public
    }
}

/// Classifies an address after normalizing IPv4-mapped IPv6 forms.
pub fn classify(ip: IpAddr) -> AddressClass {
    match normalize(ip) {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => classify_v6(v6),
    }
}

/// Returns `true` if the address is a known cloud metadata endpoint.
pub fn is_cloud_metadata(ip: IpAddr) -> bool {
    classify(ip) == AddressClass::Metadata
}

/// Returns the reason the address is blocked, or `None` if it is allowed.
///
/// Broadcast, multicast, unspecified and metadata addresses are always
/// blocked; other non-public classes are blocked unless `allow_private` is set.
pub fn block_reason(ip: IpAddr, allow_private: bool) -> Option<AddressClass> {
    let class = classify(ip);
    match class {
        AddressClass::Public => None,
        // Never valid unicast destinations, regardless of configuration.
        AddressClass::Metadata | AddressClass::Multicast | AddressClass::Unspecified => Some(class),
        _ if allow_private => None,
        other => Some(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn a(s: &str) -> IpAddr {
        IpAddr::from_str(s).unwrap()
    }

    #[test]
    fn public_addresses_are_allowed() {
        assert_eq!(classify(a("1.1.1.1")), AddressClass::Public);
        assert_eq!(classify(a("8.8.8.8")), AddressClass::Public);
        assert_eq!(classify(a("2606:4700:4700::1111")), AddressClass::Public);
        assert!(block_reason(a("1.1.1.1"), false).is_none());
    }

    #[test]
    fn loopback_is_blocked_by_default() {
        assert_eq!(classify(a("127.0.0.1")), AddressClass::Loopback);
        assert_eq!(classify(a("::1")), AddressClass::Loopback);
        assert_eq!(
            block_reason(a("127.0.0.1"), false),
            Some(AddressClass::Loopback)
        );
        assert_eq!(block_reason(a("::1"), false), Some(AddressClass::Loopback));
    }

    #[test]
    fn private_ranges_are_blocked_by_default() {
        assert_eq!(classify(a("10.0.0.5")), AddressClass::Private);
        assert_eq!(classify(a("172.16.4.1")), AddressClass::Private);
        assert_eq!(classify(a("192.168.1.1")), AddressClass::Private);
        assert!(block_reason(a("192.168.1.1"), false).is_some());
    }

    #[test]
    fn shared_and_reserved_ranges_are_blocked() {
        assert_eq!(classify(a("100.64.0.1")), AddressClass::Shared);
        assert_eq!(classify(a("198.18.0.1")), AddressClass::Reserved);
        assert_eq!(classify(a("192.0.2.10")), AddressClass::Reserved);
        assert_eq!(classify(a("2001:db8::1")), AddressClass::Reserved);
    }

    #[test]
    fn unique_local_and_link_local_blocked() {
        assert_eq!(classify(a("fd00::1")), AddressClass::UniqueLocal);
        assert_eq!(classify(a("fe80::1")), AddressClass::LinkLocal);
        assert_eq!(classify(a("169.254.1.1")), AddressClass::LinkLocal);
    }

    #[test]
    fn metadata_is_always_blocked_even_with_private_allowed() {
        assert_eq!(classify(a("169.254.169.254")), AddressClass::Metadata);
        assert_eq!(
            block_reason(a("169.254.169.254"), true),
            Some(AddressClass::Metadata)
        );
        assert_eq!(
            block_reason(a("169.254.169.254"), false),
            Some(AddressClass::Metadata)
        );
        assert_eq!(
            block_reason(a("fd00:ec2::254"), true),
            Some(AddressClass::Metadata)
        );
    }

    #[test]
    fn mapped_ipv6_is_normalized() {
        assert_eq!(normalize(a("::ffff:127.0.0.1")), a("127.0.0.1"));
        assert_eq!(classify(a("::ffff:10.0.0.1")), AddressClass::Private);
        assert_eq!(
            block_reason(a("::ffff:127.0.0.1"), false),
            Some(AddressClass::Loopback)
        );
    }

    #[test]
    fn multicast_unspecified_and_broadcast_always_blocked() {
        assert_eq!(
            block_reason(a("224.0.0.1"), true),
            Some(AddressClass::Multicast)
        );
        assert_eq!(
            block_reason(a("ff02::1"), true),
            Some(AddressClass::Multicast)
        );
        assert_eq!(
            block_reason(a("0.0.0.0"), true),
            Some(AddressClass::Unspecified)
        );
        assert_eq!(
            block_reason(a("255.255.255.255"), true),
            Some(AddressClass::Multicast)
        );
    }

    #[test]
    fn private_allowed_when_explicitly_enabled() {
        assert!(block_reason(a("127.0.0.1"), true).is_none());
        assert!(block_reason(a("192.168.1.1"), true).is_none());
        assert!(block_reason(a("192.168.1.1"), false).is_some());
    }
}
