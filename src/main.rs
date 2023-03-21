use std::path::PathBuf;
use std::{fs, thread};
use windows::core::{PCSTR, PSTR};

mod win32;

const INSTALL_PATH: &str = "C:\\Program Files\\Hassium\\hassium.exe";
const INSTALL_FOLDER: &str = "C:\\Program Files\\Hassium";
const TEMP_FILE: &str = "C:\\Program Files\\Hassium\\temp";

fn main() {
    // If we're running in the install path, start the watcher
    if std::env::current_exe().ok() == Some(PathBuf::from(INSTALL_PATH)) {
        let watcher = win32::WindowWatcher::create();

        let handle = thread::spawn(move || {
            println!("Starting watcher thread");
            watcher.start();
        });

        hide_console_window();

        handle.join().unwrap();
    } else {
        let opts = vec!["Install", "Uninstall", "Exit"];

        let selection = dialoguer::Select::new()
            .with_prompt("Select an option")
            .items(&opts)
            .default(0)
            .interact()
            .unwrap();

        match selection {
            0 => install(),
            1 => uninstall(),
            _ => {}
        }
    }
}

fn hide_console_window() {
    use windows::Win32::System::Console::GetConsoleWindow;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    unsafe {
        let window = GetConsoleWindow();
        // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow
        if !(window.0 as *mut usize).is_null() {
            ShowWindow(window, SW_HIDE);
        }
    }
}

fn install() {
    let cur_path = std::env::current_exe().unwrap();

    if !ensure_admin_privileges() {
        println!("Installation requires running as administrator to copy files.");

        thread::sleep(std::time::Duration::from_secs(5));

        return;
    }

    fs::copy(cur_path, INSTALL_PATH).unwrap();

    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = key
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .unwrap()
        .0;

    key.set_value("Hassium", &INSTALL_PATH).unwrap();

    println!("Installation complete!");

    run_child_detached();
}

fn run_child_detached() {
    unsafe {
        windows::Win32::System::Threading::CreateProcessA(
            PCSTR::null(),
            PSTR::from_raw(INSTALL_PATH.as_ptr() as *mut _),
            None,
            None,
            false,
            windows::Win32::System::Threading::CREATE_NEW_CONSOLE,
            None,
            None,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
}

fn uninstall() {
    if !ensure_admin_privileges() {
        println!("Uninstallation requires running as administrator to delete files.");

        thread::sleep(std::time::Duration::from_secs(5));

        return;
    }

    if fs::remove_dir_all(INSTALL_FOLDER).is_err() {
        println!("Failed to uninstall Hassium. Please delete the folder manually.");
    } else {
        let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let key = key
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .unwrap()
            .0;

        let _ = key.delete_value("Hassium");

        println!("Uninstallation complete!");
    }
}

fn ensure_admin_privileges() -> bool {
    if !PathBuf::from(INSTALL_FOLDER).exists() && fs::create_dir(INSTALL_FOLDER).is_err() {
        return false;
    }

    fs::write(TEMP_FILE, "").is_ok()
}
