//! `port-info-search` — it-tools PR #1029.
//!
//! A reference table, so it renders in the reference pane like `git-memo` and
//! `http-status-codes` do: markdown, one row per port.

use crate::*;
use anyhow::Result;
use std::fmt::Write as _;

/// `(port, protocol, service, what it is)` — the IANA well-known list trimmed
/// to what actually turns up in a firewall rule or a `docker ps`.
#[rustfmt::skip]
pub(crate) const PORTS: &[(u16, &str, &str, &str)] = &[
    (20,    "TCP",     "ftp-data",   "FTP data transfer"),
    (21,    "TCP",     "ftp",        "FTP control"),
    (22,    "TCP",     "ssh",        "SSH, SCP, SFTP and git over SSH"),
    (23,    "TCP",     "telnet",     "Telnet, unencrypted"),
    (25,    "TCP",     "smtp",       "Mail submission between servers"),
    (53,    "TCP/UDP", "domain",     "DNS"),
    (67,    "UDP",     "bootps",     "DHCP server"),
    (68,    "UDP",     "bootpc",     "DHCP client"),
    (69,    "UDP",     "tftp",       "Trivial FTP, netboot"),
    (80,    "TCP",     "http",       "HTTP"),
    (88,    "TCP/UDP", "kerberos",   "Kerberos authentication"),
    (110,   "TCP",     "pop3",       "POP3 mail retrieval"),
    (111,   "TCP/UDP", "rpcbind",    "ONC RPC portmapper, NFS"),
    (123,   "UDP",     "ntp",        "Network time"),
    (135,   "TCP",     "msrpc",      "Microsoft RPC endpoint mapper"),
    (137,   "UDP",     "netbios-ns", "NetBIOS name service"),
    (139,   "TCP",     "netbios-ssn","NetBIOS session, SMB over NetBIOS"),
    (143,   "TCP",     "imap",       "IMAP mail retrieval"),
    (161,   "UDP",     "snmp",       "SNMP polling"),
    (162,   "UDP",     "snmptrap",   "SNMP traps"),
    (389,   "TCP",     "ldap",       "LDAP directory"),
    (443,   "TCP",     "https",      "HTTP over TLS, and HTTP/3 over UDP"),
    (445,   "TCP",     "microsoft-ds","SMB, Windows file sharing"),
    (465,   "TCP",     "smtps",      "SMTP over implicit TLS"),
    (500,   "UDP",     "isakmp",     "IPsec IKE"),
    (514,   "UDP",     "syslog",     "Syslog"),
    (587,   "TCP",     "submission", "Mail submission, STARTTLS"),
    (631,   "TCP",     "ipp",        "Internet printing, CUPS"),
    (636,   "TCP",     "ldaps",      "LDAP over TLS"),
    (873,   "TCP",     "rsync",      "rsync daemon"),
    (993,   "TCP",     "imaps",      "IMAP over TLS"),
    (995,   "TCP",     "pop3s",      "POP3 over TLS"),
    (1080,  "TCP",     "socks",      "SOCKS proxy"),
    (1194,  "UDP",     "openvpn",    "OpenVPN"),
    (1433,  "TCP",     "ms-sql-s",   "Microsoft SQL Server"),
    (1521,  "TCP",     "oracle",     "Oracle database listener"),
    (1723,  "TCP",     "pptp",       "PPTP VPN"),
    (1883,  "TCP",     "mqtt",       "MQTT"),
    (2049,  "TCP",     "nfs",        "NFS"),
    (2375,  "TCP",     "docker",     "Docker daemon, unencrypted"),
    (2376,  "TCP",     "docker-s",   "Docker daemon over TLS"),
    (2379,  "TCP",     "etcd",       "etcd client API"),
    (3000,  "TCP",     "dev",        "Node, Rails and Grafana by convention"),
    (3128,  "TCP",     "squid",      "Squid proxy"),
    (3306,  "TCP",     "mysql",      "MySQL and MariaDB"),
    (3389,  "TCP",     "rdp",        "Windows Remote Desktop"),
    (4369,  "TCP",     "epmd",       "Erlang port mapper, RabbitMQ clustering"),
    (5000,  "TCP",     "dev",        "Flask, and the container registry"),
    (5432,  "TCP",     "postgresql", "PostgreSQL"),
    (5601,  "TCP",     "kibana",     "Kibana"),
    (5672,  "TCP",     "amqp",       "RabbitMQ / AMQP"),
    (5900,  "TCP",     "vnc",        "VNC remote desktop"),
    (6379,  "TCP",     "redis",      "Redis"),
    (6443,  "TCP",     "kube-api",   "Kubernetes API server"),
    (8000,  "TCP",     "dev",        "Django and friends"),
    (8080,  "TCP",     "http-alt",   "HTTP alternate, Tomcat, proxies"),
    (8086,  "TCP",     "influxdb",   "InfluxDB"),
    (8443,  "TCP",     "https-alt",  "HTTPS alternate"),
    (8883,  "TCP",     "mqtts",      "MQTT over TLS"),
    (9000,  "TCP",     "php-fpm",    "PHP-FPM, MinIO, SonarQube"),
    (9042,  "TCP",     "cassandra",  "Cassandra CQL"),
    (9090,  "TCP",     "prometheus", "Prometheus"),
    (9092,  "TCP",     "kafka",      "Apache Kafka"),
    (9200,  "TCP",     "elasticsearch","Elasticsearch HTTP API"),
    (11211, "TCP/UDP", "memcached",  "Memcached"),
    (15672, "TCP",     "rabbitmq",   "RabbitMQ management UI"),
    (25565, "TCP",     "minecraft",  "Minecraft server"),
    (27017, "TCP",     "mongodb",    "MongoDB"),
];

