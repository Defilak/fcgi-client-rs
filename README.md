# fcgi-client-rs

[![Crate](https://img.shields.io/crates/v/fcgi-client.svg)](https://crates.io/crates/fcgi-client)

## Description

### RU
Это форк библиотеки [fastcgi-client-rs](https://github.com/jmjoy/fastcgi-client-rs) от jmjoy.  
Основые отличия от оригинала:

-   Исправлена ошибка с переполнением стека на windows (MAX_LENGTH выделеный в стеке перенесен в кучу).
-   Использования Vec для сборки данных заменены на bytes::Bytes.
-   Код обновлен под Rust 2021.
-   Обновлены зависимости (и удалены лишние).

### EN
This is a fork of the [fastcgi-client-rs](https://github.com/jmjoy/fastcgi-client-rs) library by jmjoy.  
Main differences from the original:

- Fixed a stack overflow on windows (MAX_LENGTH allocated in the stack moved to the heap).
- All cases of using Vec are replaced with bytes::Bytes.
- Code updated for Rust 2021.
- Updated depending on (and removed unnecessary ones).

## Installation

Add dependencies to your `Cargo.toml` by `cargo add`:

```shell
cargo add tokio --features full
cargo add fastcgi-client
```

## Examples

Short connection mode:

```rust, no_run
use fastcgi_client::{Client, Params, Request};
use std::env;
use tokio::{io, net::TcpStream};

#[tokio::main]
async fn main() {
    let script_filename = env::current_dir()
        .unwrap()
        .join("tests")
        .join("php")
        .join("index.php");
    let script_filename = script_filename.to_str().unwrap();
    let script_name = "/index.php";

    // Connect to php-fpm default listening address.
    let stream = TcpStream::connect(("127.0.0.1", 9000)).await.unwrap();
    let mut client = Client::new(stream);

    // Fastcgi params, please reference to nginx-php-fpm config.
    let params = Params::default()
        .request_method("GET")
        .script_name(script_name)
        .script_filename(script_filename)
        .request_uri(script_name)
        .document_uri(script_name)
        .remote_addr("127.0.0.1")
        .remote_port(12345)
        .server_addr("127.0.0.1")
        .server_port(80)
        .server_name("jmjoy-pc")
        .content_type("")
        .content_length(0);

    // Fetch fastcgi server(php-fpm) response.
    let output = client.execute_once(Request::new(params, &mut io::empty())).await.unwrap();

    // "Content-type: text/html; charset=UTF-8\r\n\r\nhello"
    let stdout = String::from_utf8(output.stdout.unwrap()).unwrap();

    assert!(stdout.contains("Content-type: text/html; charset=UTF-8"));
    assert!(stdout.contains("hello"));
    assert_eq!(output.stderr, None);
}
```

Keep alive mode:

```rust, no_run
use fastcgi_client::{Client, Params, Request};
use std::env;
use tokio::{io, net::TcpStream};

#[tokio::main]
async fn main() {
    // Connect to php-fpm default listening address.
    let stream = TcpStream::connect(("127.0.0.1", 9000)).await.unwrap();
    let mut client = Client::new_keep_alive(stream);

    // Fastcgi params, please reference to nginx-php-fpm config.
    let params = Params::default();

    for _ in (0..3) {
        // Fetch fastcgi server(php-fpm) response.
        let output = client.execute(Request::new(params.clone(), &mut io::empty())).await.unwrap();

        // "Content-type: text/html; charset=UTF-8\r\n\r\nhello"
        let stdout = String::from_utf8(output.stdout.unwrap()).unwrap();

        assert!(stdout.contains("Content-type: text/html; charset=UTF-8"));
        assert!(stdout.contains("hello"));
        assert_eq!(output.stderr, None);
    }
}
```

## License

[Apache-2.0](https://github.com/Defilak/fcgi-client-rs/blob/master/LICENSE).
