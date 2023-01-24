mod win32;

fn main() {
    let mut watcher = win32::WindowWatcher::create();

    let handle = std::thread::spawn(move || {
        println!("Starting watcher thread");
        watcher.start();
    });

    handle.join().unwrap();
}
