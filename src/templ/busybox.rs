use crate::Config;


pub fn get_service_definition() -> String {
    return "
::sysinit:/etc/init.d/rcS
::respawn:/bin/sh
::wait:/etc/init.d/minit.sh
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
    return format!("
#!/bin/sh

exec {command_string}
");
}