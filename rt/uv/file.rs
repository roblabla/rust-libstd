// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use prelude::*;
use ptr::null;
use libc::c_void;
use rt::uv::{Request, NativeHandle, Loop, FsCallback, Buf,
             status_to_maybe_uv_error_with_loop,
             vec_to_uv_buf, vec_from_uv_buf};
use rt::uv::uvll;
use rt::uv::uvll::*;
use path::Path;
use cast::transmute;
use libc::{c_int};
use option::{None, Some, Option};
use vec;

pub struct FsRequest(*uvll::uv_fs_t);
impl Request for FsRequest;

#[allow(non_camel_case_types)]
pub enum UvFileFlag {
    O_RDONLY,
    O_WRONLY,
    O_RDWR,
    O_CREAT,
    O_TRUNC
}
// just want enough to get 0644
#[allow(non_camel_case_types)]
pub enum UvFileMode {
    S_IWUSR,
    S_IRUSR,
    S_IRGRP,
    S_IROTH
}
pub fn map_flag(v: UvFileFlag) -> int {
    unsafe {
        match v {
            O_RDONLY => uvll::get_O_RDONLY() as int,
            O_WRONLY => uvll::get_O_WRONLY() as int,
            O_RDWR => uvll::get_O_RDWR() as int,
            O_CREAT => uvll::get_O_CREAT() as int,
            O_TRUNC => uvll::get_O_TRUNC() as int
        }
    }
}
pub fn map_mode(v: UvFileMode) -> int {
    unsafe {
        match v {
            S_IWUSR => uvll::get_S_IWUSR() as int,
            S_IRUSR => uvll::get_S_IRUSR() as int,
            S_IRGRP => uvll::get_S_IRGRP() as int,
            S_IROTH => uvll::get_S_IROTH() as int
        }
    }
}

pub struct RequestData {
    complete_cb: Option<FsCallback>,
    buf: Option<Buf>,
    raw_fd: Option<c_int>
}

impl FsRequest {
    pub fn new(cb: Option<FsCallback>) -> FsRequest {
        let fs_req = unsafe { malloc_req(UV_FS) };
        assert!(fs_req.is_not_null());
        let fs_req: FsRequest = NativeHandle::from_native_handle(fs_req);
        fs_req.install_req_data(cb);
        fs_req
    }

    pub fn install_req_data(&self, cb: Option<FsCallback>) {
        let fs_req = (self.native_handle()) as *uvll::uv_write_t;
        let data = ~RequestData {
            complete_cb: cb,
            buf: None,
            raw_fd: None
        };
        unsafe {
            let data = transmute::<~RequestData, *c_void>(data);
            uvll::set_data_for_req(fs_req, data);
        }
    }

    fn get_req_data<'r>(&'r mut self) -> &'r mut RequestData {
        unsafe {
            let data = uvll::get_data_for_req((self.native_handle()));
            let data = transmute::<&*c_void, &mut ~RequestData>(&data);
            return &mut **data;
        }
    }

    pub fn get_result(&mut self) -> c_int {
        unsafe {
            uvll::get_result_from_fs_req(self.native_handle())
        }
    }

    pub fn get_loop(&self) -> Loop {
        unsafe { Loop{handle:uvll::get_loop_from_fs_req(self.native_handle())} }
    }

    fn cleanup_and_delete(self) {
        unsafe {
            let data = uvll::get_data_for_req(self.native_handle());
            let mut _data = transmute::<*c_void, ~RequestData>(data);
            // if set we're going to convert the buf param back into
            // a rust vec, as that's the mechanism by which the raw
            // uv_buf_t's .base field gets freed. We immediately discard
            // the result
            if _data.buf.is_some() {
                let buf = _data.buf.take_unwrap();
                vec_from_uv_buf(buf);
            }
            uvll::set_data_for_req(self.native_handle(), null::<()>());
            uvll::fs_req_cleanup(self.native_handle());
            free_req(self.native_handle() as *c_void)
        }
    }
}

