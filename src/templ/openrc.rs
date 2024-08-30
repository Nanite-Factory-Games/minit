use crate::Config;


pub fn get_service_definition(config: &Config) -> String {
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
    let first_path = config.cmd[0].clone();
    let args = config.cmd[1..].join(" ");
    return format!("
#!/sbin/openrc-run

description=\"Run main command on startup\"

command=\"{first_path}\"
command_args=\"{args}\"
command_background=false

depend() {{
    after *
}}
");
}
