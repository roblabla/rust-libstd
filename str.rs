// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * String manipulation
 *
 * Strings are a packed UTF-8 representation of text, stored as null
 * terminated buffers of u8 bytes.  Strings should be indexed in bytes,
 * for efficiency, but UTF-8 unsafe operations should be avoided.  For
 * some heavy-duty uses, try std::rope.
 */

use at_vec;
use cast::transmute;
use cast;
use char;
use clone::Clone;
use cmp::{TotalOrd, Ordering, Less, Equal, Greater};
use container::Container;
use iter::Times;
use iterator::{Iterator, IteratorUtil, FilterIterator};
use libc;
use option::{None, Option, Some};
use old_iter::{BaseIter, EqIter};
use ptr;
use ptr::RawPtr;
use str;
use to_str::ToStr;
use uint;
use vec;
use vec::{OwnedVector, OwnedCopyableVector, ImmutableVector};

#[cfg(not(test))] use cmp::{Eq, Ord, Equiv, TotalEq};

/*
Section: Conditions
*/
condition! {
    not_utf8: (~str) -> ~str;
}

/*
Section: Creating a string
*/

/**
 * Convert a vector of bytes to a new UTF-8 string
 *
 * # Failure
 *
 * Raises the `not_utf8` condition if invalid UTF-8
 */

pub fn from_bytes(vv: &[u8]) -> ~str {
    use str::not_utf8::cond;

    if !is_utf8(vv) {
        let first_bad_byte = vec::find(vv, |b| !is_utf8([*b])).get();
        cond.raise(fmt!("from_bytes: input is not UTF-8; first bad byte is %u",
                        first_bad_byte as uint))
    }
    else {
        return unsafe { raw::from_bytes(vv) }
    }
}

/**
 * Convert a vector of bytes to a UTF-8 string.
 * The vector needs to be one byte longer than the string, and end with a 0 byte.
 *
 * Compared to `from_bytes()`, this fn doesn't need to allocate a new owned str.
 *
 * # Failure
 *
 * Fails if invalid UTF-8
 * Fails if not null terminated
 */
pub fn from_bytes_with_null<'a>(vv: &'a [u8]) -> &'a str {
    assert_eq!(vv[vv.len() - 1], 0);
    assert!(is_utf8(vv));
    return unsafe { raw::from_bytes_with_null(vv) };
}

/**
 * Converts a vector to a string slice without performing any allocations.
 *
 * Once the slice has been validated as utf-8, it is transmuted in-place and
 * returned as a '&str' instead of a '&[u8]'
 *
 * # Failure
 *
 * Fails if invalid UTF-8
 */
pub fn from_bytes_slice<'a>(vector: &'a [u8]) -> &'a str {
    unsafe {
        assert!(is_utf8(vector));
        let (ptr, len): (*u8, uint) = ::cast::transmute(vector);
        let string: &'a str = ::cast::transmute((ptr, len + 1));
        string
    }
}

/// Copy a slice into a new unique str
#[inline(always)]
pub fn to_owned(s: &str) -> ~str {
    unsafe { raw::slice_bytes_owned(s, 0, s.len()) }
}

impl ToStr for ~str {
    #[inline(always)]
    fn to_str(&self) -> ~str { to_owned(*self) }
}
impl<'self> ToStr for &'self str {
    #[inline(always)]
    fn to_str(&self) -> ~str { to_owned(*self) }
}
impl ToStr for @str {
    #[inline(always)]
    fn to_str(&self) -> ~str { to_owned(*self) }
}

/**
 * Convert a byte to a UTF-8 string
 *
 * # Failure
 *
 * Fails if invalid UTF-8
 */
pub fn from_byte(b: u8) -> ~str {
    assert!(b < 128u8);
    unsafe { ::cast::transmute(~[b, 0u8]) }
}

/// Appends a character at the end of a string
pub fn push_char(s: &mut ~str, ch: char) {
    unsafe {
        let code = ch as uint;
        let nb = if code < max_one_b { 1u }
        else if code < max_two_b { 2u }
        else if code < max_three_b { 3u }
        else if code < max_four_b { 4u }
        else if code < max_five_b { 5u }
        else { 6u };
        let len = s.len();
        let new_len = len + nb;
        reserve_at_least(&mut *s, new_len);
        let off = len;
        do as_buf(*s) |buf, _len| {
            let buf: *mut u8 = ::cast::transmute(buf);
            match nb {
                1u => {
                    *ptr::mut_offset(buf, off) = code as u8;
                }
                2u => {
                    *ptr::mut_offset(buf, off) = (code >> 6u & 31u | tag_two_b) as u8;
                    *ptr::mut_offset(buf, off + 1u) = (code & 63u | tag_cont) as u8;
                }
                3u => {
                    *ptr::mut_offset(buf, off) = (code >> 12u & 15u | tag_three_b) as u8;
                    *ptr::mut_offset(buf, off + 1u) = (code >> 6u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 2u) = (code & 63u | tag_cont) as u8;
                }
                4u => {
                    *ptr::mut_offset(buf, off) = (code >> 18u & 7u | tag_four_b) as u8;
                    *ptr::mut_offset(buf, off + 1u) = (code >> 12u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 2u) = (code >> 6u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 3u) = (code & 63u | tag_cont) as u8;
                }
                5u => {
                    *ptr::mut_offset(buf, off) = (code >> 24u & 3u | tag_five_b) as u8;
                    *ptr::mut_offset(buf, off + 1u) = (code >> 18u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 2u) = (code >> 12u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 3u) = (code >> 6u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 4u) = (code & 63u | tag_cont) as u8;
                }
                6u => {
                    *ptr::mut_offset(buf, off) = (code >> 30u & 1u | tag_six_b) as u8;
                    *ptr::mut_offset(buf, off + 1u) = (code >> 24u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 2u) = (code >> 18u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 3u) = (code >> 12u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 4u) = (code >> 6u & 63u | tag_cont) as u8;
                    *ptr::mut_offset(buf, off + 5u) = (code & 63u | tag_cont) as u8;
                }
                _ => {}
            }
        }
        raw::set_len(s, new_len);
    }
}

/// Convert a char to a string
pub fn from_char(ch: char) -> ~str {
    let mut buf = ~"";
    push_char(&mut buf, ch);
    buf
}

/// Convert a vector of chars to a string
pub fn from_chars(chs: &[char]) -> ~str {
    let mut buf = ~"";
    reserve(&mut buf, chs.len());
    for chs.each |ch| {
        push_char(&mut buf, *ch);
    }
    buf
}

/// Appends a string slice to the back of a string, without overallocating
#[inline(always)]
pub fn push_str_no_overallocate(lhs: &mut ~str, rhs: &str) {
    unsafe {
        let llen = lhs.len();
        let rlen = rhs.len();
        reserve(&mut *lhs, llen + rlen);
        do as_buf(*lhs) |lbuf, _llen| {
            do as_buf(rhs) |rbuf, _rlen| {
                let dst = ptr::offset(lbuf, llen);
                let dst = ::cast::transmute_mut_unsafe(dst);
                ptr::copy_memory(dst, rbuf, rlen);
            }
        }
        raw::set_len(lhs, llen + rlen);
    }
}

/// Appends a string slice to the back of a string
#[inline(always)]
pub fn push_str(lhs: &mut ~str, rhs: &str) {
    unsafe {
        let llen = lhs.len();
        let rlen = rhs.len();
        reserve_at_least(&mut *lhs, llen + rlen);
        do as_buf(*lhs) |lbuf, _llen| {
            do as_buf(rhs) |rbuf, _rlen| {
                let dst = ptr::offset(lbuf, llen);
                let dst = ::cast::transmute_mut_unsafe(dst);
                ptr::copy_memory(dst, rbuf, rlen);
            }
        }
        raw::set_len(lhs, llen + rlen);
    }
}

/// Concatenate two strings together
#[inline(always)]
pub fn append(lhs: ~str, rhs: &str) -> ~str {
    let mut v = lhs;
    push_str_no_overallocate(&mut v, rhs);
    v
}

/// Concatenate a vector of strings
pub fn concat(v: &[~str]) -> ~str { v.concat() }

/// Concatenate a vector of strings
pub fn concat_slices(v: &[&str]) -> ~str { v.concat() }

/// Concatenate a vector of strings, placing a given separator between each
pub fn connect(v: &[~str], sep: &str) -> ~str { v.connect(sep) }

/// Concatenate a vector of strings, placing a given separator between each
pub fn connect_slices(v: &[&str], sep: &str) -> ~str { v.connect(sep) }

#[allow(missing_doc)]
pub trait StrVector {
    pub fn concat(&self) -> ~str;
    pub fn connect(&self, sep: &str) -> ~str;
}

impl<'self> StrVector for &'self [~str] {
    /// Concatenate a vector of strings.
    pub fn concat(&self) -> ~str {
        if self.is_empty() { return ~""; }

        let mut len = 0;
        for self.each |ss| {
            len += ss.len();
        }
        let mut s = ~"";

        reserve(&mut s, len);

        unsafe {
            do as_buf(s) |buf, _| {
                let mut buf = ::cast::transmute_mut_unsafe(buf);
                for self.each |ss| {
                    do as_buf(*ss) |ssbuf, sslen| {
                        let sslen = sslen - 1;
                        ptr::copy_memory(buf, ssbuf, sslen);
                        buf = buf.offset(sslen);
                    }
                }
            }
            raw::set_len(&mut s, len);
        }
        s
    }

    /// Concatenate a vector of strings, placing a given separator between each.
    pub fn connect(&self, sep: &str) -> ~str {
        if self.is_empty() { return ~""; }

        // concat is faster
        if sep.is_empty() { return self.concat(); }

        // this is wrong without the guarantee that `self` is non-empty
        let mut len = sep.len() * (self.len() - 1);
        for self.each |ss| {
            len += ss.len();
        }
        let mut s = ~"";
        let mut first = true;

        reserve(&mut s, len);

        unsafe {
            do as_buf(s) |buf, _| {
                do as_buf(sep) |sepbuf, seplen| {
                    let seplen = seplen - 1;
                    let mut buf = ::cast::transmute_mut_unsafe(buf);
                    for self.each |ss| {
                        do as_buf(*ss) |ssbuf, sslen| {
                            let sslen = sslen - 1;
                            if first {
                                first = false;
                            } else {
                                ptr::copy_memory(buf, sepbuf, seplen);
                                buf = buf.offset(seplen);
                            }
                            ptr::copy_memory(buf, ssbuf, sslen);
                            buf = buf.offset(sslen);
                        }
                    }
                }
            }
            raw::set_len(&mut s, len);
        }
        s
    }
}

impl<'self> StrVector for &'self [&'self str] {
    /// Concatenate a vector of strings.
    pub fn concat(&self) -> ~str {
        if self.is_empty() { return ~""; }

        let mut len = 0;
        for self.each |ss| {
            len += ss.len();
        }
        let mut s = ~"";

        reserve(&mut s, len);

        unsafe {
            do as_buf(s) |buf, _| {
                let mut buf = ::cast::transmute_mut_unsafe(buf);
                for self.each |ss| {
                    do as_buf(*ss) |ssbuf, sslen| {
                        let sslen = sslen - 1;
                        ptr::copy_memory(buf, ssbuf, sslen);
                        buf = buf.offset(sslen);
                    }
                }
            }
            raw::set_len(&mut s, len);
        }
        s
    }

    /// Concatenate a vector of strings, placing a given separator between each.
    pub fn connect(&self, sep: &str) -> ~str {
        if self.is_empty() { return ~""; }

        // concat is faster
        if sep.is_empty() { return self.concat(); }

        // this is wrong without the guarantee that `self` is non-empty
        let mut len = sep.len() * (self.len() - 1);
        for self.each |ss| {
            len += ss.len();
        }
        let mut s = ~"";
        let mut first = true;

        reserve(&mut s, len);

        unsafe {
            do as_buf(s) |buf, _| {
                do as_buf(sep) |sepbuf, seplen| {
                    let seplen = seplen - 1;
                    let mut buf = ::cast::transmute_mut_unsafe(buf);
                    for self.each |ss| {
                        do as_buf(*ss) |ssbuf, sslen| {
                            let sslen = sslen - 1;
                            if first {
                                first = false;
                            } else {
                                ptr::copy_memory(buf, sepbuf, seplen);
                                buf = buf.offset(seplen);
                            }
                            ptr::copy_memory(buf, ssbuf, sslen);
                            buf = buf.offset(sslen);
                        }
                    }
                }
            }
            raw::set_len(&mut s, len);
        }
        s
    }
}

/// Given a string, make a new string with repeated copies of it
pub fn repeat(ss: &str, nn: uint) -> ~str {
    do as_buf(ss) |buf, len| {
        let mut ret = ~"";
        // ignore the NULL terminator
        let len = len - 1;
        reserve(&mut ret, nn * len);

        unsafe {
            do as_buf(ret) |rbuf, _len| {
                let mut rbuf = ::cast::transmute_mut_unsafe(rbuf);

                for nn.times {
                    ptr::copy_memory(rbuf, buf, len);
                    rbuf = rbuf.offset(len);
                }
            }
            raw::set_len(&mut ret, nn * len);
        }
        ret
    }
}

/*
Section: Adding to and removing from a string
*/

/**
 * Remove the final character from a string and return it
 *
 * # Failure
 *
 * If the string does not contain any characters
 */
pub fn pop_char(s: &mut ~str) -> char {
    let end = s.len();
    assert!(end > 0u);
    let CharRange {ch, next} = char_range_at_reverse(*s, end);
    unsafe { raw::set_len(s, next); }
    return ch;
}

/**
 * Remove the first character from a string and return it
 *
 * # Failure
 *
 * If the string does not contain any characters
 */
pub fn shift_char(s: &mut ~str) -> char {
    let CharRange {ch, next} = char_range_at(*s, 0u);
    *s = unsafe { raw::slice_bytes_owned(*s, next, s.len()) };
    return ch;
}

/**
 * Removes the first character from a string slice and returns it. This does
 * not allocate a new string; instead, it mutates a slice to point one
 * character beyond the character that was shifted.
 *
 * # Failure
 *
 * If the string does not contain any characters
 */
#[inline]
pub fn slice_shift_char<'a>(s: &'a str) -> (char, &'a str) {
    let CharRange {ch, next} = char_range_at(s, 0u);
    let next_s = unsafe { raw::slice_bytes(s, next, s.len()) };
    return (ch, next_s);
}

/// Prepend a char to a string
pub fn unshift_char(s: &mut ~str, ch: char) {
    // This could be more efficient.
    let mut new_str = ~"";
    new_str.push_char(ch);
    new_str.push_str(*s);
    *s = new_str;
}

