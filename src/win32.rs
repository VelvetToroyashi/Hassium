use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetSystemMetrics, GetWindowInfo, IsWindowVisible, MoveWindow, SM_CMONITORS,
    WINDOWINFO, WS_POPUP,
};
use windows::{
    Devices::{
        Display::DisplayMonitor,
        Enumeration::{DeviceInformation, DeviceInformationUpdate, DeviceWatcher},
    },
    Foundation::{EventRegistrationToken, TypedEventHandler},
};

type WindowList = Arc<RwLock<Vec<Window>>>;

pub struct WindowWatcher {
    watcher: DeviceWatcher,
    windows: WindowList,
    is_sleep: AtomicBool,
    monitor_count: i8,
    awake_monitors: i8,
    add_token: Option<EventRegistrationToken>,
    remove_token: Option<EventRegistrationToken>,
}

pub(self) struct Window {
    pub id: HWND,
    pub pos: RECT,
}

impl WindowWatcher {
    pub fn create() -> Self {
        let filter = DisplayMonitor::GetDeviceSelector().unwrap();

        let watcher = DeviceInformation::CreateWatcherAqsFilter(&filter).unwrap();

        let windows = Arc::new(RwLock::new(Vec::<Window>::new()));

        let window_count = unsafe { GetSystemMetrics(SM_CMONITORS) } as i8;

        WindowWatcher {
            watcher,
            windows,
            is_sleep: AtomicBool::new(false),
            monitor_count: window_count,
            awake_monitors: window_count,
            add_token: None,
            remove_token: None,
        }
    }

    pub fn start(self) -> ! {
        let leak: &'static mut Self = Box::leak(Box::new(self));

        let leak = Arc::new(Mutex::new(leak));

        Self::hook_events(leak.clone());

        {
            let s = leak.lock().expect("Mutex is poisoned?");
            s.watcher.Start().expect("Failed to start watcher");
        }

        loop {
            // Wait for the monitor to be woken up
            {
                loop {
                    let lock = leak.lock().expect("Mutex is poisoned");
                    let is_sleep = lock.is_sleep.load(Ordering::Relaxed);

                    if !is_sleep {
                        break;
                    }

                    drop(lock); // Drop the lock
                                // We sleep instead of spinning/yielding because in a tight loop
                                // that runs for seconds, minutes, or even hours, we'll be hogging
                                // the CPU for no reason. (Spin = 10%, Yield = 12%, Sleep = 0%)
                    thread::sleep(std::time::Duration::from_millis(100));
                }
            }

            let self_lock = leak.lock().expect("Mutex is poisoned");

            let mut windows = self_lock.windows.write().unwrap();

            let window_list = &mut Vec::<Window>::new();
            unsafe {
                EnumWindows(
                    Some(window_callback),
                    LPARAM((window_list as *mut _) as isize),
                );
            }

            windows.clear();
            windows.append(window_list);

            drop(windows); // drop the borrow
            drop(self_lock); // drop the lock

            thread::sleep(std::time::Duration::from_millis(1500));
        }
    }

    fn hook_events(this: Arc<Mutex<&'static mut Self>>) {
        let add_token: EventRegistrationToken;
        let remove_token: EventRegistrationToken;

        {
            let add_clone = this.clone();
            add_token = this
                .lock()
                .expect("Mutex is poisoned")
                .watcher
                .Added(&TypedEventHandler::new(
                    move |w: &Option<DeviceWatcher>, info: &Option<DeviceInformation>| {
                        let s = &mut *add_clone.lock().expect("Mutex is poisoned");
                        s.added(w, info, s.windows.clone())
                    },
                ))
                .unwrap();
        }

        {
            let remove_clone = this.clone();
            remove_token = this
                .lock()
                .expect("Mutex is poisoned")
                .watcher
                .Removed(&TypedEventHandler::new(
                    move |watcher, info: &Option<DeviceInformationUpdate>| {
                        let s = &mut *remove_clone.lock().expect("Mutex is poisoned");

                        s.removed(watcher, info)
                    },
                ))
                .unwrap();
        }

        let mut this = this.lock().expect("Mutex is poisoned");

        this.add_token = Some(add_token);
        this.remove_token = Some(remove_token)
    }

    fn added(
        &mut self,
        _watcher: &Option<DeviceWatcher>,
        info: &Option<DeviceInformation>,
        lock: WindowList,
    ) -> Result<(), windows::core::Error> {
        if info.is_none() {
            return Ok(());
        }

        if !self.is_sleep.load(Ordering::Relaxed) && self.awake_monitors < self.monitor_count {
            self.awake_monitors += 1;

            return Ok(());
        }

        thread::sleep(std::time::Duration::from_millis(4000));

        self.is_sleep.store(false, Ordering::Relaxed);

        let mut windows = lock.write().unwrap();

        for i in (0..windows.len()).rev() {
            unsafe {
                let window = windows.get(i).unwrap();
                let exists = IsWindowVisible(window.id).as_bool();
                if !exists {
                    windows.remove(i);
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
        Ok(())
    }

    fn removed(
        &mut self,
        _watcher: &Option<DeviceWatcher>,
        info: &Option<DeviceInformationUpdate>,
    ) -> Result<(), windows::core::Error> {
        match info {
            Some(_) => info,
            None => return Ok(()),
        };

        self.is_sleep.store(true, Ordering::Relaxed);
        self.awake_monitors = self.awake_monitors.checked_sub(1).unwrap_or(0);

        Ok(())
    }
}

impl Drop for WindowWatcher {
    fn drop(&mut self) {
        let _ = self.watcher.RemoveAdded(self.add_token.unwrap());
        let _ = self.watcher.RemoveRemoved(self.remove_token.unwrap());

        let _ = self.watcher.Stop();
    }
}

extern "system" fn window_callback(hwnd: HWND, ptr: LPARAM) -> BOOL {
    unsafe {
        let window_info = &mut WINDOWINFO::default();
        GetWindowInfo(hwnd, window_info);

        if IsWindowVisible(hwnd).as_bool() && is_app_window(hwnd, *window_info) {
            let vec = &mut *(ptr.0 as *mut Vec<Window>);

            vec.push(Window {
                id: hwnd,
                pos: window_info.rcWindow,
            });
        }

        true.into()
    }
}

unsafe fn is_app_window(hwnd: HWND, info: WINDOWINFO) -> bool {
    if !IsWindowVisible(hwnd).as_bool() {
        return false;
    }

    if (info.dwStyle.0 & WS_POPUP.0) != 0 {
        return false;
    }

    let mut cloak = 0;

    // turn cloak_val into *c_void
    let cloak_val = &mut cloak as *mut _ as *mut c_void;

    DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, cloak_val, 4).expect("Windows please");

    cloak == 0
}
