use std::collections::HashMap;
use std::ffi::CString;
use std::fs::{self, File, Permissions};
use std::io::{Error, ErrorKind};
use std::convert::TryFrom;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use libc::c_int;

use log::{debug, info};
use nix::errno::Errno;
use nix::unistd::{self, setpgid, Pid};
use nix::sys::wait::{self, waitpid};
use nix::sys::signal::{self, kill, Signal};
use nix::sys::signalfd;
use nix::unistd::tcsetpgrp;
use serde::{de, Deserialize};
use anyhow::{bail, Context, Result};

pub mod templ;
pub enum InitType {
    Systemd,
    SysVinit,
    Upstart,
    OpenRC,
    Runit,
    S6,
    Busybox 
}

#[derive(Deserialize)]
pub struct Config {
    /// If defined, should be the entrypoint minit passes cmd args to
    pub entrypoint: Option<Vec<String>>,
    /// Must be defined as it is the main program the os will run
    pub cmd: Vec<String>,
    /// Mapping of environment variables to their values
    pub environment: Option<HashMap<String, String>>,
}

impl InitType {
    pub fn from_binpath(bin_path: &Path) -> Result<InitType> {
        let bin_name = bin_path.file_name().context("Init isn't linked to a file")?.to_str().context("File name couldn't be converted to string")?;
        let full_path = bin_path.to_str().context("Init isn't linked to a file")?;
        match bin_name {
            "systemd" => Ok(InitType::Systemd),
            "upstart" => Ok(InitType::Upstart),
            "openrc" => Ok(InitType::OpenRC),
            "runit" => Ok(InitType::Runit),
            "s6-rc" => Ok(InitType::S6),
            "busybox" => Ok(InitType::Busybox),
            _ => {
                if full_path.ends_with("sysvinit/init") {
                    Ok(InitType::SysVinit)
                } else {
                    Err(anyhow::anyhow!("Init isn't a supported init system"))
                }
            }
        }
    }

    /// Updates the system so that it will run the established command and entrypoint on startup
    pub fn setup_system(&self, config: &Config) -> Result<()>{
        match self {
            // For systemd we need to create a service definition and then symlink it to the directory for the program to find it
            InitType::Systemd => {
                let definition = templ::systemd::get_service_definition(config);
                fs::write(Path::new("/etc/systemd/system/minit.service"), definition.as_bytes())?;
                std::os::unix::fs::symlink(
                    Path::new("/etc/systemd/system/minit.service"),
                    Path::new("/etc/systemd/system/default.target.wants/minit.service"),
                )?;
            },
            InitType::Upstart => {bail!("Upstart is not yet supported")},
            InitType::OpenRC => {
                let definition = templ::openrc::get_service_definition(config);
                fs::write(Path::new("/etc/init.d/minit"), definition.as_bytes())?;
                std::os::unix::fs::symlink(
                    Path::new("/etc/init.d/minit"),
                    Path::new("/etc/runlevels/default/minit"),
                )?;
            },
            InitType::Runit => {bail!("Runit is not yet supported")},
            InitType::S6 => {bail!("S6 is not yet supported")},
            InitType::Busybox => {
                let definition = if Path::new("/sbin/openrc").exists() {
                    println!("Using openrc chain");
                    InitType::OpenRC.setup_system(config)?;
                    templ::busybox::get_service_definition_with_openrc()
                } else {
                    println!("Using busybox with no init");
                    let runfile = templ::busybox::get_runfile_definition(config);
                    println!("Writing runfile");
                    fs::create_dir_all(Path::new("/etc/init.d"))?;
                    fs::write(Path::new("/etc/init.d/minit.sh"), runfile.as_bytes())?;
                    fs::set_permissions(Path::new("/etc/init.d/minit.sh"), Permissions::from_mode(0o777))?;
                    templ::busybox::get_service_definition()
                };
                
                println!("Writing inittab");
                fs::write(Path::new("/etc/inittab"), definition.as_bytes())?;
            },
            InitType::SysVinit => {bail!("SysVinit is not yet supported")},
        }
        Ok(())
    }
}

