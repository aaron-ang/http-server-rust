use async_std::{io::BufReader, net::TcpListener, net::TcpStream, prelude::*};
use futures::stream::StreamExt;
use futures::{future, AsyncBufReadExt};
use std::{collections::HashMap, env, fs};

type Headers<'a> = HashMap<&'a str, &'a str>;

static PORT: &str = "4221";

#[async_std::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let directory = if args.len() > 2 { &args[2] } else { "." };

    let listener = TcpListener::bind(format!("127.0.0.1:{PORT}"))
        .await
        .unwrap();

    println!("Server listening on port {PORT}, using directory: {directory}");

    listener
        .incoming()
        .for_each_concurrent(/* limit */ None, |tcpstream| async move {
            let tcpstream = tcpstream.unwrap();
            handle_connection(tcpstream, directory).await;
        })
        .await;
}

async fn handle_connection(mut stream: TcpStream, directory: &str) {
    let mut buf_reader = BufReader::new(&mut stream);
    assert!(buf_reader.fill_buf().await.is_ok());
    let http_request_raw = buf_reader.buffer();
    let http_request_top = AsyncBufReadExt::lines(http_request_raw).map(|l| l.unwrap());
    let http_request: Vec<String> = http_request_top
        .take_while(|l| future::ready(!l.is_empty()))
        .collect()
        .await;
    let body = http_request_raw[http_request.join("\r\n").len() + 4..].to_vec();

    println!("Request: {:#?}", http_request);
    println!("Body: {:#?}", String::from_utf8_lossy(&body));

    let mut start_line = http_request.first().unwrap().split_whitespace();
    let method = start_line.next().unwrap();
    let path = start_line.next().unwrap();
    let headers: Headers = http_request[1..]
        .iter()
        .map(|line| {
            let mut parts = line.splitn(2, ": ");
            (parts.next().unwrap(), parts.next().unwrap())
        })
        .collect();

    let response = match path {
        "/" => "HTTP/1.1 200 OK\r\n\r\n".to_string(),
        p if p.starts_with("/echo") => handle_echo_endpoint(p),
        p if p.starts_with("/user-agent") => {
            handle_user_agent_endpoint(headers.get("User-Agent").unwrap())
        }
        p if p.starts_with("/files") => handle_files_endpoint(method, directory, p, body),
        _ => "HTTP/1.1 404 Not Found\r\n\r\n".to_string(),
    };

    stream.write_all(response.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

fn handle_echo_endpoint(path: &str) -> String {
    let string = path.split('/').last().unwrap();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length:{}\r\n\r\n{}",
        string.len(),
        string
    );
    response
}

fn handle_user_agent_endpoint(user_agent: &str) -> String {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length:{}\r\n\r\n{}",
        user_agent.len(),
        user_agent
    );
    response
}

fn handle_files_endpoint(method: &str, directory: &str, path: &str, body: Vec<u8>) -> String {
    let file_name = path.split('/').last().unwrap();
    let file_path = format!("{}/{}", directory, file_name);
    let response: String;

    if method == "POST" {
        let res = fs::write(file_path, body);
        response = match res {
            Ok(_) => "HTTP/1.1 201 Created\r\n\r\n".to_string(),
            Err(_) => "HTTP/1.1 500 Internal Server Error\r\n\r\n".to_string(),
        };
    } else {
        let file_content = fs::read_to_string(file_path);
        response = match file_content {
            Ok(content) => format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length:{}\r\n\r\n{}",
                content.len(),
                content
            ),
            Err(_) => "HTTP/1.1 404 Not Found\r\n\r\n".to_string(),
        };
    }
    response
}
