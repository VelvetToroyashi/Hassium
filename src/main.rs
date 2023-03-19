#[macro_use]
extern crate windows_service;
extern crate core;

use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;
use std::ffi::OsString;
use std::panic::catch_unwind;
use std::path::Path;
use std::time::{Duration, Instant};
use std::{fs, thread};
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::ServiceControlHandlerResult;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{service_control_handler, service_dispatcher};

mod win32;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--from_os" => {
                hassium_service::run().unwrap();
            }
            _ => {
                println!("Unknown argument");
            }
        }
    } else {
        if !check_for_admin() {
            println!("Please run this program as administrator");

            thread::sleep(Duration::from_secs(8));
            return;
        }

        let _ = catch_unwind(get_user_intention);

        thread::sleep(Duration::from_secs(8));
    }
}

fn get_user_intention() {
    let options = vec!["Install", "Uninstall", "Exit"];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .items(&options)
        .default(0)
        .interact();

    match selection {
        Ok(0) => {
            let _ = install_service();
        }
        Ok(1) => {
            uninstall_service();
        }
        _ => {
            std::process::exit(-1);
        }
    }
}

fn uninstall_service() {
    println!("Uninstalling service");
    let service_name = "hassium";
    let service_manager =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).unwrap();

    let service = service_manager
        .open_service(service_name, ServiceAccess::DELETE)
        .expect("Failed to open service; it may not be installed yet.");
    service
        .delete()
        .expect("Failed to remove the service. Was it installed to begin with?");

    println!("Service uninstalled successfully!");
}

fn install_service() -> windows_service::Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service_binary_path = std::env::current_exe().unwrap();

    let service_info = ServiceInfo {
        name: OsString::from("hassium"),
        display_name: OsString::from("Hassium DisplayPort Fix"),
        service_type: ServiceType::USER_OWN_PROCESS,
        start_type: ServiceStartType::OnDemand,
        error_control: ServiceErrorControl::Normal,
        executable_path: service_binary_path,
        launch_arguments: vec!["--from_os".into()],
        dependencies: vec![],
        account_name: None, // run as System
        account_password: None,
    };

    println!("Installing service");
    let service = service_manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;
    service.set_description("Win10 DisplayPort Fix (VelvetThePanda/Hassium)")?;

    println!("Service installed successfully!");

    thread::sleep(Duration::from_secs(8));

    Ok(())
}

fn check_for_admin() -> bool {
    if fs::read_dir("C:\\Program Files\\Hassium").is_err() {
        fs::create_dir("C:\\Program Files\\Hassium").is_ok()
    } else {
        fs::write("C:\\Program Files\\Hassium\\temp", "").is_ok()
    }
}

mod hassium_service {
    use super::*;
    use crate::win32::WindowWatcher;

    pub fn run() -> windows_service::Result<()> {
        service_dispatcher::start("hassium", ffi_service_main)
    }

    define_windows_service!(ffi_service_main, service_main);

    #[allow(unused_must_use)]
    pub fn service_main(_: Vec<OsString>) -> windows_service::Result<()> {
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    // Handle stop event and return control back to the system.
                    ServiceControlHandlerResult::Other(1)
                }
                // All services must accept Interrogate even if it's a no-op.
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        // Register system service event handler
        let status_handle = service_control_handler::register("hassium", event_handler)?;

        let next_status = ServiceStatus {
            // Should match the one from system service registry
            service_type: ServiceType::USER_OWN_PROCESS,
            // The new state
            current_state: ServiceState::Running,
            // Accept stop events when running
            controls_accepted: ServiceControlAccept::STOP,
            // Used to report an error when starting or stopping only, otherwise must be zero
            exit_code: ServiceExitCode::Win32(0),
            // Only used for pending states, otherwise must be zero
            checkpoint: 0,
            // Only used for pending states, otherwise must be zero
            wait_hint: Duration::default(),
            process_id: None,
        };

        // Tell the system that the service is running now
        status_handle.set_service_status(next_status)?;

        let watcher = WindowWatcher::create();

        let handle = thread::spawn(move || {
            println!("Starting watcher thread");
            watcher.start();
        });

        handle.join().unwrap()
    }
}