/**
 * Returns a string with leading `chars_to_trim` removed.
 *
 * # Arguments
 *
 * * s - A string
 * * chars_to_trim - A vector of chars
 *
 */
pub fn trim_left_chars<'a>(s: &'a str, chars_to_trim: &[char]) -> &'a str {
    if chars_to_trim.is_empty() { return s; }

    match find(s, |c| !chars_to_trim.contains(&c)) {
      None => "",
      Some(first) => unsafe { raw::slice_bytes(s, first, s.len()) }
    }
}

/**
 * Returns a string with trailing `chars_to_trim` removed.
 *
 * # Arguments
 *
 * * s - A string
 * * chars_to_trim - A vector of chars
 *
 */
pub fn trim_right_chars<'a>(s: &'a str, chars_to_trim: &[char]) -> &'a str {
    if chars_to_trim.is_empty() { return s; }

    match rfind(s, |c| !chars_to_trim.contains(&c)) {
      None => "",
      Some(last) => {
        let next = char_range_at(s, last).next;
        unsafe { raw::slice_bytes(s, 0u, next) }
      }
    }
}

/**
 * Returns a string with leading and trailing `chars_to_trim` removed.
 *
 * # Arguments
 *
 * * s - A string
 * * chars_to_trim - A vector of chars
 *
 */
pub fn trim_chars<'a>(s: &'a str, chars_to_trim: &[char]) -> &'a str {
    trim_left_chars(trim_right_chars(s, chars_to_trim), chars_to_trim)
}

/// Returns a string with leading whitespace removed
pub fn trim_left<'a>(s: &'a str) -> &'a str {
    match find(s, |c| !char::is_whitespace(c)) {
      None => "",
      Some(first) => unsafe { raw::slice_bytes(s, first, s.len()) }
    }
}

/// Returns a string with trailing whitespace removed
pub fn trim_right<'a>(s: &'a str) -> &'a str {
    match rfind(s, |c| !char::is_whitespace(c)) {
      None => "",
      Some(last) => {
        let next = char_range_at(s, last).next;
        unsafe { raw::slice_bytes(s, 0u, next) }
      }
    }
}

/// Returns a string with leading and trailing whitespace removed
pub fn trim<'a>(s: &'a str) -> &'a str { trim_left(trim_right(s)) }

/*
Section: Transforming strings
*/

/**
 * Converts a string to a unique vector of bytes
 *
 * The result vector is not null-terminated.
 */
pub fn to_bytes(s: &str) -> ~[u8] {
    unsafe {
        let mut v: ~[u8] = ::cast::transmute(to_owned(s));
        vec::raw::set_len(&mut v, s.len());
        v
    }
}

/// Work with the string as a byte slice, not including trailing null.
#[inline(always)]
pub fn byte_slice<T>(s: &str, f: &fn(v: &[u8]) -> T) -> T {
    do as_buf(s) |p,n| {
        unsafe { vec::raw::buf_as_slice(p, n-1u, f) }
    }
}

/// Work with the string as a byte slice, not including trailing null, without
/// a callback.
#[inline(always)]
pub fn byte_slice_no_callback<'a>(s: &'a str) -> &'a [u8] {
    unsafe {
        cast::transmute(s)
    }
}

/// Convert a string to a unique vector of characters
pub fn to_chars(s: &str) -> ~[char] {
    s.iter().collect()
}

/**
 * Take a substring of another.
 *
 * Returns a slice pointing at `n` characters starting from byte offset
 * `begin`.
 */
pub fn substr<'a>(s: &'a str, begin: uint, n: uint) -> &'a str {
    s.slice(begin, begin + count_bytes(s, begin, n))
}

/// An iterator over the substrings of a string, separated by `sep`.
pub struct StrCharSplitIterator<'self,Sep> {
    priv string: &'self str,
    priv position: uint,
    priv sep: Sep,
    /// The number of splits remaining
    priv count: uint,
    /// Whether an empty string at the end is allowed
    priv allow_trailing_empty: bool,
    priv finished: bool,
    priv only_ascii: bool
}

/// An iterator over the words of a string, separated by an sequence of whitespace
pub type WordIterator<'self> =
    FilterIterator<'self, &'self str,
             StrCharSplitIterator<'self, extern "Rust" fn(char) -> bool>>;

/// A separator for splitting a string character-wise
pub trait StrCharSplitSeparator {
    /// Determine if the splitter should split at the given character
    fn should_split(&self, char) -> bool;
    /// Indicate if the splitter only uses ASCII characters, which
    /// allows for a faster implementation.
    fn only_ascii(&self) -> bool;
}
impl StrCharSplitSeparator for char {
    #[inline(always)]
    fn should_split(&self, c: char) -> bool { *self == c }

    fn only_ascii(&self) -> bool { (*self as uint) < 128 }
}
impl<'self> StrCharSplitSeparator for &'self fn(char) -> bool {
    #[inline(always)]
    fn should_split(&self, c: char) -> bool { (*self)(c) }

    fn only_ascii(&self) -> bool { false }
}
impl<'self> StrCharSplitSeparator for extern "Rust" fn(char) -> bool {
    #[inline(always)]
    fn should_split(&self, c: char) -> bool { (*self)(c) }

    fn only_ascii(&self) -> bool { false }
}

impl<'self, Sep: StrCharSplitSeparator> Iterator<&'self str> for StrCharSplitIterator<'self, Sep> {
    fn next(&mut self) -> Option<&'self str> {
        if self.finished { return None }

        let l = self.string.len();
        let start = self.position;

        if self.only_ascii {
            // this gives a *huge* speed up for splitting on ASCII
            // characters (e.g. '\n' or ' ')
            while self.position < l && self.count > 0 {
                let byte = self.string[self.position];

                if self.sep.should_split(byte as char) {
                    let slice = unsafe { raw::slice_bytes(self.string, start, self.position) };
                    self.position += 1;
                    self.count -= 1;
                    return Some(slice);
                }
                self.position += 1;
            }
        } else {
            while self.position < l && self.count > 0 {
                let CharRange {ch, next} = char_range_at(self.string, self.position);

                if self.sep.should_split(ch) {
                    let slice = unsafe { raw::slice_bytes(self.string, start, self.position) };
                    self.position = next;
                    self.count -= 1;
                    return Some(slice);
                }
                self.position = next;
            }
        }
        self.finished = true;
        if self.allow_trailing_empty || start < l {
            Some(unsafe { raw::slice_bytes(self.string, start, l) })
        } else {
            None
        }
    }
}

// See Issue #1932 for why this is a naive search
fn iter_matches<'a,'b>(s: &'a str, sep: &'b str,
                       f: &fn(uint, uint) -> bool) -> bool {
    let (sep_len, l) = (sep.len(), s.len());
    assert!(sep_len > 0u);
    let mut (i, match_start, match_i) = (0u, 0u, 0u);

    while i < l {
        if s[i] == sep[match_i] {
            if match_i == 0u { match_start = i; }
            match_i += 1u;
            // Found a match
            if match_i == sep_len {
                if !f(match_start, i + 1u) { return false; }
                match_i = 0u;
            }
            i += 1u;
        } else {
            // Failed match, backtrack
            if match_i > 0u {
                match_i = 0u;
                i = match_start + 1u;
            } else {
                i += 1u;
            }
        }
    }
    return true;
}

fn iter_between_matches<'a,'b>(s: &'a str,
                               sep: &'b str,
                               f: &fn(uint, uint) -> bool) -> bool {
    let mut last_end = 0u;
    for iter_matches(s, sep) |from, to| {
        if !f(last_end, from) { return false; }
        last_end = to;
    }
    return f(last_end, s.len());
}

/**
 * Splits a string into a vector of the substrings separated by a given string
 *
 * # Example
 *
 * ~~~ {.rust}
 * let mut v = ~[];
 * for each_split_str(".XXX.YYY.", ".") |subs| { v.push(subs); }
 * assert!(v == ["", "XXX", "YYY", ""]);
 * ~~~
 */
