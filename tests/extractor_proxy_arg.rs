//! The metadata extractors run their own yt-dlp command lines, so they must get
//! the configured proxy. A listener stands in for the proxy: the extractor's
//! yt-dlp process should open a CONNECT to YouTube through it.

use std::path::PathBuf;
use std::time::Duration;

use yt_dlp::Downloader;
use yt_dlp::client::Libraries;
use yt_dlp::client::proxy::{ProxyConfig, ProxyType};

#[tokio::test]
async fn extractor_metadata_fetch_goes_through_the_proxy() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind proxy stand-in");
    let addr = listener.local_addr().expect("local addr");
    let (line_tx, line_rx) = tokio::sync::oneshot::channel::<String>();
    tokio::spawn(async move {
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

    let libraries = Libraries::new(PathBuf::from("yt-dlp"), PathBuf::from("ffmpeg"));
    let output = std::env::temp_dir().join("yt-dlp-extractor-proxy-test");
    let downloader = Downloader::builder(libraries, output)
        .with_timeout(Duration::from_secs(20))
        .with_proxy(ProxyConfig::new(ProxyType::Http, format!("http://{addr}")))
        .build()
        .await
        .expect("downloader builds");

    let _ = tokio::time::timeout(
        Duration::from_secs(20),
        downloader.fetch_video_infos("https://www.youtube.com/watch?v=aqz-KE-bpKQ"),
    )
    .await;

    let first = tokio::time::timeout(Duration::from_secs(5), line_rx)
        .await
        .expect("the extractor's yt-dlp used the proxy")
        .expect("request line");
    assert!(first.starts_with("CONNECT "), "{first}");
    assert!(first.contains("youtube.com"), "{first}");
}