fn port_info(args: &Args) -> Result<String> {
    let q = arg(args, tool("port-info-search"), "query")
        .trim()
        .to_lowercase();
    let hits: Vec<_> = PORTS
        .iter()
        .filter(|(port, proto, service, desc)| {
            q.is_empty()
                || port.to_string() == q
                || service.contains(&q)
                || desc.to_lowercase().contains(&q)
                || proto.to_lowercase().contains(&q)
        })
        .collect();
    if hits.is_empty() {
        return Ok(format!(
            "## Search results\n\nNo well-known port matches {q:?}.\n"
        ));
    }
    let mut out = String::from(
        "## Well-known ports\n\nPort | Protocol | Service | What runs there\n:--|:--|:--|:--\n",
    );
    for (port, proto, service, desc) in hits {
        let _ = writeln!(out, "`{port}` | {proto} | `{service}` | {desc}");
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "port-info-search",
    name: "Port lookup",
    description: "Which service sits behind a port number, and which port a service wants.",
    category: Category::Network,
    keywords: &[
        "port",
        "service",
        "iana",
        "firewall",
        "well-known",
        "tcp",
        "udp",
    ],
    fields: &[Field::text(
        "query",
        "Search a port, service or description",
    )],
    kind: Kind::Pure,
    run: port_info,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn finds_a_port_by_number_and_by_name() {
        let out = ok("port-info-search", &[("query", "5432")]);
        assert!(out.contains("PostgreSQL"), "{out}");
        // One row per hit, so an exact number does not drag in 54320.
        assert_eq!(
            out.lines().filter(|l| l.starts_with('`')).count(),
            1,
            "{out}"
        );
        assert!(ok("port-info-search", &[("query", "redis")]).contains("6379"));
        assert!(ok("port-info-search", &[("query", "kubernetes")]).contains("6443"));
        // An empty query is the whole table.
        assert!(ok("port-info-search", &[]).lines().count() > PORTS.len());
        assert!(ok("port-info-search", &[("query", "zzz")]).contains("No well-known port"));
    }
}
