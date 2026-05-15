use crate::dns_client::DNSClient;

#[allow(dead_code, unused_variables, unused_mut)]
mod dns_client;

#[tokio::main]
async fn main() {
    let mut client = DNSClient::get_client("127.0.0.1:5300".to_string()).await;

    let chunk = client.request_chunk(43).await.unwrap();
}
