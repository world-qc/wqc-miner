//! Host memory budget helpers (mirrors wqc-core `memory_budget`).

pub const HOST_MEMORY_RESERVE_THRESHOLD_GIB: u64 = 16;
pub const HOST_MEMORY_RESERVE_SMALL_GIB: u64 = 1;
pub const HOST_MEMORY_RESERVE_LARGE_GIB: u64 = 2;

const GIB: u64 = 1024 * 1024 * 1024;

pub fn host_memory_reserve_gib(total_gib: u64) -> u64 {
    if total_gib >= HOST_MEMORY_RESERVE_THRESHOLD_GIB {
        HOST_MEMORY_RESERVE_LARGE_GIB
    } else {
        HOST_MEMORY_RESERVE_SMALL_GIB
    }
}

pub fn max_wqc_memory_gib_from_total(total_gib: u64) -> u64 {
    if total_gib == 0 {
        return 1;
    }
    total_gib
        .saturating_sub(host_memory_reserve_gib(total_gib))
        .max(1)
}

pub fn host_memory_limits_gib() -> (Option<u64>, Option<u64>) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_bytes = sys.total_memory();
    if total_bytes == 0 {
        return (None, None);
    }
    let total_gib = total_bytes / GIB;
    if total_gib == 0 {
        return (None, None);
    }
    let max_gib = max_wqc_memory_gib_from_total(total_gib);
    (Some(total_gib), Some(max_gib))
}
