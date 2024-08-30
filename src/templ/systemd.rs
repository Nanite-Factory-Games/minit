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
    return format!("
[Unit]
Description=Run main command at startup
After=default.target

[Service]
Type=simple
ExecStart={command_string}
RemainAfterExit=false

[Install]
WantedBy=dafault.target
");
}