pub fn each_split_str<'a,'b>(s: &'a str,
                             sep: &'b str,
                             it: &fn(&'a str) -> bool) -> bool {
    for iter_between_matches(s, sep) |from, to| {
        if !it( unsafe { raw::slice_bytes(s, from, to) } ) { return false; }
    }
    return true;
}

/**
 * Splits the string `s` based on `sep`, yielding all splits to the iterator
 * function provide
 *
 * # Example
 *
 * ~~~ {.rust}
 * let mut v = ~[];
 * for each_split_str(".XXX.YYY.", ".") |subs| { v.push(subs); }
 * assert!(v == ["XXX", "YYY"]);
 * ~~~
 */
pub fn each_split_str_nonempty<'a,'b>(s: &'a str,
                                      sep: &'b str,
                                      it: &fn(&'a str) -> bool) -> bool {
    for iter_between_matches(s, sep) |from, to| {
        if to > from {
            if !it( unsafe { raw::slice_bytes(s, from, to) } ) { return false; }
        }
    }
    return true;
}

/// Levenshtein Distance between two strings
pub fn levdistance(s: &str, t: &str) -> uint {

    let slen = s.len();
    let tlen = t.len();

    if slen == 0 { return tlen; }
    if tlen == 0 { return slen; }

    let mut dcol = vec::from_fn(tlen + 1, |x| x);

    for s.iter().enumerate().advance |(i, sc)| {

        let mut current = i;
        dcol[0] = current + 1;

        for t.iter().enumerate().advance |(j, tc)| {

            let next = dcol[j + 1];

            if sc == tc {
                dcol[j + 1] = current;
            } else {
                dcol[j + 1] = ::cmp::min(current, next);
                dcol[j + 1] = ::cmp::min(dcol[j + 1], dcol[j]) + 1;
            }

            current = next;
        }
    }

    return dcol[tlen];
}

/**
 * Splits a string into substrings separated by LF ('\n')
 * and/or CR LF ("\r\n")
 */
pub fn each_line_any<'a>(s: &'a str, it: &fn(&'a str) -> bool) -> bool {
    for s.line_iter().advance |s| {
        let l = s.len();
        if l > 0u && s[l - 1u] == '\r' as u8 {
            if !it( unsafe { raw::slice_bytes(s, 0, l - 1) } ) { return false; }
        } else {
            if !it( s ) { return false; }
        }
    }
    return true;
}

/** Splits a string into substrings with possibly internal whitespace,
 *  each of them at most `lim` bytes long. The substrings have leading and trailing
 *  whitespace removed, and are only cut at whitespace boundaries.
 *
 *  #Failure:
 *
 *  Fails during iteration if the string contains a non-whitespace
 *  sequence longer than the limit.
 */
pub fn each_split_within<'a>(ss: &'a str,
                              lim: uint,
                              it: &fn(&'a str) -> bool) -> bool {
    // Just for fun, let's write this as an state machine:

    enum SplitWithinState {
        A,  // leading whitespace, initial state
        B,  // words
        C,  // internal and trailing whitespace
    }
    enum Whitespace {
        Ws, // current char is whitespace
        Cr  // current char is not whitespace
    }
    enum LengthLimit {
        UnderLim, // current char makes current substring still fit in limit
        OverLim   // current char makes current substring no longer fit in limit
    }

    let mut slice_start = 0;
    let mut last_start = 0;
    let mut last_end = 0;
    let mut state = A;

    let mut cont = true;
    let slice: &fn() = || { cont = it(ss.slice(slice_start, last_end)) };

    let machine: &fn((uint, char)) -> bool = |(i, c)| {
        let whitespace = if char::is_whitespace(c)       { Ws }       else { Cr };
        let limit      = if (i - slice_start + 1) <= lim { UnderLim } else { OverLim };

        state = match (state, whitespace, limit) {
            (A, Ws, _)        => { A }
            (A, Cr, _)        => { slice_start = i; last_start = i; B }

            (B, Cr, UnderLim) => { B }
            (B, Cr, OverLim)  if (i - last_start + 1) > lim
                              => fail!("word starting with %? longer than limit!",
                                       ss.slice(last_start, i + 1)),
            (B, Cr, OverLim)  => { slice(); slice_start = last_start; B }
            (B, Ws, UnderLim) => { last_end = i; C }
            (B, Ws, OverLim)  => { last_end = i; slice(); A }

            (C, Cr, UnderLim) => { last_start = i; B }
            (C, Cr, OverLim)  => { slice(); slice_start = i; last_start = i; last_end = i; B }
            (C, Ws, OverLim)  => { slice(); A }
            (C, Ws, UnderLim) => { C }
        };

        cont
    };

    ss.iter().enumerate().advance(machine);

    // Let the automaton 'run out' by supplying trailing whitespace
    let mut fake_i = ss.len();
    while cont && match state { B | C => true, A => false } {
        machine((fake_i, ' '));
        fake_i += 1;
    }
    return cont;
}

/**
 * Replace all occurrences of one string with another
 *
 * # Arguments
 *
 * * s - The string containing substrings to replace
 * * from - The string to replace
 * * to - The replacement string
 *
 * # Return value
 *
 * The original string with all occurances of `from` replaced with `to`
 */
pub fn replace(s: &str, from: &str, to: &str) -> ~str {
    let mut (result, first) = (~"", true);
    for iter_between_matches(s, from) |start, end| {
        if first {
            first = false;
        } else {
            push_str(&mut result, to);
        }
        push_str(&mut result, unsafe{raw::slice_bytes(s, start, end)});
    }
    result
}

/*
Section: Comparing strings
*/

/// Bytewise slice equality
#[cfg(not(test))]
#[lang="str_eq"]
#[inline]
pub fn eq_slice(a: &str, b: &str) -> bool {
    do as_buf(a) |ap, alen| {
        do as_buf(b) |bp, blen| {
            if (alen != blen) { false }
            else {
                unsafe {
                    libc::memcmp(ap as *libc::c_void,
                                 bp as *libc::c_void,
                                 (alen - 1) as libc::size_t) == 0
                }
            }
        }
    }
}

#[cfg(test)]
#[inline]
pub fn eq_slice(a: &str, b: &str) -> bool {
    do as_buf(a) |ap, alen| {
        do as_buf(b) |bp, blen| {
            if (alen != blen) { false }
            else {
                unsafe {
                    libc::memcmp(ap as *libc::c_void,
                                 bp as *libc::c_void,
                                 (alen - 1) as libc::size_t) == 0
                }
            }
        }
    }
}

/// Bytewise string equality
#[cfg(not(test))]
#[lang="uniq_str_eq"]
#[inline]
pub fn eq(a: &~str, b: &~str) -> bool {
    eq_slice(*a, *b)
}

#[cfg(test)]
#[inline]
pub fn eq(a: &~str, b: &~str) -> bool {
    eq_slice(*a, *b)
}

#[inline]
fn cmp(a: &str, b: &str) -> Ordering {
    let low = uint::min(a.len(), b.len());

    for uint::range(0, low) |idx| {
        match a[idx].cmp(&b[idx]) {
          Greater => return Greater,
          Less => return Less,
          Equal => ()
        }
    }

    a.len().cmp(&b.len())
}

#[cfg(not(test))]
impl<'self> TotalOrd for &'self str {
    #[inline]
    fn cmp(&self, other: & &'self str) -> Ordering { cmp(*self, *other) }
}

#[cfg(not(test))]
impl TotalOrd for ~str {
    #[inline]
    fn cmp(&self, other: &~str) -> Ordering { cmp(*self, *other) }
}

#[cfg(not(test))]
impl TotalOrd for @str {
    #[inline]
    fn cmp(&self, other: &@str) -> Ordering { cmp(*self, *other) }
}

/// Bytewise slice less than
#[inline]
fn lt(a: &str, b: &str) -> bool {
    let (a_len, b_len) = (a.len(), b.len());
    let end = uint::min(a_len, b_len);

    let mut i = 0;
    while i < end {
        let (c_a, c_b) = (a[i], b[i]);
        if c_a < c_b { return true; }
        if c_a > c_b { return false; }
        i += 1;
    }

    return a_len < b_len;
}

/// Bytewise less than or equal
#[inline]
pub fn le(a: &str, b: &str) -> bool {
    !lt(b, a)
}

/// Bytewise greater than or equal
#[inline]
fn ge(a: &str, b: &str) -> bool {
    !lt(a, b)
}

/// Bytewise greater than
#[inline]
fn gt(a: &str, b: &str) -> bool {
    !le(a, b)
}

#[cfg(not(test))]
impl<'self> Eq for &'self str {
    #[inline(always)]
    fn eq(&self, other: & &'self str) -> bool {
        eq_slice((*self), (*other))
    }
    #[inline(always)]
    fn ne(&self, other: & &'self str) -> bool { !(*self).eq(other) }
}

#[cfg(not(test))]
impl Eq for ~str {
    #[inline(always)]
    fn eq(&self, other: &~str) -> bool {
        eq_slice((*self), (*other))
    }
    #[inline(always)]
    fn ne(&self, other: &~str) -> bool { !(*self).eq(other) }
}

#[cfg(not(test))]
impl Eq for @str {
    #[inline(always)]
    fn eq(&self, other: &@str) -> bool {
        eq_slice((*self), (*other))
    }
    #[inline(always)]
    fn ne(&self, other: &@str) -> bool { !(*self).eq(other) }
}

#[cfg(not(test))]
impl<'self> TotalEq for &'self str {
    #[inline(always)]
    fn equals(&self, other: & &'self str) -> bool {
        eq_slice((*self), (*other))
    }
}

#[cfg(not(test))]
impl TotalEq for ~str {
    #[inline(always)]
    fn equals(&self, other: &~str) -> bool {
        eq_slice((*self), (*other))
    }
}

#[cfg(not(test))]
impl TotalEq for @str {
    #[inline(always)]
    fn equals(&self, other: &@str) -> bool {
        eq_slice((*self), (*other))
    }
}

#[cfg(not(test))]
impl Ord for ~str {
    #[inline(always)]
    fn lt(&self, other: &~str) -> bool { lt((*self), (*other)) }
    #[inline(always)]
    fn le(&self, other: &~str) -> bool { le((*self), (*other)) }
    #[inline(always)]
    fn ge(&self, other: &~str) -> bool { ge((*self), (*other)) }
    #[inline(always)]
    fn gt(&self, other: &~str) -> bool { gt((*self), (*other)) }
}

#[cfg(not(test))]
impl<'self> Ord for &'self str {
    #[inline(always)]
    fn lt(&self, other: & &'self str) -> bool { lt((*self), (*other)) }
    #[inline(always)]
    fn le(&self, other: & &'self str) -> bool { le((*self), (*other)) }
    #[inline(always)]
    fn ge(&self, other: & &'self str) -> bool { ge((*self), (*other)) }
    #[inline(always)]
    fn gt(&self, other: & &'self str) -> bool { gt((*self), (*other)) }
}

#[cfg(not(test))]
impl Ord for @str {
    #[inline(always)]
    fn lt(&self, other: &@str) -> bool { lt((*self), (*other)) }
    #[inline(always)]
    fn le(&self, other: &@str) -> bool { le((*self), (*other)) }
    #[inline(always)]
    fn ge(&self, other: &@str) -> bool { ge((*self), (*other)) }
    #[inline(always)]
    fn gt(&self, other: &@str) -> bool { gt((*self), (*other)) }
}

#[cfg(not(test))]
impl<'self> Equiv<~str> for &'self str {
    #[inline(always)]
    fn equiv(&self, other: &~str) -> bool { eq_slice(*self, *other) }
}

/*
Section: Iterating through strings
*/

/// Apply a function to each character
pub fn map(ss: &str, ff: &fn(char) -> char) -> ~str {
    let mut result = ~"";
    reserve(&mut result, ss.len());
    for ss.iter().advance |cc| {
        str::push_char(&mut result, ff(cc));
    }
    result
}

/*
Section: Searching
*/

/**
 * Returns the byte index of the first matching character
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `c` - The character to search for
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching character
 * or `none` if there is no match
 */
pub fn find_char(s: &str, c: char) -> Option<uint> {
    find_char_between(s, c, 0u, s.len())
}

/**
 * Returns the byte index of the first matching character beginning
 * from a given byte offset
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `c` - The character to search for
 * * `start` - The byte index to begin searching at, inclusive
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `s.len()`. `start` must be the
 * index of a character boundary, as defined by `is_char_boundary`.
 */
pub fn find_char_from(s: &str, c: char, start: uint) -> Option<uint> {
    find_char_between(s, c, start, s.len())
}

/**
 * Returns the byte index of the first matching character within a given range
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `c` - The character to search for
 * * `start` - The byte index to begin searching at, inclusive
 * * `end` - The byte index to end searching at, exclusive
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `end` and `end` must be less than
 * or equal to `s.len()`. `start` must be the index of a character boundary,
 * as defined by `is_char_boundary`.
 */
pub fn find_char_between(s: &str, c: char, start: uint, end: uint)
    -> Option<uint> {
    if c < 128u as char {
        assert!(start <= end);
        assert!(end <= s.len());
        let mut i = start;
        let b = c as u8;
        while i < end {
            if s[i] == b { return Some(i); }
            i += 1u;
        }
        return None;
    } else {
        find_between(s, start, end, |x| x == c)
    }
}

/**
 * Returns the byte index of the last matching character
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `c` - The character to search for
 *
 * # Return value
 *
 * An `option` containing the byte index of the last matching character
 * or `none` if there is no match
 */
pub fn rfind_char(s: &str, c: char) -> Option<uint> {
    rfind_char_between(s, c, s.len(), 0u)
}

/**
 * Returns the byte index of the last matching character beginning
 * from a given byte offset
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `c` - The character to search for
 * * `start` - The byte index to begin searching at, exclusive
 *
 * # Return value
 *
 * An `option` containing the byte index of the last matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `s.len()`. `start` must be
 * the index of a character boundary, as defined by `is_char_boundary`.
 */
pub fn rfind_char_from(s: &str, c: char, start: uint) -> Option<uint> {
    rfind_char_between(s, c, start, 0u)
}

/**
 * Returns the byte index of the last matching character within a given range
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `c` - The character to search for
 * * `start` - The byte index to begin searching at, exclusive
 * * `end` - The byte index to end searching at, inclusive
 *
 * # Return value
 *
 * An `option` containing the byte index of the last matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `end` must be less than or equal to `start` and `start` must be less than
 * or equal to `s.len()`. `start` must be the index of a character boundary,
 * as defined by `is_char_boundary`.
 */
pub fn rfind_char_between(s: &str, c: char, start: uint, end: uint) -> Option<uint> {
    if c < 128u as char {
        assert!(start >= end);
        assert!(start <= s.len());
        let mut i = start;
        let b = c as u8;
        while i > end {
            i -= 1u;
            if s[i] == b { return Some(i); }
        }
        return None;
    } else {
        rfind_between(s, start, end, |x| x == c)
    }
}

/**
 * Returns the byte index of the first character that satisfies
 * the given predicate
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `f` - The predicate to satisfy
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching character
 * or `none` if there is no match
 */
pub fn find(s: &str, f: &fn(char) -> bool) -> Option<uint> {
    find_between(s, 0u, s.len(), f)
}

/**
 * Returns the byte index of the first character that satisfies
 * the given predicate, beginning from a given byte offset
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `start` - The byte index to begin searching at, inclusive
 * * `f` - The predicate to satisfy
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching charactor
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `s.len()`. `start` must be the
 * index of a character boundary, as defined by `is_char_boundary`.
 */
pub fn find_from(s: &str, start: uint, f: &fn(char)
    -> bool) -> Option<uint> {
    find_between(s, start, s.len(), f)
}

/**
 * Returns the byte index of the first character that satisfies
 * the given predicate, within a given range
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `start` - The byte index to begin searching at, inclusive
 * * `end` - The byte index to end searching at, exclusive
 * * `f` - The predicate to satisfy
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `end` and `end` must be less than
 * or equal to `s.len()`. `start` must be the index of a character
 * boundary, as defined by `is_char_boundary`.
 */
pub fn find_between(s: &str, start: uint, end: uint, f: &fn(char) -> bool) -> Option<uint> {
    assert!(start <= end);
    assert!(end <= s.len());
    assert!(is_char_boundary(s, start));
    let mut i = start;
    while i < end {
        let CharRange {ch, next} = char_range_at(s, i);
        if f(ch) { return Some(i); }
        i = next;
    }
    return None;
}

/**
 * Returns the byte index of the last character that satisfies
 * the given predicate
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `f` - The predicate to satisfy
 *
 * # Return value
 *
 * An option containing the byte index of the last matching character
 * or `none` if there is no match
 */
pub fn rfind(s: &str, f: &fn(char) -> bool) -> Option<uint> {
    rfind_between(s, s.len(), 0u, f)
}

/**
 * Returns the byte index of the last character that satisfies
 * the given predicate, beginning from a given byte offset
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `start` - The byte index to begin searching at, exclusive
 * * `f` - The predicate to satisfy
 *
 * # Return value
 *
 * An `option` containing the byte index of the last matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `s.len()', `start` must be the
 * index of a character boundary, as defined by `is_char_boundary`
 */
pub fn rfind_from(s: &str, start: uint, f: &fn(char) -> bool) -> Option<uint> {
    rfind_between(s, start, 0u, f)
}

/**
 * Returns the byte index of the last character that satisfies
 * the given predicate, within a given range
 *
 * # Arguments
 *
 * * `s` - The string to search
 * * `start` - The byte index to begin searching at, exclusive
 * * `end` - The byte index to end searching at, inclusive
 * * `f` - The predicate to satisfy
 *
 * # Return value
 *
 * An `option` containing the byte index of the last matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `end` must be less than or equal to `start` and `start` must be less
 * than or equal to `s.len()`. `start` must be the index of a character
 * boundary, as defined by `is_char_boundary`
 */
pub fn rfind_between(s: &str, start: uint, end: uint, f: &fn(char) -> bool) -> Option<uint> {
    assert!(start >= end);
    assert!(start <= s.len());
    assert!(is_char_boundary(s, start));
    let mut i = start;
    while i > end {
        let CharRange {ch, next: prev} = char_range_at_reverse(s, i);
        if f(ch) { return Some(prev); }
        i = prev;
    }
    return None;
}

// Utility used by various searching functions
fn match_at<'a,'b>(haystack: &'a str, needle: &'b str, at: uint) -> bool {
    let mut i = at;
    for needle.bytes_iter().advance |c| { if haystack[i] != c { return false; } i += 1u; }
    return true;
}

/**
 * Returns the byte index of the first matching substring
 *
 * # Arguments
 *
 * * `haystack` - The string to search
 * * `needle` - The string to search for
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching substring
 * or `none` if there is no match
 */
pub fn find_str<'a,'b>(haystack: &'a str, needle: &'b str) -> Option<uint> {
    find_str_between(haystack, needle, 0u, haystack.len())
}

/**
 * Returns the byte index of the first matching substring beginning
 * from a given byte offset
 *
 * # Arguments
 *
 * * `haystack` - The string to search
 * * `needle` - The string to search for
 * * `start` - The byte index to begin searching at, inclusive
 *
 * # Return value
 *
 * An `option` containing the byte index of the last matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `s.len()`
 */
pub fn find_str_from<'a,'b>(haystack: &'a str,
                            needle: &'b str,
                            start: uint)
                         -> Option<uint> {
    find_str_between(haystack, needle, start, haystack.len())
}

/**
 * Returns the byte index of the first matching substring within a given range
 *
 * # Arguments
 *
 * * `haystack` - The string to search
 * * `needle` - The string to search for
 * * `start` - The byte index to begin searching at, inclusive
 * * `end` - The byte index to end searching at, exclusive
 *
 * # Return value
 *
 * An `option` containing the byte index of the first matching character
 * or `none` if there is no match
 *
 * # Failure
 *
 * `start` must be less than or equal to `end` and `end` must be less than
 * or equal to `s.len()`.
 */
pub fn find_str_between<'a,'b>(haystack: &'a str,
                               needle: &'b str,
                               start: uint,
                               end:uint)
                            -> Option<uint> {
    // See Issue #1932 for why this is a naive search
    assert!(end <= haystack.len());
    let needle_len = needle.len();
    if needle_len == 0u { return Some(start); }
    if needle_len > end { return None; }

    let mut i = start;
    let e = end - needle_len;
    while i <= e {
        if match_at(haystack, needle, i) { return Some(i); }
        i += 1u;
    }
    return None;
}

/**
 * Returns true if one string contains another
 *
 * # Arguments
 *
 * * haystack - The string to look in
 * * needle - The string to look for
 */
pub fn contains<'a,'b>(haystack: &'a str, needle: &'b str) -> bool {
    find_str(haystack, needle).is_some()
}

/**
 * Returns true if a string contains a char.
 *
 * # Arguments
 *
 * * haystack - The string to look in
 * * needle - The char to look for
 */
pub fn contains_char(haystack: &str, needle: char) -> bool {
    find_char(haystack, needle).is_some()
}

/**
 * Returns true if one string starts with another
 *
 * # Arguments
 *
 * * haystack - The string to look in
 * * needle - The string to look for
 */
pub fn starts_with<'a,'b>(haystack: &'a str, needle: &'b str) -> bool {
    let (haystack_len, needle_len) = (haystack.len(), needle.len());
    if needle_len == 0u { true }
    else if needle_len > haystack_len { false }
    else { match_at(haystack, needle, 0u) }
}

/**
 * Returns true if one string ends with another
 *
 * # Arguments
 *
 * * haystack - The string to look in
 * * needle - The string to look for
 */
pub fn ends_with<'a,'b>(haystack: &'a str, needle: &'b str) -> bool {
    let (haystack_len, needle_len) = (haystack.len(), needle.len());
    if needle_len == 0u { true }
    else if needle_len > haystack_len { false }
    else { match_at(haystack, needle, haystack_len - needle_len) }
}

/*
Section: String properties
*/

/**
 * Returns true if the string contains only whitespace
 *
 * Whitespace characters are determined by `char::is_whitespace`
 */
pub fn is_whitespace(s: &str) -> bool {
    s.iter().all(char::is_whitespace)
}

/**
 * Returns true if the string contains only alphanumerics
 *
 * Alphanumeric characters are determined by `char::is_alphanumeric`
 */
fn is_alphanumeric(s: &str) -> bool {
    s.iter().all(char::is_alphanumeric)
}

/// Returns the number of characters that a string holds
#[inline(always)]
pub fn char_len(s: &str) -> uint { count_chars(s, 0u, s.len()) }

/*
Section: Misc
*/

/// Determines if a vector of bytes contains valid UTF-8
pub fn is_utf8(v: &const [u8]) -> bool {
    let mut i = 0u;
    let total = v.len();
    while i < total {
        let mut chsize = utf8_char_width(v[i]);
        if chsize == 0u { return false; }
        if i + chsize > total { return false; }
        i += 1u;
        while chsize > 1u {
            if v[i] & 192u8 != tag_cont_u8 { return false; }
            i += 1u;
            chsize -= 1u;
        }
    }
    return true;
}

/// Determines if a vector of `u16` contains valid UTF-16
pub fn is_utf16(v: &[u16]) -> bool {
    let len = v.len();
    let mut i = 0u;
    while (i < len) {
        let u = v[i];

        if  u <= 0xD7FF_u16 || u >= 0xE000_u16 {
            i += 1u;

        } else {
            if i+1u < len { return false; }
            let u2 = v[i+1u];
            if u < 0xD7FF_u16 || u > 0xDBFF_u16 { return false; }
            if u2 < 0xDC00_u16 || u2 > 0xDFFF_u16 { return false; }
            i += 2u;
        }
    }
    return true;
}

/// Converts to a vector of `u16` encoded as UTF-16
pub fn to_utf16(s: &str) -> ~[u16] {
    let mut u = ~[];
    for s.iter().advance |ch| {
        // Arithmetic with u32 literals is easier on the eyes than chars.
        let mut ch = ch as u32;

        if (ch & 0xFFFF_u32) == ch {
            // The BMP falls through (assuming non-surrogate, as it
            // should)
            assert!(ch <= 0xD7FF_u32 || ch >= 0xE000_u32);
            u.push(ch as u16)
        } else {
            // Supplementary planes break into surrogates.
            assert!(ch >= 0x1_0000_u32 && ch <= 0x10_FFFF_u32);
            ch -= 0x1_0000_u32;
            let w1 = 0xD800_u16 | ((ch >> 10) as u16);
            let w2 = 0xDC00_u16 | ((ch as u16) & 0x3FF_u16);
            u.push_all([w1, w2])
        }
    }
    u
}

/// Iterates over the utf-16 characters in the specified slice, yielding each
/// decoded unicode character to the function provided.
///
/// # Failures
///
/// * Fails on invalid utf-16 data
pub fn utf16_chars(v: &[u16], f: &fn(char)) {
    let len = v.len();
    let mut i = 0u;
    while (i < len && v[i] != 0u16) {
        let u = v[i];

        if  u <= 0xD7FF_u16 || u >= 0xE000_u16 {
            f(u as char);
            i += 1u;

        } else {
            let u2 = v[i+1u];
            assert!(u >= 0xD800_u16 && u <= 0xDBFF_u16);
            assert!(u2 >= 0xDC00_u16 && u2 <= 0xDFFF_u16);
            let mut c = (u - 0xD800_u16) as char;
            c = c << 10;
            c |= (u2 - 0xDC00_u16) as char;
            c |= 0x1_0000_u32 as char;
            f(c);
            i += 2u;
        }
    }
}

/**
 * Allocates a new string from the utf-16 slice provided
 */
pub fn from_utf16(v: &[u16]) -> ~str {
    let mut buf = ~"";
    reserve(&mut buf, v.len());
    utf16_chars(v, |ch| push_char(&mut buf, ch));
    buf
}

/**
 * Allocates a new string with the specified capacity. The string returned is
 * the empty string, but has capacity for much more.
 */
pub fn with_capacity(capacity: uint) -> ~str {
    let mut buf = ~"";
    reserve(&mut buf, capacity);
    buf
}

/**
 * As char_len but for a slice of a string
 *
 * # Arguments
 *
 * * s - A valid string
 * * start - The position inside `s` where to start counting in bytes
 * * end - The position where to stop counting
 *
 * # Return value
 *
 * The number of Unicode characters in `s` between the given indices.
 */
pub fn count_chars(s: &str, start: uint, end: uint) -> uint {
    assert!(is_char_boundary(s, start));
    assert!(is_char_boundary(s, end));
    let mut (i, len) = (start, 0u);
    while i < end {
        let next = char_range_at(s, i).next;
        len += 1u;
        i = next;
    }
    return len;
}

/// Counts the number of bytes taken by the first `n` chars in `s`
/// starting from `start`.
pub fn count_bytes<'b>(s: &'b str, start: uint, n: uint) -> uint {
    assert!(is_char_boundary(s, start));
    let mut (end, cnt) = (start, n);
    let l = s.len();
    while cnt > 0u {
        assert!(end < l);
        let next = char_range_at(s, end).next;
        cnt -= 1u;
        end = next;
    }
    end - start
}

/// Given a first byte, determine how many bytes are in this UTF-8 character
pub fn utf8_char_width(b: u8) -> uint {
    let byte: uint = b as uint;
    if byte < 128u { return 1u; }
    // Not a valid start byte
    if byte < 192u { return 0u; }
    if byte < 224u { return 2u; }
    if byte < 240u { return 3u; }
    if byte < 248u { return 4u; }
    if byte < 252u { return 5u; }
    return 6u;
}

/**
 * Returns false if the index points into the middle of a multi-byte
 * character sequence.
 */
pub fn is_char_boundary(s: &str, index: uint) -> bool {
    if index == s.len() { return true; }
    let b = s[index];
    return b < 128u8 || b >= 192u8;
}

/**
 * Pluck a character out of a string and return the index of the next
 * character.
 *
 * This function can be used to iterate over the unicode characters of a
 * string.
 *
 * # Example
 *
 * ~~~ {.rust}
 * let s = "中华Việt Nam";
 * let i = 0u;
 * while i < s.len() {
 *     let CharRange {ch, next} = str::char_range_at(s, i);
 *     std::io::println(fmt!("%u: %c",i,ch));
 *     i = next;
 * }
 * ~~~
 *
 * # Example output
 *
 * ~~~
 * 0: 中
 * 3: 华
 * 6: V
 * 7: i
 * 8: ệ
 * 11: t
 * 12:
 * 13: N
 * 14: a
 * 15: m
 * ~~~
 *
 * # Arguments
 *
 * * s - The string
 * * i - The byte offset of the char to extract
 *
 * # Return value
 *
 * A record {ch: char, next: uint} containing the char value and the byte
 * index of the next unicode character.
 *
 * # Failure
 *
 * If `i` is greater than or equal to the length of the string.
 * If `i` is not the index of the beginning of a valid UTF-8 character.
 */
pub fn char_range_at(s: &str, i: uint) -> CharRange {
    let b0 = s[i];
    let w = utf8_char_width(b0);
    assert!((w != 0u));
    if w == 1u { return CharRange {ch: b0 as char, next: i + 1u}; }
    let mut val = 0u;
    let end = i + w;
    let mut i = i + 1u;
    while i < end {
        let byte = s[i];
        assert_eq!(byte & 192u8, tag_cont_u8);
        val <<= 6u;
        val += (byte & 63u8) as uint;
        i += 1u;
    }
    // Clunky way to get the right bits from the first byte. Uses two shifts,
    // the first to clip off the marker bits at the left of the byte, and then
    // a second (as uint) to get it to the right position.
    val += ((b0 << ((w + 1u) as u8)) as uint) << ((w - 1u) * 6u - w - 1u);
    return CharRange {ch: val as char, next: i};
}

/// Plucks the character starting at the `i`th byte of a string
pub fn char_at(s: &str, i: uint) -> char {
    return char_range_at(s, i).ch;
}

#[allow(missing_doc)]
pub struct CharRange {
    ch: char,
    next: uint
}

/**
 * Given a byte position and a str, return the previous char and its position.
 *
 * This function can be used to iterate over a unicode string in reverse.
 *
 * Returns 0 for next index if called on start index 0.
 */
pub fn char_range_at_reverse(ss: &str, start: uint) -> CharRange {
    let mut prev = start;

    // while there is a previous byte == 10......
    while prev > 0u && ss[prev - 1u] & 192u8 == tag_cont_u8 {
        prev -= 1u;
    }

    // now refer to the initial byte of previous char
    if prev > 0u {
        prev -= 1u;
    } else {
        prev = 0u;
    }


    let ch = char_at(ss, prev);
    return CharRange {ch:ch, next:prev};
}

/// Plucks the character ending at the `i`th byte of a string
pub fn char_at_reverse(s: &str, i: uint) -> char {
    char_range_at_reverse(s, i).ch
}

/**
 * Loop through a substring, char by char
 *
 * # Safety note
 *
 * * This function does not check whether the substring is valid.
 * * This function fails if `start` or `end` do not
 *   represent valid positions inside `s`
 *
 * # Arguments
 *
 * * s - A string to traverse. It may be empty.
 * * start - The byte offset at which to start in the string.
 * * end - The end of the range to traverse
 * * it - A block to execute with each consecutive character of `s`.
 *        Return `true` to continue, `false` to stop.
 *
 * # Return value
 *
 * `true` If execution proceeded correctly, `false` if it was interrupted,
 * that is if `it` returned `false` at any point.
 */
pub fn all_between(s: &str, start: uint, end: uint,
                    it: &fn(char) -> bool) -> bool {
    assert!(is_char_boundary(s, start));
    let mut i = start;
    while i < end {
        let CharRange {ch, next} = char_range_at(s, i);
        if !it(ch) { return false; }
        i = next;
    }
    return true;
}

/**
 * Loop through a substring, char by char
 *
 * # Safety note
 *
 * * This function does not check whether the substring is valid.
 * * This function fails if `start` or `end` do not
 *   represent valid positions inside `s`
 *
 * # Arguments
 *
 * * s - A string to traverse. It may be empty.
 * * start - The byte offset at which to start in the string.
 * * end - The end of the range to traverse
 * * it - A block to execute with each consecutive character of `s`.
 *        Return `true` to continue, `false` to stop.
 *
 * # Return value
 *
 * `true` if `it` returns `true` for any character
 */
pub fn any_between(s: &str, start: uint, end: uint,
                    it: &fn(char) -> bool) -> bool {
    !all_between(s, start, end, |c| !it(c))
}

// UTF-8 tags and ranges
static tag_cont_u8: u8 = 128u8;
static tag_cont: uint = 128u;
static max_one_b: uint = 128u;
static tag_two_b: uint = 192u;
static max_two_b: uint = 2048u;
static tag_three_b: uint = 224u;
static max_three_b: uint = 65536u;
static tag_four_b: uint = 240u;
static max_four_b: uint = 2097152u;
static tag_five_b: uint = 248u;
static max_five_b: uint = 67108864u;
static tag_six_b: uint = 252u;

/**
 * Work with the byte buffer of a string.
 *
 * Allows for unsafe manipulation of strings, which is useful for foreign
 * interop.
 *
 * # Example
 *
 * ~~~ {.rust}
 * let i = str::as_bytes("Hello World") { |bytes| bytes.len() };
 * ~~~
 */
#[inline]
pub fn as_bytes<T>(s: &const ~str, f: &fn(&~[u8]) -> T) -> T {
    unsafe {
        let v: *~[u8] = cast::transmute(copy s);
        f(&*v)
    }
}

/**
 * Work with the byte buffer of a string as a byte slice.
 *
 * The byte slice does not include the null terminator.
 */
pub fn as_bytes_slice<'a>(s: &'a str) -> &'a [u8] {
    unsafe {
        let (ptr, len): (*u8, uint) = ::cast::transmute(s);
        let outgoing_tuple: (*u8, uint) = (ptr, len - 1);
        return ::cast::transmute(outgoing_tuple);
    }
}

/**
 * A dummy trait to hold all the utility methods that we implement on strings.
 */
pub trait StrUtil {
    /**
     * Work with the byte buffer of a string as a null-terminated C string.
     *
     * Allows for unsafe manipulation of strings, which is useful for foreign
     * interop. This is similar to `str::as_buf`, but guarantees null-termination.
     * If the given slice is not already null-terminated, this function will
     * allocate a temporary, copy the slice, null terminate it, and pass
     * that instead.
     *
     * # Example
     *
     * ~~~ {.rust}
     * let s = "PATH".as_c_str(|path| libc::getenv(path));
     * ~~~
     */
    fn as_c_str<T>(self, f: &fn(*libc::c_char) -> T) -> T;
}

impl<'self> StrUtil for &'self str {
    #[inline]
    fn as_c_str<T>(self, f: &fn(*libc::c_char) -> T) -> T {
        do as_buf(self) |buf, len| {
            // NB: len includes the trailing null.
            assert!(len > 0);
            if unsafe { *(ptr::offset(buf,len-1)) != 0 } {
                to_owned(self).as_c_str(f)
            } else {
                f(buf as *libc::c_char)
            }
        }
    }
}

/**
 * Deprecated. Use the `as_c_str` method on strings instead.
 */
#[inline(always)]
pub fn as_c_str<T>(s: &str, f: &fn(*libc::c_char) -> T) -> T {
    s.as_c_str(f)
}

/**
 * Work with the byte buffer and length of a slice.
 *
 * The given length is one byte longer than the 'official' indexable
 * length of the string. This is to permit probing the byte past the
 * indexable area for a null byte, as is the case in slices pointing
 * to full strings, or suffixes of them.
 */
#[inline(always)]
pub fn as_buf<T>(s: &str, f: &fn(*u8, uint) -> T) -> T {
    unsafe {
        let v : *(*u8,uint) = transmute(&s);
        let (buf,len) = *v;
        f(buf, len)
    }
}

/**
 * Returns the byte offset of an inner slice relative to an enclosing outer slice
 *
 * # Example
 *
 * ~~~ {.rust}
 * let string = "a\nb\nc";
 * let mut lines = ~[];
 * for string.line_iter().advance |line| { lines.push(line) }
 *
 * assert!(subslice_offset(string, lines[0]) == 0); // &"a"
 * assert!(subslice_offset(string, lines[1]) == 2); // &"b"
 * assert!(subslice_offset(string, lines[2]) == 4); // &"c"
 * ~~~
 */
#[inline(always)]
pub fn subslice_offset(outer: &str, inner: &str) -> uint {
    do as_buf(outer) |a, a_len| {
        do as_buf(inner) |b, b_len| {
            let a_start: uint;
            let a_end: uint;
            let b_start: uint;
            let b_end: uint;
            unsafe {
                a_start = cast::transmute(a); a_end = a_len + cast::transmute(a);
                b_start = cast::transmute(b); b_end = b_len + cast::transmute(b);
            }
            assert!(a_start <= b_start);
            assert!(b_end <= a_end);
            b_start - a_start
        }
    }
}

/**
 * Reserves capacity for exactly `n` bytes in the given string, not including
 * the null terminator.
 *
 * Assuming single-byte characters, the resulting string will be large
 * enough to hold a string of length `n`. To account for the null terminator,
 * the underlying buffer will have the size `n` + 1.
 *
 * If the capacity for `s` is already equal to or greater than the requested
 * capacity, then no action is taken.
 *
 * # Arguments
 *
 * * s - A string
 * * n - The number of bytes to reserve space for
 */
#[inline(always)]
pub fn reserve(s: &mut ~str, n: uint) {
    unsafe {
        let v: *mut ~[u8] = cast::transmute(s);
        vec::reserve(&mut *v, n + 1);
    }
}

/**
 * Reserves capacity for at least `n` bytes in the given string, not including
 * the null terminator.
 *
 * Assuming single-byte characters, the resulting string will be large
 * enough to hold a string of length `n`. To account for the null terminator,
 * the underlying buffer will have the size `n` + 1.
 *
 * This function will over-allocate in order to amortize the allocation costs
 * in scenarios where the caller may need to repeatedly reserve additional
 * space.
 *
 * If the capacity for `s` is already equal to or greater than the requested
 * capacity, then no action is taken.
 *
 * # Arguments
 *
 * * s - A string
 * * n - The number of bytes to reserve space for
 */
#[inline(always)]
pub fn reserve_at_least(s: &mut ~str, n: uint) {
    reserve(s, uint::next_power_of_two(n + 1u) - 1u)
}

/**
 * Returns the number of single-byte characters the string can hold without
 * reallocating
 */
pub fn capacity(s: &const ~str) -> uint {
    do as_bytes(s) |buf| {
        let vcap = vec::capacity(buf);
        assert!(vcap > 0u);
        vcap - 1u
    }
}

/// Escape each char in `s` with char::escape_default.
pub fn escape_default(s: &str) -> ~str {
    let mut out: ~str = ~"";
    reserve_at_least(&mut out, s.len());
    for s.iter().advance |c| {
        push_str(&mut out, char::escape_default(c));
    }
    out
}

/// Escape each char in `s` with char::escape_unicode.
pub fn escape_unicode(s: &str) -> ~str {
    let mut out: ~str = ~"";
    reserve_at_least(&mut out, s.len());
    for s.iter().advance |c| {
        push_str(&mut out, char::escape_unicode(c));
    }
    out
}

/// Unsafe operations
pub mod raw {
    use cast;
    use libc;
    use ptr;
    use str::raw;
    use str::{as_buf, is_utf8, reserve_at_least};
    use vec;

    /// Create a Rust string from a null-terminated *u8 buffer
    pub unsafe fn from_buf(buf: *u8) -> ~str {
        let mut (curr, i) = (buf, 0u);
        while *curr != 0u8 {
            i += 1u;
            curr = ptr::offset(buf, i);
        }
        return from_buf_len(buf, i);
    }

    /// Create a Rust string from a *u8 buffer of the given length
    pub unsafe fn from_buf_len(buf: *const u8, len: uint) -> ~str {
        let mut v: ~[u8] = vec::with_capacity(len + 1);
        vec::as_mut_buf(v, |vbuf, _len| {
            ptr::copy_memory(vbuf, buf as *u8, len)
        });
        vec::raw::set_len(&mut v, len);
        v.push(0u8);

        assert!(is_utf8(v));
        return ::cast::transmute(v);
    }

    /// Create a Rust string from a null-terminated C string
    pub unsafe fn from_c_str(c_str: *libc::c_char) -> ~str {
        from_buf(::cast::transmute(c_str))
    }

    /// Create a Rust string from a `*c_char` buffer of the given length
    pub unsafe fn from_c_str_len(c_str: *libc::c_char, len: uint) -> ~str {
        from_buf_len(::cast::transmute(c_str), len)
    }

    /// Converts a vector of bytes to a new owned string.
    pub unsafe fn from_bytes(v: &const [u8]) -> ~str {
        do vec::as_const_buf(v) |buf, len| {
            from_buf_len(buf, len)
        }
    }

    /// Converts a vector of bytes to a string.
    /// The byte slice needs to contain valid utf8 and needs to be one byte longer than
    /// the string, if possible ending in a 0 byte.
    pub unsafe fn from_bytes_with_null<'a>(v: &'a [u8]) -> &'a str {
        cast::transmute(v)
    }

    /// Converts a byte to a string.
    pub unsafe fn from_byte(u: u8) -> ~str { raw::from_bytes([u]) }

    /// Form a slice from a *u8 buffer of the given length without copying.
    pub unsafe fn buf_as_slice<T>(buf: *u8, len: uint,
                              f: &fn(v: &str) -> T) -> T {
        let v = (buf, len + 1);
        assert!(is_utf8(::cast::transmute(v)));
        f(::cast::transmute(v))
    }

    /**
     * Takes a bytewise (not UTF-8) slice from a string.
     *
     * Returns the substring from [`begin`..`end`).
     *
     * # Failure
     *
     * If begin is greater than end.
     * If end is greater than the length of the string.
     */
    pub unsafe fn slice_bytes_owned(s: &str, begin: uint, end: uint) -> ~str {
        do as_buf(s) |sbuf, n| {
            assert!((begin <= end));
            assert!((end <= n));

            let mut v = vec::with_capacity(end - begin + 1u);
            do vec::as_imm_buf(v) |vbuf, _vlen| {
                let vbuf = ::cast::transmute_mut_unsafe(vbuf);
                let src = ptr::offset(sbuf, begin);
                ptr::copy_memory(vbuf, src, end - begin);
            }
            vec::raw::set_len(&mut v, end - begin);
            v.push(0u8);
            ::cast::transmute(v)
        }
    }

    /**
     * Takes a bytewise (not UTF-8) slice from a string.
     *
     * Returns the substring from [`begin`..`end`).
     *
     * # Failure
     *
     * If begin is greater than end.
     * If end is greater than the length of the string.
     */
    #[inline]
    pub unsafe fn slice_bytes(s: &str, begin: uint, end: uint) -> &str {
        do as_buf(s) |sbuf, n| {
             assert!((begin <= end));
             assert!((end <= n));

             let tuple = (ptr::offset(sbuf, begin), end - begin + 1);
             ::cast::transmute(tuple)
        }
    }

    /// Appends a byte to a string. (Not UTF-8 safe).
    pub unsafe fn push_byte(s: &mut ~str, b: u8) {
        let new_len = s.len() + 1;
        reserve_at_least(&mut *s, new_len);
        do as_buf(*s) |buf, len| {
            let buf: *mut u8 = ::cast::transmute(buf);
            *ptr::mut_offset(buf, len) = b;
        }
        set_len(&mut *s, new_len);
    }

    /// Appends a vector of bytes to a string. (Not UTF-8 safe).
    unsafe fn push_bytes(s: &mut ~str, bytes: &[u8]) {
        let new_len = s.len() + bytes.len();
        reserve_at_least(&mut *s, new_len);
        for bytes.each |byte| { push_byte(&mut *s, *byte); }
    }

    /// Removes the last byte from a string and returns it. (Not UTF-8 safe).
    pub unsafe fn pop_byte(s: &mut ~str) -> u8 {
        let len = s.len();
        assert!((len > 0u));
        let b = s[len - 1u];
        set_len(s, len - 1u);
        return b;
    }

    /// Removes the first byte from a string and returns it. (Not UTF-8 safe).
    pub unsafe fn shift_byte(s: &mut ~str) -> u8 {
        let len = s.len();
        assert!((len > 0u));
        let b = s[0];
        *s = raw::slice_bytes_owned(*s, 1u, len);
        return b;
    }

    /// Sets the length of the string and adds the null terminator
    #[inline]
    pub unsafe fn set_len(v: &mut ~str, new_len: uint) {
        let v: **mut vec::raw::VecRepr = cast::transmute(v);
        let repr: *mut vec::raw::VecRepr = *v;
        (*repr).unboxed.fill = new_len + 1u;
        let null = ptr::mut_offset(cast::transmute(&((*repr).unboxed.data)),
                                   new_len);
        *null = 0u8;
    }

    #[test]
    fn test_from_buf_len() {
        unsafe {
            let a = ~[65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 0u8];
            let b = vec::raw::to_ptr(a);
            let c = from_buf_len(b, 3u);
            assert_eq!(c, ~"AAA");
        }
    }

}

#[cfg(not(test))]
pub mod traits {
    use ops::Add;
    use str::append;

    impl<'self> Add<&'self str,~str> for ~str {
        #[inline(always)]
        fn add(&self, rhs: & &'self str) -> ~str {
            append(copy *self, (*rhs))
        }
    }
}

#[cfg(test)]
pub mod traits {}

#[allow(missing_doc)]
pub trait StrSlice<'self> {
    fn contains<'a>(&self, needle: &'a str) -> bool;
    fn contains_char(&self, needle: char) -> bool;
    fn iter(&self) -> StrCharIterator<'self>;
    fn rev_iter(&self) -> StrCharRevIterator<'self>;
    fn bytes_iter(&self) -> StrBytesIterator<'self>;
    fn bytes_rev_iter(&self) -> StrBytesRevIterator<'self>;
    fn split_iter<Sep: StrCharSplitSeparator>(&self, sep: Sep) -> StrCharSplitIterator<'self, Sep>;
    fn splitn_iter<Sep: StrCharSplitSeparator>(&self, sep: Sep, count: uint)
        -> StrCharSplitIterator<'self, Sep>;
    fn split_options_iter<Sep: StrCharSplitSeparator>(&self, sep: Sep,
                                                      count: uint, allow_trailing_empty: bool)
        -> StrCharSplitIterator<'self, Sep>;
    /// An iterator over the lines of a string (subsequences separated
    /// by `\n`).
    fn line_iter(&self) -> StrCharSplitIterator<'self, char>;
    /// An iterator over the words of a string (subsequences separated
    /// by any sequence of whitespace).
    fn word_iter(&self) -> WordIterator<'self>;
    fn ends_with(&self, needle: &str) -> bool;
    fn is_empty(&self) -> bool;
    fn is_whitespace(&self) -> bool;
    fn is_alphanumeric(&self) -> bool;
    fn len(&self) -> uint;
    fn char_len(&self) -> uint;
    fn slice(&self, begin: uint, end: uint) -> &'self str;
    fn each_split_str<'a>(&self, sep: &'a str, it: &fn(&'self str) -> bool) -> bool;
    fn starts_with<'a>(&self, needle: &'a str) -> bool;
    fn substr(&self, begin: uint, n: uint) -> &'self str;
    fn escape_default(&self) -> ~str;
    fn escape_unicode(&self) -> ~str;
    fn trim(&self) -> &'self str;
    fn trim_left(&self) -> &'self str;
    fn trim_right(&self) -> &'self str;
    fn trim_chars(&self, chars_to_trim: &[char]) -> &'self str;
    fn trim_left_chars(&self, chars_to_trim: &[char]) -> &'self str;
    fn trim_right_chars(&self, chars_to_trim: &[char]) -> &'self str;
    fn to_owned(&self) -> ~str;
    fn to_managed(&self) -> @str;
    fn char_at(&self, i: uint) -> char;
    fn char_at_reverse(&self, i: uint) -> char;
    fn to_bytes(&self) -> ~[u8];
}

/// Extension methods for strings
impl<'self> StrSlice<'self> for &'self str {
    /// Returns true if one string contains another
    #[inline]
    fn contains<'a>(&self, needle: &'a str) -> bool {
        contains(*self, needle)
    }
    /// Returns true if a string contains a char
    #[inline]
    fn contains_char(&self, needle: char) -> bool {
        contains_char(*self, needle)
    }

    #[inline]
    fn iter(&self) -> StrCharIterator<'self> {
        StrCharIterator {
            index: 0,
            string: *self
        }
    }
    #[inline]
    fn rev_iter(&self) -> StrCharRevIterator<'self> {
        StrCharRevIterator {
            index: self.len(),
            string: *self
        }
    }

    fn bytes_iter(&self) -> StrBytesIterator<'self> {
        StrBytesIterator { it: as_bytes_slice(*self).iter() }
    }
    fn bytes_rev_iter(&self) -> StrBytesRevIterator<'self> {
        StrBytesRevIterator { it: as_bytes_slice(*self).rev_iter() }
    }

    fn split_iter<Sep: StrCharSplitSeparator>(&self, sep: Sep) -> StrCharSplitIterator<'self, Sep> {
        self.split_options_iter(sep, self.len(), true)
    }

    fn splitn_iter<Sep: StrCharSplitSeparator>(&self, sep: Sep, count: uint)
        -> StrCharSplitIterator<'self, Sep> {
        self.split_options_iter(sep, count, true)
    }
    fn split_options_iter<Sep: StrCharSplitSeparator>(&self, sep: Sep,
                                                      count: uint, allow_trailing_empty: bool)
        -> StrCharSplitIterator<'self, Sep> {
        let only_ascii = sep.only_ascii();
        StrCharSplitIterator {
            string: *self,
            position: 0,
            sep: sep,
            count: count,
            allow_trailing_empty: allow_trailing_empty,
            finished: false,
            only_ascii: only_ascii
        }
    }

    fn line_iter(&self) -> StrCharSplitIterator<'self, char> {
        self.split_options_iter('\n', self.len(), false)
    }
    fn word_iter(&self) -> WordIterator<'self> {
        self.split_iter(char::is_whitespace).filter(|s| !s.is_empty())
    }


    /// Returns true if one string ends with another
    #[inline]
    fn ends_with(&self, needle: &str) -> bool {
        ends_with(*self, needle)
    }
    /// Returns true if the string has length 0
    #[inline]
    fn is_empty(&self) -> bool { self.len() == 0 }
    /**
     * Returns true if the string contains only whitespace
     *
     * Whitespace characters are determined by `char::is_whitespace`
     */
    #[inline]
    fn is_whitespace(&self) -> bool { is_whitespace(*self) }
    /**
     * Returns true if the string contains only alphanumerics
     *
     * Alphanumeric characters are determined by `char::is_alphanumeric`
     */
    #[inline]
    fn is_alphanumeric(&self) -> bool { is_alphanumeric(*self) }
    /// Returns the size in bytes not counting the null terminator
    #[inline(always)]
    fn len(&self) -> uint {
        do as_buf(*self) |_p, n| { n - 1u }
    }
    /// Returns the number of characters that a string holds
    #[inline]
    fn char_len(&self) -> uint { char_len(*self) }
    /**
     * Returns a slice of the given string from the byte range
     * [`begin`..`end`)
     *
     * Fails when `begin` and `end` do not point to valid characters or
     * beyond the last character of the string
     */
    #[inline]
    fn slice(&self, begin: uint, end: uint) -> &'self str {
        assert!(is_char_boundary(*self, begin));
        assert!(is_char_boundary(*self, end));
        unsafe { raw::slice_bytes(*self, begin, end) }
    }
    /**
     * Splits a string into a vector of the substrings separated by a given
     * string
     */
    #[inline]
    fn each_split_str<'a>(&self, sep: &'a str, it: &fn(&'self str) -> bool) -> bool {
        each_split_str(*self, sep, it)
    }
    /// Returns true if one string starts with another
    #[inline]
    fn starts_with<'a>(&self, needle: &'a str) -> bool {
        starts_with(*self, needle)
    }
    /**
     * Take a substring of another.
     *
     * Returns a string containing `n` characters starting at byte offset
     * `begin`.
     */
    #[inline]
    fn substr(&self, begin: uint, n: uint) -> &'self str {
        substr(*self, begin, n)
    }
    /// Escape each char in `s` with char::escape_default.
    #[inline]
    fn escape_default(&self) -> ~str { escape_default(*self) }
    /// Escape each char in `s` with char::escape_unicode.
    #[inline]
    fn escape_unicode(&self) -> ~str { escape_unicode(*self) }

    /// Returns a string with leading and trailing whitespace removed
    #[inline]
    fn trim(&self) -> &'self str { trim(*self) }
    /// Returns a string with leading whitespace removed
    #[inline]
    fn trim_left(&self) -> &'self str { trim_left(*self) }
    /// Returns a string with trailing whitespace removed
    #[inline]
    fn trim_right(&self) -> &'self str { trim_right(*self) }

    #[inline]
    fn trim_chars(&self, chars_to_trim: &[char]) -> &'self str {
        trim_chars(*self, chars_to_trim)
    }
    #[inline]
    fn trim_left_chars(&self, chars_to_trim: &[char]) -> &'self str {
        trim_left_chars(*self, chars_to_trim)
    }
    #[inline]
    fn trim_right_chars(&self, chars_to_trim: &[char]) -> &'self str {
        trim_right_chars(*self, chars_to_trim)
    }


    #[inline]
    fn to_owned(&self) -> ~str { to_owned(*self) }

    #[inline]
    fn to_managed(&self) -> @str {
        let v = at_vec::from_fn(self.len() + 1, |i| {
            if i == self.len() { 0 } else { self[i] }
        });
        unsafe { ::cast::transmute(v) }
    }

    #[inline]
    fn char_at(&self, i: uint) -> char { char_at(*self, i) }

    #[inline]
    fn char_at_reverse(&self, i: uint) -> char {
        char_at_reverse(*self, i)
    }

    fn to_bytes(&self) -> ~[u8] { to_bytes(*self) }
}

