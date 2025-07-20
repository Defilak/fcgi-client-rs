// Copyright 2022 jmjoy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use fcgi_client::{conn::KeepAlive, request::Request, Client, Params};
use std::env::current_dir;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    net::TcpStream,
    runtime::Runtime,
};

mod common;

async fn test_client<S: AsyncRead + AsyncWrite + Unpin>(client: &mut Client<S, KeepAlive>) {
    let document_root = current_dir().unwrap().join("tests").join("php");
    let document_root = document_root.to_str().unwrap();
    let script_name = current_dir()
        .unwrap()
        .join("tests")
        .join("php")
        .join("index.php");
    let script_name = script_name.to_str().unwrap();

    let params = Params::default()
        .request_method("GET")
        .document_root(document_root)
        .script_name("/index.php")
        .script_filename(script_name)
        .request_uri("/index.php")
        .document_uri("/index.php")
        .remote_addr("127.0.0.1")
        .remote_port(12345)
        .server_addr("127.0.0.1")
        .server_port(80)
        .server_name("rust-fastcgi-bench")
        .content_type("")
        .content_length(0);

    let output = client
        .execute(Request::new(params, &mut io::empty()))
        .await
        .unwrap();

    let stdout = String::from_utf8(output.stdout.unwrap_or_default().to_vec()).unwrap();
    assert!(stdout.contains("Content-type: text/html; charset=UTF-8"));
    assert!(stdout.contains("\r\n\r\n"));
    assert!(stdout.contains("hello"));
    assert_eq!(output.stderr, None);
}

fn bench_execute(c: &mut Criterion) {
    common::setup();

    let rt = Runtime::new().expect("Failed to create Tokio runtime");

    c.bench_function("fastcgi_execute", |b| {
        b.to_async(&rt).iter(|| async {
            // Создаем новый клиент для каждого запуска
            let stream = TcpStream::connect(("127.0.0.1", 9000))
                .await
                .expect("Failed to connect to FastCGI server on 127.0.0.1:9000");
            let mut client = Client::new_keep_alive(stream);
            
            black_box(test_client(&mut client).await);
        });
    });
}

criterion_group!(benches, bench_execute);
criterion_main!(benches);
