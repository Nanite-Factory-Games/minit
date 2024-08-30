use std::env::{self, set_var};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::os::unix::process::CommandExt;

use env_logger::Env;

use log::{debug, info};
use minit::{make_foreground, process_signals, Config, InitType};
use nix::unistd::Pid;
use nix::sys::signal::{self, SigSet};
use nix::sys::signalfd;
use anyhow::Result;

pub fn main() {
    println!("Running minit init system");
    unsafe {
        wrapped_main().unwrap();
    }
}

/// We wrap the main function in an unsafe block to keep the code just
/// a little bit more readable.
unsafe fn wrapped_main() -> Result<()>{

    // Load the configuration used to initialize the main program
    let config_path = Path::new("/etc/minit.json");
    let config = serde_json::from_str::<Config>(
        &fs::read_to_string(config_path)
            .expect("Failed to read /etc/minit.json")
        )
        .expect("Failed to parse minit.json");

    // Set environment variables from config
    if let Some(environment) = &config.environment {
        for (key, value) in environment {
            set_var(key, value);
        }
    }

    // Set up logging.
    let env = Env::new()
        .filter("MINIT_LOG")
        .write_style("MINIT_LOG_STYLE");
    env_logger::init_from_env(env);

    // Check if the current exe is the same as the /sbin/init link
    let current_path = env::current_exe().expect("could not get current exe path");
    let init_path = fs::read_link("/sbin/init");

    if let Ok(init_path) = init_path {
        if current_path == init_path {
            println!("minit is running as /sbin/init, it will act as the init process");
        } else {
            println!("An init binary is linked to /sbin/init, it will act as the init process from now on");
            let init_type = InitType::from_binpath(&init_path)?;
            init_type.setup_system(&config)?;
            // This will either return if there was an error or it will replace this process with the init process
            return Err(Command::new("/sbin/init").exec().into());
        }
    }
    // If we are here, we are continuing on as the primary init process

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

    let mut command = match config.entrypoint {
        Some(entrypoint) => {
            let mut command = Command::new(entrypoint[0].clone());
            command.args(entrypoint[1..].iter().map(|s| s.as_str()));
            command.args(config.cmd);
            command
        },
        None => {
            let mut command = Command::new(config.cmd[0].clone());
            command.args(config.cmd[1..].iter().map(|s| s.as_str()));
            command
        }
    };

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
            Err(err) => println!("unexpected error during signal handling: {}", err),
            Ok(pids) => {
                if pids.contains(&pid1) {
                    break;
                }
            }
        };
    }

    debug!("bailing: pid1 {} has exited", pid1);
    Ok(())
}