#[allow(missing_doc)]
pub trait OwnedStr {
    fn push_str(&mut self, v: &str);
    fn push_char(&mut self, c: char);
}

impl OwnedStr for ~str {
    #[inline]
    fn push_str(&mut self, v: &str) {
        push_str(self, v);
    }
    #[inline]
    fn push_char(&mut self, c: char) {
        push_char(self, c);
    }
}

impl Clone for ~str {
    #[inline(always)]
    fn clone(&self) -> ~str {
        to_owned(*self)
    }
}

/// External iterator for a string's characters. Use with the `std::iterator`
/// module.
pub struct StrCharIterator<'self> {
    priv index: uint,
    priv string: &'self str,
}

impl<'self> Iterator<char> for StrCharIterator<'self> {
    #[inline]
    fn next(&mut self) -> Option<char> {
        if self.index < self.string.len() {
            let CharRange {ch, next} = char_range_at(self.string, self.index);
            self.index = next;
            Some(ch)
        } else {
            None
        }
    }
}
/// External iterator for a string's characters in reverse order. Use
/// with the `std::iterator` module.
pub struct StrCharRevIterator<'self> {
    priv index: uint,
    priv string: &'self str,
}

impl<'self> Iterator<char> for StrCharRevIterator<'self> {
    #[inline]
    fn next(&mut self) -> Option<char> {
        if self.index > 0 {
            let CharRange {ch, next} = char_range_at_reverse(self.string, self.index);
            self.index = next;
            Some(ch)
        } else {
            None
        }
    }
}

