// Copyright © 2017 Felix Obenhuber
// This program is free software. It comes without any warranty, to the extent
// permitted by applicable law. You can redistribute it and/or modify it under
// the terms of the Do What The Fuck You Want To Public License, Version 2, as
// published by Sam Hocevar. See the COPYING file for more details.

use clap::{App, AppSettings, Arg, ArgMatches, Shell, SubCommand};
use failure::Error;
use record::Level;
use std::io::stdout;

lazy_static! {
    static ref ABOUT: String = { format!("A 'adb logcat' wrapper and log processor. Your config directory is \"{}\".",
                                         ::config_dir().unwrap_or_else(|_| "unknown".into()).display()) };
}

pub fn cli() -> App<'static, 'static> {
    App::new(crate_name!())
        .setting(AppSettings::ColoredHelp)
        .version(crate_version!())
        .author(crate_authors!())
        .about(ABOUT.as_str())
        .arg(Arg::with_name("buffer")
             .short("b")
             .long("buffer")
             .multiple(true)
             .takes_value(true)
             .conflicts_with_all(&["input", "COMMAND"])
             .help("Select specific (logcat) log buffers. Defaults to main, events, kernel and crash (logcat default)"))
        .arg(Arg::with_name("clear")
             .short("c")
             .long("clear")
             .help("Clear (flush) the entire log and exit"))
        .arg(Arg::with_name("dump")
             .short("d")
             .long("dump")
             .conflicts_with_all(&["input", "COMMAND", "restart"])
             .help("Dump the log and then exit (don't block)"))
        .arg(Arg::with_name("format")
             .long("format")
             .short("f")
             .takes_value(true)
             .possible_values(&["csv", "html", "human", "json", "raw"]).help("Output format. Defaults to human on stdout and raw on file output"))
        .arg(Arg::with_name("filename_format")
             .long("filename-format")
             .short("a")
             .takes_value(true)
             .requires("output")
             .possible_values(&["single", "enumerate", "date"])
             .help( "Select a format for output file names. By passing 'single' the filename provided with the '-o' option is used (default).\
                    'enumerate' appends a file sequence number after the filename passed with '-o' option whenever a new file is created \
                    (see 'records-per-file' option). 'date' will prefix the output filename with the current local date when a new file is created"))
        .arg(Arg::with_name("head")
             .short("H")
             .long("head")
             .takes_value(true)
             .conflicts_with_all(&["tail", "restart"])
             .help( "Read n records and exit"))
        .arg(Arg::with_name("highlight")
             .short("h")
             .long("highlight")
             .takes_value(true)
             .multiple(true)
             .conflicts_with_all(&["monochrome", "output"])
             .help( "Highlight messages that match this pattern in RE2. The prefix '!' inverts the match"))
        .arg(Arg::with_name("input")
             .short("i")
             .long("input")
             .takes_value(true)
             .multiple(true)
             .help( "Read from file instead of command. Use 'serial://COM0@115200,8N1 or similiar for reading a serial port"))
        .arg(Arg::with_name("level")
             .short("l")
             .long("level")
             .takes_value(true)
             .possible_values(Level::values()).help("Minimum level"))
        .arg(Arg::with_name("message")
             .short("m")
             .long("message")
             .takes_value(true)
             .multiple(true)
             .help("Message filters in RE2. The prefix '!' inverts the match"))
        .arg(Arg::with_name("monochrome")
             .long("monochrome")
             .conflicts_with_all(&["highlight", "output"])
             .help("Monochrome terminal output"))
        .arg(Arg::with_name("no_dimm")
             .long("no-dimm")
             .conflicts_with("output")
             .help("Use white as dimm color"))
        .arg(Arg::with_name("hide_timestamp")
             .long("hide-timestamp")
             .conflicts_with("output")
             .help("Hide timestamp in terminal output"))
        .arg(Arg::with_name("output")
             .short("o")
             .long("output")
             .takes_value(true)
             .help("Write output to file"))
        .arg(Arg::with_name("overwrite")
             .long("overwrite")
             .requires("output")
             .help("Overwrite output file if present"))
        .arg(Arg::with_name("profiles_path")
             .short("P")
             .long("profiles-path")
             .takes_value(true)
             .help("Manually specify profile file (overrules ROGCAT_PROFILES)"))
        .arg(Arg::with_name("profile")
             .short("p")
             .long("profile")
             .takes_value(true)
             .help("Select profile"))
        .arg(Arg::with_name("records_per_file")
             .short("n")
             .long("records-per-file")
             .takes_value(true)
             .requires("output")
             .help( "Write n records per file. Use k, M, G suffixes or a plain number"))
        .arg(Arg::with_name("restart")
             .short("r")
             .long("restart")
             .conflicts_with_all(&["dump", "input", "tail"])
             .help("Restart command on exit"))
        .arg(Arg::with_name("skip")
             .short("s")
             .long("skip")
             .help("Skip records on a command restart until the last received last record is received again. Use with caution!"))
        .arg(Arg::with_name("shorten_tags")
             .long("shorten-tags")
             .conflicts_with("output")
             .help( "Shorten tags by removing vovels if too long for human terminal format"))
        .arg(Arg::with_name("show_date")
             .long("show-date")
             .conflicts_with("output")
             .help("Show month and day in terminal output"))
        .arg(Arg::with_name("show_time_diff")
             .long("show-time-diff")
             .conflicts_with("output")
             .help( "Show the time difference between the occurence of equal tags in terminal output"))
        .arg(Arg::with_name("tag")
             .short("t")
             .long("tag")
             .takes_value(true)
             .multiple(true).help("Tag filters in RE2. The prefix '!' inverts the match"))
        .arg(Arg::with_name("tail")
             .short("T")
             .long("tail")
             .takes_value(true)
             .conflicts_with_all(&["input", "COMMAND", "restart"])
             .help("Dump only the most recent <COUNT> lines (implies --dump)"))
        .arg(Arg::with_name("COMMAND")
             .help( "Optional command to run and capture stdout from. Pass \"-\" to d capture stdin'. If omitted, rogcat will run \"adb logcat -b all\" and restarts this commmand if 'adb' terminates",))
        .subcommand(SubCommand::with_name("bugreport")
                .about("Capture bugreport. This is only works for Android versions < 7.")
                .arg(Arg::with_name("zip").short("z").long("zip").help("Zip report"))
                .arg(Arg::with_name("overwrite").long("overwrite").help("Overwrite report file if present"))
                .arg(Arg::with_name("file").help("Output file name - defaults to <now>-bugreport")))
        .subcommand(SubCommand::with_name("completions")
                .about("Generates completion scripts")
                .arg(Arg::with_name("shell")
                        .required(true)
                        .possible_values(&["bash", "fish", "zsh"])
                        .help("The shell to generate the script for")))
        .subcommand(SubCommand::with_name("devices")
                .about("Show list of available devices"))
        .subcommand(SubCommand::with_name("profiles")
                .about("Show and manage profiles")
                .arg(Arg::with_name("list")
                     .short("l")
                     .long("list")
                     .help("List profiles"))
                .arg(Arg::with_name("examples")
                     .short("e")
                     .long("examples")
                     .help("Show example profiles settings")))
        .subcommand(SubCommand::with_name("log")
                .about("Add log message(s) log buffer")
                .arg(Arg::with_name("tag")
                        .short("t")
                        .long("tag")
                        .takes_value(true)
                        .help("Log tag"))
                .arg(Arg::with_name("level")
                        .short("l")
                        .long("level")
                        .takes_value(true)
                        .possible_values(&[ "trace", "debug", "info", "warn", "error", "fatal", "assert", "T", "D", "I", "W", "E", "F", "A" ],)
                        .help("Log on level"))
                .arg_from_usage("[MESSAGE] 'Log message. Pass \"-\" to capture from stdin'."))
}

pub fn subcommand_completions(args: &ArgMatches) -> Result<i32, Error> {
    args.value_of("shell")
        .ok_or(format_err!("Missing required argument shell"))
        .map(|s| s.parse::<Shell>())
        .map(|s| {
            cli().gen_completions_to(crate_name!(), s.unwrap(), &mut stdout());
            0
        })
}
