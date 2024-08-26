use std::{env, fs};
use std::path::Path;
use std::process::Command;
use std::os::unix::process::CommandExt;

use env_logger::Env;

use std::env::set_var;
use log::{debug, info};
use minit::{make_foreground, process_signals};
use nix::unistd::Pid;
use nix::sys::signal::{self, SigSet};
use nix::sys::signalfd;
use clap::Parser;
use jsonic::parse;

#[derive(Debug, Parser)]
#[clap(version, about)]
struct Cli {
	#[arg(trailing_var_arg = true)]
    args: Vec<String>,
}


pub fn main() {
    unsafe {
        wrapped_main();
    }
}

/// We wrap the main function in an unsafe block to keep the code just
/// a little bit more readable.
unsafe fn wrapped_main() {

    // Load environment variables into the system from /etc/environment.json
    let env_path = Path::new("/etc/environment.json");
    if env_path.exists() {
        minit::load_environment(
            &fs::read_to_string(env_path).expect("Failed to read environment.json")
        );
    }

    // Parse options.
    let cli = Cli::parse();

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

    // Get the arguments for the process to run.
    let (cmd, args) = cli.args.as_slice().split_first().unwrap();

    // Spawn the child.
    let child = Command::new(cmd)
        .args(args)
        .pre_exec(move || {
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
    debug!("spawned '{}' as pid1 with pid {}", cmd, pid1);
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