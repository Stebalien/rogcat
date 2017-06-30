// Copyright © 2016 Felix Obenhuber
// This program is free software. It comes without any warranty, to the extent
// permitted by applicable law. You can redistribute it and/or modify it under
// the terms of the Do What The Fuck You Want To Public License, Version 2, as
// published by Sam Hocevar. See the COPYING file for more details.

extern crate appdirs;
extern crate boolinator;
#[macro_use]
extern crate clap;
extern crate csv;
extern crate crc;
#[macro_use]
extern crate error_chain;
extern crate handlebars;
extern crate futures;
extern crate indicatif;
#[macro_use]
extern crate nom;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate serial;
extern crate tempdir;
extern crate time;
extern crate term_painter;
extern crate term_size;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_process;
extern crate toml;
extern crate which;
extern crate zip;

use clap::ArgMatches;
use cli::*;
use configuration::Configuration;
use error_chain::ChainedError;
use errors::*;
use filewriter::FileWriter;
use filter::Filter;
use futures::{Sink, Stream};
use parser::Parser;
use reader::{FileReader, SerialReader, StdinReader};
use record::Record;
use runner::Runner;
use std::env;
use std::io::{stderr, Write};
use std::path::PathBuf;
use std::process::{exit, Command};
use terminal::Terminal;
use tokio_core::reactor::Core;
use tokio_process::CommandExt;
use which::which_in;

mod bugreport;
mod cli;
mod configuration;
mod devices;
mod errors;
mod filewriter;
mod filter;
mod log;
mod parser;
mod record;
mod reader;
mod runner;
mod terminal;

type RSink = Box<Sink<SinkItem = Record, SinkError = Error>>;

fn main() {
    match run() {
        Err(e) => {
            let stderr = &mut stderr();
            let errmsg = "Error writing to stderr";
            writeln!(stderr, "{}", e.display()).expect(errmsg);
            exit(1)
        }
        Ok(r) => exit(r),
    }
}

fn adb() -> Result<PathBuf> {
    which_in("adb", env::var_os("PATH"), env::current_dir()?)
        .map_err(|e| format!("Cannot find adb: {}", e).into())
}

fn input(core: &Core, args: &ArgMatches) -> Result<Box<Stream<Item = Record, Error = Error>>> {
    if args.is_present("input") {
        let input = args.value_of("input").ok_or("Invalid input value")?;
        if SerialReader::parse_serial_arg(input).is_ok() {
            Ok(Box::new(SerialReader::new(args, input, core)?))
        } else {
            Ok(Box::new(FileReader::new(args, core)?))
        }
    } else {
        match args.value_of("COMMAND") {
            Some(c) => {
                if c == "-" {
                    Ok(Box::new(StdinReader::new(args, core)))
                } else if SerialReader::parse_serial_arg(c).is_ok() {
                    Ok(Box::new(SerialReader::new(args, c, core)?))
                } else {
                    Ok(Box::new(Runner::new(&args, core.handle())?))
                }
            }
            None => {
                Ok(Box::new(Runner::new(args, core.handle())?))
            }
        }
    }
}

fn run() -> Result<i32> {
    let args = cli().get_matches();
    let configuration = Configuration::new(&args)?;
    let mut core = Core::new()?;

    match args.subcommand() {
        ("bugreport", Some(sub_matches)) => exit(bugreport::create(sub_matches, &mut core)?),
        ("configuration", Some(sub_matches)) => exit(
            configuration.command_configuration(sub_matches)?,
        ),
        ("completions", Some(sub_matches)) => exit(cli::subcommand_completions(sub_matches)?),
        ("devices", _) => exit(devices::devices(&mut core)?),
        ("log", Some(sub_matches)) => exit(log::run(sub_matches, &mut core)?),
        ("profiles", Some(sub_matches)) => exit(configuration.command_profiles(sub_matches)?),
        (_, _) => (),
    }

    if args.is_present("clear") {
        // TODO: Add buffer selection
        let child = Command::new(adb()?).arg("logcat").arg("-c").spawn_async(
            &core.handle(),
        )?;
        let output = core.run(child)?;
        exit(output.code().ok_or("Failed to get exit code")?);
    }

    let output = if args.is_present("output") {
        Box::new(FileWriter::new(&args)?) as RSink
    } else {
        Box::new(Terminal::new(&args, &configuration)?) as RSink
    };
    let mut parser = Parser::new();
    let mut filter = Filter::new(&args, &configuration)?;

    let result = input(&core, &args)?
        .and_then(|m| parser.process(m))
        .filter(|m| filter.filter(m))
        .forward(output);

    core.run(result).map(|_| 0)
}
