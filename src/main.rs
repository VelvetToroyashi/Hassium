mod win32;

fn main() {
    let mut watcher = win32::MonitorHandler::create_watcher();

    let handle = std::thread::spawn(move || {
        watcher.start();
    });

    handle.join().unwrap();
}
