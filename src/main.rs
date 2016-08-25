// Copyright © 2016 Felix Obenhuber
// This program is free software. It comes without any warranty, to the extent
// permitted by applicable law. You can redistribute it and/or modify it under
// the terms of the Do What The Fuck You Want To Public License, Version 2, as
// published by Sam Hocevar. See the COPYING file for more details.

extern crate ansi_term;
#[macro_use]
extern crate clap;
extern crate regex;
extern crate term_size;
extern crate time;

use clap::{App, Arg};
use std::process::{Command, Stdio};
use std::io::{BufReader, BufRead};
use regex::Regex;

mod message;
mod terminal;
mod parser;
mod filewriter;

#[derive (PartialOrd, PartialEq)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Assert,
}

impl ::std::fmt::Display for Level {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f,
               "{}",
               match *self {
                   Level::Trace => "T",
                   Level::Debug => "D",
                   Level::Info => "I",
                   Level::Warn => "W",
                   Level::Error => "E",
                   Level::Fatal => "F",
                   Level::Assert => "A",
               })
    }
}

impl<'a> From<&'a str> for Level {
    fn from(s: &str) -> Self {
        match s {
            "T" => Level::Trace,
            "I" => Level::Info,
            "W" => Level::Warn,
            "E" => Level::Error,
            "F" => Level::Fatal,
            "A" => Level::Assert,
            "D" | _ => Level::Debug,
        }
    }
}

impl std::str::FromStr for Level {
    type Err = bool;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

pub trait Sink {
    fn process(&mut self, message: &message::Message);
    fn close(&self);
}

fn main() {
    let matches = App::new("rogcat")
        .version(crate_version!())
        .author("Felix Obenhuber <f.obenhuber@gmail.com>")
        .about("A logcat wrapper")
        .arg_from_usage("--adb=[ADB BINARY] 'Path to adb'")
        .arg(Arg::from_usage("--tag [FILTER] 'Tag filters in RE2'").multiple(true))
        .arg(Arg::from_usage("--msg [FILTER] 'Message filters in RE2'").multiple(true))
        .arg_from_usage("--file [FILE] 'Write to file'")
        .arg_from_usage("--format [FORMAT] 'csv or human readable (default)'")
        .arg_from_usage("--input [INPUT] 'Read from file or \"stdin\". Defaults to live log'")
        .arg_from_usage("--level [LEVEL] 'Minumum loglevel'")
        .arg_from_usage("--stdout 'Write to stdout (default)'")
        .arg_from_usage("[DISABLE_COLOR_OUTPUT] --disable-color-output 'Monochrome output'")
        .arg_from_usage("[DISABLE-TAG-SHORTENING] --disable-tag-shortening 'Disable shortening \
                         of tag in human format'")
        .arg_from_usage("-c 'Clear (flush) the entire log and exit'")
        .arg_from_usage("-g 'Get the size of the log's ring buffer and exit'")
        .arg_from_usage("-S 'Output statistics'")
        .get_matches();

    let binary = matches.value_of("adb").unwrap_or("adb");

    let single_shots = ["c", "g", "S"];
    for arg in &single_shots {
        if matches.is_present(arg) {
            let arg = format!("-{}", arg);
            let mut child = Command::new(binary)
                .arg("logcat")
                .arg(arg)
                .spawn()
                .expect("failed to execute adb logcat");
            child.wait().ok();
            return;
        }
    }

    let level = value_t!(matches, "level", Level).unwrap_or(Level::Debug);
    let is_level = |message: &message::Message| -> bool { message.level >= level };

    let prepare_filter = |opt| {
        if matches.is_present(opt) {
            matches.values_of(opt).unwrap().collect()
        } else {
            Vec::<&str>::new()
        }
    };

    let tag_filter: Vec<Regex> =
        prepare_filter("tag").iter().map(|f| Regex::new(f).unwrap()).collect();
    let is_match_tag = |message: &message::Message| -> bool {
        if matches.is_present("tag") {
            for f in &tag_filter {
                if f.is_match(&message.tag) {
                    return true;
                }
            }
            false
        } else {
            true
        }
    };

    let message_filter: Vec<Regex> =
        prepare_filter("msg").iter().map(|f| Regex::new(f).unwrap()).collect();
    let is_match_message = |message: &message::Message| -> bool {
        if matches.is_present("msg") {
            for f in &message_filter {
                if f.is_match(&message.message) {
                    return true;
                }
            }
            false
        } else {
            true
        }
    };

    let mut reader = if matches.is_present("input") {
        match matches.value_of("input") {
            Some("stdin") => BufReader::new(Box::new(std::io::stdin()) as Box<std::io::Read>),
            Some(f) => {
                match std::fs::File::open(f) {
                    Ok(file) => BufReader::new(Box::new(file) as Box<std::io::Read>),
                    Err(e) => panic!("{}", e),
                }
            }
            _ => BufReader::new(Box::new(std::io::stdin()) as Box<std::io::Read>),
        }
    } else {
        let logcat = Command::new(binary)
            .arg("logcat")
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to execute adb");
        BufReader::new(Box::new(logcat.stdout.unwrap()) as Box<std::io::Read>)
    };

    let mut sinks: Vec<Box<Sink>> = Vec::new();
    if matches.is_present("file") {
        sinks.push(Box::new(filewriter::FileWriter::new(&matches)) as Box<Sink>);
    }
    if matches.is_present("stdout") || !matches.is_present("file") {
        sinks.push(Box::new(terminal::Terminal::new(&matches)) as Box<Sink>);
    }

    let mut parser = parser::Parser::new();

    loop {
        let mut buffer: Vec<u8> = Vec::new();
        match reader.read_until(10, &mut buffer) {
            Ok(len) => {
                if len == 0 {
                    break;
                } else {
                    let line = String::from_utf8_lossy(&buffer);
                    let message = parser.parse(&line);
                    if is_match_message(&message) && is_match_tag(&message) && is_level(&message) {
                        for s in &mut sinks {
                            s.process(&message);
                        }
                    }
                }
            }
            Err(e) => println!("Invalid line: {}", e),
        }
    }

    for s in &sinks {
        s.close();
    }
}
