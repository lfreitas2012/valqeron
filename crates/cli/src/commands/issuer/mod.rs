pub mod info;
pub mod list;
pub mod register;

use clap::Subcommand;

use crate::commands::Command;

#[derive(Subcommand, Debug)]
pub enum IssuerCommand {
    /// Register a new issuer.
    Register(register::RegisterArgs),

    /// List all issuers.
    List(list::ListArgs),

    /// Retrieve issuer info by id.
    Info(info::InfoArgs),
}

impl IssuerCommand {
    pub fn as_command(&self) -> &dyn Command {
        match self {
            IssuerCommand::Register(args) => args,
            IssuerCommand::List(args) => args,
            IssuerCommand::Info(args) => args,
        }
    }
}
