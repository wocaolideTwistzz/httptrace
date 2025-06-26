use httptrace::client::ClientBuilder;

#[tokio::main]
pub async fn main() {
    let client = ClientBuilder::new().build().unwrap();
    let result = client.get("https://www.bing.com").send().await.unwrap();

    println!("https-status: {}", result.status());
    println!("https-body: {:?}", result.text().await);

    let result1 = client.get("https://www.bing.com").send().await.unwrap();

    println!("http-status: {}", result1.status());
    println!("http-body: {:?}", result1.text().await);
}
