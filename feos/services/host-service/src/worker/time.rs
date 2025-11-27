// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Local, TimeZone};
use log::{error, info, warn};
use sntpc::{NtpContext, StdTimestampGen};
use std::net::{Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::sleep;

const FALLBACK_NTP_SERVER: &str = "pool.ntp.org";
const SYNC_INTERVAL: Duration = Duration::from_secs(86400); // 24 Hours
const RETRY_INTERVAL: Duration = Duration::from_secs(300); // 5 Minutes
const NTP_PORT: u16 = 123;

pub struct TimeSyncWorker {
    ntp_servers: Vec<Ipv6Addr>,
}

impl TimeSyncWorker {
    pub fn new(ntp_servers: Vec<Ipv6Addr>) -> Self {
        Self { ntp_servers }
    }

    pub async fn run(self) {
        info!("TimeSyncWorker: Started.");

        // Initial sync
        self.perform_sync_loop().await;
    }

    async fn perform_sync_loop(&self) {
        loop {
            match self.synchronize_time().await {
                Ok(_) => {
                    info!(
                        "TimeSyncWorker: Time synchronization successful. Sleeping for 24 hours."
                    );
                    sleep(SYNC_INTERVAL).await;
                }
                Err(e) => {
                    error!(
                        "TimeSyncWorker: Time synchronization failed: {e}. Retrying in 5 minutes."
                    );
                    sleep(RETRY_INTERVAL).await;
                }
            }
        }
    }

    async fn synchronize_time(&self) -> Result<(), String> {
        let socket = UdpSocket::bind("[::]:0")
            .await
            .map_err(|e| format!("Failed to bind UDP socket: {e}"))?;

        for server_ip in &self.ntp_servers {
            info!("TimeSyncWorker: Attempting sync with DHCPv6 server: {server_ip}");
            let target = SocketAddr::from((*server_ip, NTP_PORT));

            match self.query_ntp_server(&socket, target, server_ip).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    warn!("TimeSyncWorker: Failed to sync with {server_ip}: {e}");
                }
            }
        }

        info!("TimeSyncWorker: Attempting sync with fallback server: {FALLBACK_NTP_SERVER}");

        match self.resolve_and_sync(&socket, FALLBACK_NTP_SERVER).await {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to sync with fallback server: {e}")),
        }
    }

    async fn resolve_and_sync(&self, socket: &UdpSocket, hostname: &str) -> Result<(), String> {
        use tokio::net::lookup_host;

        let server_with_port = format!("{hostname}:{NTP_PORT}");
        let mut addrs = lookup_host(&server_with_port)
            .await
            .map_err(|e| format!("Failed to resolve {hostname}: {e}"))?;

        let target = addrs
            .find(|addr| addr.is_ipv6())
            .or_else(|| {
                warn!("TimeSyncWorker: No IPv6 address found for {hostname}, trying IPv4");
                None
            })
            .ok_or_else(|| format!("No suitable address found for {hostname}"))?;

        info!("TimeSyncWorker: Resolved {hostname} to {target}");

        let server_ip = match target.ip() {
            std::net::IpAddr::V6(ip) => ip,
            std::net::IpAddr::V4(_) => {
                return Err(format!(
                    "Only IPv6 addresses are supported, got IPv4 for {hostname}"
                ));
            }
        };

        self.query_ntp_server(socket, target, &server_ip).await
    }

    async fn query_ntp_server(
        &self,
        socket: &UdpSocket,
        target: SocketAddr,
        server_ip: &Ipv6Addr,
    ) -> Result<(), String> {
        let context = NtpContext::new(StdTimestampGen::default());

        let result = sntpc::get_time(target, socket, context)
            .await
            .map_err(|e| format!("NTP query failed: {e:?}"))?;

        let seconds = result.sec();
        let sec_fraction = result.sec_fraction();

        let nanoseconds = sntpc::fraction_to_nanoseconds(sec_fraction);
        let server_time = Local
            .timestamp_opt(seconds as i64, nanoseconds)
            .single()
            .ok_or_else(|| "Failed to convert NTP time to local time".to_string())?;

        let offset_sec = result.offset() as f64 / 1_000_000.0;
        let delay_sec = result.roundtrip() as f64 / 1_000_000.0;

        info!(
            "TimeSyncWorker: {} ({:+.6}s offset) +/- {:.6}s delay, server: {} stratum: {}",
            server_time.format("%Y-%m-%d %H:%M:%S%.6f"),
            offset_sec,
            delay_sec,
            server_ip,
            result.stratum()
        );

        self.set_system_time(server_time)
            .map_err(|e| format!("Failed to set system time: {e}"))?;

        Ok(())
    }

    fn set_system_time(&self, dt: DateTime<Local>) -> Result<(), std::io::Error> {
        use libc::{clock_settime, timespec, CLOCK_REALTIME};

        let ts = timespec {
            tv_sec: dt.timestamp(),
            tv_nsec: dt.timestamp_subsec_nanos() as _,
        };

        let ret = unsafe { clock_settime(CLOCK_REALTIME, &ts) };

        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }

        info!("TimeSyncWorker: System clock updated to: {dt}");
        Ok(())
    }
}