pub fn remount_root_rw() -> Result<()> {
    // Remount the root filesystem as read-write
    Command::new("mount")
        .args(["-o", "remount,rw", "/"])
        .output()?;
    Ok(())
}

/// Reaps all zombies that are children of minit, returning the list of pids
/// that were reaped. If there are no children left alive or no children were
/// reaped, no error is returned. Unknown status codes from waitpid(2) are
/// logged and ignored.
pub fn reap_zombies() -> Result<Vec<Pid>, Error> {
    let mut pids = Vec::new();
    loop {
        match waitpid(None, Some(wait::WaitPidFlag::WNOHANG)) {
            // Did anything die?
            Ok(wait::WaitStatus::Exited(cpid, _)) |
            Ok(wait::WaitStatus::Signaled(cpid, _, _)) => {
                debug!("child process exited: {}", cpid);
                pids.push(cpid);
            }

            // No children left or none of them have died.
            // TODO: ECHILD really should cause us to quit (but doesn't currently), since
            //       if we get ECHILD we know that we have no children and thus will never get a
            //       SIGCHLD again. But this assumes we missed the SIGCHLD of the main process
            //       (which shouldn't be possible).
            Ok(wait::WaitStatus::StillAlive) |
            Err(Errno::ECHILD) => break,

            // Error conditions.
            status @ Ok(_) => info!("saw unknown status: {:?}", status),
            Err(err) => return Err(Error::from(err)),
        };
    }
    return Ok(pids);
}

/// Forward the given signal to the provided process.
pub fn forward_signal(pid: Pid, sig: Signal) -> Result<(), Error> {
    kill(pid, sig)?;

    debug!("forwarded {:?} to {}", sig, pid);
    return Ok(());
}

/// process_signals reads a signal from the given SignalFd and then handles it. If any child pids
/// were detected as having died, they are returned (an empty Vec means that no children died or
/// the signal wasn't SIGCHLD).
pub fn process_signals(pid1: Pid, sfd: &mut signalfd::SignalFd) -> Result<Vec<Pid>, Error> {
    let siginfo = sfd.read_signal()?.ok_or(Error::new(
        ErrorKind::Other,
        "no signals read",
    ))?;
    let signum = Signal::try_from(siginfo.ssi_signo as c_int)?;

    match signum {
        Signal::SIGCHLD => reap_zombies(),
        _ => forward_signal(pid1, signum).map(|_| Vec::new()),
    }
}

/// Places a process in the foreground (this function should be called in the
/// context of a `Command::before_exec` closure), making it the leader of a new
/// process group that is set to be the foreground process group in its session
/// with the current pty.
pub fn make_foreground() -> Result<(), Error> {
    // Create a new process group.
    setpgid(Pid::from_raw(0), Pid::from_raw(0))?;
    let pgrp = unistd::getpgrp();

    // Open /dev/tty, to avoid issues of std{in,out,err} being duped.
    let tty = match File::open("/dev/tty") {
        Ok(tty) => tty,
        // We ignore errors opening. This means that there's no PTY set up.
        Err(err) => {
            info!("failed to open /dev/tty: {}", err);
            return Ok(());
        },
    };

    // We have to block SIGTTOU here otherwise we will get stopped if we are in
    // a background process group.
    let mut sigmask = signal::SigSet::empty();
    sigmask.add(signal::Signal::SIGTTOU);
    sigmask.thread_block()?;

    // Set ourselves to be the foreground process group in our session.
    return match tcsetpgrp(tty, pgrp) {
        // We have succeeded in being the foreground process.
        Ok(_) => Ok(()),

        // ENOTTY [no tty] and ENXIO [lx zones] can happen in "normal" usage,
        // which indicate that we aren't in the foreground.
        // TODO: Should we undo the setpgid(0, 0) here?
        err @ Err(Errno::ENOTTY) |
        err @ Err(Errno::ENXIO) => {
            info!("failed to set process in foreground: {:?}", err);
            Ok(())
        }

        Err(err) => Err(Error::from(err)),
    };
}