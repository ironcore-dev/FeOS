use log::{info, warn};
use nix::mount::{mount, MsFlags};
use std::io;
use tokio::fs;

const HUGEPAGE_FS_TYPE: &[u8] = b"hugetlbfs";
const HUGEPAGE_MOUNT_POINT: &str = "/dev/hugepages";

pub async fn configure_hugepages(num_pages: u32) -> io::Result<()> {
    let nr_hugepages_path = "/sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages";

    info!("Attempting to allocate {num_pages} hugepages...");
    fs::write(nr_hugepages_path, num_pages.to_string()).await?;
    info!("Successfully wrote to {nr_hugepages_path}");

    let allocated_pages_str = fs::read_to_string(nr_hugepages_path).await?;
    let allocated_pages = allocated_pages_str.trim().parse::<u32>().unwrap_or(0);

    if allocated_pages < num_pages {
        warn!(
            "System only allocated {allocated_pages} of the requested {num_pages} hugepages. This might happen due to memory fragmentation."
        );
    } else {
        info!("System successfully allocated {allocated_pages} hugepages.");
    }

    if !is_mounted(HUGEPAGE_MOUNT_POINT).await {
        info!("Mounting hugetlbfs at {HUGEPAGE_MOUNT_POINT}...");
        fs::create_dir_all(HUGEPAGE_MOUNT_POINT).await?;
        mount_hugetlbfs()?;
        info!("Successfully mounted hugetlbfs.");
    } else {
        info!("hugetlbfs is already mounted at {HUGEPAGE_MOUNT_POINT}.");
    }

    Ok(())
}

fn mount_hugetlbfs() -> Result<(), io::Error> {
    const NONE: Option<&'static [u8]> = None;
    mount(
        Some(b"none".as_ref()),
        HUGEPAGE_MOUNT_POINT,
        Some(HUGEPAGE_FS_TYPE),
        MsFlags::empty(),
        NONE,
    )
    .map_err(|e| io::Error::other(format!("Failed to mount hugetlbfs: {e}")))
}

async fn is_mounted(path: &str) -> bool {
    let Ok(mounts) = fs::read_to_string("/proc/mounts").await else {
        return false;
    };
    mounts.lines().any(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        parts.get(1) == Some(&path)
    })
}
