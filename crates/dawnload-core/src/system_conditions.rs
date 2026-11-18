//! Local OS observations for courtesy mode. Unavailable signals remain unknown to the OS
//! adapter and do not by themselves trigger a pause.

use crate::seeding::{SeedConditions, SeedConditionsSource};

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub(crate) struct SystemConditions {
    _private: (),
}

#[cfg(target_os = "macos")]
#[derive(Default)]
pub(crate) struct SystemConditions {
    monitor: std::sync::OnceLock<Option<mac::PathMonitor>>,
}

impl SeedConditionsSource for SystemConditions {
    fn sample(&self) -> SeedConditions {
        #[cfg(target_os = "macos")]
        {
            let metered = self.monitor.get_or_init(mac::PathMonitor::start).as_ref().is_some_and(mac::PathMonitor::metered);
            SeedConditions { metered, on_battery: mac::on_battery() }
        }
        #[cfg(target_os = "linux")]
        {
            linux::sample()
        }
        #[cfg(windows)]
        {
            win::sample()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            SeedConditions::default()
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::SeedConditions;
    use std::time::Duration;
    use zbus::blocking::{Connection, Proxy, connection::Builder};

    fn property<T>(bus: &Connection, name: &str, path: &str, key: &str) -> Option<T>
    where
        T: TryFrom<zbus::zvariant::OwnedValue>,
        T::Error: Into<zbus::Error>,
    {
        let proxy = Proxy::new(bus, name, path, name).ok()?;
        proxy.get_property(key).ok()
    }

    pub fn sample() -> SeedConditions {
        let Some(bus) =
            Builder::system().ok().and_then(|builder| builder.method_timeout(Duration::from_millis(700)).build().ok())
        else {
            return SeedConditions::default();
        };
        let metered = matches!(
            property::<u32>(&bus, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "Metered"),
            Some(1 | 3) // NM_METERED_YES or NM_METERED_GUESS_YES
        );
        let on_battery =
            property::<bool>(&bus, "org.freedesktop.UPower", "/org/freedesktop/UPower", "OnBattery").unwrap_or(false);
        SeedConditions { metered, on_battery }
    }
}

#[cfg(windows)]
mod win {
    use super::SeedConditions;
    use windows::Networking::Connectivity::{NetworkCostType, NetworkInformation};
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};

    pub fn sample() -> SeedConditions {
        let mut power = SYSTEM_POWER_STATUS::default();
        // SAFETY: the pointer is to a live, writable SYSTEM_POWER_STATUS.
        let on_battery = unsafe { GetSystemPowerStatus(&mut power) }.is_ok()
            && power.ACLineStatus == 0
            && power.BatteryFlag != 255
            && power.BatteryFlag & 128 == 0;
        SeedConditions { on_battery, metered: metered().unwrap_or(false) }
    }

    fn metered() -> Option<bool> {
        // SAFETY: this runs on a blocking worker. Balance each successful initialization on it.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.ok()?;
        let result = (|| {
            let cost = NetworkInformation::GetInternetConnectionProfile().ok()?.GetConnectionCost().ok()?;
            Some(
                matches!(cost.NetworkCostType().ok(), Some(NetworkCostType::Fixed | NetworkCostType::Variable))
                    || cost.Roaming().unwrap_or(false)
                    || cost.OverDataLimit().unwrap_or(false)
                    || cost.ApproachingDataLimit().unwrap_or(false)
                    || cost.BackgroundDataUsageRestricted().unwrap_or(false),
            )
        })();
        // SAFETY: same thread as the successful RoInitialize; WinRT objects have been dropped.
        unsafe { RoUninitialize() };
        result
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_long, c_void};
    use std::ptr::NonNull;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use block2::RcBlock;

    type Object = *mut c_void;

    #[link(name = "Network", kind = "framework")]
    unsafe extern "C" {
        fn nw_path_monitor_create() -> Object;
        fn nw_path_monitor_set_queue(monitor: Object, queue: Object);
        fn nw_path_monitor_set_update_handler(monitor: Object, handler: *const c_void);
        fn nw_path_monitor_start(monitor: Object);
        fn nw_path_monitor_cancel(monitor: Object);
        fn nw_path_is_constrained(path: Object) -> bool;
        fn nw_path_is_expensive(path: Object) -> bool;
        fn nw_release(object: Object);
    }

    unsafe extern "C" {
        fn dispatch_get_global_queue(identifier: c_long, flags: usize) -> Object;
    }

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPSCopyPowerSourcesInfo() -> *const c_void;
        fn IOPSGetProvidingPowerSourceType(info: *const c_void) -> *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: *const c_void);
        fn CFStringGetCString(string: *const c_void, buffer: *mut u8, capacity: isize, encoding: u32) -> bool;
    }

    pub struct PathMonitor {
        monitor: NonNull<c_void>,
        value: Arc<AtomicBool>,
    }

    // SAFETY: Network.framework monitors can be canceled and released on any thread.
    // The callback owns only an Arc to an atomic bool, and no Rust reference to the monitor.
    unsafe impl Send for PathMonitor {}
    // SAFETY: shared access reads only the atomic state from the update callback.
    unsafe impl Sync for PathMonitor {}

    impl PathMonitor {
        pub fn start() -> Option<Self> {
            let value = Arc::new(AtomicBool::new(false));
            let updated = value.clone();
            let (first, observed) = std::sync::mpsc::sync_channel(1);
            let handler = RcBlock::new(move |path: Object| {
                // SAFETY: the API supplies a live path for the callback invocation.
                let metered = unsafe { nw_path_is_constrained(path) || nw_path_is_expensive(path) };
                updated.store(metered, Ordering::Relaxed);
                let _ = first.try_send(());
            });
            // SAFETY: set the queue and handler before starting a newly owned monitor.
            // Network.framework copies the block, which retains its atomic state.
            let monitor = unsafe {
                let monitor = NonNull::new(nw_path_monitor_create())?;
                nw_path_monitor_set_queue(monitor.as_ptr(), dispatch_get_global_queue(0, 0));
                nw_path_monitor_set_update_handler(monitor.as_ptr(), RcBlock::as_ptr(&handler).cast());
                nw_path_monitor_start(monitor.as_ptr());
                monitor
            };
            let _ = observed.recv_timeout(std::time::Duration::from_secs(1));
            Some(Self { monitor, value })
        }

        pub fn metered(&self) -> bool {
            self.value.load(Ordering::Relaxed)
        }
    }

    impl Drop for PathMonitor {
        fn drop(&mut self) {
            // SAFETY: these are the matching cancel and release for this owned monitor.
            unsafe {
                nw_path_monitor_cancel(self.monitor.as_ptr());
                nw_release(self.monitor.as_ptr());
            }
        }
    }

    pub fn on_battery() -> bool {
        // SAFETY: the copied power snapshot owns the borrowed type string. Read it into a
        // bounded UTF-8 buffer before releasing the snapshot.
        unsafe {
            let info = IOPSCopyPowerSourcesInfo();
            if info.is_null() {
                return false;
            }
            let source = IOPSGetProvidingPowerSourceType(info);
            let mut text = [0u8; 64];
            let on_battery = !source.is_null()
                && CFStringGetCString(source, text.as_mut_ptr(), text.len() as isize, 0x0800_0100)
                && text.starts_with(b"Battery Power\0");
            CFRelease(info);
            on_battery
        }
    }
}
