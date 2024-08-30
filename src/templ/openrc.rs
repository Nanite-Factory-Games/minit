use crate::Config;


pub fn get_service_definition(config: &Config) -> String {
    let (first_path, args) = match &config.entrypoint {
        Some(entrypoint) => {
            let first_path = entrypoint[0].clone();
            let args = [&entrypoint[1..], &(&config.cmd)[..]].concat().join(" ");
            (first_path, args)
        },
        None => {
            let first_path = config.cmd[0].clone();
            let args = config.cmd[1..].join(" ");
            (first_path, args)
        }
    };
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