/// External iterator for a string's bytes. Use with the `std::iterator`
/// module.
pub struct StrBytesIterator<'self> {
    priv it: vec::VecIterator<'self, u8>
}

impl<'self> Iterator<u8> for StrBytesIterator<'self> {
    #[inline]
    fn next(&mut self) -> Option<u8> {
        self.it.next().map_consume(|&x| x)
    }
}

/// External iterator for a string's bytes in reverse order. Use with
/// the `std::iterator` module.
pub struct StrBytesRevIterator<'self> {
    priv it: vec::VecRevIterator<'self, u8>
}

impl<'self> Iterator<u8> for StrBytesRevIterator<'self> {
    #[inline]
    fn next(&mut self) -> Option<u8> {
        self.it.next().map_consume(|&x| x)
    }
}

#[cfg(test)]
mod tests {
    use iterator::IteratorUtil;
    use container::Container;
    use char;
    use option::Some;
    use libc::c_char;
    use libc;
    use old_iter::BaseIter;
    use ptr;
    use str::*;
    use vec;
    use vec::ImmutableVector;
    use cmp::{TotalOrd, Less, Equal, Greater};

    #[test]
    fn test_eq() {
        assert!((eq(&~"", &~"")));
        assert!((eq(&~"foo", &~"foo")));
        assert!((!eq(&~"foo", &~"bar")));
    }

