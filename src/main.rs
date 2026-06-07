use env_logger;
use log;
use std::{env, io::Cursor, sync::Arc, thread};
use tiny_http::{Request, Response, Server};

fn request_is_authorised(request: &Request) -> bool {
    let api_key = request.headers().iter().find(|h| h.field.equiv("API_KEY"));
    match api_key {
        Some(api_key) => {
            if api_key.value == env::var("API_KEY").expect("[Error] API_KEY environment variable not set") {
                true
            } else {
                false
            }
        }
        None => false,
    }
}

fn log_request(request: &tiny_http::Request, status: u16, size: usize) {
    let remote_addr = request.remote_addr().unwrap().ip();
    let date_time = chrono::Local::now().format("%d/%b/%Y:%H:%M:%S %z");
    let method = request.method();
    let uri = request.url();
    let protocol = request.http_version();
    let status = status;
    let size = size;
    let referer = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Referer"))
        .map(|header| header.value.to_string())
        .unwrap_or("-".to_string());
    let user_agent = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("User-Agent"))
        .map(|header| header.value.to_string())
        .unwrap_or("-".to_string());
    println!(
        "{} [{}] \"{} {} {}\" {} {} \"{}\" \"{}\"",
        remote_addr, date_time, method, uri, protocol, status, size, referer, user_agent
    );
}

fn main() {
    env_logger::init();
    if env::var("API_KEY").is_err() {
        log::error!("[Error] API_KEY environment variable not set");
        return;
    }
    let server = Server::http("0.0.0.0:5000").expect("[Error] Could not start server");
    let server = Arc::new(server);

    for _ in 0..4 {
        let server = server.clone();
        thread::spawn(move || loop {
            let request = match server.recv() {
                Ok(r) => r,
                Err(e) => {
                    log::error!("[Error] Could not receive request: {}", e);
                    continue;
                }
            };

            let response = if !request_is_authorised(&request) {
                let response = Response::new(
                    tiny_http::StatusCode(401),
                    vec![],
                    Cursor::new(vec![]),
                    None,
                    None,
                );
                log_request(&request, 401, response.data_length().unwrap_or(0));
                response
            } else {
                let x_forwarded_for = request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv("X-Forwarded-For"))
                    .map(|header| header.value.to_string())
                    .unwrap_or("".to_string());
                let response = Response::from_string(x_forwarded_for);
                log_request(&request, 200, response.data_length().unwrap_or(0));
                response 
            };

            if let Err(e) = request.respond(response) {
                log::error!("[Error] Could not send response: {}", e);
            }
        });
    }

    loop {
        thread::park();
    }
}
