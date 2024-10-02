extern crate nix;
use feos::daemon::start_feos;
use std::env;
use std::net::Ipv6Addr;
use std::str::FromStr;
use std::{env::args, ffi::CString};
use feos::move_root::{get_root_fstype, move_root};
use nix::unistd::execv;

use tokio::io;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut ipv6_address = Ipv6Addr::UNSPECIFIED;
    let mut prefix_length = 64;

    if std::process::id() == 1 {
        let root_fstype = get_root_fstype().unwrap_or_default();
        if root_fstype == "rootfs" {
            move_root().map_err(|e| format!("move_root: {}", e))?;

            let argv: Vec<CString> = args()
                .map(|arg| CString::new(arg).unwrap_or_default())
                .collect();
            execv(&argv[0], &argv).map_err(|e| format!("execv: {}", e))?;
        }
    } else {
        (ipv6_address, prefix_length) = parse_command_line()?;
    }

    let test_mode = env::var("RUN_MODE").map_or(false, |v| v == "test");

    start_feos(ipv6_address, prefix_length, test_mode).await?;
    Err("FeOS exited".to_string())
}

fn parse_command_line() -> Result<(Ipv6Addr, u8), String> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        return Err("Usage: <program> --ipam <IPv6>/<prefix-length>".into());
    }

    if args[1] != "--ipam" {
        return Err("Expected '--ipam' flag".into());
    }

    let prefix_input = &args[2];
    let parts: Vec<&str> = prefix_input.split('/').collect();

    if parts.len() != 2 {
        return Err("Invalid IPv6 prefix format. Use <IPv6>/<prefix-length>".into());
    }

    let ipv6_address =
        Ipv6Addr::from_str(parts[0]).map_err(|_| "Invalid IPv6 address".to_string())?;

    let prefix_length: u8 = parts[1]
        .parse()
        .map_err(|_| "Invalid prefix length".to_string())?;

    if prefix_length > 128 {
        return Err("Prefix length must be between 0 and 128".into());
    }

    Ok((ipv6_address, prefix_length))
}

async fn _read_and_echo() {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await.unwrap();

        // If no bytes were read, it means EOF has been reached
        if bytes_read == 0 {
            break;
        }

        // Trim the newline character
        let trimmed_line = line.trim_end();

        println!("this is echoed \"{}\"", trimmed_line);
    }
}
