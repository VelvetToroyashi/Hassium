use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::thread::{JoinHandle, ScopedJoinHandle};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible, MoveWindow};
use windows::{
    Devices::{
        Display::DisplayMonitor,
        Enumeration::{DeviceInformation, DeviceInformationUpdate, DeviceWatcher},
    },
    Foundation::{EventRegistrationToken, TypedEventHandler},
};

type MonitorList = Arc<RwLock<Vec<Monitor>>>;

pub struct MonitorHandler {
    watcher: DeviceWatcher,
    monitors: MonitorList,
    is_sleep: AtomicBool,
    add_token: Option<EventRegistrationToken>,
    remove_token: Option<EventRegistrationToken>,
}

pub struct Monitor {
    pub info: DeviceInformation,
    pub windows: Vec<Window>,
}

pub(self) struct Window {
    pub id: HWND,
    pub pos: RECT,
}

impl MonitorHandler {
    pub(crate) fn create_watcher() -> Self {
        let filter = DisplayMonitor::GetDeviceSelector().unwrap();

        let watcher = DeviceInformation::CreateWatcherAqsFilter(&filter).unwrap();

        let monitors = Arc::new(RwLock::new(Vec::<Monitor>::new()));

        MonitorHandler {
            watcher,
            monitors,
            is_sleep: AtomicBool::new(false),
            add_token: None,
            remove_token: None,
        }
    }

    pub fn start(mut self) -> ! {
        use std::hint;
        use std::sync::atomic::Ordering;
        use std::thread;

        self.hook_events();
        self.watcher.Start().expect("Failed to start watcher");

        let is_sleep = &mut self.is_sleep;
        let monitors_clone = &mut self.monitors.clone();

        loop {
            // Wait for the monitor to be woken up
            while is_sleep.load(Ordering::Relaxed) {
                hint::spin_loop();
            }

            let mut monitors = monitors_clone.write().unwrap();
            for monitor in monitors.iter_mut() {
                let mut windows = Vec::<Window>::new();
                unsafe {
                    EnumWindows(
                        Some(window_callback),
                        LPARAM(((&mut windows) as *mut _) as isize),
                    );
                }
                monitor.windows = windows;
            }
            thread::sleep(std::time::Duration::from_millis(3000));
        }
    }

    fn hook_events(&mut self) {
        let add_clone = &self.monitors.clone();
        let add_token = self
            .watcher
            .Added(&TypedEventHandler::new(
                |w: &Option<DeviceWatcher>, info: &Option<DeviceInformation>| {
                    self.added(w, info, add_clone)
                },
            ))
            .unwrap();

        let remove_token = self
            .watcher
            .Removed(&TypedEventHandler::new(
                |watcher, info: &Option<DeviceInformationUpdate>| self.removed(watcher, info),
            ))
            .unwrap();

        self.add_token = Some(add_token);
        self.remove_token = Some(remove_token);
    }

    fn added(
        &self,
        _watcher: &Option<DeviceWatcher>,
        info: &Option<DeviceInformation>,
        lock: &Arc<RwLock<Vec<Monitor>>>,
    ) -> Result<(), windows::core::Error> {
        let unwrapped = match info {
            Some(info) => info,
            None => return Ok(()),
        };

        let read = lock.read().unwrap();

        if read.iter().any(|m| m.info.Id() == unwrapped.Id()) {
            return Ok(());
        }

        drop(read);

        let mut monitors = lock.write().unwrap();

        for monitor in monitors.iter_mut() {
            if monitor.info.Id() != unwrapped.Id() {
                monitor.info = unwrapped.clone();
                return Ok(());
            } else {
                for (index, window) in monitor.windows.iter_mut().enumerate() {
                    unsafe {
                        let exists = IsWindowVisible(window.id).as_bool();
                        if !exists {
                            monitor.windows.remove(index);
                            continue;
                        }

                        let (x, y, w, h) = (
                            window.pos.left,
                            window.pos.top,
                            window.pos.right - window.pos.left,
                            window.pos.bottom - window.pos.top,
                        );
                        MoveWindow(window.id, x, y, w, h, false);
                    }
                }
            }
        }
        Ok(())
    }

    fn removed(
        &self,
        _watcher: &Option<DeviceWatcher>,
        info: &Option<DeviceInformationUpdate>,
    ) -> Result<(), windows::core::Error> {
        match info {
            Some(_) => info,
            None => return Ok(()),
        };

        self.is_sleep
            .store(true, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }
}

impl Drop for MonitorHandler {
    fn drop(&mut self) {
        let _ = self.watcher.RemoveAdded(self.add_token.unwrap());
        let _ = self.watcher.RemoveRemoved(self.remove_token.unwrap());
    }
}

extern "system" fn window_callback(hwnd: HWND, ptr: LPARAM) -> BOOL {
    unsafe {
        if IsWindowVisible(hwnd).as_bool() {
            let vec = &mut *(ptr.0 as *mut Vec<Window>);

            let mut rect = std::mem::zeroed();
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect);

            vec.push(Window {
                id: hwnd,
                pos: rect,
            });
        }

        true.into()
    }
}