    #[test]
    fn test_eq_slice() {
        assert!((eq_slice("foobar".slice(0, 3), "foo")));
        assert!((eq_slice("barfoo".slice(3, 6), "foo")));
        assert!((!eq_slice("foo1", "foo2")));
    }

    #[test]
    fn test_le() {
        assert!((le(&"", &"")));
        assert!((le(&"", &"foo")));
        assert!((le(&"foo", &"foo")));
        assert!((!eq(&~"foo", &~"bar")));
    }

    #[test]
    fn test_len() {
        assert_eq!("".len(), 0u);
        assert_eq!("hello world".len(), 11u);
        assert_eq!("\x63".len(), 1u);
        assert_eq!("\xa2".len(), 2u);
        assert_eq!("\u03c0".len(), 2u);
        assert_eq!("\u2620".len(), 3u);
        assert_eq!("\U0001d11e".len(), 4u);

        assert_eq!(char_len(""), 0u);
        assert_eq!(char_len("hello world"), 11u);
        assert_eq!(char_len("\x63"), 1u);
        assert_eq!(char_len("\xa2"), 1u);
        assert_eq!(char_len("\u03c0"), 1u);
        assert_eq!(char_len("\u2620"), 1u);
        assert_eq!(char_len("\U0001d11e"), 1u);
        assert_eq!(char_len("ประเทศไทย中华Việt Nam"), 19u);
    }

    #[test]
    fn test_rfind_char() {
        assert_eq!(rfind_char("hello", 'l'), Some(3u));
        assert_eq!(rfind_char("hello", 'o'), Some(4u));
        assert_eq!(rfind_char("hello", 'h'), Some(0u));
        assert!(rfind_char("hello", 'z').is_none());
        assert_eq!(rfind_char("ประเทศไทย中华Việt Nam", '华'), Some(30u));
    }

    #[test]
    fn test_pop_char() {
        let mut data = ~"ประเทศไทย中华";
        let cc = pop_char(&mut data);
        assert_eq!(~"ประเทศไทย中", data);
        assert_eq!('华', cc);
    }

    #[test]
    fn test_pop_char_2() {
        let mut data2 = ~"华";
        let cc2 = pop_char(&mut data2);
        assert_eq!(~"", data2);
        assert_eq!('华', cc2);
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_pop_char_fail() {
        let mut data = ~"";
        let _cc3 = pop_char(&mut data);
    }

    #[test]
    fn test_split_str() {
        fn t<'a>(s: &str, sep: &'a str, u: &[~str]) {
            let mut v = ~[];
            for each_split_str(s, sep) |s| { v.push(s.to_owned()) }
            assert!(v.iter().zip(u.iter()).all(|(a,b)| a == b));
        }
        t("--1233345--", "12345", [~"--1233345--"]);
        t("abc::hello::there", "::", [~"abc", ~"hello", ~"there"]);
        t("::hello::there", "::", [~"", ~"hello", ~"there"]);
        t("hello::there::", "::", [~"hello", ~"there", ~""]);
        t("::hello::there::", "::", [~"", ~"hello", ~"there", ~""]);
        t("ประเทศไทย中华Việt Nam", "中华", [~"ประเทศไทย", ~"Việt Nam"]);
        t("zzXXXzzYYYzz", "zz", [~"", ~"XXX", ~"YYY", ~""]);
        t("zzXXXzYYYz", "XXX", [~"zz", ~"zYYYz"]);
        t(".XXX.YYY.", ".", [~"", ~"XXX", ~"YYY", ~""]);
        t("", ".", [~""]);
        t("zz", "zz", [~"",~""]);
        t("ok", "z", [~"ok"]);
        t("zzz", "zz", [~"",~"z"]);
        t("zzzzz", "zz", [~"",~"",~"z"]);
    }


    #[test]
    fn test_split_within() {
        fn t(s: &str, i: uint, u: &[~str]) {
            let mut v = ~[];
            for each_split_within(s, i) |s| { v.push(s.to_owned()) }
            assert!(v.iter().zip(u.iter()).all(|(a,b)| a == b));
        }
        t("", 0, []);
        t("", 15, []);
        t("hello", 15, [~"hello"]);
        t("\nMary had a little lamb\nLittle lamb\n", 15,
            [~"Mary had a", ~"little lamb", ~"Little lamb"]);
    }

    #[test]
    fn test_find_str() {
        // byte positions
        assert!(find_str("banana", "apple pie").is_none());
        assert_eq!(find_str("", ""), Some(0u));

        let data = "ประเทศไทย中华Việt Nam";
        assert_eq!(find_str(data, ""), Some(0u));
        assert_eq!(find_str(data, "ประเ"), Some( 0u));
        assert_eq!(find_str(data, "ะเ"), Some( 6u));
        assert_eq!(find_str(data, "中华"), Some(27u));
        assert!(find_str(data, "ไท华").is_none());
    }

    #[test]
    fn test_find_str_between() {
        // byte positions
        assert_eq!(find_str_between("", "", 0u, 0u), Some(0u));

        let data = "abcabc";
        assert_eq!(find_str_between(data, "ab", 0u, 6u), Some(0u));
        assert_eq!(find_str_between(data, "ab", 2u, 6u), Some(3u));
        assert!(find_str_between(data, "ab", 2u, 4u).is_none());

        let mut data = ~"ประเทศไทย中华Việt Nam";
        data = data + data;
        assert_eq!(find_str_between(data, "", 0u, 43u), Some(0u));
        assert_eq!(find_str_between(data, "", 6u, 43u), Some(6u));

        assert_eq!(find_str_between(data, "ประ", 0u, 43u), Some( 0u));
        assert_eq!(find_str_between(data, "ทศไ", 0u, 43u), Some(12u));
        assert_eq!(find_str_between(data, "ย中", 0u, 43u), Some(24u));
        assert_eq!(find_str_between(data, "iệt", 0u, 43u), Some(34u));
        assert_eq!(find_str_between(data, "Nam", 0u, 43u), Some(40u));

        assert_eq!(find_str_between(data, "ประ", 43u, 86u), Some(43u));
        assert_eq!(find_str_between(data, "ทศไ", 43u, 86u), Some(55u));
        assert_eq!(find_str_between(data, "ย中", 43u, 86u), Some(67u));
        assert_eq!(find_str_between(data, "iệt", 43u, 86u), Some(77u));
        assert_eq!(find_str_between(data, "Nam", 43u, 86u), Some(83u));
    }

    #[test]
    fn test_substr() {
        fn t(a: &str, b: &str, start: int) {
            assert_eq!(substr(a, start as uint, b.len()), b);
        }
        t("hello", "llo", 2);
        t("hello", "el", 1);
        assert_eq!("ะเทศไท", substr("ประเทศไทย中华Việt Nam", 6u, 6u));
    }

    #[test]
    fn test_concat() {
        fn t(v: &[~str], s: &str) {
            assert_eq!(concat(v), s.to_str());
            assert_eq!(v.concat(), s.to_str());
        }
        t([~"you", ~"know", ~"I'm", ~"no", ~"good"], "youknowI'mnogood");
        let v: &[~str] = [];
        t(v, "");
        t([~"hi"], "hi");
    }

    #[test]
    fn test_connect() {
        fn t(v: &[~str], sep: &str, s: &str) {
            assert_eq!(connect(v, sep), s.to_str());
            assert_eq!(v.connect(sep), s.to_str());
        }
        t([~"you", ~"know", ~"I'm", ~"no", ~"good"],
          " ", "you know I'm no good");
        let v: &[~str] = [];
        t(v, " ", "");
        t([~"hi"], " ", "hi");
    }

    #[test]
    fn test_concat_slices() {
        fn t(v: &[&str], s: &str) {
            assert_eq!(concat_slices(v), s.to_str());
            assert_eq!(v.concat(), s.to_str());
        }
        t(["you", "know", "I'm", "no", "good"], "youknowI'mnogood");
        let v: &[&str] = [];
        t(v, "");
        t(["hi"], "hi");
    }

