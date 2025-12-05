use kurosabi::Kurosabi;
use observer::context::ObserverContext;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    env_logger::try_init_from_env(env_logger::Env::default().default_filter_or("debug")).unwrap_or_else(|_| ());

    // コンテキスト初期化
    let ob_ctx = ObserverContext::new().await;

    let config = ob_ctx.config.clone();

    let mut kurosabi = Kurosabi::with_context(ob_ctx.clone());

    kurosabi.get("/latex_expr_render", |mut c| async move {
        c.res.html(include_str!("../data/latex_render.html"));
        c
    });

    let server = kurosabi
        .server()
        .thread(16)
        .host(config.web_server_host)
        .port(config.web_server_port)
        .build();

    println!("server started. Press Ctrl-C to shutdown...");

    tokio::select! {
        _ = server.run_async() => {
            println!("server stopped (run_async returned)");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("received Ctrl-C, shutting down server and browser engine...");
        }
    }

    // サーバ停止後にエンジンもshutdown
    if let Err(e) = ob_ctx.shutdown().await {
        eprintln!("engine shutdown error: {}", e);
    }
    println!("shutdown complete. Exiting.");
}