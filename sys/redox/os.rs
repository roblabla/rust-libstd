// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Implementation of `std::os` functionality for unix systems

#![allow(unused_imports)] // lots of cfg code here

use os::unix::prelude::*;

use error::Error as StdError;
use ffi::{CString, CStr, OsString, OsStr};
use fmt;
use io;
use iter;
use libc::{self, c_int, c_char, c_void};
use marker::PhantomData;
use mem;
use memchr;
use path::{self, PathBuf};
use ptr;
use slice;
use str;
use sys_common::mutex::Mutex;
use sys::cvt;
use sys::fd;
use vec;

const TMPBUF_SZ: usize = 128;
static ENV_LOCK: Mutex = Mutex::new();

/// Returns the platform-specific value of errno
pub fn errno() -> i32 {
    0
}

/// Gets a detailed string description for the given error number.
pub fn error_string(errno: i32) -> String {
    if let Some(string) = libc::STR_ERROR.get(errno as usize) {
        string.to_string()
    } else {
        "unknown error".to_string()
    }
}

pub fn getcwd() -> io::Result<PathBuf> {
    let mut buf = [0; 4096];
    let count = cvt(libc::getcwd(&mut buf))?;
    Ok(PathBuf::from(OsString::from_vec(buf[.. count].to_vec())))
}

pub fn chdir(p: &path::Path) -> io::Result<()> {
    cvt(libc::chdir(p.to_str().unwrap())).and(Ok(()))
}

pub struct SplitPaths<'a> {
    iter: iter::Map<slice::Split<'a, u8, fn(&u8) -> bool>,
                    fn(&'a [u8]) -> PathBuf>,
}

pub fn split_paths(unparsed: &OsStr) -> SplitPaths {
    fn bytes_to_path(b: &[u8]) -> PathBuf {
        PathBuf::from(<OsStr as OsStrExt>::from_bytes(b))
    }
    fn is_colon(b: &u8) -> bool { *b == b':' }
    let unparsed = unparsed.as_bytes();
    SplitPaths {
        iter: unparsed.split(is_colon as fn(&u8) -> bool)
                      .map(bytes_to_path as fn(&[u8]) -> PathBuf)
    }
}

impl<'a> Iterator for SplitPaths<'a> {
    type Item = PathBuf;
    fn next(&mut self) -> Option<PathBuf> { self.iter.next() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}

#[derive(Debug)]
pub struct JoinPathsError;

pub fn join_paths<I, T>(paths: I) -> Result<OsString, JoinPathsError>
    where I: Iterator<Item=T>, T: AsRef<OsStr>
{
    let mut joined = Vec::new();
    let sep = b':';

    for (i, path) in paths.enumerate() {
        let path = path.as_ref().as_bytes();
        if i > 0 { joined.push(sep) }
        if path.contains(&sep) {
            return Err(JoinPathsError)
        }
        joined.extend_from_slice(path);
    }
    Ok(OsStringExt::from_vec(joined))
}

impl fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "path segment contains separator `:`".fmt(f)
    }
}

impl StdError for JoinPathsError {
    fn description(&self) -> &str { "failed to join paths" }
}

pub fn current_exe() -> io::Result<PathBuf> {
    use io::ErrorKind;
    Err(io::Error::new(ErrorKind::Other, "Not yet implemented on redox"))
}

pub struct Env {
    iter: vec::IntoIter<(OsString, OsString)>,
    _dont_send_or_sync_me: PhantomData<*mut ()>,
}

impl Iterator for Env {
    type Item = (OsString, OsString);
    fn next(&mut self) -> Option<(OsString, OsString)> { self.iter.next() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}

/// Returns a vector of (variable, value) byte-vector pairs for all the
/// environment variables of the current process.
pub fn env() -> Env {
    unimplemented!();
}

pub fn getenv(_k: &OsStr) -> io::Result<Option<OsString>> {
    unimplemented!();
}

pub fn setenv(_k: &OsStr, _v: &OsStr) -> io::Result<()> {
    unimplemented!();
}

pub fn unsetenv(_n: &OsStr) -> io::Result<()> {
    unimplemented!();
}

pub fn page_size() -> usize {
    4096
}

pub fn temp_dir() -> PathBuf {
    ::env::var_os("TMPDIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("/tmp")
    })
}

pub fn home_dir() -> Option<PathBuf> {
    return ::env::var_os("HOME").map(PathBuf::from);
}

pub fn exit(code: i32) -> ! {
    let _ = libc::exit(code as usize);
    unreachable!();
}