    #[test]
    fn test_connect_slices() {
        fn t(v: &[&str], sep: &str, s: &str) {
            assert_eq!(connect_slices(v, sep), s.to_str());
            assert_eq!(v.connect(sep), s.to_str());
        }
        t(["you", "know", "I'm", "no", "good"],
          " ", "you know I'm no good");
        t([], " ", "");
        t(["hi"], " ", "hi");
    }

    #[test]
    fn test_repeat() {
        assert_eq!(repeat("x", 4), ~"xxxx");
        assert_eq!(repeat("hi", 4), ~"hihihihi");
        assert_eq!(repeat("ไท华", 3), ~"ไท华ไท华ไท华");
        assert_eq!(repeat("", 4), ~"");
        assert_eq!(repeat("hi", 0), ~"");
    }

    #[test]
    fn test_unsafe_slice() {
        assert_eq!("ab", unsafe {raw::slice_bytes("abc", 0, 2)});
        assert_eq!("bc", unsafe {raw::slice_bytes("abc", 1, 3)});
        assert_eq!("", unsafe {raw::slice_bytes("abc", 1, 1)});
        fn a_million_letter_a() -> ~str {
            let mut i = 0;
            let mut rs = ~"";
            while i < 100000 { push_str(&mut rs, "aaaaaaaaaa"); i += 1; }
            rs
        }
        fn half_a_million_letter_a() -> ~str {
            let mut i = 0;
            let mut rs = ~"";
            while i < 100000 { push_str(&mut rs, "aaaaa"); i += 1; }
            rs
        }
        let letters = a_million_letter_a();
        assert!(half_a_million_letter_a() ==
            unsafe {raw::slice_bytes(letters, 0u, 500000)}.to_owned());
    }

    #[test]
    fn test_starts_with() {
        assert!((starts_with("", "")));
        assert!((starts_with("abc", "")));
        assert!((starts_with("abc", "a")));
        assert!((!starts_with("a", "abc")));
        assert!((!starts_with("", "abc")));
    }

    #[test]
    fn test_ends_with() {
        assert!((ends_with("", "")));
        assert!((ends_with("abc", "")));
        assert!((ends_with("abc", "c")));
        assert!((!ends_with("a", "abc")));
        assert!((!ends_with("", "abc")));
    }

    #[test]
    fn test_is_empty() {
        assert!("".is_empty());
        assert!(!"a".is_empty());
    }

    #[test]
    fn test_replace() {
        let a = "a";
        assert_eq!(replace("", a, "b"), ~"");
        assert_eq!(replace("a", a, "b"), ~"b");
        assert_eq!(replace("ab", a, "b"), ~"bb");
        let test = "test";
        assert!(replace(" test test ", test, "toast") ==
            ~" toast toast ");
        assert_eq!(replace(" test test ", test, ""), ~"   ");
    }

    #[test]
    fn test_replace_2a() {
        let data = ~"ประเทศไทย中华";
        let repl = ~"دولة الكويت";

        let a = ~"ประเ";
        let A = ~"دولة الكويتทศไทย中华";
        assert_eq!(replace(data, a, repl), A);
    }

    #[test]
    fn test_replace_2b() {
        let data = ~"ประเทศไทย中华";
        let repl = ~"دولة الكويت";

        let b = ~"ะเ";
        let B = ~"ปรدولة الكويتทศไทย中华";
        assert_eq!(replace(data, b,   repl), B);
    }

    #[test]
    fn test_replace_2c() {
        let data = ~"ประเทศไทย中华";
        let repl = ~"دولة الكويت";

        let c = ~"中华";
        let C = ~"ประเทศไทยدولة الكويت";
        assert_eq!(replace(data, c, repl), C);
    }

    #[test]
    fn test_replace_2d() {
        let data = ~"ประเทศไทย中华";
        let repl = ~"دولة الكويت";

        let d = ~"ไท华";
        assert_eq!(replace(data, d, repl), data);
    }

    #[test]
    fn test_slice() {
        assert_eq!("ab", "abc".slice(0, 2));
        assert_eq!("bc", "abc".slice(1, 3));
        assert_eq!("", "abc".slice(1, 1));
        assert_eq!("\u65e5", "\u65e5\u672c".slice(0, 3));

        let data = "ประเทศไทย中华";
        assert_eq!("ป", data.slice(0, 3));
        assert_eq!("ร", data.slice(3, 6));
        assert_eq!("", data.slice(3, 3));
        assert_eq!("华", data.slice(30, 33));

        fn a_million_letter_X() -> ~str {
            let mut i = 0;
            let mut rs = ~"";
            while i < 100000 {
                push_str(&mut rs, "华华华华华华华华华华");
                i += 1;
            }
            rs
        }
        fn half_a_million_letter_X() -> ~str {
            let mut i = 0;
            let mut rs = ~"";
            while i < 100000 { push_str(&mut rs, "华华华华华"); i += 1; }
            rs
        }
        let letters = a_million_letter_X();
        assert!(half_a_million_letter_X() ==
            letters.slice(0u, 3u * 500000u).to_owned());
    }

