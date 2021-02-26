# fastcgi-client-rs

[![Build Status](https://travis-ci.org/jmjoy/fastcgi-client-rs.svg?branch=master)](https://travis-ci.org/jmjoy/fastcgi-client-rs)
[![Crate](https://img.shields.io/crates/v/fastcgi-client.svg)](https://crates.io/crates/fastcgi-client)
[![API](https://docs.rs/fastcgi-client/badge.svg)](https://docs.rs/fastcgi-client)

Fastcgi client implemented for Rust.

## Features

Support both `async(futures, async-std)` and `sync(std)` clients.

Be default, both `async` and `sync` client are included, if you don't want to include `async` client,
You can specify `default-features = false` in `Cargo.toml`.

## Installation

With [cargo add](https://github.com/killercup/cargo-edit) installed run:

```bash
$ cargo add fastcgi-client
```

## Examples

Async `async-std` client:

```
use fastcgi_client::{AsyncClient, Params};
use std::env;
use async_std::{io, task};
use async_std::net::TcpStream;

task::block_on(async {
    let script_filename = env::current_dir()
        .unwrap()
        .join("tests")
        .join("php")
        .join("index.php");
    let script_filename = script_filename.to_str().unwrap();
    let script_name = "/index.php";

    // Connect to php-fpm default listening address.
    let stream = TcpStream::connect(("127.0.0.1", 9000)).await.unwrap();
    let mut client = AsyncClient::new(stream, false);

    // Fastcgi params, please reference to nginx-php-fpm config.
    let params = Params::with_predefine()
        .set_request_method("GET")
        .set_script_name(script_name)
        .set_script_filename(script_filename)
        .set_request_uri(script_name)
        .set_document_uri(script_name)
        .set_remote_addr("127.0.0.1")
        .set_remote_port("12345")
        .set_server_addr("127.0.0.1")
        .set_server_port("80")
        .set_server_name("jmjoy-pc")
        .set_content_type("")
        .set_content_length("0");

    // Fetch fastcgi server(php-fpm) response.
    let output = client.do_request(&params, &mut io::empty()).await.unwrap();

    // "Content-type: text/html; charset=UTF-8\r\n\r\nhello"
    let stdout = String::from_utf8(output.get_stdout().unwrap()).unwrap();

    assert!(stdout.contains("Content-type: text/html; charset=UTF-8"));
    assert!(stdout.contains("hello"));
    assert_eq!(output.get_stderr(), None);
});
```

Sync `std` client:

```
use fastcgi_client::{Client, Params};
use std::{env, io};
use std::net::TcpStream;

let script_filename = env::current_dir()
    .unwrap()
    .join("tests")
    .join("php")
    .join("index.php");
let script_filename = script_filename.to_str().unwrap();
let script_name = "/index.php";

// Connect to php-fpm default listening address.
let stream = TcpStream::connect(("127.0.0.1", 9000)).unwrap();
let mut client = Client::new(stream, false);

// Fastcgi params, please reference to nginx-php-fpm config.
let params = Params::with_predefine()
    .set_request_method("GET")
    .set_script_name(script_name)
    .set_script_filename(script_filename)
    .set_request_uri(script_name)
    .set_document_uri(script_name)
    .set_remote_addr("127.0.0.1")
    .set_remote_port("12345")
    .set_server_addr("127.0.0.1")
    .set_server_port("80")
    .set_server_name("jmjoy-pc")
    .set_content_type("")
    .set_content_length("0");

// Fetch fastcgi server(php-fpm) response.
let output = client.do_request(&params, &mut io::empty()).unwrap();

// "Content-type: text/html; charset=UTF-8\r\n\r\nhello"
let stdout = String::from_utf8(output.get_stdout().unwrap()).unwrap();

assert!(stdout.contains("Content-type: text/html; charset=UTF-8"));
assert!(stdout.contains("hello"));
assert_eq!(output.get_stderr(), None);
```

## License
[MIT](https://github.com/jmjoy/fastcgi-client-rs/blob/master/LICENSE).

