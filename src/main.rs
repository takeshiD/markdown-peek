use axum::{Router, response::Html, routing::get};
use std::{fs, net::SocketAddr};
use tokio::net::TcpListener;
use pulldown_cmark::{Parser, Options};

#[tokio::main]
async fn main() {
    let static_files = tower_http::services::ServeDir::new("./static");
    let app = Router::new()
        .route("/", get(hello_handler))
        .nest_service("/static", static_files);
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("Listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}

async fn hello_handler() -> Html<String> {
    let markdown_input = r#"
# HelloWorld
hello_handler
- [ ] task1
- [ ] task2
    - [ ] task3
    - [ ] task4

> Quote  
> Hello Quote

## Header Level2
Hello Header

# Table
| name | age |
|------|-----|
| takeshid | 99 |
    "#;
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(markdown_input, options);
    let mut html_body = String::new();
    pulldown_cmark::html::push_html(&mut html_body, parser);
    let template = fs::read_to_string("templates/preview.html").unwrap();
    let page = template
        .replace("{{ content }}", &html_body)
        .replace("{{theme}}", "github");
    Html(page)
}
