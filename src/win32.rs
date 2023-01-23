use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock, Mutex};
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

pub(self) struct Monitor {
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

        let mut leak: &'static mut Self = Box::leak(Box::new(self));

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
                    let self_lock = leak.lock().expect("Mutex is poisoned :(");
                
                    let is_sleep = self_lock.is_sleep.load(Ordering::Relaxed);

                    if !is_sleep {
                        break
                    }

                    drop(self_lock); // Drop before we spin or we'll deadlock
                    hint::spin_loop();
                }
            }

            let self_lock = leak.lock().expect("Mutex is poisoned");

            let mut monitors = self_lock.monitors.write().unwrap();
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
                        let mut s = add_clone.lock().expect("Mutex is poisoned");
                        s.added(w, info, s.monitors.clone())
                    },
                ))
                .unwrap();
        }

        let remove_clone = this.clone();
        remove_token = this
            .lock()
            .expect("Mutex is poisoned")
            .watcher
            .Removed(&TypedEventHandler::new(
                move |watcher, info: &Option<DeviceInformationUpdate>| 
                remove_clone.lock().expect("Mutex is poisoned").removed(watcher, info),
            ))
            .unwrap();


        let mut this = this.lock().expect("Mutex is poisoned");

        this.add_token = Some(add_token);
        this.remove_token = Some(remove_token)
    }

    fn added(
        &self,
        _watcher: &Option<DeviceWatcher>,
        info: &Option<DeviceInformation>,
        lock: Arc<RwLock<Vec<Monitor>>>,
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
                for i in (0..monitor.windows.len()).rev() {
                    unsafe {
                        let window = monitor.windows.get(i).unwrap();
                        let exists = IsWindowVisible(window.id).as_bool();
                        if !exists {
                            monitor.windows.remove(i);
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

        self.watcher.Stop();
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
