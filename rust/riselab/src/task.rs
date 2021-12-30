mod compute_node_service;
mod configure_tmux_service;
mod frontend_service;
mod meta_node_service;
mod minio_service;
mod prometheus_service;
mod task_configure_grpc_node;
mod task_configure_minio;

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use anyhow::Result;
use indicatif::ProgressBar;
use tempfile::TempDir;

pub use self::compute_node_service::*;
pub use self::configure_tmux_service::*;
pub use self::frontend_service::*;
pub use self::meta_node_service::*;
pub use self::minio_service::*;
pub use self::prometheus_service::*;
pub use self::task_configure_grpc_node::*;
pub use self::task_configure_minio::*;
use crate::util::{complete_spin, get_program_args, get_program_name};
use crate::wait_tcp::wait_tcp;

pub trait Task: 'static + Send {
    /// Execute the task
    fn execute(&mut self, ctx: &mut ExecuteContext<impl std::io::Write>) -> anyhow::Result<()>;

    /// Get task id used in progress bar
    fn id(&self) -> String {
        "<task>".into()
    }
}

/// A context used in task execution
pub struct ExecuteContext<W>
where
    W: std::io::Write,
{
    /// Global log file object. (aka. riselab.log)
    pub log: W,

    /// Progress bar on screen.
    pub pb: ProgressBar,

    /// The directory for checking status.
    ///
    /// RiseLAB will instruct every task to output their status to a file in temporary folder. By
    /// checking this file, we can know whether a task has early exited.
    pub status_dir: Arc<TempDir>,

    /// The current service id running in this context.
    pub id: Option<String>,

    /// The status file corresponding to the current context.
    pub status_file: Option<PathBuf>,
}

impl<W> ExecuteContext<W>
where
    W: std::io::Write,
{
    pub fn new(log: W, pb: ProgressBar, status_dir: Arc<TempDir>) -> Self {
        Self {
            log,
            pb,
            status_dir,
            status_file: None,
            id: None,
        }
    }

    pub fn service(&mut self, task: &impl Task) {
        let id = task.id();
        if !id.is_empty() {
            self.pb.set_prefix(id.clone());
            self.status_file = Some(self.status_dir.path().join(format!("{}.status", id)));
            self.id = Some(id);
        }
    }

    pub fn run_command(&mut self, mut cmd: Command) -> Result<Output> {
        let program_name = get_program_name(&cmd);

        writeln!(self.log, "> {} {}", program_name, get_program_args(&cmd))?;

        let output = cmd.output()?;

        let mut full_output = String::from_utf8_lossy(&output.stdout).to_string();
        full_output.extend(String::from_utf8_lossy(&output.stderr).chars());

        write!(self.log, "{}", full_output)?;

        writeln!(
            self.log,
            "({} exited with {:?})",
            program_name,
            output.status.code()
        )?;

        writeln!(self.log, "---")?;

        output.status.exit_ok()?;

        Ok(output)
    }

    pub fn complete_spin(&mut self) {
        complete_spin(&self.pb);
    }

    pub fn status_path(&self) -> PathBuf {
        self.status_file.clone().unwrap()
    }

    pub fn log_path(&self) -> anyhow::Result<PathBuf> {
        let prefix_log = env::var("PREFIX_LOG")?;
        Ok(Path::new(&prefix_log).join(format!("{}.log", self.id.as_ref().unwrap())))
    }

    pub fn wait_tcp(&mut self, server: impl AsRef<str>) -> anyhow::Result<()> {
        wait_tcp(
            server,
            &mut self.log,
            self.status_file.as_ref().unwrap(),
            self.id.as_ref().unwrap(),
        )?;
        Ok(())
    }

    pub fn tmux_run(&self, user_cmd: Command) -> anyhow::Result<Command> {
        let prefix_path = env::var("PREFIX_BIN")?;
        let mut cmd = Command::new("tmux");
        cmd.arg("new-window")
            // Set target name
            .arg("-t")
            .arg(RISELAB_SESSION_NAME)
            // Switch to background window
            .arg("-d")
            // Set session name for this window
            .arg("-n")
            .arg(self.id.as_ref().unwrap());
        cmd.arg(Path::new(&prefix_path).join("run_command.sh"));
        cmd.arg(self.log_path()?);
        cmd.arg(self.status_path());
        cmd.arg(user_cmd.get_program());
        for arg in user_cmd.get_args() {
            cmd.arg(arg);
        }
        Ok(cmd)
    }
}