    #[test]
    fn test_slice_2() {
        let ss = "中华Việt Nam";

        assert_eq!("华", ss.slice(3u, 6u));
        assert_eq!("Việt Nam", ss.slice(6u, 16u));

        assert_eq!("ab", "abc".slice(0u, 2u));
        assert_eq!("bc", "abc".slice(1u, 3u));
        assert_eq!("", "abc".slice(1u, 1u));

        assert_eq!("中", ss.slice(0u, 3u));
        assert_eq!("华V", ss.slice(3u, 7u));
        assert_eq!("", ss.slice(3u, 3u));
        /*0: 中
          3: 华
          6: V
          7: i
          8: ệ
         11: t
         12:
         13: N
         14: a
         15: m */
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_slice_fail() {
        "中华Việt Nam".slice(0u, 2u);
    }

    #[test]
    fn test_trim_left_chars() {
        assert!(trim_left_chars(" *** foo *** ", []) == " *** foo *** ");
        assert!(trim_left_chars(" *** foo *** ", ['*', ' ']) == "foo *** ");
        assert_eq!(trim_left_chars(" ***  *** ", ['*', ' ']), "");
        assert!(trim_left_chars("foo *** ", ['*', ' ']) == "foo *** ");
    }

    #[test]
    fn test_trim_right_chars() {
        assert!(trim_right_chars(" *** foo *** ", []) == " *** foo *** ");
        assert!(trim_right_chars(" *** foo *** ", ['*', ' ']) == " *** foo");
        assert_eq!(trim_right_chars(" ***  *** ", ['*', ' ']), "");
        assert!(trim_right_chars(" *** foo", ['*', ' ']) == " *** foo");
    }

    #[test]
    fn test_trim_chars() {
        assert_eq!(trim_chars(" *** foo *** ", []), " *** foo *** ");
        assert_eq!(trim_chars(" *** foo *** ", ['*', ' ']), "foo");
        assert_eq!(trim_chars(" ***  *** ", ['*', ' ']), "");
        assert_eq!(trim_chars("foo", ['*', ' ']), "foo");
    }

    #[test]
    fn test_trim_left() {
        assert_eq!(trim_left(""), "");
        assert_eq!(trim_left("a"), "a");
        assert_eq!(trim_left("    "), "");
        assert_eq!(trim_left("     blah"), "blah");
        assert_eq!(trim_left("   \u3000  wut"), "wut");
        assert_eq!(trim_left("hey "), "hey ");
    }

    #[test]
    fn test_trim_right() {
        assert_eq!(trim_right(""), "");
        assert_eq!(trim_right("a"), "a");
        assert_eq!(trim_right("    "), "");
        assert_eq!(trim_right("blah     "), "blah");
        assert_eq!(trim_right("wut   \u3000  "), "wut");
        assert_eq!(trim_right(" hey"), " hey");
    }

    #[test]
    fn test_trim() {
        assert_eq!(trim(""), "");
        assert_eq!(trim("a"), "a");
        assert_eq!(trim("    "), "");
        assert_eq!(trim("    blah     "), "blah");
        assert_eq!(trim("\nwut   \u3000  "), "wut");
        assert_eq!(trim(" hey dude "), "hey dude");
    }

    #[test]
    fn test_is_whitespace() {
        assert!(is_whitespace(""));
        assert!(is_whitespace(" "));
        assert!(is_whitespace("\u2009")); // Thin space
        assert!(is_whitespace("  \n\t   "));
        assert!(!is_whitespace("   _   "));
    }

    #[test]
    fn test_shift_byte() {
        let mut s = ~"ABC";
        let b = unsafe{raw::shift_byte(&mut s)};
        assert_eq!(s, ~"BC");
        assert_eq!(b, 65u8);
    }

    #[test]
    fn test_pop_byte() {
        let mut s = ~"ABC";
        let b = unsafe{raw::pop_byte(&mut s)};
        assert_eq!(s, ~"AB");
        assert_eq!(b, 67u8);
    }

    #[test]
    fn test_unsafe_from_bytes() {
        let a = ~[65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 65u8];
        let b = unsafe { raw::from_bytes(a) };
        assert_eq!(b, ~"AAAAAAA");
    }

    #[test]
    fn test_from_bytes() {
        let ss = ~"ศไทย中华Việt Nam";
        let bb = ~[0xe0_u8, 0xb8_u8, 0xa8_u8,
                  0xe0_u8, 0xb9_u8, 0x84_u8,
                  0xe0_u8, 0xb8_u8, 0x97_u8,
                  0xe0_u8, 0xb8_u8, 0xa2_u8,
                  0xe4_u8, 0xb8_u8, 0xad_u8,
                  0xe5_u8, 0x8d_u8, 0x8e_u8,
                  0x56_u8, 0x69_u8, 0xe1_u8,
                  0xbb_u8, 0x87_u8, 0x74_u8,
                  0x20_u8, 0x4e_u8, 0x61_u8,
                  0x6d_u8];

        assert_eq!(ss, from_bytes(bb));
    }

    #[test]
    #[ignore(cfg(windows))]
    fn test_from_bytes_fail() {
        use str::not_utf8::cond;

        let bb = ~[0xff_u8, 0xb8_u8, 0xa8_u8,
                  0xe0_u8, 0xb9_u8, 0x84_u8,
                  0xe0_u8, 0xb8_u8, 0x97_u8,
                  0xe0_u8, 0xb8_u8, 0xa2_u8,
                  0xe4_u8, 0xb8_u8, 0xad_u8,
                  0xe5_u8, 0x8d_u8, 0x8e_u8,
                  0x56_u8, 0x69_u8, 0xe1_u8,
                  0xbb_u8, 0x87_u8, 0x74_u8,
                  0x20_u8, 0x4e_u8, 0x61_u8,
                  0x6d_u8];

        let mut error_happened = false;
        let _x = do cond.trap(|err| {
            assert_eq!(err, ~"from_bytes: input is not UTF-8; first bad byte is 255");
            error_happened = true;
            ~""
        }).in {
            from_bytes(bb)
        };
        assert!(error_happened);
    }

    #[test]
    fn test_unsafe_from_bytes_with_null() {
        let a = [65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 0u8];
        let b = unsafe { raw::from_bytes_with_null(a) };
        assert_eq!(b, "AAAAAAA");
    }

    #[test]
    fn test_from_bytes_with_null() {
        let ss = "ศไทย中华Việt Nam";
        let bb = [0xe0_u8, 0xb8_u8, 0xa8_u8,
                  0xe0_u8, 0xb9_u8, 0x84_u8,
                  0xe0_u8, 0xb8_u8, 0x97_u8,
                  0xe0_u8, 0xb8_u8, 0xa2_u8,
                  0xe4_u8, 0xb8_u8, 0xad_u8,
                  0xe5_u8, 0x8d_u8, 0x8e_u8,
                  0x56_u8, 0x69_u8, 0xe1_u8,
                  0xbb_u8, 0x87_u8, 0x74_u8,
                  0x20_u8, 0x4e_u8, 0x61_u8,
                  0x6d_u8, 0x0_u8];

        assert_eq!(ss, from_bytes_with_null(bb));
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_from_bytes_with_null_fail() {
        let bb = [0xff_u8, 0xb8_u8, 0xa8_u8,
                  0xe0_u8, 0xb9_u8, 0x84_u8,
                  0xe0_u8, 0xb8_u8, 0x97_u8,
                  0xe0_u8, 0xb8_u8, 0xa2_u8,
                  0xe4_u8, 0xb8_u8, 0xad_u8,
                  0xe5_u8, 0x8d_u8, 0x8e_u8,
                  0x56_u8, 0x69_u8, 0xe1_u8,
                  0xbb_u8, 0x87_u8, 0x74_u8,
                  0x20_u8, 0x4e_u8, 0x61_u8,
                  0x6d_u8, 0x0_u8];

         let _x = from_bytes_with_null(bb);
    }

    #[test]
    #[should_fail]
    #[ignore(cfg(windows))]
    fn test_from_bytes_with_null_fail_2() {
        let bb = [0xff_u8, 0xb8_u8, 0xa8_u8,
                  0xe0_u8, 0xb9_u8, 0x84_u8,
                  0xe0_u8, 0xb8_u8, 0x97_u8,
                  0xe0_u8, 0xb8_u8, 0xa2_u8,
                  0xe4_u8, 0xb8_u8, 0xad_u8,
                  0xe5_u8, 0x8d_u8, 0x8e_u8,
                  0x56_u8, 0x69_u8, 0xe1_u8,
                  0xbb_u8, 0x87_u8, 0x74_u8,
                  0x20_u8, 0x4e_u8, 0x61_u8,
                  0x6d_u8, 0x60_u8];

         let _x = from_bytes_with_null(bb);
    }

    #[test]
    fn test_from_buf() {
        unsafe {
            let a = ~[65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 0u8];
            let b = vec::raw::to_ptr(a);
            let c = raw::from_buf(b);
            assert_eq!(c, ~"AAAAAAA");
        }
    }

    #[test]
    #[ignore(cfg(windows))]
    #[should_fail]
    fn test_as_bytes_fail() {
        // Don't double free
        as_bytes::<()>(&~"", |_bytes| fail!() );
    }

    #[test]
    fn test_as_buf() {
        let a = "Abcdefg";
        let b = as_buf(a, |buf, _l| {
            assert_eq!(unsafe { *buf }, 65u8);
            100
        });
        assert_eq!(b, 100);
    }

    #[test]
    fn test_as_buf_small() {
        let a = "A";
        let b = as_buf(a, |buf, _l| {
            assert_eq!(unsafe { *buf }, 65u8);
            100
        });
        assert_eq!(b, 100);
    }

    #[test]
    fn test_as_buf2() {
        unsafe {
            let s = ~"hello";
            let sb = as_buf(s, |b, _l| b);
            let s_cstr = raw::from_buf(sb);
            assert_eq!(s_cstr, s);
        }
    }

    #[test]
    fn test_as_buf_3() {
        let a = ~"hello";
        do as_buf(a) |buf, len| {
            unsafe {
                assert_eq!(a[0], 'h' as u8);
                assert_eq!(*buf, 'h' as u8);
                assert_eq!(len, 6u);
                assert_eq!(*ptr::offset(buf,4u), 'o' as u8);
                assert_eq!(*ptr::offset(buf,5u), 0u8);
            }
        }
    }

    #[test]
    fn test_subslice_offset() {
        let a = "kernelsprite";
        let b = a.slice(7, a.len());
        let c = a.slice(0, a.len() - 6);
        assert_eq!(subslice_offset(a, b), 7);
        assert_eq!(subslice_offset(a, c), 0);

        let string = "a\nb\nc";
        let mut lines = ~[];
        for string.line_iter().advance |line| { lines.push(line) }
        assert_eq!(subslice_offset(string, lines[0]), 0);
        assert_eq!(subslice_offset(string, lines[1]), 2);
        assert_eq!(subslice_offset(string, lines[2]), 4);
    }

    #[test]
    #[should_fail]
    fn test_subslice_offset_2() {
        let a = "alchemiter";
        let b = "cruxtruder";
        subslice_offset(a, b);
    }

    #[test]
    fn vec_str_conversions() {
        let s1: ~str = ~"All mimsy were the borogoves";

        let v: ~[u8] = to_bytes(s1);
        let s2: ~str = from_bytes(v);
        let mut i: uint = 0u;
        let n1: uint = s1.len();
        let n2: uint = v.len();
        assert_eq!(n1, n2);
        while i < n1 {
            let a: u8 = s1[i];
            let b: u8 = s2[i];
            debug!(a);
            debug!(b);
            assert_eq!(a, b);
            i += 1u;
        }
    }

    #[test]
    fn test_contains() {
        assert!(contains("abcde", "bcd"));
        assert!(contains("abcde", "abcd"));
        assert!(contains("abcde", "bcde"));
        assert!(contains("abcde", ""));
        assert!(contains("", ""));
        assert!(!contains("abcde", "def"));
        assert!(!contains("", "a"));

        let data = ~"ประเทศไทย中华Việt Nam";
        assert!(contains(data, "ประเ"));
        assert!(contains(data, "ะเ"));
        assert!(contains(data, "中华"));
        assert!(!contains(data, "ไท华"));
    }

    #[test]
    fn test_contains_char() {
        assert!(contains_char("abc", 'b'));
        assert!(contains_char("a", 'a'));
        assert!(!contains_char("abc", 'd'));
        assert!(!contains_char("", 'a'));
    }

    #[test]
    fn test_map() {
        assert_eq!(~"", map("", |c| unsafe {libc::toupper(c as c_char)} as char));
        assert_eq!(~"YMCA", map("ymca", |c| unsafe {libc::toupper(c as c_char)} as char));
    }

    #[test]
    fn test_chars() {
        let ss = ~"ศไทย中华Việt Nam";
        assert!(~['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a',
                       'm']
            == to_chars(ss));
    }

    #[test]
    fn test_utf16() {
        let pairs =
            [(~"𐍅𐌿𐌻𐍆𐌹𐌻𐌰\n",
              ~[0xd800_u16, 0xdf45_u16, 0xd800_u16, 0xdf3f_u16,
                0xd800_u16, 0xdf3b_u16, 0xd800_u16, 0xdf46_u16,
                0xd800_u16, 0xdf39_u16, 0xd800_u16, 0xdf3b_u16,
                0xd800_u16, 0xdf30_u16, 0x000a_u16]),

             (~"𐐒𐑉𐐮𐑀𐐲𐑋 𐐏𐐲𐑍\n",
              ~[0xd801_u16, 0xdc12_u16, 0xd801_u16,
                0xdc49_u16, 0xd801_u16, 0xdc2e_u16, 0xd801_u16,
                0xdc40_u16, 0xd801_u16, 0xdc32_u16, 0xd801_u16,
                0xdc4b_u16, 0x0020_u16, 0xd801_u16, 0xdc0f_u16,
                0xd801_u16, 0xdc32_u16, 0xd801_u16, 0xdc4d_u16,
                0x000a_u16]),

             (~"𐌀𐌖𐌋𐌄𐌑𐌉·𐌌𐌄𐌕𐌄𐌋𐌉𐌑\n",
              ~[0xd800_u16, 0xdf00_u16, 0xd800_u16, 0xdf16_u16,
                0xd800_u16, 0xdf0b_u16, 0xd800_u16, 0xdf04_u16,
                0xd800_u16, 0xdf11_u16, 0xd800_u16, 0xdf09_u16,
                0x00b7_u16, 0xd800_u16, 0xdf0c_u16, 0xd800_u16,
                0xdf04_u16, 0xd800_u16, 0xdf15_u16, 0xd800_u16,
                0xdf04_u16, 0xd800_u16, 0xdf0b_u16, 0xd800_u16,
                0xdf09_u16, 0xd800_u16, 0xdf11_u16, 0x000a_u16 ]),

             (~"𐒋𐒘𐒈𐒑𐒛𐒒 𐒕𐒓 𐒈𐒚𐒍 𐒏𐒜𐒒𐒖𐒆 𐒕𐒆\n",
              ~[0xd801_u16, 0xdc8b_u16, 0xd801_u16, 0xdc98_u16,
                0xd801_u16, 0xdc88_u16, 0xd801_u16, 0xdc91_u16,
                0xd801_u16, 0xdc9b_u16, 0xd801_u16, 0xdc92_u16,
                0x0020_u16, 0xd801_u16, 0xdc95_u16, 0xd801_u16,
                0xdc93_u16, 0x0020_u16, 0xd801_u16, 0xdc88_u16,
                0xd801_u16, 0xdc9a_u16, 0xd801_u16, 0xdc8d_u16,
                0x0020_u16, 0xd801_u16, 0xdc8f_u16, 0xd801_u16,
                0xdc9c_u16, 0xd801_u16, 0xdc92_u16, 0xd801_u16,
                0xdc96_u16, 0xd801_u16, 0xdc86_u16, 0x0020_u16,
                0xd801_u16, 0xdc95_u16, 0xd801_u16, 0xdc86_u16,
                0x000a_u16 ]) ];

        for pairs.each |p| {
            let (s, u) = copy *p;
            assert!(to_utf16(s) == u);
            assert!(from_utf16(u) == s);
            assert!(from_utf16(to_utf16(s)) == s);
            assert!(to_utf16(from_utf16(u)) == u);
        }
    }

    #[test]
    fn test_char_at() {
        let s = ~"ศไทย中华Việt Nam";
        let v = ~['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a','m'];
        let mut pos = 0;
        for v.each |ch| {
            assert!(s.char_at(pos) == *ch);
            pos += from_char(*ch).len();
        }
    }

    #[test]
    fn test_char_at_reverse() {
        let s = ~"ศไทย中华Việt Nam";
        let v = ~['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a','m'];
        let mut pos = s.len();
        for v.rev_iter().advance |ch| {
            assert!(s.char_at_reverse(pos) == *ch);
            pos -= from_char(*ch).len();
        }
    }

    #[test]
    fn test_escape_unicode() {
        assert_eq!(escape_unicode("abc"), ~"\\x61\\x62\\x63");
        assert_eq!(escape_unicode("a c"), ~"\\x61\\x20\\x63");
        assert_eq!(escape_unicode("\r\n\t"), ~"\\x0d\\x0a\\x09");
        assert_eq!(escape_unicode("'\"\\"), ~"\\x27\\x22\\x5c");
        assert!(escape_unicode("\x00\x01\xfe\xff") ==
                     ~"\\x00\\x01\\xfe\\xff");
        assert_eq!(escape_unicode("\u0100\uffff"), ~"\\u0100\\uffff");
        assert!(escape_unicode("\U00010000\U0010ffff") ==
            ~"\\U00010000\\U0010ffff");
        assert_eq!(escape_unicode("ab\ufb00"), ~"\\x61\\x62\\ufb00");
        assert_eq!(escape_unicode("\U0001d4ea\r"), ~"\\U0001d4ea\\x0d");
    }

    #[test]
    fn test_escape_default() {
        assert_eq!(escape_default("abc"), ~"abc");
        assert_eq!(escape_default("a c"), ~"a c");
        assert_eq!(escape_default("\r\n\t"), ~"\\r\\n\\t");
        assert_eq!(escape_default("'\"\\"), ~"\\'\\\"\\\\");
        assert_eq!(escape_default("\u0100\uffff"), ~"\\u0100\\uffff");
        assert!(escape_default("\U00010000\U0010ffff") ==
            ~"\\U00010000\\U0010ffff");
        assert_eq!(escape_default("ab\ufb00"), ~"ab\\ufb00");
        assert_eq!(escape_default("\U0001d4ea\r"), ~"\\U0001d4ea\\r");
    }

    #[test]
    fn test_to_managed() {
        assert_eq!("abc".to_managed(), @"abc");
        assert_eq!("abcdef".slice(1, 5).to_managed(), @"bcde");
    }

    #[test]
    fn test_total_ord() {
        "1234".cmp(& &"123") == Greater;
        "123".cmp(& &"1234") == Less;
        "1234".cmp(& &"1234") == Equal;
        "12345555".cmp(& &"123456") == Less;
        "22".cmp(& &"1234") == Greater;
    }

    #[test]
    fn test_char_range_at_reverse_underflow() {
        assert_eq!(char_range_at_reverse("abc", 0).next, 0);
    }

    #[test]
    fn test_iterator() {
        use iterator::*;
        let s = ~"ศไทย中华Việt Nam";
        let v = ~['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a','m'];

        let mut pos = 0;
        let mut it = s.iter();

        for it.advance |c| {
            assert_eq!(c, v[pos]);
            pos += 1;
        }
        assert_eq!(pos, v.len());
    }

    #[test]
    fn test_rev_iterator() {
        use iterator::*;
        let s = ~"ศไทย中华Việt Nam";
        let v = ~['m', 'a', 'N', ' ', 't', 'ệ','i','V','华','中','ย','ท','ไ','ศ'];

        let mut pos = 0;
        let mut it = s.rev_iter();

        for it.advance |c| {
            assert_eq!(c, v[pos]);
            pos += 1;
        }
        assert_eq!(pos, v.len());
    }

    #[test]
    fn test_bytes_iterator() {
        let s = ~"ศไทย中华Việt Nam";
        let v = [
            224, 184, 168, 224, 185, 132, 224, 184, 151, 224, 184, 162, 228,
            184, 173, 229, 141, 142, 86, 105, 225, 187, 135, 116, 32, 78, 97,
            109
        ];
        let mut pos = 0;

        for s.bytes_iter().advance |b| {
            assert_eq!(b, v[pos]);
            pos += 1;
        }
    }

    #[test]
    fn test_bytes_rev_iterator() {
        let s = ~"ศไทย中华Việt Nam";
        let v = [
            224, 184, 168, 224, 185, 132, 224, 184, 151, 224, 184, 162, 228,
            184, 173, 229, 141, 142, 86, 105, 225, 187, 135, 116, 32, 78, 97,
            109
        ];
        let mut pos = v.len();

        for s.bytes_rev_iter().advance |b| {
            pos -= 1;
            assert_eq!(b, v[pos]);
        }
    }

    #[test]
    fn test_split_char_iterator() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let split: ~[&str] = data.split_iter(' ').collect();
        assert_eq!(split, ~["\nMäry", "häd", "ä", "little", "lämb\nLittle", "lämb\n"]);

        let split: ~[&str] = data.split_iter(|c: char| c == ' ').collect();
        assert_eq!(split, ~["\nMäry", "häd", "ä", "little", "lämb\nLittle", "lämb\n"]);

        // Unicode
        let split: ~[&str] = data.split_iter('ä').collect();
        assert_eq!(split, ~["\nM", "ry h", "d ", " little l", "mb\nLittle l", "mb\n"]);

        let split: ~[&str] = data.split_iter(|c: char| c == 'ä').collect();
        assert_eq!(split, ~["\nM", "ry h", "d ", " little l", "mb\nLittle l", "mb\n"]);
    }
    #[test]
    fn test_splitn_char_iterator() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let split: ~[&str] = data.splitn_iter(' ', 3).collect();
        assert_eq!(split, ~["\nMäry", "häd", "ä", "little lämb\nLittle lämb\n"]);

        let split: ~[&str] = data.splitn_iter(|c: char| c == ' ', 3).collect();
        assert_eq!(split, ~["\nMäry", "häd", "ä", "little lämb\nLittle lämb\n"]);

        // Unicode
        let split: ~[&str] = data.splitn_iter('ä', 3).collect();
        assert_eq!(split, ~["\nM", "ry h", "d ", " little lämb\nLittle lämb\n"]);

        let split: ~[&str] = data.splitn_iter(|c: char| c == 'ä', 3).collect();
        assert_eq!(split, ~["\nM", "ry h", "d ", " little lämb\nLittle lämb\n"]);
    }

    #[test]
    fn test_split_char_iterator_no_trailing() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let split: ~[&str] = data.split_options_iter('\n', 1000, true).collect();
        assert_eq!(split, ~["", "Märy häd ä little lämb", "Little lämb", ""]);

        let split: ~[&str] = data.split_options_iter('\n', 1000, false).collect();
        assert_eq!(split, ~["", "Märy häd ä little lämb", "Little lämb"]);
    }

    #[test]
    fn test_word_iter() {
        let data = "\n \tMäry   häd\tä  little lämb\nLittle lämb\n";
        let words: ~[&str] = data.word_iter().collect();
        assert_eq!(words, ~["Märy", "häd", "ä", "little", "lämb", "Little", "lämb"])
    }

    #[test]
    fn test_line_iter() {
        let data = "\nMäry häd ä little lämb\n\nLittle lämb\n";
        let lines: ~[&str] = data.line_iter().collect();
        assert_eq!(lines, ~["", "Märy häd ä little lämb", "", "Little lämb"]);

        let data = "\nMäry häd ä little lämb\n\nLittle lämb"; // no trailing \n
        let lines: ~[&str] = data.line_iter().collect();
        assert_eq!(lines, ~["", "Märy häd ä little lämb", "", "Little lämb"]);
    }
}
