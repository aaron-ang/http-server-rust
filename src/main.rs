use async_std::{io::BufReader, net::TcpListener, net::TcpStream, prelude::*};
use futures::future;
use futures::stream::StreamExt;
use std::{collections::HashMap, env, fs};

static PORT: &str = "4221";

#[async_std::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let directory = if args.len() > 2 { &args[2] } else { "." };

    let listener = TcpListener::bind("127.0.0.1:4221").await.unwrap();

    println!(
        "Server listening on port {}, using directory: {}",
        PORT, directory
    );

    listener
        .incoming()
        .for_each_concurrent(/* limit */ None, |tcpstream| async move {
            let tcpstream = tcpstream.unwrap();
            handle_connection(tcpstream, directory).await;
        })
        .await;
}

async fn handle_connection(mut stream: TcpStream, directory: &str) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| future::ready(!line.is_empty()))
        .collect()
        .await;

    println!("Request: {:#?}", http_request);

    let start_line = http_request.first().unwrap();
    let path = start_line.split_whitespace().nth(1).unwrap();
    let headers: HashMap<&str, &str> = http_request[1..]
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
        p if p.starts_with("/files") => handle_files_endpoint(p, directory),
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

fn handle_files_endpoint(path: &str, directory: &str) -> String {
    let file_name = path.split('/').last().unwrap();
    let file_path = format!("{}/{}", directory, file_name);
    let file_content = fs::read_to_string(file_path);
    let response: String = match file_content {
        Ok(content) => format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length:{}\r\n\r\n{}",
            content.len(),
            content
        ),
        Err(_) => "HTTP/1.1 404 Not Found\r\n\r\n".to_string(),
    };
    response
}