impl NativeHandle<*uvll::uv_fs_t> for FsRequest {
    fn from_native_handle(handle: *uvll:: uv_fs_t) -> FsRequest {
        FsRequest(handle)
    }
    fn native_handle(&self) -> *uvll::uv_fs_t {
        match self { &FsRequest(ptr) => ptr }
    }
}

pub struct FileDescriptor(c_int);
impl FileDescriptor {
    fn new(fd: c_int) -> FileDescriptor {
        FileDescriptor(fd)
    }

    pub fn from_open_req(req: &mut FsRequest) -> FileDescriptor {
        FileDescriptor::new(req.get_result())
    }

    pub fn open(loop_: Loop, path: Path, flags: int, mode: int,
               cb: FsCallback) -> int {
        let req = FsRequest::new(Some(cb));
        path.to_str().to_c_str().with_ref(|p| unsafe {
            uvll::fs_open(loop_.native_handle(),
                          req.native_handle(), p, flags, mode, complete_cb) as int
        })
    }

    pub fn unlink(loop_: Loop, path: Path, cb: FsCallback) -> int {
        let req = FsRequest::new(Some(cb));
        path.to_str().to_c_str().with_ref(|p| unsafe {
            uvll::fs_unlink(loop_.native_handle(),
                          req.native_handle(), p, complete_cb) as int
        })
    }

    // as per bnoordhuis in #libuv: offset >= 0 uses prwrite instead of write
    pub fn write(&self, loop_: Loop, buf: ~[u8], offset: i64, cb: FsCallback)
          -> int {
        let mut req = FsRequest::new(Some(cb));
        let len = buf.len();
        let buf = vec_to_uv_buf(buf);
        let base_ptr = buf.base as *c_void;
        req.get_req_data().buf = Some(buf);
        req.get_req_data().raw_fd = Some(self.native_handle());
        unsafe {
            uvll::fs_write(loop_.native_handle(), req.native_handle(),
                           self.native_handle(), base_ptr,
                           len, offset, complete_cb) as int
        }
    }

    // really contemplated having this just take a read_len param and have
    // the buf live in the scope of this request.. but decided that exposing
    // an unsafe mechanism that takes a buf_ptr and len would be much more
    // flexible, but the caller is now in the position of managing that
    // buf (with all of the sadface that this entails)
    pub fn read(&self, loop_: Loop, buf_ptr: Option<*c_void>, len: uint, offset: i64, cb: FsCallback)
          -> int {
        let mut req = FsRequest::new(Some(cb));
        req.get_req_data().raw_fd = Some(self.native_handle());
        unsafe {
            let buf_ptr = match buf_ptr {
                Some(ptr) => ptr,
                None => {
                    let buf = vec::from_elem(len, 0u8);
                    let buf = vec_to_uv_buf(buf);
                    req.get_req_data().buf = Some(buf);
                    buf.base as *c_void
                }
            };
            uvll::fs_read(loop_.native_handle(), req.native_handle(),
                           self.native_handle(), buf_ptr,
                           len, offset, complete_cb) as int
        }
    }

    pub fn close(self, loop_: Loop, cb: FsCallback) -> int {
        let req = FsRequest::new(Some(cb));
        unsafe {
            uvll::fs_close(loop_.native_handle(), req.native_handle(),
                           self.native_handle(), complete_cb) as int
        }
    }
}
extern fn complete_cb(req: *uv_fs_t) {
    let mut req: FsRequest = NativeHandle::from_native_handle(req);
    let loop_ = req.get_loop();
    // pull the user cb out of the req data
    let cb = {
        let data = req.get_req_data();
        assert!(data.complete_cb.is_some());
        // option dance, option dance. oooooh yeah.
        data.complete_cb.take_unwrap()
    };
    // in uv_fs_open calls, the result will be the fd in the
    // case of success, otherwise it's -1 indicating an error
    let result = req.get_result();
    let status = status_to_maybe_uv_error_with_loop(
        loop_.native_handle(), result);
    // we have a req and status, call the user cb..
    // only giving the user a ref to the FsRequest, as we
    // have to clean it up, afterwards (and they aren't really
    // reusable, anyways
    cb(&mut req, status);
    // clean up the req (and its data!) after calling the user cb
    req.cleanup_and_delete();
}

