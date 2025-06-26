use httptrace::stats::StatsRecorder;

#[tokio::main]
pub async fn main() {
    let recorder = StatsRecorder::new();

    let client = httptrace::client::ClientBuilder::new().build().unwrap();
    let _ = client
        .get("https://www.example.com")
        .recorder(Box::new(recorder.clone()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    println!("{}", recorder.finish())
}
