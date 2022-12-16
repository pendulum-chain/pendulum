//! Pendulum/Amplitude Collator CLI

#![warn(missing_docs)]

mod chain_spec;
#[macro_use]
mod service;
mod cli;
mod command;
mod constants;
mod rpc;

fn main() -> sc_cli::Result<()> {
	command::run()
}