impl NativeHandle<c_int> for FileDescriptor {
    fn from_native_handle(handle: c_int) -> FileDescriptor {
        FileDescriptor(handle)
    }
    fn native_handle(&self) -> c_int {
        match self { &FileDescriptor(ptr) => ptr }
    }
}

mod test {
    use super::*;
    //use rt::test::*;
    use libc::{STDOUT_FILENO};
    use str;
    use unstable::run_in_bare_thread;
    use path::Path;
    use rt::uv::{Loop, vec_from_uv_buf};//, slice_to_uv_buf};
    use option::{None};

    fn file_test_full_simple_impl() {
        debug!("hello?")
        do run_in_bare_thread {
            debug!("In bare thread")
            let mut loop_ = Loop::new();
            let create_flags = map_flag(O_RDWR) |
                map_flag(O_CREAT);
            let read_flags = map_flag(O_RDONLY);
            // 0644
            let mode = map_mode(S_IWUSR) |
                map_mode(S_IRUSR) |
                map_mode(S_IRGRP) |
                map_mode(S_IROTH);
            let path_str = "./file_full_simple.txt";
            let write_val = "hello";
            do FileDescriptor::open(loop_, Path(path_str), create_flags, mode)
            |req, uverr| {
                let loop_ = req.get_loop();
                assert!(uverr.is_none());
                let fd = FileDescriptor::from_open_req(req);
                let msg: ~[u8] = write_val.as_bytes().to_owned();
                let raw_fd = fd.native_handle();
                do fd.write(loop_, msg, -1) |_, uverr| {
                    let fd = FileDescriptor(raw_fd);
                    do fd.close(loop_) |req, _| {
                        let loop_ = req.get_loop();
                        assert!(uverr.is_none());
                        do FileDescriptor::open(loop_, Path(path_str), read_flags,0)
                            |req, uverr| {
                            assert!(uverr.is_none());
                            let loop_ = req.get_loop();
                            let len = 1028;
                            let fd = FileDescriptor::from_open_req(req);
                            let raw_fd = fd.native_handle();
                            do fd.read(loop_, None, len, 0) |req, uverr| {
                                assert!(uverr.is_none());
                                let loop_ = req.get_loop();
                                // we know nread >=0 because uverr is none..
                                let nread = req.get_result() as uint;
                                // nread == 0 would be EOF
                                if nread > 0 {
                                    let buf = vec_from_uv_buf(
                                        req.get_req_data().buf.take_unwrap())
                                        .take_unwrap();
                                    let read_str = str::from_bytes(
                                        buf.slice(0,
                                                  nread));
                                    assert!(read_str == ~"hello");
                                    do FileDescriptor(raw_fd).close(loop_) |_,uverr| {
                                        assert!(uverr.is_none());
                                        do FileDescriptor::unlink(loop_, Path(path_str))
                                        |_,uverr| {
                                            assert!(uverr.is_none());
                                        };
                                    };
                                }
                            };
                        };
                    };
                };
            };
            loop_.run();
            loop_.close();
        }
    }

    #[test]
    fn file_test_full_simple() {
        file_test_full_simple_impl();
    }

    fn naive_print(loop_: Loop, input: ~str) {
        let stdout = FileDescriptor(STDOUT_FILENO);
        let msg = input.as_bytes().to_owned();
        do stdout.write(loop_, msg, -1) |_, uverr| {
            assert!(uverr.is_none());
        };
    }

    #[test]
    fn file_test_write_to_stdout() {
        do run_in_bare_thread {
            let mut loop_ = Loop::new();
            naive_print(loop_, ~"zanzibar!\n");
            loop_.run();
            loop_.close();
        };
    }
}
