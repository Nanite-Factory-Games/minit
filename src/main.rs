use std::env::set_var;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::os::unix::process::CommandExt;

use env_logger::Env;

use log::{debug, info};
use minit::{make_foreground, process_signals, Config};
use nix::unistd::Pid;
use nix::sys::signal::{self, SigSet};
use nix::sys::signalfd;

pub fn main() {
    unsafe {
        wrapped_main();
    }
}

/// We wrap the main function in an unsafe block to keep the code just
/// a little bit more readable.
unsafe fn wrapped_main() {

    // Load the configuration used to initialize the main program
    let config_path = Path::new("/etc/minit.json");
    let config = serde_json::from_str::<Config>(
        &fs::read_to_string(config_path)
            .expect("Failed to read /etc/minit.json")
        )
        .expect("Failed to parse minit.json");

    // Set environment variables from config
    if let Some(environment) = config.environment {
        for (key, value) in environment {
            set_var(key, value);
        }
    }

    // Set up logging.
    let env = Env::new()
        .filter("MINIT_LOG")
        .write_style("MINIT_LOG_STYLE");
    env_logger::init_from_env(env);

    // We need to store the initial signal mask first, which we will restore
    // before execing the user process (signalfd requires us to block all
    // signals we are masking but this would be inherited by our child).
    let init_sigmask =
        SigSet::thread_get_mask()
        .expect("could not get main thread sigmask");

    // Block all signals so we can use signalfd. Note that while it would be
    // great for us to just set SIGCHLD to SIG_IGN (that way zombies will be
    // auto-reaped by the kernel for us, as guaranteed by POSIX-2001 and SUS)
    // this way we can more easily handle the child we are forwarding our
    // signals to dying.
    let sigmask = signal::SigSet::all();
    sigmask.thread_block().expect("could not block all signals");
    let mut sfd =
        signalfd::SignalFd::new(&sigmask).expect("could not create signalfd for all signals");

    // Spawn the child. if entrypoint is defined, use it, otherwise use the main command
    let mut command = Command::new(config.minit_entrypoint_path.clone().unwrap_or(config.minit_cmd.clone()));
    // We only need to pass command as args if we are using the entrypoint
    if config.minit_entrypoint_path.is_some() {
        command.args(config.minit_cmd.split_whitespace());
    }

    let child = command.pre_exec(move || {
            make_foreground()?;
            init_sigmask.thread_set_mask()?;
            return Ok(());
        })
        .spawn()
        .expect("failed to start child process");

    // Loop, reading all signals we recieved to figure out what the correct response is (forward
    // all signals other than SIGCHLD which we react to by reaping the children). In addition all
    // errors are logged and ignored from here on out -- because we *must not exit* as we are pid1
    // and exiting will kill the container.
    let pid1 = Pid::from_raw(child.id() as i32);
    loop {
        match process_signals(pid1, &mut sfd) {
            Err(err) => info!("unexpected error during signal handling: {}", err),
            Ok(pids) => {
                if pids.contains(&pid1) {
                    break;
                }
            }
        };
    }

    debug!("bailing: pid1 {} has exited", pid1);
}