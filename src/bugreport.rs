// Copyright © 2017 Felix Obenhuber
// This program is free software. It comes without any warranty, to the extent
// permitted by applicable law. You can redistribute it and/or modify it under
// the terms of the Do What The Fuck You Want To Public License, Version 2, as
// published by Sam Hocevar. See the COPYING file for more details.

use adb;
use clap::ArgMatches;
use failure::{err_msg, Error};
use futures::{Future, Stream};
use futures::future::ok;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{DirBuilder, File};
use std::io::BufReader;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use time::{now, strftime};
use tokio_core::reactor::Core;
use tokio_process::CommandExt;
use utils::lossy_lines;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

struct ZipFile {
    zip: ZipWriter<File>,
}

impl ZipFile {
    fn new(filename: &str) -> Result<Self, Error> {
        let file = File::create(&format!("{}.zip", filename))?;
        let options = FileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        let filename_path = PathBuf::from(&filename);
        let f = filename_path
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| err_msg("Failed to get filename"))?;
        let mut zip = ZipWriter::new(file);
        zip.start_file(f, options)?;
        Ok(ZipFile { zip: zip })
    }
}

impl Write for ZipFile {
    fn write(&mut self, buf: &[u8]) -> ::std::io::Result<usize> {
        self.zip.write_all(buf).map(|_| buf.len())
    }

    fn flush(&mut self) -> ::std::io::Result<()> {
        self.zip.finish().map_err(|e| e.into()).map(|_| ())
    }
}

impl Drop for ZipFile {
    fn drop(&mut self) {
        self.flush().expect("Failed to close zipfile");
    }
}

fn report_filename() -> Result<String, Error> {
    #[cfg(not(windows))]
    let sep = ":";
    #[cfg(windows)]
    let sep = "_";

    let format = format!("%m-%d_%H{}%M{}%S", sep, sep);
    Ok(format!("{}-bugreport.txt", strftime(&format, &now())?))
}

/// Performs a dumpstate and write to fs. Note: The Android 7+ dumpstate is not supported.
pub fn create(args: &ArgMatches, core: &mut Core) -> Result<i32, Error> {
    let filename = value_t!(args.value_of("file"), String).unwrap_or(report_filename()?);
    let filename_path = PathBuf::from(&filename);
    if !args.is_present("overwrite") && filename_path.exists() {
        return Err(format_err!("File {} already exists", filename));
    }

    let mut child = Command::new(adb()?)
        .arg("bugreport")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_async(&core.handle())?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| err_msg("Failed get stdout"))?;
    let stdout_reader = BufReader::new(stdout);

    let dir = filename_path.parent().unwrap_or_else(|| Path::new(""));
    if !dir.is_dir() {
        DirBuilder::new()
            .recursive(true)
            .create(dir)
            .map_err(|e| format_err!("Failed to create outfile parent directory: {}", e))?
    }

    let progress = ProgressBar::new(::std::u64::MAX);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.yellow} {msg:.dim.bold} {pos:>7.dim} {elapsed_precise:.dim}")
            .progress_chars(" • "),
    );
    progress.set_message("Connecting");

    let mut write = if args.is_present("zip") {
        Box::new(ZipFile::new(&filename)?) as Box<Write>
    } else {
        Box::new(File::create(&filename)?) as Box<Write>
    };

    progress.set_message("Pulling bugreport line");

    let output = lossy_lines(stdout_reader)
        .for_each(|l| {
            write.write_all(l.as_bytes()).expect("Failed to write");
            write.write_all(b"\n").expect("Failed to write");
            progress.inc(1);
            ok(())
        })
        .then(|r| {
            progress.set_style(ProgressStyle::default_bar().template("{msg:.dim.bold}"));
            progress.finish_with_message(&format!("Finished {}.", filename_path.display()));
            r
        });
    core.run(output).map_err(|e| e.into()).map(|_| 0)
}
