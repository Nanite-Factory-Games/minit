use crate::Config;


pub fn get_service_definition() -> String {
    return "
::respawn:/bin/sh
::wait:/etc/init.d/minit.sh
".to_string();
}

/// if we are chaining busybox into openrc, we need a different config
/// for the service definition
pub fn get_service_definition_with_openrc() -> String {
    return "# /etc/inittab
::sysinit:/sbin/openrc sysinit
::sysinit:/sbin/openrc boot
::wait:/sbin/openrc default

::shutdown:/sbin/openrc shutdown
".to_string();
}

pub fn get_runfile_definition(config: &Config) -> String {
    let command_string = match &config.entrypoint {
        Some(entrypoint) => {
            let mut command = String::new();
            command.push_str(&entrypoint[..].join(" "));
            command.push_str(" ");
            command.push_str(&config.cmd.join(" "));
            command
        },
        None => {
            let mut command = String::new();
            command.push_str(&config.cmd[..].join(" "));
            command
        }
    };
    return format!("#!/bin/sh

exec {command_string}
");
}