#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("bind 127.0.0.1:3000");
    println!("bv-vacation-api listening on http://127.0.0.1:3000");
    axum::serve(listener, bv_vacation_api::app())
        .await
        .expect("server error");
}
