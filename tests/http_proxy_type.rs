//! `ProxyType::Http` must proxy HTTPS requests too (via CONNECT), not only
//! plain http:// ones. A listener stands in for the proxy and records the
//! first request line it receives.

use std::time::Duration;

use yt_dlp::client::proxy::{ProxyConfig, ProxyType};
use yt_dlp::utils::http::create_http_client;

#[tokio::test]
async fn http_proxy_type_carries_https_through_connect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind proxy stand-in");
    let addr = listener.local_addr().expect("local addr");
    let (line_tx, line_rx) = tokio::sync::oneshot::channel::<String>();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        if let Ok((socket, _)) = listener.accept().await {
            let mut reader = BufReader::new(socket);
            let mut first = String::new();
            let _ = reader.read_line(&mut first).await;
            let _ = line_tx.send(first.trim_end().to_owned());
            let _ = reader
                .get_mut()
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                .await;
        }
    });

    let proxy = ProxyConfig::new(ProxyType::Http, format!("http://{addr}"));
    let client = create_http_client(Some(&proxy)).expect("client builds");
    // The request is expected to fail (the stand-in answers 502). What matters
    // is that it reached the proxy as a CONNECT, not a direct connection.
    let _ = client
        .get("https://example.invalid/")
        .timeout(Duration::from_secs(5))
        .send()
        .await;

    let first = tokio::time::timeout(Duration::from_secs(5), line_rx)
        .await
        .expect("the proxy received a request")
        .expect("request line");
    assert_eq!(first, "CONNECT example.invalid:443 HTTP/1.1");
}
