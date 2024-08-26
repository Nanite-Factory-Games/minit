use serde::Deserialize;

#[derive(Deserialize)]
pub struct Environment {
    /// If defined, should be the entrypoint minit passes cmd args to
    pub minit_entrypoint_path: Option<String>,
    /// If defined will be used as the command minit passes to exec
    pub minit_cmd: Option<String>,
}