use color_eyre::{
    eyre::{bail, Context},
    Result,
};

use std::ffi::{OsStr, OsString};
use thiserror::Error;

use subprocess::{Exec, ExitStatus, Redirection};
use tracing::{debug, info};

use crate::interface::FlakeRef;

#[derive(Debug, derive_builder::Builder)]
#[builder(derive(Debug), setter(into))]
pub struct Command {
    /// Whether to actually run the command or just log it
    #[builder(default = "false")]
    dry: bool,
    /// Human-readable message regarding what the command does
    #[builder(setter(strip_option), default = "None")]
    message: Option<String>,
    /// Arguments 0..N
    #[builder(setter(custom))]
    args: Vec<OsString>,
}

impl CommandBuilder {
    pub fn args<S, I>(&mut self, input: I) -> &mut Self
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = S>,
    {
        self.args
            .get_or_insert_with(Default::default)
            .extend(input.into_iter().map(|s| s.as_ref().to_owned()));
        self
    }
}

impl Command {
    pub fn exec(&self) -> Result<()> {
        let [head, tail @ ..] = &*self.args else {
            bail!("Args was length 0");
        };

        let cmd = Exec::cmd(head)
            .args(tail)
            .stderr(Redirection::None)
            .stdout(Redirection::None);

        if let Some(m) = &self.message {
            info!("{}", m);
        }
        debug!(?cmd);

        if !self.dry {
            if let Some(m) = &self.message {
                cmd.join().wrap_err(m.clone())?;
            } else {
                cmd.join()?;
            }
        }

        Ok(())
    }

    pub fn exec_capture(&self) -> Result<Option<String>> {
        let [head, tail @ ..] = &*self.args else {
            bail!("Args was length 0");
        };

        let cmd = Exec::cmd(head)
            .args(tail)
            .stderr(Redirection::None)
            .stdout(Redirection::Pipe);

        if let Some(m) = &self.message {
            info!("{}", m);
        }
        debug!(?cmd);

        if !self.dry {
            Ok(Some(cmd.capture()?.stdout_str()))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, derive_builder::Builder)]
#[builder(setter(into))]
pub struct BuildCommand {
    /// Human-readable message regarding what the command does
    message: String,
    // Flakeref to build
    flakeref: String,
    // Extra arguments passed to nix build
    #[builder(setter(custom))]
    extra_args: Vec<OsString>,
    /// Use nom for the nix build
    nom: bool,
}

impl BuildCommandBuilder {
    pub fn extra_args<S, I>(&mut self, input: I) -> &mut Self
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = S>,
    {
        self.extra_args
            .get_or_insert_with(Default::default)
            .extend(input.into_iter().map(|s| s.as_ref().to_owned()));
        self
    }
}

impl BuildCommand {
    pub fn exec(&self) -> Result<()> {
        info!("{}", self.message);

        let exit = if self.nom {
            let cmd = {
                Exec::cmd("nix")
                    .args(&[
                        "build",
                        &self.flakeref,
                        "--log-format",
                        "internal-json",
                        "--verbose",
                    ])
                    .args(&self.extra_args)
                    .stdout(Redirection::Pipe)
                    .stderr(Redirection::Merge)
                    | Exec::cmd("nom").args(&["--json"])
            }
            .stdout(Redirection::None);
            debug!(?cmd);
            cmd.join()
        } else {
            let cmd = Exec::cmd("nix")
                .args(&["build", &self.flakeref])
                .args(&self.extra_args)
                .stdout(Redirection::None)
                .stderr(Redirection::Merge);

            debug!(?cmd);
            cmd.join()
        };

        match exit.wrap_err(self.message.clone())? {
            ExitStatus::Exited(0) => (),
            other => bail!(ExitError(other)),
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
#[error("Command exited with status {0:?}")]
pub struct ExitError(ExitStatus);

pub fn edit(flakeref: FlakeRef) -> Result<()> {
    let editor = std::env::var("EDITOR").expect("EDITOR not set");
    edit_with(flakeref, editor)
}

pub fn edit_with(flakeref: FlakeRef, editor: String) -> Result<()> {
    let mut pieces: Vec<&str> = flakeref.split('/').collect();
    let mut final_piece: &str = pieces.remove(pieces.len() - 1);
    final_piece = final_piece.split('#').next().unwrap();
    pieces.push(final_piece);

    let flakedir = pieces.join("/");

    Exec::cmd(editor)
        .args(&vec!["."])
        .cwd(flakedir)
        .stderr(Redirection::None)
        .stdout(Redirection::None)
        .join()?;

    Ok(())
}
