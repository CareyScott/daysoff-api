#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("bind 127.0.0.1:3000");
    println!("daysoff-api listening on http://127.0.0.1:3000");
    axum::serve(listener, daysoff_api::app())
        .await
        .expect("server error");
}
